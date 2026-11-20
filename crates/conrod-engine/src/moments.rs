//! The moment lane: ask the album's judge what is clearly visible in each
//! keeper frame (and, for a judge that can say, which frame of a burst is the
//! peak), and store it on the frame. Ranking reads it back in `passes`.
//!
//! A failed or missing answer leaves the frame as it was: no moment, no
//! demotion, and the scan never waits on it.

use crate::desktop::{rows, Desktop, Result};
use crate::{err, lock};
use conrod_core::decision::{peak_frame, InProcess};
use conrod_core::moments::{Moment, MomentVerdict};
use conrod_core::profile::Lanes;
use conrod_core::settings::{self, Settings};
use conrod_io::moments::{judge_for, MomentJudge};
use conrod_io::{settings::Settings as IoSettings, vlm};
use conrod_vision::imageops::Rgb;
use rusqlite::params;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// Longest side of a frame sent to the judge: well inside the decider's 1.6 MP.
const FRAME_EDGE: u32 = 1280;
/// Longest side of each frame of a burst, so about eight fit a request.
const PEAK_EDGE: u32 = 640;
const PEAK_FRAMES: usize = 8;
/// A peak must be at least this likely, and beat the runner-up, to count.
const PEAK_FLOOR: f64 = 0.5;

/// The in-process model's seat. The decider crate swaps this for a loaded
/// model; until then the in-process transport fails plainly and the OpenRouter
/// transport is used by choice only.
pub(crate) fn in_process_seat() -> InProcess {
    InProcess::unavailable()
}

fn data_url(image: &Rgb, edge: u32) -> String {
    format!(
        "data:image/jpeg;base64,{}",
        vlm::encode_for_model(image, edge)
    )
}

/// The frame as the judge sees it: the cached preview when there is one,
/// else the file's own embedded preview.
fn picture(preview: Option<&str>, path: &str) -> Result<Rgb> {
    if let Some(cached) = preview.filter(|p| Path::new(p).is_file()) {
        if let Ok(bytes) = std::fs::read(cached) {
            if let Ok(image) = Rgb::decode_jpeg(&bytes, 1) {
                return Ok(image);
            }
        }
    }
    let raw = conrod_io::raw::read(Path::new(path))?;
    Ok(Rgb::decode_jpeg(&raw.preview, 1)?.orient(raw.orientation))
}

/// Keepers of the album, or every frame, that have not been judged yet.
fn pending(d: &Desktop, job: i64, everything: bool) -> Result<Vec<Value>> {
    let keepers = if everything {
        String::new()
    } else {
        "AND (COALESCE(i.picked,0)=1 OR COALESCE(i.burst_pick,0)=1 OR COALESCE((SELECT max(d.burst_pick) FROM detections d WHERE d.image_id=i.id),0)=1)".to_string()
    };
    rows(
        &lock(&d.reader),
        &format!("SELECT i.id,i.path,i.preview_path FROM images i WHERE i.job_id=? AND i.status='done' AND COALESCE(i.rejected,0)=0 AND COALESCE(i.copy_of,0)=0 AND i.moments IS NULL {keepers} ORDER BY i.id"),
        [job],
    )
}

fn store(d: &Desktop, image: i64, verdict: &MomentVerdict) -> Result<()> {
    lock(&d.db)
        .execute(
            "UPDATE images SET moments=? WHERE id=?",
            params![verdict.to_json(), image],
        )
        .map_err(err)?;
    Ok(())
}

