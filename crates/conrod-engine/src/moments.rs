//! The moment lane: ask the album's judge what is clearly visible in each
//! keeper frame (and, for a judge that can say, which frame of a burst is the
//! peak), and store it on the frame. Ranking reads it back in `passes`.
//!
//! A failed or missing answer leaves the frame as it was: no moment, no
//! demotion, and the scan never waits on it.

use crate::desktop::{rows, Desktop, Result};
use crate::identify_live::{IdentifyLive, MOMENTS_PHASE, PEAKS_PHASE};
use crate::{err, lock};
use conrod_core::decision::{peak_frame, InProcess};
use conrod_core::moments::{Moment, MomentVerdict};
use conrod_core::profile::{Lanes, ShootPreset};
use conrod_core::progress::IDENTIFY_DECISION;
use conrod_core::settings::{self, Settings};
use conrod_io::moments::{judge_for, MomentJudge};
use conrod_io::{settings::Settings as IoSettings, vlm};
use conrod_vision::imageops::Rgb;
use rusqlite::params;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

/// Frames of one burst the type round asks about: enough to see the action
/// without asking about the whole burst.
const TYPE_FRAMES_PER_BURST: usize = 3;
/// Frames of the album the `auto` type question sees, spread evenly.
const TYPE_SAMPLE: usize = 8;
/// The concrete types an `auto` album can be told it fits. A suggestion only;
/// the photographer still accepts it.
const TYPE_CHOICES: [(&str, &str); 10] = [
    (
        "motorsport",
        "Circuit racing or a track day: cars and motorcycles in action",
    ),
    (
        "motorsport-meet",
        "A car meet or show: parked and posed vehicles",
    ),
    ("sport-rodeo", "A rodeo: riders on bucking bulls or horses"),
    (
        "sport-equestrian",
        "Equestrian: horses and riders over fences",
    ),
    ("wedding", "A wedding: the couple, vows and guests"),
    (
        "portrait",
        "A portrait session: people posed for the camera",
    ),
    ("event", "An event: people, performers and the crowd"),
    ("concert", "A concert: performers and stage light"),
    ("wildlife-birds", "Birds or wildlife: animals in the wild"),
    (
        "landscape",
        "Landscape or property: places, rooms and horizons",
    ),
];

/// Longest side of a frame sent to the judge: well inside the decider's 1.6 MP.
const FRAME_EDGE: u32 = 1280;
/// Longest side of each frame of a burst, so about eight fit a request.
const PEAK_EDGE: u32 = 640;
const PEAK_FRAMES: usize = 8;
/// A peak must be at least this likely, and beat the runner-up, to count.
const PEAK_FLOOR: f64 = 0.5;

thread_local! {
    /// What the DECIDER node shows for the call this thread is making: the frame's
    /// name and its picture. Set around each judge call by [`shown_as`].
    static SHOWN: RefCell<Option<(String, Option<PathBuf>)>> = const { RefCell::new(None) };
}

/// Run `f` with the decision model's calls labelled for the DECIDER node.
fn shown_as<T>(label: &str, thumb: Option<PathBuf>, f: impl FnOnce() -> T) -> T {
    SHOWN.with(|s| *s.borrow_mut() = Some((label.to_string(), thumb)));
    let out = f();
    SHOWN.with(|s| *s.borrow_mut() = None);
    out
}

/// The in-process model's seat: the decider service when it is turned on and ready
/// (installed, with room on the card), else an unavailable seat, so the judge
/// falls back to OpenRouter by its transport setting or to no judgement at all.
/// Every call goes through [`crate::decider::Service::decide`], so it is queued on the
/// single GPU slot, shown on the DECIDER node (`decider-progress`), and gated against the
/// scan's GPU stages on a small card.
pub(crate) fn in_process_seat() -> InProcess {
    seat(crate::decider::service())
}

fn seat(service: &'static crate::decider::Service) -> InProcess {
    if !service.available() {
        return InProcess::unavailable();
    }
    InProcess::with(move |request| {
        let (label, thumb) = SHOWN
            .with(|s| s.borrow().clone())
            .unwrap_or_else(|| ("Moments".to_string(), None));
        service.decide(&label, thumb.as_deref(), request)
    })
}

/// The frame's file name, for the DECIDER node.
fn frame_label(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned())
}

/// The cached preview or thumbnail of a frame, when one exists on disk.
/// Most freshly scanned frames only have a thumbnail until opened in Review,
/// so falling back to `thumb_path` lets the DECIDER lane show the photo live.
fn thumb_of(preview: Option<&str>, thumb: Option<&str>) -> Option<PathBuf> {
    [preview, thumb]
        .into_iter()
        .flatten()
        .map(PathBuf::from)
        .find(|p| p.is_file())
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

/// Keepers of the album, or every frame, that have not been judged yet. A
/// `rejudge` run looks at every frame again, judged or not: the photographer
/// asked for the decisions to be made again, and a failed answer still leaves
/// the frame's old verdict in place.
fn pending(d: &Desktop, job: i64, everything: bool, rejudge: bool) -> Result<Vec<Value>> {
    let keepers = if everything {
        String::new()
    } else {
        "AND (COALESCE(i.picked,0)=1 OR COALESCE(i.burst_pick,0)=1 OR COALESCE((SELECT max(d.burst_pick) FROM detections d WHERE d.image_id=i.id),0)=1)".to_string()
    };
    let unread = if rejudge { "" } else { "AND i.moments IS NULL" };
    rows(
        &lock(&d.reader),
        &format!("SELECT i.id,i.path,i.preview_path,i.thumb_path,i.moments,(SELECT d.crop_path FROM detections d WHERE d.image_id=i.id AND d.crop_path IS NOT NULL AND d.crop_path!='' ORDER BY COALESCE(d.sharpness,0) DESC,d.id LIMIT 1) AS crop_path FROM images i WHERE i.job_id=? AND i.status='done' AND COALESCE(i.rejected,0)=0 AND COALESCE(i.copy_of,0)=0 {unread} {keepers} ORDER BY i.id"),
        [job],
    )
}

/// How many bursts still need comparing for their peak: at least two frames
/// and no frame carrying a peak from an earlier pass.
fn remaining_bursts(d: &Desktop, job: i64) -> Result<usize> {
    let frames = rows(
        &lock(&d.reader),
        "SELECT i.id,i.burst_key,i.moments FROM images i WHERE i.job_id=? AND i.burst_key IS NOT NULL AND i.status='done' AND COALESCE(i.rejected,0)=0 AND COALESCE(i.copy_of,0)=0 ORDER BY i.burst_key,i.id",
        [job],
    )?;
    let mut bursts: BTreeMap<i64, Vec<&Value>> = BTreeMap::new();
    for frame in &frames {
        bursts
            .entry(frame["burst_key"].as_i64().unwrap_or_default())
            .or_default()
            .push(frame);
    }
    Ok(bursts
        .values()
        .filter(|b| b.len() >= 2)
        .filter(|b| !judged_burst(b.as_slice()))
        .count())
}

/// The live identify feed this run narrates its post-cars phases to, when one
/// is still running. A standalone judge run has none: the album's feed belongs
/// to an earlier run that has already finished.
fn live_feed(d: &Desktop) -> Option<Arc<IdentifyLive>> {
    lock(&d.identify).clone().filter(|live| !live.finished())
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

/// Judge the album's frames. Returns how many frames were judged. `rejudge`
/// looks at frames that already carry a verdict too (a manual "judge again");
/// otherwise only frames nobody has judged are asked.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run(
    d: &Desktop,
    job: i64,
    stop: &AtomicBool,
    task: &conrod_core::tasks::Task,
    cfg: &Settings,
    lanes: &Lanes,
    scope: &str,
    rejudge: bool,
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
    run_with(
        d,
        job,
        stop,
        task,
        cfg,
        lanes,
        scope,
        rejudge,
        cfg.identify_worker_count().max(1) as usize,
        judge.as_ref(),
        None,
    )
}

/// The whole Decision stage: the type round, then the frame/peak round, on one
/// judge and one set of counters. `None` means no decision service could be
/// chosen, so the stage is skipped rather than failed.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_decision(
    d: &Desktop,
    job: i64,
    stop: &AtomicBool,
    task: &conrod_core::tasks::Task,
    cfg: &Settings,
    lanes: &Lanes,
    scope: &str,
    seats: usize,
    progress: Option<&crate::progress::Progress>,
) -> Result<Option<usize>> {
    if lanes.moments().is_empty() {
        return Ok(None);
    }
    let control = d.stage_control(job);
    if stop.load(Ordering::Relaxed) || control.is_skipped(IDENTIFY_DECISION) {
        return Ok(Some(0));
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
            task.detail(format!("Moments skipped: {why}"));
            return Ok(None);
        }
    };
    run_type_round(d, job, stop, task, cfg, lanes, judge.as_ref(), progress)?;
    if stop.load(Ordering::Relaxed) || control.is_skipped(IDENTIFY_DECISION) {
        return Ok(Some(0));
    }
    let judged = run_with(
        d,
        job,
        stop,
        task,
        cfg,
        lanes,
        scope,
        false,
        seats,
        judge.as_ref(),
        progress,
    )?;
    Ok(Some(judged))
}

