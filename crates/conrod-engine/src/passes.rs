//! Passes over a whole album that work from what is already stored and read no
//! photograph: which frame of each pass is the keeper (`pick_of_pass`), and which
//! vehicles are one car (`consolidate`, behind the Group action). Ports
//! `pipeline.pick_of_pass`, `pipeline.embed_missing` and `grouping.consolidate`.
use crate::desktop::{rows, Desktop, Result};
use crate::edits::attributes_of;
use crate::library::require_job;
use crate::lock;
use crate::operations::{launch, ml_eligible, settings, ML_ELIGIBLE};
use conrod_core::{
    framing, grouping, models,
    profile::{ScanProfile, ShootPreset, Subject},
    settings::Settings,
    tasks::Task,
};
use conrod_vision::faces::{FaceEmbedder, IDENTITY_DIM, SAME_PERSON};
use conrod_vision::{detect::Device, imageops::Rgb, similarity};
use rayon::prelude::*;
use rusqlite::{params, Connection};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// The `region_type` a profile's subject kind is stored under.
pub(crate) fn region_name(kind: Subject) -> &'static str {
    match kind {
        Subject::Eye => "eye",
        Subject::Face => "face",
        Subject::Person => "person",
        Subject::Vehicle => "vehicle",
        Subject::WholeFrame => "",
    }
}

/// Ratings this close are the same rating: differences smaller than the
/// measurement noise of the sharpness score say nothing about which frame is
/// better, so the subject's prominence decides between them.
const RATING_BAND: f64 = 0.03;

/// The same band for a face or an eye. Their score is an edge-blur estimate,
/// not the tile ratio the band was measured on: a hundredth of it is a real
/// difference between two portraits, and the keeper must follow it.
const FACE_BAND: f64 = 0.005;

/// Bucket widths of the keeper order's soft keys. The order is one lexicographic
/// tuple (see [`Candidate::key`]) and each soft key is quantised, so that a
/// difference inside measurement noise ties and the comparison is a *total*
/// order: a pairwise "within 0.10" margin is not transitive, which made a
/// three-frame pass depend on the order its rows came back in.
///
/// Only the keys that used to compare by a pairwise margin are bucketed, and
/// each width is that margin:
///
/// - rodeo `buck_score`: 0.10 (the animal's rear, read coarsely).
/// - framing score: 0.10 (the margin the earlier A/B used).
/// - lowest eye-open: 0.10 (blendshape flicker between frames of one burst).
/// - smile: 0.10 (the margin the portrait case was tuned with).
///
/// The keys that were already compared exactly stay exact (the rating, the
/// subject's area, NIMA quality, the stored sharpness, the frame and the row
/// id): they are measurements, not margins, and quantising them ties frames
/// that genuinely differ. The one A/B that tried it lost a motorsport pick.
const BUCK_BUCKET: f64 = 0.10;
const FRAMING_BUCKET: f64 = 0.10;
const EYES_BUCKET: f64 = 0.10;
const SMILE_BUCKET: f64 = 0.10;

/// An exactly ordered measurement in a key tuple: `total_cmp`, with NaN (or an
/// absent value) sorted below every real one, so a missing measurement never
/// wins a comparison it has no evidence for.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Exact(f64);

impl Eq for Exact {}

impl Ord for Exact {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

impl PartialOrd for Exact {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn exact(value: f64) -> Exact {
    Exact(if value.is_finite() {
        value
    } else {
        f64::NEG_INFINITY
    })
}

/// A value's bucket, floor-divided. Missing, non-finite and negative values
/// land at or below the first bucket, so "no measurement" ranks like the worst
/// real one and never beats it.
fn bucket(value: Option<f64>, width: f64) -> i64 {
    match value {
        Some(v) if v.is_finite() => (v / width).floor() as i64,
        _ => 0,
    }
}

/// One candidate for the keeper of a unit: a subject, or a frame with no
/// subject the profile rates.
struct Candidate {
    /// The frame it sits on, so a tie still leaves the earlier frame in place.
    frame: i64,
    /// A detection id, or the image id when the candidate is the frame itself.
    row: i64,
    on_frame: bool,
    by_hand: i64,
    culled: bool,
    score: f64,
    /// The stored subject sharpness, which ranks an exact rating tie.
    sharpness: f64,
    quality: Option<f64>,
    /// How many frame edges the subject runs off; 0 for a frame with no subject.
    sides: u32,
    /// The subject's box as a share of the frame; 0 for a frame with no subject.
    area: f64,
    /// The rating distance that counts as the same score, per subject kind.
    band: f64,
    /// Exposure is a flag, not a cull: a clipped frame loses only to a clean one.
    bright: bool,
    /// The preset's framing score for this subject, 0..1.
    framing: Option<f64>,
    /// The lowest eye-open among the frame's faces, 0..1.
    eyes: Option<f64>,
    /// The highest smile among the frame's faces, 0..1.
    smile: Option<f64>,
    /// Rodeo: how much the animal is rearing under its rider, 0..1.
    buck: Option<f64>,
}

/// Which of the optional keys this preset ranks on.
#[derive(Debug, Clone, Copy)]
struct Ranking {
    quality: bool,
    framing: bool,
    eyes: bool,
    smile: bool,
    buck: bool,
}

/// The keeper order as one lexicographic tuple: every field to the left
/// dominates every field to the right, and every field is an integer bucket or
/// an exactly ordered value, so the comparison is a total order and a sort of
/// it cannot depend on the order the rows arrived in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    /// A hand star (or the star already in the file) wins outright.
    by_hand: i64,
    /// A frame no rule culled beats one that was culled.
    unculled: i64,
    /// A rated subject that clears the keeper bar beats a frame with no
    /// subject: that frame is rated on its sharpest patch, a different
    /// measure, and is usually a shot the detector missed.
    subject: i64,
    /// Rating in the candidate's band (the wider of the two kinds).
    rating_band: i64,
    /// A clean exposure beats a clipped one; never a cull.
    bright: i64,
    /// Rodeo/equestrian only.
    buck: i64,
    /// Fewer frame edges run the subject off -- negated, so fewer wins. This
    /// sits above size: a cut-off subject never wins on size.
    cut_sides: i64,
    /// Larger subject, as a share of the frame. Size stays ahead of the soft
    /// composition keys: the readiness A/B that put framing above size lost
    /// motorsport picks (46.7 -> 33.3), and the same A/B on this ladder kept
    /// size ahead. Exact rather than bucketed: the area is a real measurement,
    /// not a margin, and quantising it tied subjects that differ.
    area: Exact,
    /// The preset's framing score (cut sides, size band, mode, occlusion,
    /// lead room, tilt), when the preset has a framing policy. Occlusion and
    /// lead room ride here, so the least-occluded composition wins between
    /// subjects of the same size.
    framing: i64,
    /// The lowest eye-open among the frame's faces, on people presets.
    eyes: i64,
    /// The highest smile among the frame's faces, on people presets.
    smile: i64,
    /// The exact rating, finer than its band.
    rating: Exact,
    /// NIMA quality, when the album asks for it.
    quality: Exact,
    /// The stored subject sharpness.
    sharpness: Exact,
    /// The earlier frame, then the earlier row, as negative ids so the earlier
    /// one is the larger key: a full tie always keeps the same candidate,
    /// whatever order the rows came back in.
    frame: i64,
    row: i64,
}

impl Candidate {
    /// The whole keeper order in one tuple; the doc comment on [`Key`] lists
    /// the fields in the order they dominate. Readiness decided the order:
    /// stars, no cull, whether a rated subject is there at all, the rating
    /// band, a clean exposure and the subject's size before composition,
    /// because framing above size cost three cases' pick
    /// rates (motorsport 46.7 -> 33.3, car meet 67 -> 33.3, portrait 42.9 ->
    /// 35.7). Framing, eyes and the smile then refine same-size, same-band
    /// frames; a cut-off subject has already lost at `cut_sides`.
    ///
    /// The preset decides which soft keys apply ([`Ranking`]); where one does
    /// not, its bucket is the constant 0 and it cannot reorder anything.
    fn key(&self, r: &Ranking) -> Key {
        let band_width = self.band;
        Key {
            by_hand: self.by_hand,
            unculled: i64::from(!self.culled),
            subject: i64::from(!self.on_frame && self.score >= conrod_vision::sharpness::KEEPER_MIN_RATING),
            rating_band: bucket(Some(self.score), band_width),
            bright: i64::from(self.bright),
            buck: if r.buck { bucket(self.buck, BUCK_BUCKET) } else { 0 },
            cut_sides: -i64::from(self.sides),
            framing: if r.framing {
                bucket(self.framing, FRAMING_BUCKET)
            } else {
                0
            },
            eyes: if r.eyes { bucket(self.eyes, EYES_BUCKET) } else { 0 },
            smile: if r.smile {
                bucket(self.smile, SMILE_BUCKET)
            } else {
                0
            },
            area: exact(self.area),
            rating: exact(self.score),
            quality: if r.quality {
                exact(self.quality.unwrap_or(f64::NEG_INFINITY))
            } else {
                exact(f64::NEG_INFINITY)
            },
            sharpness: exact(self.sharpness),
            frame: -self.frame,
            row: -self.row,
        }
    }