/// Judge the album's frames. Returns how many frames were judged.
pub(crate) fn run(
    d: &Desktop,
    job: i64,
    stop: &AtomicBool,
    task: &conrod_core::tasks::Task,
    cfg: &Settings,
    lanes: &Lanes,
    scope: &str,
) -> Result<usize> {
    let vocab: &[Moment] = lanes.moments();
    if vocab.is_empty() {
        return Ok(0);
    }
    let vlm_judge = cfg.use_vlm.then(|| {
        (
            vlm::VlmClient::new(Arc::new(vlm::RealClock::default())),
            IoSettings::from_core(cfg),
        )
    });
    let judge = match judge_for(cfg, vlm_judge, in_process_seat()) {
        Ok(judge) => judge,
        Err(why) => {
            // Not a failure of the scan: say why nothing was judged.
            task.detail(format!("Moments skipped: {why}"));
            return Ok(0);
        }
    };
    let everything = scope == settings::IDENTIFY_EVERYTHING;
    let frames = pending(d, job, everything)?;
    let context = lanes.preset.prompt_context();
    task.detail(format!("Judging moments with the {} backend", judge.name()));
    let (next, judged) = (AtomicUsize::new(0), AtomicUsize::new(0));
    let workers = (cfg.identify_worker_count().max(1) as usize)
        .min(4)
        .min(frames.len().max(1));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let Some(frame) = frames.get(next.fetch_add(1, Ordering::Relaxed)) else {
                    break;
                };
                let id = frame["id"].as_i64().unwrap_or_default();
                let Ok(image) = picture(
                    frame["preview_path"].as_str(),
                    frame["path"].as_str().unwrap_or_default(),
                ) else {
                    continue;
                };
                // A failed answer leaves the frame unjudged; a retry judges it later.
                if let Ok(verdict) = judge.judge(context, vocab, &data_url(&image, FRAME_EDGE)) {
                    if store(d, id, &verdict).is_ok() {
                        judged.fetch_add(1, Ordering::Relaxed);
                    }
                }
                task.progress(
                    next.load(Ordering::Relaxed).min(frames.len()) as u64,
                    frames.len() as u64,
                );
            });
        }
    });
    if lanes.moments_rank && !stop.load(Ordering::Relaxed) {
        peaks(d, job, stop, task, judge.as_ref(), context)?;
    }
    Ok(judged.load(Ordering::Relaxed))
}

/// For each burst, ask a judge that can say which frame shows the peak, and
/// mark the one it picks clearly. Everything else in the burst loses any
/// earlier mark.
fn peaks(
    d: &Desktop,
    job: i64,
    stop: &AtomicBool,
    task: &conrod_core::tasks::Task,
    judge: &dyn MomentJudge,
    context: &str,
) -> Result<()> {
    let frames = rows(
        &lock(&d.reader),
        "SELECT i.id,i.path,i.preview_path,i.burst_key,i.moments FROM images i WHERE i.job_id=? AND i.burst_key IS NOT NULL AND i.status='done' AND COALESCE(i.rejected,0)=0 AND COALESCE(i.copy_of,0)=0 ORDER BY i.burst_key,i.id",
        [job],
    )?;
    let mut bursts: BTreeMap<i64, Vec<&Value>> = BTreeMap::new();
    for frame in &frames {
        bursts
            .entry(frame["burst_key"].as_i64().unwrap_or_default())
            .or_default()
            .push(frame);
    }
    for members in bursts.values().filter(|b| b.len() >= 2) {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        // At most eight, evenly spread, so the request stays small.
        let step = members.len().div_ceil(PEAK_FRAMES);
        let chosen: Vec<&Value> = members
            .iter()
            .copied()
            .step_by(step)
            .take(PEAK_FRAMES)
            .collect();
        let mut images = Vec::new();
        for frame in &chosen {
            match picture(
                frame["preview_path"].as_str(),
                frame["path"].as_str().unwrap_or_default(),
            ) {
                Ok(image) => images.push(data_url(&image, PEAK_EDGE)),
                Err(_) => break,
            }
        }
        if images.len() != chosen.len() {
            continue;
        }
        // A broken answer is no opinion: the burst keeps its order.
        let Ok(Some(ps)) = judge.peak(context, &images) else {
            continue;
        };
        let winner = peak_frame(&ps, PEAK_FLOOR);
        for (i, frame) in chosen.iter().enumerate() {
            let id = frame["id"].as_i64().unwrap_or_default();
            let mut verdict = frame["moments"]
                .as_str()
                .map(MomentVerdict::from_json)
                .unwrap_or_default();
            let was = verdict.peak;
            verdict.peak = (winner == Some(i)).then(|| ps[i]);
            if verdict.peak != was {
                if verdict.backend.is_empty() {
                    verdict.backend = judge.name().to_string();
                }
                store(d, id, &verdict)?;
            }
        }
        task.detail(format!(
            "Compared {} frames of a burst for the peak",
            chosen.len()
        ));
    }
    Ok(())
}