/// [`run`] with the judge already chosen: the seam the tests use to count who
/// was asked, without a model on the card. Everything below is the run itself;
/// a frame that already carries a verdict is never asked again (unless the
/// photographer asked for a rejudge).
#[allow(clippy::too_many_arguments)]
fn run_with(
    d: &Desktop,
    job: i64,
    stop: &AtomicBool,
    task: &conrod_core::tasks::Task,
    _cfg: &Settings,
    lanes: &Lanes,
    scope: &str,
    rejudge: bool,
    seats: usize,
    judge: &dyn MomentJudge,
    progress: Option<&crate::progress::Progress>,
) -> Result<usize> {
    let control = d.stage_control(job);
    if stop.load(Ordering::Relaxed) || control.is_skipped(IDENTIFY_DECISION) {
        return Ok(0);
    }
    let vocab: &[Moment] = lanes.moments();
    let everything = scope == settings::IDENTIFY_EVERYTHING;
    let frames = pending(d, job, everything, rejudge)?;
    // The stage's counters: every frame still to judge plus every burst still
    // to compare. A judged burst is not planned again, so a re-run's header
    // shows the real remaining work.
    let burst_total = if lanes.moments_rank {
        remaining_bursts(d, job)?
    } else {
        0
    };
    if let Some(p) = progress {
        p.plan(IDENTIFY_DECISION, (frames.len() + burst_total) as u64);
    }
    let context = lanes.preset.prompt_context();
    task.detail(format!("Judging moments with the {} backend", judge.name()));
    // The identify screen names this work: judging frames, then the bursts.
    let feed = live_feed(d);
    if let Some(feed) = &feed {
        feed.begin_phase(MOMENTS_PHASE, frames.len());
    }
    let (next, judged) = (AtomicUsize::new(0), AtomicUsize::new(0));
    // The album's decision seats (both models local) or today's identify count;
    // the decision model answers one request at a time, so these are queues.
    let workers = seats.clamp(1, 4).min(frames.len().max(1));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                if stop.load(Ordering::Relaxed) || control.is_skipped(IDENTIFY_DECISION) {
                    break;
                }
                let Some(frame) = frames.get(next.fetch_add(1, Ordering::Relaxed)) else {
                    break;
                };
                // Between asks: a paused Decision stage parks here, so the call
                // already in flight finishes and the worker then holds.
                if !control.park_task(IDENTIFY_DECISION, stop, progress, task)
                    || control.is_skipped(IDENTIFY_DECISION)
                {
                    break;
                }
                let id = frame["id"].as_i64().unwrap_or_default();
                let label = frame_label(frame["path"].as_str().unwrap_or_default());
                let thumb = thumb_of(frame["preview_path"].as_str(), frame["thumb_path"].as_str());
                let Ok(image) = picture(
                    frame["preview_path"].as_str(),
                    frame["path"].as_str().unwrap_or_default(),
                ) else {
                    continue;
                };
                // A failed answer leaves the frame unjudged; a retry judges it later.
                let url = data_url(&image, FRAME_EDGE);
                // The car up close rides second: the whole frame must stay first so the
                // ground line (and a wheel off it) is visible.
                let crop = frame["crop_path"]
                    .as_str()
                    .filter(|path| Path::new(path).is_file())
                    .and_then(|path| std::fs::read(path).ok())
                    .and_then(|bytes| Rgb::decode_jpeg(&bytes, 1).ok())
                    .map(|image| data_url(&image, FRAME_EDGE));
                let verdict = shown_as(&label, thumb, || {
                    judge.judge_pair(context, vocab, &url, crop.as_deref())
                });
                if let Ok(mut verdict) = verdict {
                    // The burst pass owns `peak`: reading a frame again (a
                    // rejudge) must not wipe the peak an earlier pass stored.
                    if verdict.peak.is_none() {
                        verdict.peak = stored_peak(frame);
                    }
                    if store(d, id, &verdict).is_ok() {
                        judged.fetch_add(1, Ordering::Relaxed);
                    }
                }
                let read = next.load(Ordering::Relaxed).min(frames.len());
                task.progress(read as u64, frames.len() as u64);
                if let Some(feed) = &feed {
                    feed.phase_progress(read);
                }
                if let Some(p) = progress {
                    p.advance(IDENTIFY_DECISION, 1);
                }
            });
        }
    });
    let ranked = if lanes.moments_rank
        && !stop.load(Ordering::Relaxed)
        && !control.is_skipped(IDENTIFY_DECISION)
    {
        peaks(
            d,
            job,
            stop,
            task,
            judge,
            context,
            feed.as_deref(),
            progress,
        )
    } else {
        Ok(())
    };
    if let Some(feed) = &feed {
        feed.end_phase();
    }
    ranked?;
    Ok(judged.load(Ordering::Relaxed))
}