    /// Is this candidate the better keeper? A strict comparison of the key
    /// tuple: antisymmetric and transitive, and equal keys never beat each
    /// other, so the first frame held stays (the fold in `pick_of_pass`).
    fn beats(&self, other: &Self, r: &Ranking) -> bool {
        // The rating band is the wider of the two kinds' bands, as it always
        // was: one band has to decide, not each candidate's own.
        let band = self.band.max(other.band);
        let mut mine = self.key(r);
        let mut theirs = other.key(r);
        mine.rating_band = bucket(Some(self.score), band);
        theirs.rating_band = bucket(Some(other.score), band);
        mine > theirs
    }
}

/// Mark the one frame of each pass worth keeping.
///
/// The unit is the car in the pass: (burst, group), or the burst alone where
/// grouping has not run. A star given by hand (or already in the file) wins
/// outright; then a candidate no rule culled; otherwise the measured rating
/// decides. On a rating tie, NIMA quality breaks it when `use_quality_score` is
/// on and both frames have a score, then the sharper stored subject decides,
/// and a full tie leaves the earlier frame in place, so a re-run picks the same
/// one. Of a
/// frame's subjects only those of the kind the profile rates the frame by
/// compete, as they do for the frame's own rating. A frame with no subject
/// that competes is picked on the frame's own rating instead, so a pass of
/// frames the detector found nothing in still gets a keeper. A subject or
/// frame some rule culled (a stored cull reason) only wins when every
/// candidate of its unit is culled. A face or an eye competes on the frame's
/// own rating -- the softer eye of a portrait -- and on the tighter face band,
/// because one eye of the pair says nothing about the portrait.
pub fn pick_of_pass(
    conn: &Connection,
    job: i64,
    profile: ScanProfile,
    include_people: bool,
    use_quality_score: bool,
) -> Result<Value> {
    let rank = |main: Option<Subject>, region: &str| {
        let priority = profile.frame_priority(include_people, main);
        priority
            .iter()
            .position(|k| region_name(*k) == region)
            .unwrap_or(priority.len())
    };
    let found = rows(
        conn,
        "SELECT d.id, d.x1, d.y1, d.x2, d.y2, d.rating, d.sharpness, d.quality, d.group_key, d.cut_sides, d.framing_score, COALESCE(d.stars, i.rating_in_file, 0) AS by_hand, COALESCE(d.cull_reason,'')<>'' AS culled, i.id AS image_id, i.burst_key, COALESCE(d.region_type,'vehicle') AS region, i.width, i.height, i.rating AS frame_rating, i.set_id, i.dark_frame, i.exposure_verdict, i.eye_open_min, i.smile_max, i.buck_score, (COALESCE(d.region_type,'')='eye' AND EXISTS (SELECT 1 FROM detections f WHERE f.image_id=d.image_id AND f.region_type='face' AND COALESCE(f.cull_reason,'')=?2 AND (d.x1+d.x2)>=2*f.x1 AND (d.x1+d.x2)<=2*f.x2 AND (d.y1+d.y2)>=2*f.y1 AND (d.y1+d.y2)<=2*f.y2)) AS shut_eye FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=?1 AND COALESCE(d.rejected,0)=0 AND COALESCE(d.bystander,0)=0 AND COALESCE(i.rejected,0)=0 AND i.burst_key IS NOT NULL AND COALESCE(i.copy_of,0)=0 ORDER BY i.id, d.id",
        params![job, crate::EYES_CLOSED],
    )?;
    // Which optional keys rank is the preset's decision; read it from the album
    // so every caller (scan end, regroup, re-pick) gets the same order.
    let preset = {
        let stored: Option<String> = conn
            .query_row("SELECT settings_json FROM jobs WHERE id=?", [job], |r| r.get(0))
            .ok();
        stored
            .and_then(|json| serde_json::from_str::<Map<String, Value>>(&json).ok())
            .and_then(|map| {
                map.get("scan_profile")
                    .and_then(Value::as_str)
                    .map(ShootPreset::parse)
            })
            .unwrap_or(ShootPreset::Mix)
    };
    let ranking = Ranking {
        quality: use_quality_score,
        framing: crate::framing_policy(preset).is_some(),
        // Eyes and smile decide between otherwise equal people frames: the
        // same presets as a smile, because eyes-open is a face judgement.
        eyes: matches!(
            preset,
            ShootPreset::Portrait
                | ShootPreset::PortraitGroup
                | ShootPreset::PortraitCouple
                | ShootPreset::Wedding
                | ShootPreset::Event
                | ShootPreset::EventShows
                | ShootPreset::EventParties
        ),
        smile: matches!(
            preset,
            ShootPreset::Portrait
                | ShootPreset::PortraitGroup
                | ShootPreset::PortraitCouple
                | ShootPreset::Wedding
                | ShootPreset::Event
                | ShootPreset::EventShows
                | ShootPreset::EventParties
        ),
        buck: matches!(
            preset,
            ShootPreset::SportRodeo | ShootPreset::SportEquestrian
        ),
    };
    let keeps_dark = preset.policy().dark_ok;
    // Each frame's main subject: its largest person or vehicle.
    let mut boxes: HashMap<i64, Vec<(Subject, f64)>> = HashMap::new();
    for r in &found {
        let kind = match r["region"].as_str() {
            Some("person") => Subject::Person,
            Some("vehicle") => Subject::Vehicle,
            _ => continue,
        };
        let c = |k: &str| r[k].as_f64().unwrap_or_default();
        boxes
            .entry(r["image_id"].as_i64().unwrap_or_default())
            .or_default()
            .push((kind, (c("x2") - c("x1")) * (c("y2") - c("y1"))));
    }
    let main: HashMap<i64, Subject> = boxes
        .into_iter()
        .filter_map(|(image, b)| conrod_core::profile::main_subject(b).map(|m| (image, m)))
        .collect();
    let region_of = |r: &Value| {
        let image = r["image_id"].as_i64().unwrap_or_default();
        rank(
            main.get(&image).copied(),
            r["region"].as_str().unwrap_or("vehicle"),
        )
    };
    let mut primary: HashMap<i64, usize> = HashMap::new();
    for r in &found {
        let best = primary
            .entry(r["image_id"].as_i64().unwrap_or_default())
            .or_insert(usize::MAX);
        *best = (*best).min(region_of(r));
    }
    // (burst, group) of the candidates, with the frame each sits on. Frames
    // whose subjects all lost the profile's region test still compete on the
    // frame's own rating, so a pass never goes unpicked for want of a subject.
    let mut units: Vec<((i64, Option<i64>), Candidate)> = Vec::new();
    let mut marked: HashSet<i64> = HashSet::new();
    for r in &found {
        let image = r["image_id"].as_i64().unwrap_or_default();
        if primary[&image] != region_of(r) {
            continue;
        }
        let region = r["region"].as_str().unwrap_or("vehicle");
        // A face or an eye competes on the frame's own rating: the portrait's
        // softer eye, which is what `frame_rating` stores and what the album
        // shows. Its own detection rating is one eye of the pair.
        let score = if matches!(region, "eye" | "face") {
            r["frame_rating"].as_f64().or_else(|| r["rating"].as_f64())
        } else {
            r["rating"].as_f64()
        };
        let Some(score) = score.or_else(|| r["sharpness"].as_f64()) else {
            continue;
        };
        let unit = (
            r["burst_key"].as_i64().unwrap_or_default(),
            r["group_key"].as_i64(),
        );
        let (width, height) = (
            r["width"].as_i64().unwrap_or_default(),
            r["height"].as_i64().unwrap_or_default(),
        );
        let bbox = match (
            r["x1"].as_f64(),
            r["y1"].as_f64(),
            r["x2"].as_f64(),
            r["y2"].as_f64(),
        ) {
            (Some(x1), Some(y1), Some(x2), Some(y2)) => Some([x1, y1, x2, y2]),
            _ => None,
        };
        let area = match bbox {
            Some([x1, y1, x2, y2]) if width > 0 && height > 0 => {
                (x2 - x1) * (y2 - y1) / (width as f64 * height as f64)
            }
            _ => 0.0,
        };
        let set_member = r["set_id"].as_i64().is_some();
        // A bracket, focus stack or interval is meant to hold frames that look
        // wrong one by one: membership exempts it from the exposure and blur
        // cull, and from the exposure flag.
        let dark = r["dark_frame"].as_i64().unwrap_or(0) != 0;
        units.push((
            unit,
            Candidate {
                frame: image,
                row: r["id"].as_i64().unwrap_or_default(),
                on_frame: false,
                by_hand: r["by_hand"].as_i64().unwrap_or(0),
                // An eye inside a face whose eyes are shut is not an open eye,
                // so it never wins a pass while an open-eyed frame competes.
                culled: (!set_member && r["culled"].as_i64().unwrap_or(0) != 0)
                    || r["shut_eye"].as_i64().unwrap_or(0) != 0
                    || (!set_member && !keeps_dark && dark),
                // Only a truly dark frame loses here: a bright sky or a crushed
                // shadow is a flag, not a ranking fault. The readiness motorsport
                // case proved a clipped sky must not outrank sharpness.
                bright: set_member || keeps_dark || r["exposure_verdict"].as_str() != Some("dark"),
                score,
                sharpness: r["sharpness"].as_f64().unwrap_or(score),
                quality: r["quality"].as_f64(),
                sides: framing::assess(bbox, width, height).sides,
                area,
                framing: r["framing_score"].as_f64(),
                eyes: r["eye_open_min"].as_f64(),
                smile: r["smile_max"].as_f64(),
                buck: r["buck_score"].as_f64(),
                band: if matches!(region, "eye" | "face") {
                    FACE_BAND
                } else {
                    RATING_BAND
                },
            },
        ));
        marked.insert(image);
    }
    // A frame with no competing subject: it is its own candidate, in the
    // burst's ungrouped unit, rated by the frame.
    let spare = rows(
        conn,
        "SELECT id,burst_key,COALESCE(stars,rating_in_file,0) AS by_hand,rating,sharpness,set_id,dark_frame,exposure_verdict,eye_open_min,smile_max,buck_score FROM images WHERE job_id=? AND burst_key IS NOT NULL AND COALESCE(rejected,0)=0 AND COALESCE(copy_of,0)=0 ORDER BY id",
        [job],
    )?;
    for r in &spare {
        let image = r["id"].as_i64().unwrap_or_default();
        if marked.contains(&image) {
            continue;
        }
        let Some(score) = r["rating"].as_f64().or_else(|| r["sharpness"].as_f64()) else {
            continue;
        };
        let set_member = r["set_id"].as_i64().is_some();
        let dark = r["dark_frame"].as_i64().unwrap_or(0) != 0;
        units.push((
            (r["burst_key"].as_i64().unwrap_or_default(), None),
            Candidate {
                frame: image,
                row: image,
                on_frame: true,
                by_hand: r["by_hand"].as_i64().unwrap_or(0),
                culled: !set_member && !keeps_dark && dark,
                bright: set_member || keeps_dark || r["exposure_verdict"].as_str() != Some("dark"),
                score,
                sharpness: r["sharpness"].as_f64().unwrap_or(score),
                quality: None,
                sides: 0,
                area: 0.0,
                framing: None,
                eyes: r["eye_open_min"].as_f64(),
                smile: r["smile_max"].as_f64(),
                buck: r["buck_score"].as_f64(),
                band: RATING_BAND,
            },
        ));
    }
    // Earlier frame first, so an exact tie keeps it, as a re-run always has.
    units.sort_by_key(|(_, c)| (c.frame, c.row));
    let mut seen: HashSet<(i64, Option<i64>)> = HashSet::new();
    let mut best: HashMap<(i64, Option<i64>), Candidate> = HashMap::new();
    for (unit, candidate) in units {
        seen.insert(unit);
        // A burst whose best frame is still below three stars has no keeper.
        // Proposing the least-bad soft frame of a bad set is worse than saying
        // nothing: the photographer asked for a pick only when there is one.
        // A hand star is the photographer's own word, so it always counts. Such
        // a frame never enters the fold, or it would win the ranking and be
        // dropped, hiding a bar-clearing frame of the same unit.
        if candidate.by_hand < 3 && candidate.score < conrod_vision::sharpness::KEEPER_MIN_RATING {
            continue;
        }
        if best
            .get(&unit)
            .is_none_or(|held| candidate.beats(held, &ranking))
        {
            best.insert(unit, candidate);
        }
    }
    // A frame with no subject sits in the burst's ungrouped unit, so once the
    // detections are grouped it would get a pick of its own beside the car's.
    // The car's is the keeper: the other is the shot the detector missed.
    let grouped: HashSet<i64> = best
        .iter()
        .filter(|((_, group), candidate)| group.is_some() && !candidate.on_frame)
        .map(|((burst, _), _)| *burst)
        .collect();
    let missed: HashSet<(i64, Option<i64>)> = best
        .iter()
        .filter(|((burst, group), candidate)| {
            group.is_none() && candidate.on_frame && grouped.contains(burst)
        })
        .map(|(unit, _)| *unit)
        .collect();
    for unit in &missed {
        best.remove(unit);
    }
    let passes = seen.len().saturating_sub(missed.len());
    let (mut picks, mut frame_picks): (Vec<i64>, Vec<i64>) = (Vec::new(), Vec::new());
    for candidate in best.values() {
        if candidate.on_frame {
            frame_picks.push(candidate.row);
        } else {
            picks.push(candidate.row);
        }
    }
    let tx = conn.unchecked_transaction().map_err(err)?;
    tx.execute(
        "UPDATE detections SET burst_pick=NULL WHERE image_id IN (SELECT id FROM images WHERE job_id=?)",
        [job],
    )
    .map_err(err)?;
    tx.execute("UPDATE images SET burst_pick=NULL WHERE job_id=?", [job])
        .map_err(err)?;
    for id in &picks {
        tx.execute("UPDATE detections SET burst_pick=1 WHERE id=?", [id])
            .map_err(err)?;
    }
    for id in &frame_picks {
        tx.execute("UPDATE images SET burst_pick=1 WHERE id=?", [id])
            .map_err(err)?;
    }
    tx.commit().map_err(err)?;
    Ok(json!({
        "passes": passes,
        "picks": picks.len() + frame_picks.len(),
        // Bursts where nothing cleared the keeper bar: the UI says "no keeper"
        // instead of showing a green suggestion on the least-bad frame.
        "unpicked": passes.saturating_sub(picks.len() + frame_picks.len()),
        "considered": found.len()
    }))
}

/// The action: run the pick and report it as a task.
pub fn pick_keepers(d: &Desktop, job: i64) -> Result<Value> {
    require_job(d, job)?;
    let cfg = settings(d, job)?;
    let profile = ScanProfile::parse(&cfg.scan_profile);
    let task = d.hub.start("Picking keepers", 0);
    match pick_of_pass(
        &lock(&d.db),
        job,
        profile,
        cfg.include_people,
        cfg.use_quality_score,
    ) {
        Ok(stats) => {
            task.detail(format!(
                "{} keepers from {} frames across {} passes",
                stats["picks"], stats["considered"], stats["passes"]
            ));
            task.finish();
            Ok(stats)
        }
        Err(e) => {
            task.fail(e.clone());
            Err(e)
        }
    }
}

/// The action: sort an album into one pile per car, as a background operation.
pub fn group(d: &Arc<Desktop>, job: i64) -> Result<Value> {
    require_job(d, job)?;
    launch(d, job, "Grouping vehicles", move |d, stop, task| {
        let looked = embed_missing(d, job, stop, task)?;
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        let (groups, vehicles) = consolidate(d, job, task)?;
        let dupes = near_duplicates(d, job, stop)?;
        task.detail(format!(
            "{groups} cars from {vehicles} vehicles ({looked} looked at for the first time), {dupes} frames re-shot elsewhere"
        ));
        Ok(())
    })
}

/// The action: hide the extra imports of one frame, as a background operation.
/// Nothing on disk is touched; the copies are marked `copy_of` and ignored by
/// bursts, picks, counts and XMP writes.
pub fn collapse_copies(d: &Arc<Desktop>, job: i64) -> Result<Value> {
    require_job(d, job)?;
    if !d.status()["activeJob"].is_null() {
        return Err("A scan is running. Stop it first.".into());
    }
    launch(d, job, "Collapsing copies", move |d, stop, task| {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        crate::copies::collapse(d, job, stop, task)?;
        Ok(())
    })
}

/// The action: show the copies `collapse_copies` hid again, as a background
/// operation. The album is regrouped from all of its frames.
pub fn restore_copies(d: &Arc<Desktop>, job: i64) -> Result<Value> {
    require_job(d, job)?;
    if !d.status()["activeJob"].is_null() {
        return Err("A scan is running. Stop it first.".into());
    }
    launch(d, job, "Restoring copies", move |d, stop, task| {
        if stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        crate::copies::restore(d, job, task)?;
        Ok(())
    })
}

/// The action: turn people on or off for an album. On, for a motorsport album
/// that skipped faces, finds the faces and eyes of the people already stored
/// as a background operation; off only saves the flag (the rows stay, hidden
/// by the window).
pub fn set_include_people(d: &Arc<Desktop>, job: i64, value: bool) -> Result<Value> {
    let mut cfg = settings(d, job)?;
    cfg.include_people = value;
    let preset = ShootPreset::parse(&cfg.scan_profile);
    // Faces are already found at scan time wherever the flag is not needed.
    let operation = if value && !preset.wants_faces(false) {
        let cfg = cfg.clone();
        launch(d, job, "Finding people", move |d, stop, task| {
            if crate::operations::wait_for_cull(d, stop, task) {
                find_people(d, job, &cfg, stop, task)?;
            }
            Ok(())
        })?["operation"]
            .clone()
    } else {
        Value::Null
    };
    let mut patch = Map::new();
    patch.insert("include_people".into(), json!(value));
    let mut saved = crate::library::update_job_settings(
        d,
        &crate::commands::UpdateJobSettingsArgs { job_id: job, patch },
    )?;
    saved["operation"] = operation;
    Ok(saved)
}

/// Faces and eyes for every stored person of an album whose frame has none
/// yet; vehicles are neither re-detected nor touched. Then the frames are
/// re-rated by their main subject and the faces prepared for naming.
fn find_people(
    d: &Desktop,
    job: i64,
    cfg: &Settings,
    stop: &AtomicBool,
    task: &Task,
) -> Result<()> {
    let people = rows(
        &lock(&d.reader),
        "SELECT d.image_id, i.path, d.x1, d.y1, d.x2, d.y2 FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? AND COALESCE(i.rejected,0)=0 AND COALESCE(i.copy_of,0)=0 AND d.region_type='person' AND NOT EXISTS (SELECT 1 FROM detections f WHERE f.image_id=i.id AND f.region_type IN ('face','eye')) ORDER BY i.id, d.id",
        [job],
    )?;
    let mut frames: BTreeMap<i64, (String, Vec<[f64; 4]>)> = BTreeMap::new();
    for r in &people {
        let c = |k: &str| r[k].as_f64().unwrap_or_default();
        frames
            .entry(r["image_id"].as_i64().unwrap_or_default())
            .or_insert_with(|| {
                (
                    r["path"].as_str().unwrap_or_default().to_owned(),
                    Vec::new(),
                )
            })
            .1
            .push([c("x1"), c("y1"), c("x2"), c("y2")]);
    }
    if !frames.is_empty() {
        crate::setup::ensure(&d.hub, stop, crate::setup::FACES)?;
        let detector = std::sync::Mutex::new(crate::load_faces()?);
        let expression = crate::load_expression();
        let region_models: HashMap<String, _> = ["face", "eye"]
            .into_iter()
            .filter_map(|r| crate::load_region_model(r).map(|m| (r.to_string(), m)))
            .collect();
        let total = frames.len() as u64;
        for (n, (image, (path, boxes))) in frames.iter().enumerate() {
            if stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            task.detail(format!("Looking for faces in {path}"));
            let raw = conrod_io::raw::read(std::path::Path::new(path))?;
            let rgb = Rgb::decode_jpeg(&raw.preview, 1)?.orient(raw.orientation);
            let grey = rgb.to_gray();
            let strictness = cfg.effective_blur();
            let found = crate::face_subjects(
                &rgb,
                &grey,
                boxes.iter().copied(),
                &detector,
                expression.as_ref(),
                cfg,
                &strictness,
                &region_models,
            )?;
            let db = lock(&d.db);
            let tx = db.unchecked_transaction().map_err(err)?;
            for s in &found.subjects {
                crate::desktop::insert_subject(&tx, *image, s)?;
            }
            tx.commit().map_err(err)?;
            task.progress(n as u64 + 1, total);
        }
    }
    rerate(d, job, cfg)?;
    let faces = embed_faces_missing(d, job, stop, task)?;
    task.detail(format!(
        "Found faces in {} frames; {faces} ready for naming",
        frames.len()
    ));
    Ok(())
}

/// Every frame's rating again from its stored subjects, and the keepers.
fn rerate(d: &Desktop, job: i64, cfg: &Settings) -> Result<()> {
    let profile = ScanProfile::parse(&cfg.scan_profile);
    let found = rows(
        &lock(&d.reader),
        "SELECT d.image_id, COALESCE(d.region_type,'vehicle') AS region, d.x1, d.y1, d.x2, d.y2, d.rating, i.sharpness AS whole FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? ORDER BY d.image_id",
        [job],
    )?;
    type Rated<'a> = Vec<(&'a str, [f64; 4], f64)>;
    let mut frames: BTreeMap<i64, (Option<f64>, Rated)> = BTreeMap::new();
    for r in &found {
        let c = |k: &str| r[k].as_f64().unwrap_or_default();
        let frame = frames
            .entry(r["image_id"].as_i64().unwrap_or_default())
            .or_insert((r["whole"].as_f64(), Vec::new()));
        frame.1.push((
            r["region"].as_str().unwrap_or("vehicle"),
            [c("x1"), c("y1"), c("x2"), c("y2")],
            c("rating"),
        ));
    }
    let db = lock(&d.db);
    let tx = db.unchecked_transaction().map_err(err)?;
    for (image, (whole, subjects)) in &frames {
        let rating = crate::frame_rating(profile, cfg.include_people, subjects, *whole);
        tx.execute(
            "UPDATE images SET rating=? WHERE id=?",
            params![rating, image],
        )
        .map_err(err)?;
    }
    tx.commit().map_err(err)?;
    pick_of_pass(&db, job, profile, cfg.include_people, cfg.use_quality_score)?;
    Ok(())
}

