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
}

impl Candidate {
    /// In the order the keeper is decided: a hand star first, then a frame no
    /// rule culled, then the rating in `RATING_BAND`s, then the subject that
    /// the frame edge cuts off least, then the larger subject, then the exact
    /// rating, then (opt-in) NIMA quality, then the stored subject sharpness,
    /// and finally the earlier frame.
    fn beats(&self, other: &Self, use_quality_score: bool) -> bool {
        if self.by_hand != other.by_hand {
            return self.by_hand > other.by_hand;
        }
        if self.culled != other.culled {
            return !self.culled;
        }
        let band = (self.score / RATING_BAND).floor();
        let other_band = (other.score / RATING_BAND).floor();
        if band != other_band {
            return band > other_band;
        }
        if self.sides != other.sides {
            return self.sides < other.sides;
        }
        if self.area != other.area {
            return self.area > other.area;
        }
        if self.score != other.score {
            return self.score > other.score;
        }
        if use_quality_score {
            if let (Some(a), Some(b)) = (self.quality, other.quality) {
                if a != b {
                    return a > b;
                }
            }
        }
        if self.sharpness != other.sharpness {
            return self.sharpness > other.sharpness;
        }
        self.frame < other.frame
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
/// candidate of its unit is culled.
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
        "SELECT d.id, d.x1, d.y1, d.x2, d.y2, d.rating, d.sharpness, d.quality, d.group_key, COALESCE(d.stars, i.rating_in_file, 0) AS by_hand, COALESCE(d.cull_reason,'')<>'' AS culled, i.id AS image_id, i.burst_key, COALESCE(d.region_type,'vehicle') AS region, i.width, i.height FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? AND COALESCE(d.rejected,0)=0 AND COALESCE(d.bystander,0)=0 AND COALESCE(i.rejected,0)=0 AND i.burst_key IS NOT NULL ORDER BY i.id, d.id",
        [job],
    )?;
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
        let Some(score) = r["rating"].as_f64().or_else(|| r["sharpness"].as_f64()) else {
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
        units.push((
            unit,
            Candidate {
                frame: image,
                row: r["id"].as_i64().unwrap_or_default(),
                on_frame: false,
                by_hand: r["by_hand"].as_i64().unwrap_or(0),
                culled: r["culled"].as_i64().unwrap_or(0) != 0,
                score,
                sharpness: r["sharpness"].as_f64().unwrap_or(score),
                quality: r["quality"].as_f64(),
                sides: framing::assess(bbox, width, height).sides,
                area,
            },
        ));
        marked.insert(image);
    }
    // A frame with no competing subject: it is its own candidate, in the
    // burst's ungrouped unit, rated by the frame.
    let spare = rows(
        conn,
        "SELECT id,burst_key,COALESCE(stars,rating_in_file,0) AS by_hand,rating,sharpness FROM images WHERE job_id=? AND burst_key IS NOT NULL AND COALESCE(rejected,0)=0 ORDER BY id",
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
        units.push((
            (r["burst_key"].as_i64().unwrap_or_default(), None),
            Candidate {
                frame: image,
                row: image,
                on_frame: true,
                by_hand: r["by_hand"].as_i64().unwrap_or(0),
                culled: false,
                score,
                sharpness: r["sharpness"].as_f64().unwrap_or(score),
                quality: None,
                sides: 0,
                area: 0.0,
            },
        ));
    }
    // Earlier frame first, so an exact tie keeps it, as a re-run always has.
    units.sort_by_key(|(_, c)| (c.frame, c.row));
    let mut best: HashMap<(i64, Option<i64>), Candidate> = HashMap::new();
    for (unit, candidate) in units {
        if best
            .get(&unit)
            .is_none_or(|held| candidate.beats(held, use_quality_score))
        {
            best.insert(unit, candidate);
        }
    }
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
        "passes": best.len(),
        "picks": picks.len() + frame_picks.len(),
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
        "SELECT d.image_id, i.path, d.x1, d.y1, d.x2, d.y2 FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? AND COALESCE(i.rejected,0)=0 AND d.region_type='person' AND NOT EXISTS (SELECT 1 FROM detections f WHERE f.image_id=i.id AND f.region_type IN ('face','eye')) ORDER BY i.id, d.id",
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
        let region_models: HashMap<String, _> = ["face", "eye"]
            .into_iter()
            .filter_map(|r| crate::load_region_model(r).map(|m| (r.to_string(), m)))
            .collect();
        let total = frames.len() as u64;
        for (n, (image, (path, boxes))) in frames.iter().enumerate() {
            if stop.load(Ordering::Relaxed) {
                return Ok(());
            }
            task.detail(format!("Looking for faces · {path}"));
            let raw = conrod_io::raw::read(std::path::Path::new(path))?;
            let rgb = Rgb::decode_jpeg(&raw.preview, 1)?.orient(raw.orientation);
            let found =
                crate::face_subjects(&rgb, boxes.iter().copied(), &detector, cfg, &region_models)?;
            let db = lock(&d.db);
            let tx = db.unchecked_transaction().map_err(err)?;
            for s in &found {
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
        &format!("SELECT d.id, d.crop_path FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? AND {ML_ELIGIBLE} AND d.crop_path IS NOT NULL AND (d.embedding IS NULL OR d.embedding='') AND COALESCE(d.region_type,'vehicle')='vehicle' ORDER BY d.id"),
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
        "SELECT id, thumb_path FROM images WHERE job_id=? AND phash IS NULL AND thumb_path IS NOT NULL",
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
        "SELECT id, phash, burst_key, taken_at FROM images WHERE job_id=? AND phash IS NOT NULL AND status='done' AND rejected=0",
        [job],
    )?;
    let looks = rows(
        &lock(&d.reader),
        "SELECT d.image_id, d.embedding FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? AND COALESCE(d.region_type,'vehicle')='vehicle' AND d.embedding IS NOT NULL AND d.embedding!=''",
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
        &format!("SELECT d.id,d.x1,d.y1,d.x2,d.y2,i.path,d.embedding FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? AND {ML_ELIGIBLE} AND COALESCE(d.region_type,'')='face' ORDER BY i.id,d.id"),
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
            "Matching face · {}",
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
        "SELECT d.id,d.embedding FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=? AND COALESCE(d.region_type,'')='face' AND COALESCE(d.rejected,0)=0 ORDER BY i.id,d.id",
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
         WHERE i.job_id=? AND {ML_ELIGIBLE} AND COALESCE(d.region_type,'vehicle')='vehicle' \
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
            lib.detection(f1, "vehicle", 0.60),
            lib.detection(f2, "vehicle", 0.80),
            lib.detection(f3, "vehicle", 0.80),
        );
        let (d1, d2) = (
            lib.detection(g1, "vehicle", 0.70),
            lib.detection(g2, "vehicle", 0.55),
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
            lib.detection(f1, "vehicle", 0.9),
            lib.detection(f1, "vehicle", 0.5),
        );
        let (car_a2, car_b2) = (
            lib.detection(f2, "vehicle", 0.7),
            lib.detection(f2, "vehicle", 0.6),
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
        let other = lib.detection(f3, "vehicle", 0.7);
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
        let face = lib.detection(frame, "face", 0.3);
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
        assert_eq!(rating, 0.3, "a person as main subject: the face decides");
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
    fn a_mixed_pass_keeps_one_frame_per_unit_and_never_none() {
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
            frame_picks(&lib),
            vec![f2],
            "before grouping the whole burst is one unit, and the best rating wins"
        );
        assert!(picks(&lib).is_empty());
        // Grouping makes the subject-less frame a unit of its own: one pick
        // each, neither pass nor frame left without a keeper.
        lib.sql(
            "UPDATE detections SET group_key=7 WHERE id IN(?,?)",
            params![a, c],
        );
        lib.run("pick_keepers", json!({"jobId": lib.job}))
            .unwrap();
        assert_eq!(picks(&lib), vec![a]);
        assert_eq!(frame_picks(&lib), vec![f2]);
    }

    #[test]
    fn a_culled_frame_loses_to_an_unculled_one_and_an_all_culled_pass_still_keeps_one() {
        let lib = Lib::new("pick-cull");
        let (f1, f2) = (lib.frame("1.jpg", Some(1)), lib.frame("2.jpg", Some(1)));
        let (f3, f4) = (lib.frame("3.jpg", Some(2)), lib.frame("4.jpg", Some(2)));
        let clean = lib.detection(f1, "vehicle", 0.4);
        let blurred = lib.detection(f2, "vehicle", 0.9);
        lib.sql(
            "UPDATE detections SET cull_reason='too blurred' WHERE id=?",
            [blurred],
        );
        let closed = lib.detection(f3, "vehicle", 0.7);
        lib.sql(
            "UPDATE detections SET cull_reason='eyes closed' WHERE id=?",
            [closed],
        );
        let starred = lib.detection(f4, "vehicle", 0.2);
        lib.sql(
            "UPDATE detections SET cull_reason='1 star', stars=1 WHERE id=?",
            [starred],
        );
        lib.run("pick_keepers", json!({"jobId": lib.job}))
            .unwrap();
        assert_eq!(
            picks(&lib),
            vec![clean, starred],
            "a cull only wins when every candidate of its pass is culled, and a hand star still leads"
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