/// The type round that opens the Decision stage.
///
/// A typed album asks its own finetuned questions on a small sample of each
/// burst. An `auto` album instead asks one choice question over a spread sample
/// of the album and stores the suggested type (and confidence) on the job:
/// a suggestion only, never applied, so the photographer's own type always
/// wins. A failed ask is no opinion, never a failed scan.
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_type_round(
    d: &Desktop,
    job: i64,
    stop: &AtomicBool,
    task: &conrod_core::tasks::Task,
    cfg: &Settings,
    lanes: &Lanes,
    judge: &dyn MomentJudge,
    progress: Option<&crate::progress::Progress>,
) -> Result<Option<(String, f64)>> {
    if lanes.preset == ShootPreset::Auto {
        return auto_type_round(d, job, stop, task, cfg, judge, progress);
    }
    let vocab = lanes.moments();
    if vocab.is_empty() {
        return Ok(None);
    }
    let control = d.stage_control(job);
    if stop.load(Ordering::Relaxed) || control.is_skipped(IDENTIFY_DECISION) {
        return Ok(None);
    }
    let frames = rows(
        &lock(&d.reader),
        "SELECT i.id,i.burst_key,i.path,i.preview_path,i.thumb_path,(SELECT d.crop_path FROM detections d WHERE d.image_id=i.id AND d.crop_path IS NOT NULL AND d.crop_path!='' ORDER BY COALESCE(d.sharpness,0) DESC,d.id LIMIT 1) AS crop_path FROM images i WHERE i.job_id=? AND i.burst_key IS NOT NULL AND i.status='done' AND COALESCE(i.rejected,0)=0 AND COALESCE(i.copy_of,0)=0 AND i.moments IS NULL ORDER BY i.burst_key,i.id",
        [job],
    )?;
    let mut bursts: BTreeMap<i64, Vec<Value>> = BTreeMap::new();
    for frame in frames {
        bursts
            .entry(frame["burst_key"].as_i64().unwrap_or_default())
            .or_default()
            .push(frame);
    }
    // A small, even sample per burst: enough to see the action, not the whole
    // burst. A burst an earlier pass already judged is left alone.
    let sampled: Vec<Vec<Value>> = bursts
        .into_values()
        .map(|members| {
            let step = (members.len() / TYPE_FRAMES_PER_BURST).max(1);
            members
                .into_iter()
                .step_by(step)
                .take(TYPE_FRAMES_PER_BURST)
                .collect()
        })
        .collect();
    let total: usize = sampled.iter().map(Vec::len).sum();
    if total == 0 {
        return Ok(None);
    }
    if let Some(p) = progress {
        p.plan(IDENTIFY_DECISION, total as u64);
    }
    let feed = live_feed(d);
    if let Some(feed) = &feed {
        feed.begin_phase(MOMENTS_PHASE, total);
    }
    let context = lanes.preset.prompt_context();
    let mut done = 0usize;
    for members in &sampled {
        for frame in members {
            if stop.load(Ordering::Relaxed) || control.is_skipped(IDENTIFY_DECISION) {
                break;
            }
            // Between asks: the pause takes effect here, never inside a call.
            if !control.park_task(IDENTIFY_DECISION, stop, progress, task)
                || control.is_skipped(IDENTIFY_DECISION)
            {
                break;
            }
            let id = frame["id"].as_i64().unwrap_or_default();
            let Ok(image) = picture(
                frame["preview_path"].as_str(),
                frame["path"].as_str().unwrap_or_default(),
            ) else {
                done += 1;
                continue;
            };
            let url = data_url(&image, FRAME_EDGE);
            let crop = frame["crop_path"]
                .as_str()
                .filter(|path| Path::new(path).is_file())
                .and_then(|path| std::fs::read(path).ok())
                .and_then(|bytes| Rgb::decode_jpeg(&bytes, 1).ok())
                .map(|image| data_url(&image, PEAK_EDGE));
            let label = frame_label(frame["path"].as_str().unwrap_or_default());
            let thumb = thumb_of(frame["preview_path"].as_str(), frame["thumb_path"].as_str());
            let verdict = shown_as(&label, thumb, || {
                judge.judge_pair(context, vocab, &url, crop.as_deref())
            });
            if let Ok(verdict) = verdict {
                let _ = store(d, id, &verdict);
            }
            done += 1;
            if let Some(p) = progress {
                p.advance(IDENTIFY_DECISION, 1);
            }
            if let Some(feed) = &feed {
                feed.phase_progress(done);
            }
        }
        if stop.load(Ordering::Relaxed) || control.is_skipped(IDENTIFY_DECISION) {
            break;
        }
    }
    if let Some(feed) = &feed {
        feed.end_phase();
    }
    task.detail(format!("Asked the {total} sampled frames about this shoot"));
    Ok(None)
}

/// The `auto` album's type question: a spread sample of the whole album, one
/// choice answer, stored as a suggestion on the job.
fn auto_type_round(
    d: &Desktop,
    job: i64,
    stop: &AtomicBool,
    task: &conrod_core::tasks::Task,
    cfg: &Settings,
    judge: &dyn MomentJudge,
    progress: Option<&crate::progress::Progress>,
) -> Result<Option<(String, f64)>> {
    let frames = rows(
        &lock(&d.reader),
        "SELECT i.id,i.path,i.preview_path,i.thumb_path FROM images i WHERE i.job_id=? AND i.status='done' AND COALESCE(i.rejected,0)=0 AND COALESCE(i.copy_of,0)=0 ORDER BY i.id",
        [job],
    )?;
    let control = d.stage_control(job);
    if frames.is_empty() || stop.load(Ordering::Relaxed) || control.is_skipped(IDENTIFY_DECISION) {
        return Ok(None);
    }
    let step = frames.len().div_ceil(TYPE_SAMPLE);
    let chosen: Vec<&Value> = frames
        .iter()
        .step_by(step.max(1))
        .take(TYPE_SAMPLE)
        .collect();
    let mut images = Vec::new();
    if !control.park_task(IDENTIFY_DECISION, stop, progress, task)
        || control.is_skipped(IDENTIFY_DECISION)
    {
        return Ok(None);
    }
    for frame in &chosen {
        if stop.load(Ordering::Relaxed) || control.is_skipped(IDENTIFY_DECISION) {
            return Ok(None);
        }
        let Ok(image) = picture(
            frame["preview_path"].as_str(),
            frame["path"].as_str().unwrap_or_default(),
        ) else {
            continue;
        };
        images.push(data_url(&image, PEAK_EDGE));
    }
    if images.is_empty() || stop.load(Ordering::Relaxed) || control.is_skipped(IDENTIFY_DECISION) {
        return Ok(None);
    }
    if let Some(p) = progress {
        p.plan(IDENTIFY_DECISION, 1);
        p.advance(IDENTIFY_DECISION, 1);
    }
    let label = format!("Album type ({} frames)", images.len());
    let thumb = chosen
        .first()
        .and_then(|frame| thumb_of(frame["preview_path"].as_str(), frame["thumb_path"].as_str()));
    let answer = shown_as(&label, thumb, || {
        judge.choose(
            ShootPreset::Auto.prompt_context(),
            &TYPE_CHOICES,
            images.as_slice(),
        )
    });
    let Ok(Some((choice, confidence))) = answer else {
        task.detail("No type suggestion: the decision model gave no opinion");
        return Ok(None);
    };
    if !TYPE_CHOICES.iter().any(|(name, _)| *name == choice) {
        task.detail(format!("No type suggestion: the model named {choice}"));
        return Ok(None);
    }
    lock(&d.db)
        .execute(
            "UPDATE jobs SET shoot_type_suggested=?, shoot_type_confidence=? WHERE id=?",
            params![choice, confidence, job],
        )
        .map_err(err)?;
    task.detail(format!(
        "Suggested shoot type: {choice} ({:.0}% sure)",
        confidence * 100.0
    ));
    let _ = cfg;
    Ok(Some((choice, confidence)))
}