/// Give every stored vehicle crop an embedding, for albums identified before
/// there were any. Skips crops that have one, so it is safe to run again.
fn embed_missing(d: &Desktop, job: i64, stop: &AtomicBool, task: &Task) -> Result<usize> {
    let todo = rows(
        &lock(&d.reader),
        &format!("SELECT d.id, d.crop_path FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? AND {ML_ELIGIBLE} AND d.crop_path IS NOT NULL AND (d.embedding IS NULL OR d.embedding='') AND COALESCE(d.region_type,'vehicle')='vehicle' AND COALESCE(i.copy_of,0)=0 ORDER BY d.id"),
        [job],
    )?;
    if todo.is_empty() {
        return Ok(0);
    }
    crate::setup::ensure(&d.hub, stop, crate::setup::SIMILARITY)?;
    let mut embedder =
        similarity::Embedder::load(&models::expected(models::SIMILARITY), Device::Cpu)?;
    let mut done = 0;
    for chunk in todo.chunks(50) {
        let mut found = Vec::new();
        for row in chunk {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            if !ml_eligible(d, row["id"].as_i64().ok_or("Invalid detection")?)? {
                continue;
            }
            let vector = row["crop_path"]
                .as_str()
                .and_then(|p| std::fs::read(p).ok())
                .and_then(|bytes| Rgb::decode_jpeg(&bytes, 1).ok())
                .and_then(|crop| embedder.embed(&crop).ok());
            if let (Some(id), Some(v)) = (row["id"].as_i64(), vector) {
                found.push((id, similarity::pack(&v)));
            }
            done += 1;
            task.progress(done as u64, todo.len() as u64);
        }
        let db = lock(&d.db);
        let tx = db.unchecked_transaction().map_err(err)?;
        for (id, packed) in found {
            tx.execute(
                "UPDATE detections SET embedding=? WHERE id=?",
                params![packed, id],
            )
            .map_err(err)?;
        }
        tx.commit().map_err(err)?;
        if stop.load(Ordering::Relaxed) {
            break;
        }
    }
    Ok(done)
}