/// `accept_type_suggestion {jobId, type}`: the photographer accepting the
/// suggestion the Decision stage showed. The accepted type is stored on the
/// job and governs only the vision reads still to come; already-read subjects,
/// judged bursts and every hand-set reading stay exactly as they are. An album
/// whose vision stage already finished has nothing left to affect, so it is a
/// plain no-op.
pub(crate) fn accept_type_suggestion(d: &Desktop, job: i64, name: &str) -> Result<Value> {
    let wanted = name.trim();
    let preset = ShootPreset::ALL
        .iter()
        .copied()
        .find(|p| *p != ShootPreset::Auto && p.name().eq_ignore_ascii_case(wanted))
        .ok_or_else(|| format!("Unknown shoot type `{wanted}`"))?;
    crate::library::require_job(d, job)?;
    let status: String = lock(&d.db)
        .query_row("SELECT status FROM jobs WHERE id=?", [job], |r| r.get(0))
        .map_err(err)?;
    if matches!(status.as_str(), "done" | "done_errors") {
        return Ok(json!({
            "accepted": false,
            "type": preset.name(),
            "note": "The vision stage has already run; the type only changes reads still to come.",
        }));
    }
    lock(&d.db)
        .execute(
            "UPDATE jobs SET shoot_type_accepted=? WHERE id=?",
            params![preset.name(), job],
        )
        .map_err(err)?;
    Ok(json!({"accepted": true, "type": preset.name()}))
}

/// The peak a frame's stored verdict already carries, if any: the burst pass
/// writes it on the one frame it picks, and a re-read must keep it.
fn stored_peak(frame: &Value) -> Option<f64> {
    frame["moments"]
        .as_str()
        .map(MomentVerdict::from_json)
        .and_then(|verdict| verdict.peak)
}

/// Whether a burst has already been judged for its peak: one of its frames
/// carries the peak an earlier pass stored. Asking again would only spend GPU
/// time on the answer the album already has.
fn judged_burst(members: &[&Value]) -> bool {
    members.iter().any(|frame| stored_peak(frame).is_some())
}

/// For each burst, ask a judge that can say which frame shows the peak, and
/// mark the one it picks clearly. Everything else in the burst loses any
/// earlier mark. A burst an earlier pass already judged is left alone, so a
/// re-run only asks about the bursts nobody has compared yet.
#[allow(clippy::too_many_arguments)]
fn peaks(
    d: &Desktop,
    job: i64,
    stop: &AtomicBool,
    task: &conrod_core::tasks::Task,
    judge: &dyn MomentJudge,
    context: &str,
    feed: Option<&IdentifyLive>,
    progress: Option<&crate::progress::Progress>,
) -> Result<()> {
    let control = d.stage_control(job);
    if stop.load(Ordering::Relaxed) || control.is_skipped(IDENTIFY_DECISION) {
        return Ok(());
    }
    let frames = rows(
        &lock(&d.reader),
        "SELECT i.id,i.path,i.preview_path,i.thumb_path,i.burst_key,i.moments FROM images i WHERE i.job_id=? AND i.burst_key IS NOT NULL AND i.status='done' AND COALESCE(i.rejected,0)=0 AND COALESCE(i.copy_of,0)=0 ORDER BY i.burst_key,i.id",
        [job],
    )?;
    let mut bursts: BTreeMap<i64, Vec<&Value>> = BTreeMap::new();
    for frame in &frames {
        bursts
            .entry(frame["burst_key"].as_i64().unwrap_or_default())
            .or_default()
            .push(frame);
    }
    // The bursts nobody has compared yet: a re-run counts and asks only those,
    // so its header, task and DECIDER node show the real remaining work instead
    // of the whole album again.
    let bursts: Vec<&Vec<&Value>> = bursts
        .values()
        .filter(|b| b.len() >= 2)
        .filter(|b| !judged_burst(b.as_slice()))
        .collect();
    let total = bursts.len();
    // The header and the DECIDER node follow this pass by its own count.
    if let Some(feed) = feed {
        feed.begin_phase(PEAKS_PHASE, total);
    }
    task.progress(0, total as u64);
    for (i, members) in bursts.iter().enumerate() {
        if stop.load(Ordering::Relaxed) || control.is_skipped(IDENTIFY_DECISION) {
            break;
        }
        // Between asks: a paused Decision stage parks before comparing the next
        // burst; a judged burst was never planned.
        if !control.park_task(IDENTIFY_DECISION, stop, progress, task)
            || control.is_skipped(IDENTIFY_DECISION)
        {
            break;
        }
        task.progress(i as u64, total as u64);
        task.detail(format!("Comparing bursts {}/{}", i + 1, total));
        if let Some(feed) = feed {
            feed.phase_progress(i);
        }
        if let Some(p) = progress {
            p.advance(IDENTIFY_DECISION, 1);
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
        let label = format!("Burst of {} frames", chosen.len());
        let thumb = thumb_of(
            chosen[0]["preview_path"].as_str(),
            chosen[0]["thumb_path"].as_str(),
        );
        let Ok(Some(ps)) = shown_as(&label, thumb, || judge.peak(context, &images)) else {
            continue;
        };
        let winner = peak_frame(&ps, PEAK_FLOOR);
        for (at, frame) in chosen.iter().enumerate() {
            let id = frame["id"].as_i64().unwrap_or_default();
            let mut verdict = frame["moments"]
                .as_str()
                .map(MomentVerdict::from_json)
                .unwrap_or_default();
            let was = verdict.peak;
            verdict.peak = (winner == Some(at)).then(|| ps[at]);
            if verdict.peak != was {
                if verdict.backend.is_empty() {
                    verdict.backend = judge.name().to_string();
                }
                store(d, id, &verdict)?;
            }
        }
    }
    task.progress(total as u64, total as u64);
    task.detail(format!("Compared {total} bursts for their peak"));
    if let Some(feed) = feed {
        feed.phase_progress(total);
        feed.end_phase();
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use conrod_core::decision::{DecisionTransport, Request};
    use conrod_core::moments::vocabulary;
    use conrod_core::profile::ShootPreset;

    fn leaked_service() -> &'static crate::decider::Service {
        Box::leak(Box::default())
    }

    /// A real 16x16 JPEG in the album's folder, so the frame decodes.
    fn preview(lib: &crate::testkit::Lib, name: &str) -> String {
        use image::ImageEncoder;
        let path = lib.root.join(name);
        let pixels: Vec<u8> = (0..16 * 16).flat_map(|i| [i as u8, 200, 30]).collect();
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 80)
            .write_image(&pixels, 16, 16, image::ExtendedColorType::Rgb8)
            .unwrap();
        std::fs::write(&path, bytes).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn the_seat_is_empty_unless_the_decision_model_is_on_and_ready() {
        // A fresh service is switched off: the judge falls back (OpenRouter, or nothing).
        let seat = seat(leaked_service());
        assert!(!seat.is_available());
        let request = Request::new("x");
        assert!(seat.decide(&request).unwrap_err().contains("not installed"));
    }

    #[test]
    fn peaks_skips_bursts_that_already_have_a_peak_and_counts_its_progress() {
        use crate::testkit::Lib;

        /// A judge that always picks the first frame, and counts the bursts it was asked about.
        struct Fake(AtomicUsize);
        impl MomentJudge for Fake {
            fn name(&self) -> &'static str {
                "fake"
            }
            fn judge(
                &self,
                _context: &str,
                _vocab: &[Moment],
                _image: &str,
            ) -> std::result::Result<MomentVerdict, String> {
                Ok(MomentVerdict::default())
            }
            fn peak(
                &self,
                _context: &str,
                images: &[String],
            ) -> std::result::Result<Option<Vec<f64>>, String> {
                self.0.fetch_add(1, Ordering::Relaxed);
                Ok(Some(
                    (0..images.len())
                        .map(|at| if at == 0 { 0.9 } else { 0.1 })
                        .collect(),
                ))
            }
        }

        let lib = Lib::new("peaks-skip");
        let shot = preview(&lib, "burst.jpg");
        // Two bursts of two frames; the first has already been compared.
        let judged = [lib.frame("a1.jpg", Some(3)), lib.frame("a2.jpg", Some(3))];
        let fresh = [lib.frame("b1.jpg", Some(4)), lib.frame("b2.jpg", Some(4))];
        for id in judged.iter().chain(fresh.iter()) {
            lib.sql(
                "UPDATE images SET preview_path=? WHERE id=?",
                rusqlite::params![shot, id],
            );
        }
        lib.sql(
            "UPDATE images SET moments='{\"backend\":\"fake\",\"peak\":0.9}' WHERE id=?",
            [judged[0]],
        );

        let feed = Arc::new(IdentifyLive::new(lib.job, 0));
        *lock(&lib.d().identify) = Some(feed.clone());
        let task = lib.d().hub.start("Judging moments", 0);
        let judge = Fake(AtomicUsize::new(0));
        let peaks_now = || {
            peaks(
                lib.d(),
                lib.job,
                &AtomicBool::new(false),
                &task,
                &judge,
                "Weddings.",
                Some(&feed),
                None,
            )
            .unwrap()
        };

        peaks_now();
        assert_eq!(
            judge.0.load(Ordering::Relaxed),
            1,
            "only the burst with no peak is asked"
        );
        let stored = |id: i64| -> MomentVerdict {
            let text: Option<String> = lib.one("SELECT moments FROM images WHERE id=?", [id]);
            MomentVerdict::from_json(text.as_deref().unwrap_or_default())
        };
        assert_eq!(stored(fresh[0]).peak, Some(0.9), "the first frame wins");
        assert_eq!(stored(fresh[1]).peak, None);
        assert_eq!(
            stored(judged[0]).peak,
            Some(0.9),
            "an already judged burst keeps its peak"
        );
        // The task and the DECIDER phase count only the bursts this pass will
        // compare, so a re-run never claims the album's whole burst count again.
        let task = lib.task("Judging moments");
        assert_eq!(
            (task["done"].as_u64(), task["total"].as_u64()),
            (Some(1), Some(1)),
            "the status area shows bursts compared, not frames judged"
        );
        assert_eq!(task["detail"], "Compared 1 bursts for their peak");
        let phase = feed.snapshot()["phase"].clone();
        assert_eq!(phase["name"], "peaks");
        assert_eq!(
            phase["state"], "done",
            "the DECIDER may check off only at its end"
        );
        assert_eq!(phase["total"], 1);

        // A second pass asks nobody and counts nobody: every burst now carries a peak.
        peaks_now();
        assert_eq!(
            judge.0.load(Ordering::Relaxed),
            1,
            "a re-run asks no burst again"
        );
        let task = lib.task("Judging moments");
        assert_eq!(
            (task["done"].as_u64(), task["total"].as_u64()),
            (Some(0), Some(0)),
            "a re-run's real remaining work is none"
        );
        assert_eq!(task["detail"], "Compared 0 bursts for their peak");
        assert_eq!(feed.snapshot()["phase"]["total"], 0);
    }

    #[test]
    fn a_re_run_decides_only_the_frames_nobody_has_judged() {
        use crate::testkit::Lib;
        use conrod_core::profile::{Feature, Lanes, Overrides};

        /// Counts the frames it is shown, and fails its first `fail` calls: an
        /// interrupted run leaves exactly those frames without a verdict.
        struct Fake {
            asked: AtomicUsize,
            fail: usize,
        }
        impl MomentJudge for Fake {
            fn name(&self) -> &'static str {
                "fake"
            }
            fn judge(
                &self,
                _context: &str,
                _vocab: &[Moment],
                _image: &str,
            ) -> std::result::Result<MomentVerdict, String> {
                let at = self.asked.fetch_add(1, Ordering::Relaxed);
                if at < self.fail {
                    Err("interrupted".into())
                } else {
                    Ok(MomentVerdict::default())
                }
            }
        }

        let lib = Lib::new("moments-resume");
        let shot = preview(&lib, "frame.jpg");
        // Three keepers; the judge answers two and is cut off at the third.
        for name in ["a1.jpg", "a2.jpg", "a3.jpg"] {
            let id = lib.frame(name, None);
            lib.sql(
                "UPDATE images SET preview_path=?,burst_pick=1 WHERE id=?",
                rusqlite::params![shot, id],
            );
        }
        let mut overrides = Overrides::default();
        overrides.set(Feature::MomentsDetect, Some(true));
        overrides.set(Feature::MomentsRank, Some(false));
        let lanes = Lanes::resolve(ShootPreset::Motorsport, false, &overrides);
        assert!(lanes.moments_detect, "the run has moments to judge");
        let cfg = Settings::default();
        let feed = Arc::new(IdentifyLive::new(lib.job, 0));
        *lock(&lib.d().identify) = Some(feed.clone());
        let task = lib.d().hub.start("Judging moments", 0);
        let stop = AtomicBool::new(false);
        let judge = Fake {
            asked: AtomicUsize::new(0),
            fail: 1,
        };
        let judged = run_with(
            lib.d(),
            lib.job,
            &stop,
            &task,
            &cfg,
            &lanes,
            "keepers",
            false,
            1,
            &judge,
            None,
        )
        .unwrap();
        assert_eq!(judged, 2, "two frames were decided before the cut-off");
        assert_eq!(judge.asked.load(Ordering::Relaxed), 3);
        let decided: i64 = lib.one(
            "SELECT count(*) FROM images WHERE job_id=? AND moments IS NOT NULL",
            [lib.job],
        );
        assert_eq!(decided, 2, "the answers that came back are kept");

        // The re-run asks only the frame the interrupted pass left undecided,
        // and the DECIDER phase counts that frame, not the whole album.
        let judged = run_with(
            lib.d(),
            lib.job,
            &stop,
            &task,
            &cfg,
            &lanes,
            "keepers",
            false,
            1,
            &judge,
            None,
        )
        .unwrap();
        assert_eq!(judged, 1);
        assert_eq!(
            judge.asked.load(Ordering::Relaxed),
            4,
            "only the undecided frame is asked again"
        );
        let phase = feed.snapshot()["phase"].clone();
        assert_eq!(phase["name"], "moments");
        assert_eq!(
            phase["total"], 1,
            "the DECIDER shows the remaining frames, not the album again"
        );
        assert_eq!(phase["state"], "done");
        let undecided: i64 = lib.one(
            "SELECT count(*) FROM images WHERE job_id=? AND moments IS NULL",
            [lib.job],
        );
        assert_eq!(undecided, 0);
    }

    #[test]
    fn a_frame_names_the_calls_made_for_it_and_only_for_it() {
        assert_eq!(frame_label("D:\\shoot\\IMG_0042.CR3"), "IMG_0042.CR3");
        assert!(thumb_of(None, None).is_none());
        assert!(thumb_of(Some("Z:\\nowhere\\p.jpg"), None).is_none());
        let seen = shown_as("IMG_1.CR3", None, || SHOWN.with(|s| s.borrow().clone()));
        assert_eq!(seen.unwrap().0, "IMG_1.CR3");
        assert!(SHOWN.with(|s| s.borrow().is_none()), "cleared afterwards");
    }

    #[test]
    fn an_auto_album_stores_the_type_suggestion_without_applying_it() {
        use crate::testkit::Lib;
        use conrod_core::profile::{Lanes, Overrides, ShootPreset};

        struct Typer;
        impl MomentJudge for Typer {
            fn name(&self) -> &'static str {
                "fake"
            }
            fn judge(
                &self,
                _context: &str,
                _vocab: &[Moment],
                _image: &str,
            ) -> std::result::Result<MomentVerdict, String> {
                Ok(MomentVerdict::default())
            }
            fn choose(
                &self,
                _context: &str,
                options: &[(&str, &str)],
                _images: &[String],
            ) -> std::result::Result<Option<(String, f64)>, String> {
                assert!(options.iter().any(|(name, _)| *name == "sport-rodeo"));
                Ok(Some(("sport-rodeo".into(), 0.83)))
            }
        }

        let lib = Lib::new("auto-type");
        let shot = preview(&lib, "spread.jpg");
        for name in ["a.jpg", "b.jpg", "c.jpg", "d.jpg"] {
            let id = lib.frame(name, Some(1));
            lib.sql(
                "UPDATE images SET preview_path=? WHERE id=?",
                rusqlite::params![shot, id],
            );
        }
        lib.sql(
            "UPDATE jobs SET settings_json='{\"scan_profile\":\"auto\"}' WHERE id=?",
            [lib.job],
        );
        let lanes = Lanes::resolve(ShootPreset::Auto, false, &Overrides::default());
        let task = lib.d().hub.start("Judging moments", 0);
        let stop = AtomicBool::new(false);
        let judge = Typer;
        let got = run_type_round(
            lib.d(),
            lib.job,
            &stop,
            &task,
            &Settings::default(),
            &lanes,
            &judge,
            None,
        )
        .unwrap();
        assert_eq!(got, Some(("sport-rodeo".into(), 0.83)));
        let suggested: String = lib.one(
            "SELECT shoot_type_suggested FROM jobs WHERE id=?",
            [lib.job],
        );
        assert_eq!(suggested, "sport-rodeo");
        let confidence: f64 = lib.one(
            "SELECT shoot_type_confidence FROM jobs WHERE id=?",
            [lib.job],
        );
        assert!((confidence - 0.83).abs() < 1e-9);
        // A suggestion is not an application: the album's own type is untouched.
        let profile: String = lib.one(
            "SELECT json_extract(settings_json,'$.scan_profile') FROM jobs WHERE id=?",
            [lib.job],
        );
        assert_eq!(profile, "auto", "the suggestion is shown, never applied");
        let accepted: Option<String> =
            lib.one("SELECT shoot_type_accepted FROM jobs WHERE id=?", [lib.job]);
        assert_eq!(
            accepted, None,
            "nothing was accepted on the photographer's behalf"
        );
    }

    #[test]
    fn accepting_the_type_marks_it_and_resolves_the_vision_prompts() {
        use crate::testkit::Lib;
        let lib = Lib::new("accept-type-applies");
        lib.sql(
            "UPDATE jobs SET settings_json='{\"scan_profile\":\"auto\"}', shoot_type_suggested='sport-rodeo', shoot_type_confidence=0.83 WHERE id=?",
            [lib.job],
        );
        let out = lib
            .run(
                "accept_type_suggestion",
                json!({"jobId": lib.job, "type": "sport-rodeo"}),
            )
            .unwrap();
        assert_eq!(out["accepted"], true);
        assert_eq!(out["type"], "sport-rodeo");
        let accepted: String =
            lib.one("SELECT shoot_type_accepted FROM jobs WHERE id=?", [lib.job]);
        assert_eq!(accepted, "sport-rodeo");
        let settings = crate::operations::settings(lib.d(), lib.job).unwrap();
        let (vision, context) =
            crate::operations::resolved_vision_settings(lib.d(), lib.job, &settings);
        assert_eq!(vision.scan_profile, "sport-rodeo");
        assert_eq!(context, Some(ShootPreset::SportRodeo.prompt_context()));
    }

    #[test]
    fn accepting_the_type_leaves_read_subjects_and_judged_bursts_alone() {
        use crate::testkit::Lib;
        let lib = Lib::new("accept-type-unrun");
        let read = lib.detection(lib.frame("read.jpg", Some(3)), "vehicle", 0.9);
        lib.sql(
            "UPDATE detections SET attributes='{\"make\":\"Honda\"}',plate='ABC123',read_at=1,reviewed=1 WHERE id=?",
            [read],
        );
        let judged = lib.frame("judged.jpg", Some(4));
        lib.sql(
            "UPDATE images SET moments='{\"peak\":0.9}' WHERE id=?",
            [judged],
        );
        lib.sql(
            "UPDATE jobs SET settings_json='{\"scan_profile\":\"auto\"}' WHERE id=?",
            [lib.job],
        );
        lib.run(
            "accept_type_suggestion",
            json!({"jobId": lib.job, "type": "wedding"}),
        )
        .unwrap();
        let attributes: String = lib.one("SELECT attributes FROM detections WHERE id=?", [read]);
        assert_eq!(attributes, "{\"make\":\"Honda\"}");
        let plate: String = lib.one("SELECT plate FROM detections WHERE id=?", [read]);
        assert_eq!(plate, "ABC123");
        let reviewed: i64 = lib.one("SELECT reviewed FROM detections WHERE id=?", [read]);
        assert_eq!(reviewed, 1, "a reviewed subject is never read again");
        let moments: String = lib.one("SELECT moments FROM images WHERE id=?", [judged]);
        assert_eq!(
            moments, "{\"peak\":0.9}",
            "a judged burst is not re-decided"
        );
    }

    #[test]
    fn an_unknown_shoot_type_is_refused() {
        use crate::testkit::Lib;
        let lib = Lib::new("accept-type-invalid");
        lib.sql(
            "UPDATE jobs SET settings_json='{\"scan_profile\":\"auto\"}' WHERE id=?",
            [lib.job],
        );
        let err = lib
            .run(
                "accept_type_suggestion",
                json!({"jobId": lib.job, "type": "not-a-shoot"}),
            )
            .unwrap_err();
        assert!(err.contains("Unknown shoot type"), "{err}");
        let err = lib
            .run(
                "accept_type_suggestion",
                json!({"jobId": lib.job, "type": "auto"}),
            )
            .unwrap_err();
        assert!(err.contains("Unknown shoot type"), "{err}");
        let accepted: Option<String> =
            lib.one("SELECT shoot_type_accepted FROM jobs WHERE id=?", [lib.job]);
        assert_eq!(accepted, None, "nothing is accepted on a refusal");
    }

    #[test]
    fn a_finished_album_has_no_vision_left_to_affect() {
        use crate::testkit::Lib;
        let lib = Lib::new("accept-type-finished");
        lib.sql(
            "UPDATE jobs SET status='done', settings_json='{\"scan_profile\":\"auto\"}', shoot_type_suggested='wedding' WHERE id=?",
            [lib.job],
        );
        let out = lib
            .run(
                "accept_type_suggestion",
                json!({"jobId": lib.job, "type": "wedding"}),
            )
            .unwrap();
        assert_eq!(out["accepted"], false);
        assert!(
            out["note"].as_str().unwrap().contains("already run"),
            "{out}"
        );
        let accepted: Option<String> =
            lib.one("SELECT shoot_type_accepted FROM jobs WHERE id=?", [lib.job]);
        assert_eq!(accepted, None, "a finished album is a no-op");
    }

    #[test]
    fn a_hand_set_type_wins_until_the_photographer_accepts() {
        use crate::testkit::Lib;
        let lib = Lib::new("accept-type-hand-set");
        lib.sql(
            "UPDATE jobs SET settings_json='{\"scan_profile\":\"portrait\"}', shoot_type_suggested='motorsport' WHERE id=?",
            [lib.job],
        );
        let settings = crate::operations::settings(lib.d(), lib.job).unwrap();
        let (before, context) =
            crate::operations::resolved_vision_settings(lib.d(), lib.job, &settings);
        assert_eq!(before.scan_profile, "portrait");
        assert_eq!(
            context, None,
            "the shown suggestion is never applied silently"
        );

        lib.run(
            "accept_type_suggestion",
            json!({"jobId": lib.job, "type": "motorsport"}),
        )
        .unwrap();
        let settings = crate::operations::settings(lib.d(), lib.job).unwrap();
        let (after, context) =
            crate::operations::resolved_vision_settings(lib.d(), lib.job, &settings);
        assert_eq!(
            after.scan_profile, "motorsport",
            "the explicit accept replaces the hand-set type for vision"
        );
        assert_eq!(context, Some(ShootPreset::Motorsport.prompt_context()));
    }

    #[test]
    fn a_typed_album_asks_a_sample_of_each_burst_and_skips_judged_frames() {
        use crate::testkit::Lib;
        use conrod_core::profile::{Lanes, Overrides, ShootPreset};

        struct Counter(AtomicUsize);
        impl MomentJudge for Counter {
            fn name(&self) -> &'static str {
                "fake"
            }
            fn judge(
                &self,
                _context: &str,
                _vocab: &[Moment],
                _image: &str,
            ) -> std::result::Result<MomentVerdict, String> {
                self.0.fetch_add(1, Ordering::Relaxed);
                Ok(MomentVerdict::default())
            }
        }

        let lib = Lib::new("type-round-sample");
        let shot = preview(&lib, "frame.jpg");
        // Four frames in one burst; the first already carries a verdict.
        let ids: Vec<i64> = ["a1.jpg", "a2.jpg", "a3.jpg", "a4.jpg"]
            .iter()
            .map(|name| lib.frame(name, Some(7)))
            .collect();
        for id in &ids {
            lib.sql(
                "UPDATE images SET preview_path=? WHERE id=?",
                rusqlite::params![shot, id],
            );
        }
        lib.sql(
            "UPDATE images SET moments='{\"backend\":\"fake\"}' WHERE id=?",
            [ids[0]],
        );
        let lanes = Lanes::resolve(ShootPreset::Motorsport, false, &Overrides::default());
        let task = lib.d().hub.start("Judging moments", 0);
        let stop = AtomicBool::new(false);
        let judge = Counter(AtomicUsize::new(0));
        run_type_round(
            lib.d(),
            lib.job,
            &stop,
            &task,
            &Settings::default(),
            &lanes,
            &judge,
            None,
        )
        .unwrap();
        assert_eq!(
            judge.0.load(Ordering::Relaxed),
            3,
            "the judged frame is skipped; the other three are asked"
        );
        let judged: i64 = lib.one(
            "SELECT count(*) FROM images WHERE job_id=? AND moments IS NOT NULL",
            [lib.job],
        );
        assert_eq!(judged, 4, "every sampled frame now carries its verdict");
    }

    #[test]
    fn a_rejudge_asks_frames_that_already_carry_a_verdict() {
        use crate::testkit::Lib;
        let lib = Lib::new("moments-rejudge");
        let judged = lib.frame("a.jpg", Some(1));
        let fresh = lib.frame("b.jpg", Some(1));
        lib.sql(
            "UPDATE images SET burst_pick=1,moments='{\"backend\":\"x\",\"readings\":[]}' WHERE id=?",
            [judged],
        );
        lib.sql("UPDATE images SET burst_pick=1 WHERE id=?", [fresh]);
        let ids = |rows: Vec<Value>| -> Vec<i64> {
            rows.iter().map(|r| r["id"].as_i64().unwrap()).collect()
        };
        assert_eq!(
            ids(pending(lib.d(), lib.job, false, false).unwrap()),
            vec![fresh],
            "a first run leaves the judged frame alone"
        );
        assert_eq!(
            ids(pending(lib.d(), lib.job, false, true).unwrap()),
            vec![judged, fresh],
            "a rejudge asks every keeper frame again"
        );
    }

    #[test]
    fn a_moment_request_reaches_the_service_with_its_questions_in_order() {
        // A turned-off service refuses without touching the GPU, but only after the
        // request is built exactly as the judge builds it: tags in vocabulary order.
        let vocab = vocabulary(ShootPreset::Wedding);
        let request = conrod_core::decision::moment_request("Weddings.", vocab, vec![]);
        let first = request.questions.keys().next().unwrap();
        assert_eq!(first, &conrod_core::decision::moment_id(vocab[0].key));
        let err = leaked_service()
            .decide("IMG_1", None, &request)
            .unwrap_err();
        assert!(err.contains("turned off"), "{err}");
    }

    #[test]
    fn skipping_decision_stops_type_round_moments_and_peaks_without_asking_the_judge() {
        use crate::testkit::Lib;
        use conrod_core::profile::{Lanes, Overrides};

        struct CountingJudge {
            asked: AtomicUsize,
            skip_after_first: Option<Arc<crate::stage_control::StageControl>>,
        }
        impl MomentJudge for CountingJudge {
            fn name(&self) -> &'static str {
                "fake"
            }
            fn judge(
                &self,
                _context: &str,
                _vocab: &[Moment],
                _image: &str,
            ) -> std::result::Result<MomentVerdict, String> {
                self.asked.fetch_add(1, Ordering::Relaxed);
                if let Some(control) = &self.skip_after_first {
                    control.set_skipped(IDENTIFY_DECISION, true);
                }
                Ok(MomentVerdict::default())
            }
            fn choose(
                &self,
                _context: &str,
                _options: &[(&str, &str)],
                _images: &[String],
            ) -> std::result::Result<Option<(String, f64)>, String> {
                self.asked.fetch_add(1, Ordering::Relaxed);
                Ok(Some(("motorsport".into(), 0.9)))
            }
            fn peak(
                &self,
                _context: &str,
                images: &[String],
            ) -> std::result::Result<Option<Vec<f64>>, String> {
                self.asked.fetch_add(1, Ordering::Relaxed);
                Ok(Some(vec![0.9; images.len()]))
            }
        }

        let lib = Lib::new("decision-skip");
        let shot = preview(&lib, "shot.jpg");
        for (name, burst) in [
            ("a1.jpg", 1),
            ("a2.jpg", 1),
            ("a3.jpg", 1),
            ("b1.jpg", 2),
            ("b2.jpg", 2),
        ] {
            let id = lib.frame(name, Some(burst));
            lib.sql(
                "UPDATE images SET preview_path=?, burst_pick=1 WHERE id=?",
                rusqlite::params![shot, id],
            );
        }
        let control = lib.d().stage_control(lib.job);
        control.set_skipped(IDENTIFY_DECISION, true);

        let cfg = Settings::default();
        let motorsport = Lanes::resolve(ShootPreset::Motorsport, false, &Overrides::default());
        let auto = Lanes::resolve(ShootPreset::Auto, false, &Overrides::default());
        let task = lib.d().hub.start("Judging moments", 0);
        let stop = AtomicBool::new(false);
        let judge = CountingJudge {
            asked: AtomicUsize::new(0),
            skip_after_first: None,
        };

        assert!(run_type_round(
            lib.d(),
            lib.job,
            &stop,
            &task,
            &cfg,
            &motorsport,
            &judge,
            None,
        )
        .unwrap()
        .is_none());
        assert!(
            run_type_round(lib.d(), lib.job, &stop, &task, &cfg, &auto, &judge, None,)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            run_with(
                lib.d(),
                lib.job,
                &stop,
                &task,
                &cfg,
                &motorsport,
                "keepers",
                false,
                1,
                &judge,
                None,
            )
            .unwrap(),
            0
        );
        peaks(
            lib.d(),
            lib.job,
            &stop,
            &task,
            &judge,
            "Motorsport.",
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            judge.asked.load(Ordering::Relaxed),
            0,
            "a skipped Decision stage never asks the judge"
        );

        // Live skip between asks: after the first frame in run_type_round sets
        // skip, the remaining sampled frames are not asked.
        control.set_skipped(IDENTIFY_DECISION, false);
        let live_judge = CountingJudge {
            asked: AtomicUsize::new(0),
            skip_after_first: Some(control.clone()),
        };
        run_type_round(
            lib.d(),
            lib.job,
            &stop,
            &task,
            &cfg,
            &motorsport,
            &live_judge,
            None,
        )
        .unwrap();
        assert_eq!(
            live_judge.asked.load(Ordering::Relaxed),
            1,
            "skipping mid-round stops at the next ask boundary"
        );
    }

    #[test]
    fn thumb_of_falls_back_to_thumbnail_when_preview_is_missing() {
        let lib = crate::testkit::Lib::new("moments_thumb_fallback");
        let thumb_path = preview(&lib, "thumb-1.jpg");
        let preview_path = preview(&lib, "preview-1.jpg");
        assert_eq!(
            thumb_of(None, Some(&thumb_path)),
            Some(PathBuf::from(&thumb_path)),
            "freshly scanned frames only have thumb_path on disk"
        );
        assert_eq!(
            thumb_of(Some("does-not-exist.jpg"), Some(&thumb_path)),
            Some(PathBuf::from(&thumb_path))
        );
        assert_eq!(
            thumb_of(Some(&preview_path), Some(&thumb_path)),
            Some(PathBuf::from(&preview_path)),
            "preview_path wins when it exists on disk"
        );
        assert_eq!(thumb_of(None, None), None);
    }
}