/// Hash every frame that has a thumbnail but no pHash yet (so older albums
/// backfill), then stack re-shoots of one picture across the whole shoot into
/// `images.dupe_key`. Returns how many frames landed in a stack.
pub(crate) fn near_duplicates(d: &Desktop, job: i64, stop: &AtomicBool) -> Result<usize> {
    let todo = rows(
        &lock(&d.reader),
        "SELECT id, thumb_path FROM images WHERE job_id=? AND phash IS NULL AND thumb_path IS NOT NULL AND COALESCE(copy_of,0)=0",
        [job],
    )?;
    // Opening and hashing a thumbnail is decode-bound: the rows are independent,
    // so they go through the pool and come back in the same order they went in.
    let hashed: Vec<(i64, i64)> = if stop.load(Ordering::Relaxed) {
        Vec::new()
    } else {
        todo.par_iter()
            .filter_map(|row| {
                let gray = row["thumb_path"].as_str().and_then(|p| {
                    conrod_vision::imageops::Gray::open(std::path::Path::new(p)).ok()
                })?;
                Some((row["id"].as_i64()?, similarity::phash(&gray) as i64))
            })
            .collect()
    };
    if stop.load(Ordering::Relaxed) {
        return Ok(0);
    }
    {
        let db = lock(&d.db);
        let tx = db.unchecked_transaction().map_err(err)?;
        for (id, hash) in &hashed {
            tx.execute("UPDATE images SET phash=? WHERE id=?", params![hash, id])
                .map_err(err)?;
        }
        tx.commit().map_err(err)?;
    }
    let frames = rows(
        &lock(&d.reader),
        "SELECT id, phash, burst_key, taken_at FROM images WHERE job_id=? AND phash IS NOT NULL AND status='done' AND rejected=0 AND COALESCE(copy_of,0)=0",
        [job],
    )?;
    let looks = rows(
        &lock(&d.reader),
        "SELECT d.image_id, d.embedding FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? AND COALESCE(d.region_type,'vehicle')='vehicle' AND d.embedding IS NOT NULL AND d.embedding!='' AND COALESCE(i.copy_of,0)=0",
        [job],
    )?;
    let mut by_image: HashMap<i64, Vec<Vec<f32>>> = HashMap::new();
    for r in &looks {
        if let (Some(id), Some(v)) = (
            r["image_id"].as_i64(),
            r["embedding"].as_str().and_then(similarity::unpack),
        ) {
            by_image.entry(id).or_default().push(v);
        }
    }
    let frames: Vec<_> = frames
        .iter()
        .filter_map(|r| {
            let id = r["id"].as_i64()?;
            Some(conrod_core::bursts::DupeFrame {
                id,
                phash: r["phash"].as_i64()? as u64,
                burst: r["burst_key"].as_i64(),
                taken: r["taken_at"].as_f64(),
                looks: by_image.remove(&id).unwrap_or_default(),
            })
        })
        .collect();
    let stacks = conrod_core::bursts::near_dupes(&frames, grouping::SAME_CAR as f32);
    let db = lock(&d.db);
    let tx = db.unchecked_transaction().map_err(err)?;
    tx.execute("UPDATE images SET dupe_key=NULL WHERE job_id=?", [job])
        .map_err(err)?;
    for (id, key) in &stacks {
        tx.execute("UPDATE images SET dupe_key=? WHERE id=?", params![key, id])
            .map_err(err)?;
    }
    tx.commit().map_err(err)?;
    Ok(stacks.len())
}

/// Identity vectors for detected faces: SFace on the face re-found in its
/// stored box and aligned by its landmarks. A face is (re)embedded when it has
/// no SFace vector yet, which is how the older DINOv2 face vectors (384 long,
/// not 128) are replaced. Without SFace it falls back to DINOv2 for faces that
/// have nothing at all. Matches are suggestions; a human click applies a name.
pub(crate) fn embed_faces_missing(
    d: &Desktop,
    job: i64,
    stop: &AtomicBool,
    task: &Task,
) -> Result<usize> {
    let todo: Vec<Value> = rows(
        &lock(&d.reader),
        &format!("SELECT d.id,d.x1,d.y1,d.x2,d.y2,i.path,d.embedding FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? AND {ML_ELIGIBLE} AND COALESCE(d.region_type,'')='face' AND COALESCE(i.copy_of,0)=0 ORDER BY i.id,d.id"),
        [job],
    )?
    .into_iter()
    .filter(|r| match r["embedding"].as_str().unwrap_or_default() {
        "" => true,
        text => similarity::unpack(text).is_some_and(|v| v.len() != IDENTITY_DIM),
    })
    .collect();
    if todo.is_empty() {
        cluster_people(d, job)?;
        return Ok(0);
    }
    // Optional: `ensure` reports a failed SFace install and carries on.
    crate::setup::ensure(&d.hub, stop, &[models::FACE_ID])?;
    let mut identity = models::find(models::FACE_ID)
        .and_then(|model| FaceEmbedder::load(&model).ok())
        .zip(crate::load_faces().ok());
    let (todo, mut dino) = if identity.is_some() {
        (todo, None)
    } else {
        let bare: Vec<Value> = todo
            .into_iter()
            .filter(|r| r["embedding"].as_str().is_none_or(str::is_empty))
            .collect();
        if bare.is_empty() {
            return Ok(0);
        }
        crate::setup::ensure(&d.hub, stop, crate::setup::SIMILARITY)?;
        let model = models::expected(models::SIMILARITY);
        (bare, Some(similarity::Embedder::load(&model, Device::Cpu)?))
    };
    let mut current: Option<(String, Rgb)> = None;
    let mut found = Vec::new();
    for (index, row) in todo.iter().enumerate() {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        if !ml_eligible(d, row["id"].as_i64().ok_or("Invalid detection")?)? {
            continue;
        }
        let Some(path) = row["path"].as_str() else {
            continue;
        };
        task.detail(format!(
            "Matching face in {}",
            std::path::Path::new(path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
        ));
        if current.as_ref().is_none_or(|(loaded, _)| loaded != path) {
            let raw = conrod_io::raw::read(std::path::Path::new(path))?;
            current = Some((
                path.to_owned(),
                Rgb::decode_jpeg(&raw.preview, 1)?.orient(raw.orientation),
            ));
        }
        let image = &current.as_ref().unwrap().1;
        let coords = ["x1", "y1", "x2", "y2"]
            .map(|key| row[key].as_f64().unwrap_or_default().max(0.0) as usize);
        let [x1, y1, x2, y2] = coords;
        if x2 <= x1 || y2 <= y1 {
            continue;
        }
        let crop = image.crop(x1, y1, x2.min(image.width), y2.min(image.height));
        let id = row["id"].as_i64().unwrap_or_default();
        let crop_path = d.root.join(format!("cache/native/face-{id}.jpg"));
        crate::desktop::save_jpeg(&crop, &crop_path)?;
        let vector = match (&mut identity, &mut dino) {
            (Some((sface, yunet)), _) => {
                // The landmarks are not stored: find the face again in its box
                // with a margin, which YuNet needs to see a whole head.
                let (pw, ph) = ((x2 - x1) / 3, (y2 - y1) / 3);
                let (ox, oy) = (x1.saturating_sub(pw), y1.saturating_sub(ph));
                let area = image.crop(ox, oy, x2 + pw, y2 + ph);
                let Some(face) = yunet.detect(&area)?.into_iter().next() else {
                    continue;
                };
                sface.embed(&area, &face)?
            }
            (None, Some(dino)) => dino.embed(&crop)?,
            (None, None) => continue,
        };
        found.push((
            id,
            similarity::pack(&vector),
            crop_path.to_string_lossy().into_owned(),
        ));
        task.progress((index + 1) as u64, todo.len() as u64);
    }
    {
        let db = lock(&d.db);
        let tx = db.unchecked_transaction().map_err(err)?;
        for (id, packed, crop_path) in &found {
            tx.execute(
                "UPDATE detections SET embedding=?,crop_path=? WHERE id=?",
                params![packed, crop_path, id],
            )
            .map_err(err)?;
        }
        tx.commit().map_err(err)?;
    }
    cluster_people(d, job)?;
    Ok(found.len())
}

/// Sort an album's faces into people: each face joins the person whose mean
/// it is nearest, if at least [`SAME_PERSON`], else starts a new one. The
/// person is stored as the face's `group_key` (its first face's id) and
/// `group_size`. Faces without an SFace vector are left out.
///
/// ponytail: greedy single pass, O(faces x people); agglomerative clustering
/// if order effects show up on big albums.
fn cluster_people(d: &Desktop, job: i64) -> Result<()> {
    let faces: Vec<(i64, Vec<f32>)> = rows(
        &lock(&d.reader),
        "SELECT d.id,d.embedding FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? AND COALESCE(d.region_type,'')='face' AND COALESCE(d.rejected,0)=0 AND COALESCE(i.copy_of,0)=0 ORDER BY i.id,d.id",
        [job],
    )?
    .into_iter()
    .filter_map(|r| {
        let v = similarity::unpack(r["embedding"].as_str()?)?;
        (v.len() == IDENTITY_DIM).then(|| (r["id"].as_i64().unwrap_or_default(), v))
    })
    .collect();
    let people = group_faces(&faces);
    let db = lock(&d.db);
    let tx = db.unchecked_transaction().map_err(err)?;
    tx.execute(
        "UPDATE detections SET group_key=NULL,group_size=NULL WHERE region_type='face' AND image_id IN (SELECT id FROM images WHERE job_id=?)",
        [job],
    )
    .map_err(err)?;
    for members in &people {
        for id in members {
            tx.execute(
                "UPDATE detections SET group_key=?,group_size=? WHERE id=?",
                params![members[0], members.len() as i64, id],
            )
            .map_err(err)?;
        }
    }
    tx.commit().map_err(err)?;
    Ok(())
}

/// The people among `(id, unit vector)` faces, each a list of face ids.
fn group_faces(faces: &[(i64, Vec<f32>)]) -> Vec<Vec<i64>> {
    let mut people: Vec<(Vec<f32>, Vec<i64>)> = Vec::new();
    for (id, v) in faces {
        let best = people
            .iter_mut()
            .map(|(sum, ids)| {
                let norm = sum.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-6);
                (similarity::nearness(sum, v) / norm, sum, ids)
            })
            .filter(|(near, ..)| *near >= SAME_PERSON)
            .max_by(|a, b| a.0.total_cmp(&b.0));
        match best {
            Some((_, sum, ids)) => {
                sum.iter_mut().zip(v).for_each(|(s, x)| *s += x);
                ids.push(*id);
            }
            None => people.push((v.clone(), vec![*id])),
        }
    }
    people.into_iter().map(|(_, ids)| ids).collect()
}

/// The group's own answer if it has one, else what this frame's reader said.
fn agreed_or_own(agreed: &Option<String>, current: &Map<String, Value>, own: &str) -> Value {
    agreed
        .clone()
        .filter(|s| !s.is_empty())
        .map(Value::from)
        .unwrap_or_else(|| current.get(own).cloned().unwrap_or(Value::Null))
}

fn empty(value: Option<&Value>) -> bool {
    matches!(value, None | Some(Value::Null)) || value.and_then(Value::as_str) == Some("")
}

/// Group an album's vehicles by how their crops look and settle on one identity
/// per car, written back beside (never over) what each frame's reader said.
/// Returns (cars, vehicles). Then re-picks the keepers, which are per car.
pub fn consolidate(d: &Desktop, job: i64, task: &Task) -> Result<(usize, usize)> {
    task.detail("Sorting vehicles into cars");
    let sql = format!(
        "SELECT d.id, d.cls, d.attributes, d.colour_hex, d.plate, d.number, d.embedding, i.id AS image_id, i.burst_key \
         FROM detections d JOIN images i ON i.id=d.image_id \
         WHERE i.job_id=? AND {ML_ELIGIBLE} AND COALESCE(d.region_type,'vehicle')='vehicle' AND COALESCE(i.copy_of,0)=0 \
         ORDER BY i.id, d.id"
    );
    let found = rows(&lock(&d.reader), &sql, [job])?;
    if found.is_empty() {
        return Ok((0, 0));
    }
    let mut attributes: HashMap<i64, Map<String, Value>> = HashMap::new();
    let mut looks = Vec::new();
    for row in &found {
        let id = row["id"].as_i64().unwrap_or_default();
        let mut parsed = attributes_of(row["attributes"].as_str());
        grouping::use_own_reading(&mut parsed);
        parsed.insert("colour_hex".into(), row["colour_hex"].clone());
        attributes.insert(id, parsed.clone());
        looks.push(grouping::LookRow {
            det_id: id,
            vector: row["embedding"]
                .as_str()
                .and_then(similarity::unpack)
                .map(|v| v.into_iter().map(f64::from).collect()),
            frame_index: row["image_id"].as_i64().unwrap_or_default(),
            burst: row["burst_key"].as_i64(),
            plate: row["plate"].as_str().map(str::to_owned),
            number: row["number"].as_str().map(str::to_owned).or_else(|| {
                parsed
                    .get("race_number")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            }),
            cls: row["cls"].as_str().map(str::to_owned),
        });
    }
    // The pipeline falls back to a cruder shape-and-colour measure when the look model
    // has not seen most of the album; there are no crop signatures here, so the
    // album has to be looked at first.
    let usable = looks.iter().filter(|l| l.vector.is_some()).count();
    if usable == 0 {
        return Err(format!(
            "Only 0 of {} vehicles have been looked at by the grouping model, too few to group. Install the model and group again.",
            looks.len()
        ));
    }
    let assignment = grouping::cluster_by_look(&looks, grouping::SAME_CAR);
    let mut members: BTreeMap<i64, Vec<i64>> = BTreeMap::new();
    for look in &looks {
        if let Some(key) = assignment.get(&look.det_id) {
            members.entry(*key).or_default().push(look.det_id);
        }
    }

    // (detection, attributes, group key, size, agreement, sampled colour)
    let mut writes = Vec::new();
    for (key, ids) in &members {
        let group: Vec<_> = ids.iter().map(|i| attributes[i].clone()).collect();
        let agreed = grouping::consensus(&group);
        for id in ids {
            let mut current = attributes[id].clone();
            // What this frame's reader said is a measurement and is kept; the
            // group's answer goes alongside and everything downstream prefers it.
            grouping::remember_own_reading(&mut current);
            current.insert("group_make".into(), json!(agreed.make));
            current.insert("group_model".into(), json!(agreed.model));
            let make = agreed_or_own(&agreed.make, &current, "own_make");
            let model = agreed_or_own(&agreed.model, &current, "own_model");
            current.insert("make".into(), make);
            current.insert("model".into(), model);
            if agreed.colour.as_deref().is_some_and(|c| !c.is_empty()) {
                current.insert("colour".into(), json!(agreed.colour));
            }
            if agreed.plate.is_some() && empty(current.get("plate")) {
                current.insert("plate".into(), json!(agreed.plate));
                current.insert("plate_conf".into(), json!(0.0));
            }
            if agreed.race_number.is_some() && empty(current.get("race_number")) {
                current.insert("race_number".into(), json!(agreed.race_number));
            }
            if agreed.team.is_some() && empty(current.get("team")) {
                current.insert("team".into(), json!(agreed.team));
            }
            if agreed.driver.is_some() && empty(current.get("driver")) {
                current.insert("driver".into(), json!(agreed.driver));
            }
            if agreed.country.is_some() && empty(current.get("country")) {
                current.insert("country".into(), json!(agreed.country));
            }
            if !agreed.sponsors.is_empty() {
                current.insert("sponsors".into(), json!(agreed.sponsors));
            }
            if !agreed.livery_text.is_empty() {
                current.insert("livery_text".into(), json!(agreed.livery_text));
            }
            current.insert("group_disputed".into(), json!(agreed.disputed));
            current.insert("group_second_look".into(), json!(agreed.second_look));
            writes.push((
                *id,
                Value::Object(current).to_string(),
                *key,
                agreed.size as i64,
                (agreed.agreement * 1000.0).round() / 1000.0,
                agreed.colour_hex.clone(),
            ));
        }
    }
    let cfg = settings(d, job)?;
    let profile = ScanProfile::parse(&cfg.scan_profile);
    let db = lock(&d.db);
    let tx = db.unchecked_transaction().map_err(err)?;
    for (id, attrs, key, size, agreement, hex) in &writes {
        // group_colour_hex, never colour_hex: the per-frame sample is a
        // measurement and a second regroup must not average averages.
        tx.execute(
            "UPDATE detections SET attributes=?,group_key=?,group_size=?,group_agreement=?,group_colour_hex=? WHERE id=?",
            params![attrs, key, size, agreement, hex, id],
        )
        .map_err(err)?;
    }
    tx.commit().map_err(err)?;
    pick_of_pass(&db, job, profile, cfg.include_people, cfg.use_quality_score)?;
    Ok((members.len(), looks.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Lib;

    fn picks(lib: &Lib) -> Vec<i64> {
        let db = lib.db();
        let mut stmt = db
            .prepare("SELECT id FROM detections WHERE burst_pick=1 ORDER BY id")
            .unwrap();
        let ids = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        ids
    }

    #[test]
    fn the_sharpest_frame_of_each_pass_is_the_keeper_and_a_hand_star_beats_it() {
        let lib = Lib::new("pick");
        let (f1, f2, f3) = (
            lib.frame("1.jpg", Some(1)),
            lib.frame("2.jpg", Some(1)),
            lib.frame("3.jpg", Some(1)),
        );
        let g1 = lib.frame("4.jpg", Some(2));
        let g2 = lib.frame("5.jpg", Some(2));
        let none = lib.frame("6.jpg", None);
        let (a, b, c) = (
            lib.detection(f1, "vehicle", 0.90),
            lib.detection(f2, "vehicle", 0.97),
            lib.detection(f3, "vehicle", 0.97),
        );
        let (d1, d2) = (
            lib.detection(g1, "vehicle", 0.88),
            lib.detection(g2, "vehicle", 0.80),
        );
        lib.detection(none, "vehicle", 0.99);
        let out = lib.run("pick_keepers", json!({"jobId": lib.job})).unwrap();
        assert_eq!(
            (
                out["passes"].as_i64(),
                out["picks"].as_i64(),
                out["considered"].as_i64()
            ),
            (Some(2), Some(2), Some(5)),
            "a frame with no burst has no pass"
        );
        assert_eq!(picks(&lib), vec![b, d1], "a tie goes to the earlier frame");
        let unpicked: Option<i64> = lib.one("SELECT burst_pick FROM detections WHERE id=?", [a]);
        assert_eq!(
            unpicked, None,
            "not a keeper is NULL, matching the recorded behaviour"
        );

        // A hand star outranks a better measure; a hand reject leaves the pass.
        lib.sql("UPDATE detections SET stars=3 WHERE id=?", [a]);
        lib.run("pick_keepers", json!({"jobId": lib.job})).unwrap();
        assert_eq!(picks(&lib), vec![a, d1]);
        lib.sql("UPDATE detections SET rejected=1 WHERE id=?", [a]);
        lib.sql("UPDATE detections SET bystander=1 WHERE id=?", [d1]);
        lib.run("pick_keepers", json!({"jobId": lib.job})).unwrap();
        assert_eq!(picks(&lib), vec![b, d2]);
        let _ = c;
        assert!(lib.run("pick_keepers", json!({"jobId": 999})).is_err());
        assert!(lib.task("Picking keepers")["detail"]
            .as_str()
            .unwrap()
            .contains("keepers"));
    }

    #[test]
    fn a_rating_tie_goes_to_the_sharper_subject_then_the_earlier_frame() {
        let lib = Lib::new("pick-tie");
        let (f1, f2, f3) = (
            lib.frame("1.jpg", Some(1)),
            lib.frame("2.jpg", Some(1)),
            lib.frame("3.jpg", Some(1)),
        );
        // Equal ratings: the stored sharpness has to rank them.
        let a = lib.detection(f1, "vehicle", 0.80);
        let b = lib.detection(f2, "vehicle", 0.80);
        let c = lib.detection(f3, "vehicle", 0.80);
        lib.sql("UPDATE detections SET sharpness=0.70 WHERE id=?", [a]);
        lib.sql("UPDATE detections SET sharpness=0.90 WHERE id=?", [b]);
        lib.sql("UPDATE detections SET sharpness=0.70 WHERE id=?", [c]);
        lib.run("pick_keepers", json!({"jobId": lib.job})).unwrap();
        assert_eq!(
            picks(&lib),
            vec![b],
            "the sharper subject wins an exact rating tie, even on a later frame"
        );
        // With the sharpness equal too, the tie keeps the earlier frame.
        lib.sql("UPDATE detections SET sharpness=0.80 WHERE id=?", [b]);
        lib.sql("UPDATE detections SET sharpness=0.80 WHERE id=?", [a]);
        lib.run("pick_keepers", json!({"jobId": lib.job})).unwrap();
        assert_eq!(picks(&lib), vec![a], "a full tie keeps the earlier frame");
    }

    fn place(lib: &Lib, det: i64, bbox: [f64; 4]) {
        lib.sql(
            "UPDATE detections SET x1=?,y1=?,x2=?,y2=? WHERE id=?",
            params![bbox[0], bbox[1], bbox[2], bbox[3], det],
        );
    }

    #[test]
    fn within_the_rating_band_the_larger_uncut_subject_wins() {
        let lib = Lib::new("pick-band");
        let (f1, f2, f3) = (
            lib.frame("1.jpg", Some(1)),
            lib.frame("2.jpg", Some(1)),
            lib.frame("3.jpg", Some(1)),
        );
        let small = lib.detection(f1, "vehicle", 0.80);
        let medium = lib.detection(f2, "vehicle", 0.80);
        let large = lib.detection(f3, "vehicle", 0.80);
        place(&lib, small, [2000.0, 1500.0, 3000.0, 2500.0]);
        place(&lib, medium, [1500.0, 1000.0, 3500.0, 3000.0]);
        place(&lib, large, [800.0, 600.0, 4500.0, 3400.0]);
        lib.run("pick_keepers", json!({"jobId": lib.job})).unwrap();
        assert_eq!(
            picks(&lib),
            vec![large],
            "equal ratings: the largest subject the frame edge does not cut off"
        );
    }

    #[test]
    fn within_the_rating_band_an_uncut_subject_beats_a_bigger_cut_off_one() {
        let lib = Lib::new("pick-cut");
        let (f1, f2) = (lib.frame("1.jpg", Some(1)), lib.frame("2.jpg", Some(1)));
        let cut = lib.detection(f1, "vehicle", 0.80);
        let uncut = lib.detection(f2, "vehicle", 0.79);
        place(&lib, cut, [0.0, 0.0, 4000.0, 3000.0]);
        place(&lib, uncut, [1000.0, 1000.0, 2500.0, 2500.0]);
        lib.run("pick_keepers", json!({"jobId": lib.job})).unwrap();
        assert_eq!(
            picks(&lib),
            vec![uncut],
            "a subject cut off on two sides loses to a smaller one inside the frame"
        );
    }

    /// A candidate with the plainest possible everything, for the key-ladder
    /// tests: one frame, no cull, a mid rating, an average-sized subject.
    fn plain(frame: i64) -> Candidate {
        Candidate {
            frame,
            row: frame,
            on_frame: false,
            by_hand: 0,
            culled: false,
            score: 0.80,
            sharpness: 0.80,
            quality: None,
            sides: 0,
            area: 0.20,
            band: RATING_BAND,
            bright: true,
            framing: None,
            eyes: None,
            smile: None,
            buck: None,
        }
    }

    fn everything() -> Ranking {
        Ranking {
            quality: true,
            framing: true,
            eyes: true,
            smile: true,
            buck: true,
        }
    }

    #[test]
    fn the_keeper_order_does_not_depend_on_the_order_the_rows_arrive_in() {
        let r = everything();
        // Framing 0.50, 0.56 and 0.63. Each neighbouring pair is inside the old
        // 0.10 pairwise margin, which made that comparison non-transitive: the
        // fold kept whichever frame it met first in the middle of the trio. As
        // buckets (0.10 wide) the top two are 5, 5, 6, so 0.63 is the best
        // candidate whatever order the rows come in.
        let candidates: Vec<Candidate> = [0.50, 0.56, 0.63]
            .into_iter()
            .enumerate()
            .map(|(i, framing)| Candidate {
                framing: Some(framing),
                ..plain(i as i64 + 1)
            })
            .collect();
        let fold = |order: &[&Candidate]| -> i64 {
            let mut held = order[0];
            for &candidate in &order[1..] {
                if candidate.beats(held, &r) {
                    held = candidate;
                }
            }
            held.frame
        };
        let [a, b, c] = [&candidates[0], &candidates[1], &candidates[2]];
        assert_eq!(fold(&[a, b, c]), c.frame);
        assert_eq!(fold(&[c, b, a]), c.frame);
        assert_eq!(fold(&[b, a, c]), c.frame, "the middle frame stays behind");
        assert_eq!(fold(&[b, c, a]), c.frame);
    }

    #[test]
    fn the_key_ladder_is_a_total_order() {
        let r = everything();
        let mut candidates = vec![
            plain(1),
            Candidate {
                score: 0.86,
                ..plain(2)
            },
            Candidate {
                framing: Some(0.20),
                ..plain(3)
            },
            Candidate {
                framing: Some(0.90),
                eyes: Some(0.90),
                smile: Some(0.90),
                ..plain(4)
            },
            Candidate {
                culled: true,
                score: 0.99,
                ..plain(5)
            },
            Candidate {
                sides: 2,
                area: 0.90,
                ..plain(6)
            },
            Candidate {
                by_hand: 3,
                ..plain(7)
            },
            Candidate {
                area: 0.21,
                sharpness: 0.60,
                ..plain(8)
            },
        ];
        // The exact rating a key is compared by: the quiet key override the
        // fold uses (both sides get the wider band).
        let key = |a: &Candidate, b: &Candidate| -> Key {
            let mut k = a.key(&r);
            k.rating_band = bucket(Some(a.score), a.band.max(b.band));
            k
        };
        // Totality and antisymmetry: no pair is both better and worse, and an
        // unequal pair always has exactly one direction.
        for a in &candidates {
            for b in &candidates {
                let ab = a.beats(b, &r);
                let ba = b.beats(a, &r);
                assert!(!(ab && ba), "{:?} beats both ways", (a.frame, b.frame));
                assert_eq!(ab, key(a, b) > key(b, a));
                assert_eq!(
                    ab || ba,
                    key(a, b) != key(b, a),
                    "an unequal pair must have a direction ({}, {})",
                    a.frame,
                    b.frame
                );
            }
        }
        // Transitivity: the fold can never depend on which frame it held first.
        for a in &candidates {
            for b in &candidates {
                for c in &candidates {
                    if a.beats(b, &r) && b.beats(c, &r) {
                        assert!(a.beats(c, &r), "{} > {} > {} but not >", a.frame, b.frame, c.frame);
                    }
                }
            }
        }
        // No key is dead: a stable sort of the set is the fold's answer.
        candidates.sort_by(|a, b| key(b, a).cmp(&key(a, b)));
    }

    #[test]
    fn within_the_rating_band_the_more_open_eyes_win() {
        let r = everything();
        let open = Candidate {
            eyes: Some(0.70),
            ..plain(2)
        };
        let narrow_eyed = Candidate {
            eyes: Some(0.30),
            ..plain(1)
        };
        assert!(
            open.beats(&narrow_eyed, &r) && !narrow_eyed.beats(&open, &r),
            "the more open eyes win a same-size, same-rating frame"
        );
        // A missing measurement is not evidence of open eyes: it ranks below a
        // measured low value, and equal to another missing one.
        let unmeasured = plain(3);
        assert!(narrow_eyed.beats(&unmeasured, &r));
        assert!(!unmeasured.beats(&narrow_eyed, &r));
        assert!(!plain(4).beats(&unmeasured, &r), "two unknowns tie");
    }

    #[test]
    fn a_rating_clearly_above_the_band_beats_a_bigger_subject() {
        let lib = Lib::new("pick-sharp");
        let (f1, f2) = (lib.frame("1.jpg", Some(1)), lib.frame("2.jpg", Some(1)));
        let sharp = lib.detection(f1, "vehicle", 0.90);
        let big = lib.detection(f2, "vehicle", 0.80);
        place(&lib, sharp, [2000.0, 1500.0, 3000.0, 2500.0]);
        place(&lib, big, [500.0, 500.0, 5000.0, 3500.0]);
        lib.run("pick_keepers", json!({"jobId": lib.job})).unwrap();
        assert_eq!(
            picks(&lib),
            vec![sharp],
            "a frame sharper than the band keeps the pick over a bigger subject"
        );
    }

    #[test]
    fn two_cars_in_one_burst_each_keep_a_frame_and_only_the_rated_kind_competes() {
        let lib = Lib::new("pick-groups");
        let (f1, f2) = (lib.frame("1.jpg", Some(1)), lib.frame("2.jpg", Some(1)));
        let (car_a1, car_b1) = (
            lib.detection(f1, "vehicle", 0.95),
            lib.detection(f1, "vehicle", 0.80),
        );
        let (car_a2, car_b2) = (
            lib.detection(f2, "vehicle", 0.86),
            lib.detection(f2, "vehicle", 0.90),
        );
        // A face in the frame does not compete with the cars under Motorsport.
        let face = lib.detection(f1, "face", 0.99);
        for (det, group) in [(car_a1, 1), (car_a2, 1), (car_b1, 2), (car_b2, 2)] {
            lib.sql(
                "UPDATE detections SET group_key=? WHERE id=?",
                params![group, det],
            );
        }
        lib.run("pick_keepers", json!({"jobId": lib.job})).unwrap();
        assert_eq!(picks(&lib), vec![car_a1, car_b2]);
        assert!(!picks(&lib).contains(&face));
    }

    fn look(lib: &Lib, det: i64, vector: [f32; 3]) {
        lib.sql(
            "UPDATE detections SET embedding=? WHERE id=?",
            params![similarity::pack(&vector), det],
        );
    }

    #[test]
    fn group_sorts_vehicles_into_cars_and_keeps_each_readers_own_answer() {
        let lib = Lib::new("group");
        let (f1, f2, f3) = (
            lib.frame("1.jpg", Some(1)),
            lib.frame("2.jpg", Some(1)),
            lib.frame("3.jpg", Some(1)),
        );
        let (a1, a2) = (
            lib.detection(f1, "vehicle", 0.9),
            lib.detection(f2, "vehicle", 0.8),
        );
        let other = lib.detection(f3, "vehicle", 0.82);
        look(&lib, a1, [1.0, 0.0, 0.0]);
        look(&lib, a2, [0.99, 0.14, 0.0]);
        look(&lib, other, [0.0, 1.0, 0.0]);
        lib.sql(
            "UPDATE detections SET attributes=? WHERE id=?",
            params![json!({"make": "Ford", "model": "Focus"}).to_string(), a1],
        );
        lib.sql(
            "UPDATE detections SET attributes=? WHERE id=?",
            params![
                json!({"make": "Ford", "model": "Focus", "team": "Falcon"}).to_string(),
                a2
            ],
        );
        // A hand-rejected vehicle is not grouped at all.
        let gone = lib.detection(f3, "vehicle", 0.4);
        lib.sql("UPDATE detections SET rejected=1 WHERE id=?", [gone]);

        let started = lib.run("group", json!({"jobId": lib.job})).unwrap();
        assert!(started["operation"]
            .as_str()
            .unwrap()
            .starts_with("Grouping vehicles"));
        lib.wait_idle();

        let key = |det: i64| -> Option<i64> {
            lib.one("SELECT group_key FROM detections WHERE id=?", [det])
        };
        assert!(
            key(a1).is_some() && key(a1) == key(a2),
            "one car in two frames"
        );
        assert_ne!(key(other), key(a1));
        assert_eq!(key(gone), None);
        let size: i64 = lib.one("SELECT group_size FROM detections WHERE id=?", [a1]);
        assert_eq!(size, 2);
        let raw: String = lib.one("SELECT attributes FROM detections WHERE id=?", [a1]);
        let attrs: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(attrs["group_make"], "Ford");
        assert_eq!(attrs["own_make"], "Ford");
        assert_eq!(
            attrs["team"], "Falcon",
            "a team one frame saw is shared by the group"
        );
        assert_eq!(lib.task("Grouping vehicles")["state"], "done");
        assert!(lib.task("Grouping vehicles")["detail"]
            .as_str()
            .unwrap()
            .contains("2 cars from 3 vehicles"));
        // Grouping re-picked: one keeper per car.
        assert_eq!(picks(&lib), vec![a1, other]);
    }

    #[test]
    fn group_keeps_apart_crops_alike_only_below_the_same_car_threshold() {
        // Job 39 of the real library: at 0.8 an earlier threshold merged cars that the recorded 0.90 keeps apart.
        let lib = Lib::new("group-threshold");
        let (f1, f2, f3) = (
            lib.frame("1.jpg", Some(1)),
            lib.frame("2.jpg", Some(1)),
            lib.frame("3.jpg", Some(1)),
        );
        let (a, b, c) = (
            lib.detection(f1, "vehicle", 0.9),
            lib.detection(f2, "vehicle", 0.8),
            lib.detection(f3, "vehicle", 0.7),
        );
        look(&lib, a, [1.0, 0.0, 0.0]);
        look(&lib, b, [0.85, 0.526_782_7, 0.0]); // cosine 0.85 with `a`: a different car
        look(&lib, c, [0.95, -0.312_249_9, 0.0]); // cosine 0.95 with `a` (and far from `b`): the same car
        lib.run("group", json!({"jobId": lib.job})).unwrap();
        lib.wait_idle();
        let key = |det: i64| -> Option<i64> {
            lib.one("SELECT group_key FROM detections WHERE id=?", [det])
        };
        assert_eq!(key(a), key(c), "0.95 alike is one car");
        assert_ne!(
            key(a),
            key(b),
            "0.85 alike is two cars at the recorded threshold of 0.90"
        );
    }

    #[test]
    fn group_says_so_when_the_album_has_not_been_looked_at() {
        let lib = Lib::new("group-blind");
        let f = lib.frame("1.jpg", Some(1));
        lib.detection(f, "vehicle", 0.9);
        lib.detection(f, "vehicle", 0.9);
        lib.run("group", json!({"jobId": lib.job})).unwrap();
        lib.wait_idle();
        let task = lib.task("Grouping vehicles");
        assert_eq!(task["state"], "failed");
        assert!(task["error"].as_str().unwrap().contains("Only 0 of 2"));
        assert!(lib.run("group", json!({"jobId": 4242})).is_err());
    }

    #[test]
    fn including_people_saves_the_flag_and_rates_a_frame_by_its_main_subject() {
        let lib = Lib::new("people");
        let frame = lib.frame("1.jpg", Some(1));
        let car = lib.detection(frame, "vehicle", 0.9);
        let person = lib.detection(frame, "person", 0.5);
        let face = lib.detection(frame, "face", 0.85);
        // Already prepared for naming, so no model or photo is needed.
        lib.sql("UPDATE detections SET embedding='x' WHERE id=?", [face]);
        // The person fills more of the frame than the car.
        lib.sql("UPDATE detections SET x2=1500 WHERE id=?", [car]);
        let include = |value: bool| {
            let out = lib
                .run(
                    "set_include_people",
                    json!({"jobId": lib.job, "value": value}),
                )
                .unwrap();
            lib.wait_idle();
            out
        };

        let on = include(true);
        assert_eq!(on["settings"]["include_people"], true);
        assert!(on["operation"]
            .as_str()
            .unwrap()
            .starts_with("Finding people"));
        assert_eq!(lib.task("Finding people")["state"], "done");
        assert!(settings(lib.d(), lib.job).unwrap().include_people);
        let jobs = lib.run("jobs", json!({})).unwrap();
        assert_eq!(jobs[0]["include_people"], true);
        assert_eq!(jobs[0]["scan_profile"], "motorsport");
        let rating: f64 = lib.one("SELECT rating FROM images WHERE id=?", [frame]);
        assert_eq!(rating, 0.85, "a person as main subject: the face decides");
        assert_eq!(picks(&lib).len(), 1);

        // The car as main subject: the vehicle decides again.
        lib.sql("UPDATE detections SET x2=5000 WHERE id=?", [car]);
        lib.run("pick_keepers", json!({"jobId": lib.job})).unwrap();
        assert_eq!(picks(&lib), vec![car]);

        let off = include(false);
        assert_eq!(off["settings"]["include_people"], false);
        assert!(off["operation"].is_null(), "turning off launches nothing");
        assert!(!settings(lib.d(), lib.job).unwrap().include_people);
        let kept: i64 = lib.one("SELECT count(*) FROM detections WHERE id=?", [person]);
        assert_eq!(kept, 1, "people are hidden, not deleted");
        assert!(lib
            .run("set_include_people", json!({"jobId": 4242, "value": true}))
            .is_err());
    }

    /// The frame-level keepers: frames with no subject to mark.
    fn frame_picks(lib: &Lib) -> Vec<i64> {
        let db = lib.db();
        let mut stmt = db
            .prepare("SELECT id FROM images WHERE burst_pick=1 ORDER BY id")
            .unwrap();
        let ids = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        ids
    }

    #[test]
    fn a_pass_of_frames_the_detector_found_nothing_in_keeps_its_best_rated_frame() {
        let lib = Lib::new("pick-blind");
        let (f1, f2, f3) = (
            lib.frame("1.jpg", Some(1)),
            lib.frame("2.jpg", Some(1)),
            lib.frame("3.jpg", Some(1)),
        );
        for (image, rating) in [(f1, 0.4), (f2, 0.9), (f3, 0.7)] {
            lib.sql(
                "UPDATE images SET rating=? WHERE id=?",
                params![rating, image],
            );
        }
        let out = lib
            .run("pick_keepers", json!({"jobId": lib.job}))
            .unwrap();
        assert_eq!(
            (out["passes"].as_i64(), out["picks"].as_i64()),
            (Some(1), Some(1)),
            "a pass without subjects still gets exactly one keeper"
        );
        assert_eq!(frame_picks(&lib), vec![f2], "the frame's own rating decides");
        assert!(picks(&lib).is_empty(), "there is no subject to mark");
        // The review grid carries that pick on the frame the UI already reads.
        let review = lib.run("review", json!({"jobId": lib.job})).unwrap();
        let picked: Vec<i64> = review["frames"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["burst_pick"].as_i64() == Some(1))
            .filter_map(|f| f["id"].as_i64())
            .collect();
        assert_eq!(picked, vec![f2]);
    }

    #[test]
    fn a_mixed_pass_keeps_the_rated_subject_grouped_or_not() {
        let lib = Lib::new("pick-mixed");
        let (f1, f2, f3) = (
            lib.frame("1.jpg", Some(1)),
            lib.frame("2.jpg", Some(1)),
            lib.frame("3.jpg", Some(1)),
        );
        let (a, c) = (
            lib.detection(f1, "vehicle", 0.8),
            lib.detection(f3, "vehicle", 0.5),
        );
        lib.sql("UPDATE images SET rating=0.9 WHERE id=?", [f2]);
        lib.run("pick_keepers", json!({"jobId": lib.job}))
            .unwrap();
        assert_eq!(
            picks(&lib),
            vec![a],
            "before grouping the whole burst is one unit, and the subject that clears the bar wins over the frame with none"
        );
        assert!(frame_picks(&lib).is_empty());
        // Grouping puts the subject-less frame in a unit of its own, and it
        // still gets no second keeper beside the car's.
        lib.sql(
            "UPDATE detections SET group_key=7 WHERE id IN(?,?)",
            params![a, c],
        );
        lib.run("pick_keepers", json!({"jobId": lib.job}))
            .unwrap();
        assert_eq!(picks(&lib), vec![a]);
        assert!(frame_picks(&lib).is_empty());
    }

    #[test]
    fn a_rated_subject_beats_a_sharper_looking_frame_with_no_subject() {
        let lib = Lib::new("pick-subject");
        let (f1, f2) = (lib.frame("1.jpg", Some(1)), lib.frame("2.jpg", Some(1)));
        let car = lib.detection(f1, "vehicle", 0.94);
        lib.sql("UPDATE images SET rating=1.0 WHERE id=?", [f2]);
        lib.run("pick_keepers", json!({"jobId": lib.job}))
            .unwrap();
        assert_eq!(
            picks(&lib),
            vec![car],
            "the detected subject wins although the missed frame measures sharper"
        );
        assert_eq!(
            frame_picks(&lib),
            Vec::<i64>::new(),
            "the frame with no subject is not a second keeper"
        );
    }

    #[test]
    fn a_frame_with_no_subject_still_wins_over_soft_subjects() {
        let lib = Lib::new("pick-soft-subject");
        let (f1, f2) = (lib.frame("1.jpg", Some(1)), lib.frame("2.jpg", Some(1)));
        let _soft = lib.detection(f1, "vehicle", 0.60);
        lib.sql("UPDATE images SET rating=0.95 WHERE id=?", [f2]);
        lib.run("pick_keepers", json!({"jobId": lib.job}))
            .unwrap();
        assert_eq!(frame_picks(&lib), vec![f2]);
        assert_eq!(
            picks(&lib),
            Vec::<i64>::new(),
            "a subject below the bar leaves the missed frame the keeper"
        );
    }

    #[test]
    fn a_low_hand_star_does_not_cancel_the_keeper() {
        let lib = Lib::new("pick-low-star");
        let (f1, f2) = (lib.frame("1.jpg", Some(1)), lib.frame("2.jpg", Some(1)));
        let soft = lib.detection(f1, "vehicle", 0.60);
        let sharp = lib.detection(f2, "vehicle", 0.95);
        lib.sql("UPDATE detections SET stars=2 WHERE id=?", [soft]);
        lib.run("pick_keepers", json!({"jobId": lib.job}))
            .unwrap();
        assert_eq!(
            picks(&lib),
            vec![sharp],
            "the starred-but-soft frame never hides the bar-clearing one"
        );
    }

    #[test]
    fn a_missed_frame_gets_no_second_pick_beside_a_grouped_car() {
        let lib = Lib::new("pick-missed-grouped");
        let (f1, f2) = (lib.frame("1.jpg", Some(1)), lib.frame("2.jpg", Some(1)));
        let car = lib.detection(f1, "vehicle", 0.94);
        lib.sql("UPDATE detections SET group_key=7 WHERE id=?", [car]);
        lib.sql("UPDATE images SET rating=1.0 WHERE id=?", [f2]);
        let out = lib
            .run("pick_keepers", json!({"jobId": lib.job}))
            .unwrap();
        assert_eq!(picks(&lib), vec![car], "the car's frame is the one keeper");
        assert_eq!(frame_picks(&lib), Vec::<i64>::new());
        assert_eq!(out["passes"].as_i64(), Some(1));
    }

    #[test]
    fn a_culled_frame_loses_to_an_unculled_one_and_a_pass_with_nothing_worth_keeping_picks_nothing() {
        let lib = Lib::new("pick-cull");
        let (f1, f2) = (lib.frame("1.jpg", Some(1)), lib.frame("2.jpg", Some(1)));
        let (f3, f4) = (lib.frame("3.jpg", Some(2)), lib.frame("4.jpg", Some(2)));
        let clean = lib.detection(f1, "vehicle", 0.80);
        let blurred = lib.detection(f2, "vehicle", 0.90);
        lib.sql(
            "UPDATE detections SET cull_reason='too blurred' WHERE id=?",
            [blurred],
        );
        // Nothing in the second pass can be kept: both are soft, and the hand
        // star is the only thing that can save one -- here it is absent.
        let closed = lib.detection(f3, "vehicle", 0.66);
        lib.sql(
            "UPDATE detections SET cull_reason='eyes closed' WHERE id=?",
            [closed],
        );
        let starred = lib.detection(f4, "vehicle", 0.66);
        lib.sql("UPDATE detections SET stars=1 WHERE id=?", [starred]);
        let out = lib
            .run("pick_keepers", json!({"jobId": lib.job}))
            .unwrap();
        assert_eq!(
            picks(&lib),
            vec![clean],
            "a cull loses to an unculled frame, and a soft pass has no keeper at all"
        );
        assert_eq!(out["unpicked"].as_i64(), Some(1));
    }

    #[test]
    fn a_pass_of_all_soft_frames_has_no_keeper_until_a_hand_star_says_otherwise() {
        let lib = Lib::new("pick-soft");
        let (f1, f2, f3) = (
            lib.frame("1.jpg", Some(1)),
            lib.frame("2.jpg", Some(1)),
            lib.frame("3.jpg", Some(1)),
        );
        // Three pans in one burst, all two stars: exactly the set that used to
        // get the least-bad frame proposed as its keeper.
        let a = lib.detection(f1, "vehicle", 0.66);
        let b = lib.detection(f2, "vehicle", 0.67);
        lib.detection(f3, "vehicle", 0.64);
        let out = lib
            .run("pick_keepers", json!({"jobId": lib.job}))
            .unwrap();
        assert_eq!(out["passes"].as_i64(), Some(1));
        assert_eq!(out["picks"].as_i64(), Some(0), "no keeper is the answer");
        assert_eq!(out["unpicked"].as_i64(), Some(1));
        assert_eq!(picks(&lib), Vec::<i64>::new());

        // A hand star of three is the photographer's own word and wins the pass.
        lib.sql("UPDATE detections SET stars=3 WHERE id=?", [a]);
        lib.run("pick_keepers", json!({"jobId": lib.job}))
            .unwrap();
        assert_eq!(picks(&lib), vec![a]);
        let _ = b;
    }

    #[test]
    fn a_dark_frame_loses_the_pass_except_where_the_preset_shoots_in_the_dark() {
        for (preset, dark_wins) in [("motorsport", false), ("concert", true)] {
            let lib = Lib::new(&format!("dark-{preset}"));
            lib.sql(
                "UPDATE jobs SET settings_json=? WHERE id=?",
                rusqlite::params![json!({"scan_profile": preset}).to_string(), lib.job],
            );
            let dim = lib.frame("1.jpg", Some(1));
            let lit = lib.frame("2.jpg", Some(1));
            let (dim_det, lit_det) = (
                lib.detection(dim, "vehicle", 0.90),
                lib.detection(lit, "vehicle", 0.90),
            );
            // Same rating and box: only the exposure tells them apart, and the
            // dark frame is the earlier one so a full tie would keep it.
            lib.sql(
                "UPDATE images SET dark_frame=1, exposure_verdict='dark' WHERE id=?",
                [dim],
            );
            lib.sql(
                "UPDATE images SET dark_frame=0, exposure_verdict='ok' WHERE id=?",
                [lit],
            );
            lib.sql("UPDATE detections SET sharpness=0.95 WHERE id=?", [dim_det]);
            lib.sql("UPDATE detections SET sharpness=0.90 WHERE id=?", [lit_det]);
            pick_keepers(lib.d(), lib.job).unwrap();
            assert_eq!(
                picks(&lib),
                vec![if dark_wins { dim_det } else { lit_det }],
                "{preset}"
            );
        }
    }

    #[test]
    fn a_portrait_keeps_the_frame_whose_eyes_are_open_and_sharper() {
        let lib = Lib::new("pick-eyes");
        let (f1, f2) = (lib.frame("1.jpg", Some(1)), lib.frame("2.jpg", Some(1)));
        let blink = lib.detection(f1, "eye", 0.98);
        let shut = lib.detection(f1, "face", 0.90);
        let open = lib.detection(f2, "eye", 0.94);
        lib.sql(
            "UPDATE detections SET cull_reason='eyes closed', x1=800, y1=800, x2=3200, y2=2400 WHERE id=?",
            [shut],
        );
        // The frames' own ratings, as a portrait scan stores them (the softer
        // eye of the face).
        lib.sql("UPDATE images SET rating=0.98 WHERE id=?", [f1]);
        lib.sql("UPDATE images SET rating=0.94 WHERE id=?", [f2]);
        lib.sql(
            "UPDATE jobs SET settings_json=? WHERE id=?",
            params![json!({"scan_profile": "portrait"}).to_string(), lib.job],
        );
        lib.run("pick_keepers", json!({"jobId": lib.job}))
            .unwrap();
        assert_eq!(
            picks(&lib),
            vec![open],
            "a shut eye never wins a pass, however sharp its lid"
        );
        // Both open: the sharper portrait wins although the other's box is
        // larger and the two sit inside the coarse vehicle band.
        lib.sql("UPDATE detections SET cull_reason='' WHERE id=?", [shut]);
        lib.sql(
            "UPDATE detections SET x2=5000, y2=4000 WHERE id=?",
            [open],
        );
        lib.run("pick_keepers", json!({"jobId": lib.job}))
            .unwrap();
        assert_eq!(
            picks(&lib),
            vec![blink],
            "a rating gap the vehicle band would swallow still decides a face"
        );
    }

    #[test]
    fn faces_of_one_person_share_a_group_and_older_vectors_are_left_out() {
        let lib = Lib::new("people-sface");
        let toward = |i: usize, j: usize, lean: f32| {
            let mut v = vec![0f32; IDENTITY_DIM];
            v[i] = 1.0;
            v[j] += lean;
            let n = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            v.into_iter().map(|x| x / n).collect::<Vec<_>>()
        };
        let frame = lib.frame("a.cr3", Some(1));
        let face = |v: &[f32]| {
            let id = lib.detection(frame, "face", 0.9);
            lib.sql(
                "UPDATE detections SET embedding=? WHERE id=?",
                params![similarity::pack(v), id],
            );
            id
        };
        let alex = face(&toward(0, 1, 0.0));
        let alex_again = face(&toward(0, 1, 0.5));
        let sam = face(&toward(5, 6, 0.0));
        let dino = face(&[0.5; 384]);
        cluster_people(lib.d(), lib.job).unwrap();
        let key = |id: i64| -> Option<i64> {
            lib.one("SELECT group_key FROM detections WHERE id=?", [id])
        };
        assert_eq!(key(alex), Some(alex));
        assert_eq!(key(alex_again), Some(alex));
        assert_eq!(key(sam), Some(sam));
        assert_eq!(key(dino), None, "a DINOv2 vector waits to be re-embedded");
        let size: i64 = lib.one("SELECT group_size FROM detections WHERE id=?", [alex_again]);
        assert_eq!(size, 2);
        // Shut eyes: every judged eye below one half, and at least one judged.
        assert!(crate::eyes_closed([Some(0.1), None]));
        assert!(!crate::eyes_closed([Some(0.1), Some(0.9)]));
        assert!(!crate::eyes_closed([None, None]));
    }
}
