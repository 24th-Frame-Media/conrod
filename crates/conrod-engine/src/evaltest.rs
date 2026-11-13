//! Export recall: of the frames the photographer chose and delivered (a human
//! sharp keeper), how many does the app's own rating, cull and burst pick keep?
//!
//! `CONROD_EVAL_SHOOT="2026-10-08_Thursday" cargo test -p conrod-engine --release export_recall -- --ignored --nocapture`
//!
//! Env: `CONROD_EVAL_LABELS` (labels.json), `CONROD_EVAL_SHOOT` (required
//! substring of the shoot folder), `CONROD_EVAL_NEIGHBOURS` (12), `CONROD_EVAL_MAX`
//! (40 exported frames), `CONROD_EVAL_ONLY` (comma-separated file-name
//! substrings; when set, only exported frames matching one are scanned).
//!
//! `subjects.csv` has one row per detected subject, and `frames.csv` gained a
//! `raw_detections` column: every class the detector produced before the
//! profile's class list and confidence cut it down.
//!
//! Reads nothing but file names from the shoot folder -- the frames themselves
//! are opened read-only by `raw::read`. Every write (DB, CSV) goes under
//! `target/eval/<shoot-slug>/`, and the harness asserts that before it starts
//! and that the shoot folder is unchanged afterwards.
//!
//! Per frame it calls the app's own `cull_frame`, stores the result the way
//! `desktop::persist_frame` does (without the thumbnail), then runs the same
//! burst grouping `desktop::regroup` runs and the same keeper pick
//! (`passes::pick_of_pass`). No scoring is re-implemented here.

use super::*;
use crate::desktop::Desktop;
use conrod_core::profile::ShootPreset;
use serde_json::json;
use std::collections::HashSet;
use std::time::Instant;

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

fn slug(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// Names, sizes and mtimes of everything in `dir`: the same list before and
/// after a run means nothing was written beside the photographer's originals.
fn snapshot(dir: &Path) -> Vec<(String, u64, u128)> {
    let mut out: Vec<(String, u64, u128)> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| {
                    let meta = e.metadata().ok();
                    let mtime = meta
                        .as_ref()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map_or(0, |d| d.as_nanos());
                    (
                        e.file_name().to_string_lossy().into_owned(),
                        meta.map_or(0, |m| m.len()),
                        mtime,
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// Store one measured frame as `desktop::persist_frame` does, minus the
/// thumbnail (which the evaluation never shows).
fn persist(db: &rusqlite::Connection, job: i64, f: &FrameResult) -> Result<(), String> {
    let image: i64 = db
        .query_row(
            "SELECT id FROM images WHERE job_id=? AND path=?",
            rusqlite::params![job, f.path.to_string_lossy()],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let tx = db.unchecked_transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM detections WHERE image_id=?", [image])
        .map_err(|e| e.to_string())?;
    for s in &f.subjects {
        crate::desktop::insert_subject(&tx, image, s)?;
    }
    tx.execute(
        "UPDATE images SET status='done',error=NULL,width=?,height=?,camera=?,taken_at=?,sharpness=?,rating=? WHERE id=?",
        rusqlite::params![
            f.size.0 as i64,
            f.size.1 as i64,
            f.camera,
            f.taken,
            f.whole,
            f.rating,
            image
        ],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

/// One scanned frame, as printed to `frames.csv`.
struct Rec {
    path: String,
    rating: f64,
    whole: f64,
    stars: u8,
    verdict: &'static str,
    panning: bool,
    cull: String,
    subjects: usize,
    secs: f64,
    raw: String,
    top_raw: String,
}

/// One detected subject, as printed to `subjects.csv`. `Subject` keeps only
/// the score, so the rest of the `Sharpness` is re-measured from the same
/// decoded frame with the same crop and model (see `remeasure`).
struct Subj {
    file: String,
    exported: bool,
    index: usize,
    class: &'static str,
    region: &'static str,
    conf: f32,
    bbox: [f64; 4],
    frame: (usize, usize),
    fraction: f64,
    sharpness: f64,
    learned: bool,
    measured: bool,
    uncertain: bool,
    background: f64,
    panning: bool,
    partly: bool,
    bands: Vec<f64>,
    factor: f64,
    sides: u32,
    rating: f64,
    stars: u8,
    cull: String,
    setter: bool,
}

/// Every class the network produced on the frame, before the profile's class
/// list and the confidence threshold in `detect::decode` drop any of them.
/// `Detector::raw` runs the network alone (`detect.rs:161`); asking for all
/// 80 COCO classes at a floor of 0.01 is what production never does.
fn raw_listing(
    path: &Path,
    detector: &Mutex<Detector>,
) -> Result<(Rgb, Vec<(usize, f32, [f64; 4])>), String> {
    let frame = raw::read(path)?;
    let image = Rgb::decode_jpeg(&frame.preview, 1)?.orient(frame.orientation);
    let mut opts = DetectOptions::default();
    opts.classes = (0..80).collect();
    opts.conf = 0.01;
    let found = lock(detector).raw(&image, &opts)?;
    Ok((image, found))
}

fn raw_text(found: &[(usize, f32, [f64; 4])], image: &Rgb) -> String {
    let area = (image.width.max(1) * image.height.max(1)) as f64;
    found
        .iter()
        .map(|(class, conf, bbox)| {
            let box_area = ((bbox[2] - bbox[0]).max(0.0) * (bbox[3] - bbox[1]).max(0.0)) / area;
            format!(
                "{}#{}:{:.3}@{:.4}",
                detect::class_name(*class),
                class,
                conf,
                box_area
            )
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// `cull_frame`'s own crop and measure for one detector box (`lib.rs:380-394`),
/// so the fields `Subject` drops (background, bands, measured, uncertain) are
/// the production values and not a re-derivation.
fn measure_box(
    image: &Rgb,
    bbox: [f64; 4],
    region: &str,
    settings: &Settings,
    profile: ScanProfile,
    models: &std::collections::HashMap<String, conrod_core::ridge::SharpModel>,
) -> Option<sharpness::Sharpness> {
    if region != "person" && region != "vehicle" {
        return None;
    }
    let opts = options(settings, profile);
    let crop_box = detect::expand_box(bbox, image.width, image.height, &opts);
    if crop_box[2] <= crop_box[0] || crop_box[3] <= crop_box[1] {
        return None;
    }
    let crop = cut(image, crop_box, settings);
    let scale = crop.width as f64 / (crop_box[2] - crop_box[0]);
    let inner = [
        (bbox[0] - crop_box[0]) * scale,
        (bbox[1] - crop_box[1]) * scale,
        (bbox[2] - crop_box[0]) * scale,
        (bbox[3] - crop_box[1]) * scale,
    ];
    let mut focus = sharpness::measure(&crop.to_gray(), Some(inner), models.get(region));
    if !ShootPreset::parse(&settings.scan_profile).pan_compatible() {
        focus.panning = false;
    }
    Some(focus)
}

fn remeasure(
    image: &Rgb,
    subjects: &[Subject],
    settings: &Settings,
    profile: ScanProfile,
    models: &std::collections::HashMap<String, conrod_core::ridge::SharpModel>,
) -> Vec<Option<sharpness::Sharpness>> {
    subjects
        .iter()
        .map(|s| measure_box(image, s.bbox, s.region_type, settings, profile, models))
        .collect()
}

/// The index of the subject whose rating became the frame's rating, decided
/// the way `frame_rating` decides it (`lib.rs:325-355`).
fn frame_setter(profile: ScanProfile, include_people: bool, subjects: &[Subject]) -> Option<usize> {
    use conrod_core::profile::{main_subject, Subject as Kind};
    let kind_of = |region: &str| match region {
        "person" => Some(Kind::Person),
        "vehicle" => Some(Kind::Vehicle),
        _ => None,
    };
    let main = main_subject(subjects.iter().filter_map(|s| {
        kind_of(s.region_type)
            .map(|k| (k, (s.bbox[2] - s.bbox[0]) * (s.bbox[3] - s.bbox[1])))
    }));
    let region = profile
        .frame_priority(include_people, main)
        .iter()
        .find_map(|kind| {
            if *kind == Kind::WholeFrame {
                return None;
            }
            let name = crate::passes::region_name(*kind);
            subjects.iter().any(|s| s.region_type == name).then_some(name)
        })?;
    subjects
        .iter()
        .enumerate()
        .filter(|(_, s)| s.region_type == region)
        .max_by(|a, b| a.1.rating.total_cmp(&b.1.rating))
        .map(|(i, _)| i)
}

fn band(bands: &[f64], i: usize) -> String {
    bands.get(i).map(|v| format!("{v:.4}")).unwrap_or_default()
}

#[test]
#[ignore]
fn export_recall() {
    let labels_path = std::env::var("CONROD_EVAL_LABELS").unwrap_or_else(|_| {
        String::from(r"E:\code\photo\conrod\target\groundtruth\labels.json")
    });
    let filter = std::env::var("CONROD_EVAL_SHOOT").expect("CONROD_EVAL_SHOOT");
    let neighbours = env_usize("CONROD_EVAL_NEIGHBOURS", 12);
    let max = env_usize("CONROD_EVAL_MAX", 40);
    // Filename substrings: when set, only exported frames that match are run.
    let only: Vec<String> = std::env::var("CONROD_EVAL_ONLY")
        .map(|v| {
            v.split(',')
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let labels: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&labels_path).expect("labels.json"))
            .expect("labels.json is JSON");
    let frames = labels["frames"].as_array().expect("frames");

    // The exported frames of the matching shoot(s): human-confirmed keepers.
    let exported: Vec<(String, PathBuf)> = frames
        .iter()
        .filter(|f| f["exported"].as_bool() == Some(true))
        .filter_map(|f| Some((f["shoot"].as_str()?.to_string(), PathBuf::from(f["path"].as_str()?))))
        .filter(|(shoot, _)| shoot.contains(&filter))
        .filter(|(_, path)| {
            only.is_empty()
                || only.iter().any(|needle| {
                    path.file_name()
                        .map(|n| n.to_string_lossy().to_lowercase().contains(needle))
                        .unwrap_or(false)
                })
        })
        .take(max)
        .collect();
    assert!(
        !exported.is_empty(),
        "no exported frame in {labels_path} has a shoot containing {filter:?}"
    );

    // Each exported frame plus its burst-mates: the NEIGHBOURS files before and
    // after it in filename order, within the same folder.
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (_, frame) in &exported {
        let dir = frame.parent().expect("a frame has a parent folder");
        let name = frame
            .file_name()
            .expect("a frame has a file name")
            .to_string_lossy()
            .into_owned();
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("list {}: {e}", dir.display()))
            .flatten()
            .filter_map(|e| {
                let ext = e
                    .path()
                    .extension()
                    .and_then(|x| x.to_str())
                    .map(|x| x.to_ascii_lowercase());
                matches!(ext.as_deref(), Some("cr3") | Some("dng"))
                    .then(|| e.file_name().to_string_lossy().into_owned())
            })
            .collect();
        names.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
        let at = names
            .iter()
            .position(|n| n.eq_ignore_ascii_case(&name))
            .unwrap_or_else(|| panic!("{name} is not in {}", dir.display()));
        let lo = at.saturating_sub(neighbours);
        let hi = (at + neighbours + 1).min(names.len());
        for n in &names[lo..hi] {
            let full = dir.join(n);
            if seen.insert(full.to_string_lossy().to_lowercase()) {
                paths.push(full);
            }
        }
    }
    assert!(
        paths.iter().all(|p| p.is_absolute()),
        "every frame must be an absolute path"
    );

    let shoot = exported[0].0.clone();
    let out_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/eval")
        .join(slug(&shoot));
    let target = out_dir.to_string_lossy().replace('\\', "/").to_lowercase();
    // The guard: nothing may be written outside the repo's target folder.
    assert!(
        target.contains("/target/eval/"),
        "refusing to write to {}",
        out_dir.display()
    );

    // Nothing beside the originals may change: remember what is there now.
    let mut watched: Vec<PathBuf> = paths
        .iter()
        .filter_map(|p| p.parent().map(Path::to_path_buf))
        .collect();
    watched.sort();
    watched.dedup();
    let before: Vec<(PathBuf, Vec<(String, u64, u128)>)> =
        watched.iter().map(|d| (d.clone(), snapshot(d))).collect();

    let _ = std::fs::remove_dir_all(&out_dir);
    let desktop = Desktop::open(out_dir.clone()).expect("open the evaluation library");

    let settings = Settings::default();
    let profile = ScanProfile::parse(&settings.scan_profile);
    let job = {
        let db = lock(&desktop.db);
        let job = conrod_store::create_job(
            &db,
            Path::new(&shoot),
            Some(&slug(&shoot)),
            &json!(settings),
        )
        .expect("create the album");
        conrod_store::add_images(&db, job, &paths).expect("add the frames");
        job
    };

    // The model setup `scan_files_filtered` does, so the per-frame work is the
    // app's own (lib.rs:554-607).
    let hub = desktop.hub.clone();
    let stop = AtomicBool::new(false);
    let preset = ShootPreset::parse(&settings.scan_profile);
    let mut needs = crate::setup::SCAN.to_vec();
    if preset.wants_faces(settings.include_people) {
        needs.extend(crate::setup::FACES);
    }
    crate::setup::ensure(&hub, &stop, &needs).expect("install the models the app installs");
    let detector = Mutex::new(
        Detector::load(&detector_model(), Device::Auto).expect("load the detector"),
    );
    let device = lock(&detector).device;
    let faces = preset
        .wants_faces(settings.include_people)
        .then(|| Mutex::new(load_faces().expect("load the face detector")));
    let models: std::collections::HashMap<String, conrod_core::ridge::SharpModel> =
        ["vehicle", "person", "face", "eye"]
            .into_iter()
            .filter_map(|region| load_region_model(region).map(|m| (region.to_string(), m)))
            .collect();
    let quality = settings
        .use_quality_score
        .then(|| {
            let path = conrod_core::models::expected(conrod_core::models::QUALITY);
            path.is_file()
                .then(|| conrod_vision::quality::QualityModel::load(&path, true).ok())
                .flatten()
                .map(Mutex::new)
        })
        .flatten();
    println!(
        "device {device}; detector {}; faces {}; learned sharpness models {:?}; quality {}",
        crate::detector_model().display(),
        faces.is_some(),
        models.keys().collect::<Vec<_>>(),
        quality.is_some()
    );

    let mut records: Vec<Rec> = Vec::new();
    let mut subs: Vec<Subj> = Vec::new();
    let exported_names: HashSet<String> = exported
        .iter()
        .map(|(_, p)| p.to_string_lossy().to_lowercase())
        .collect();
    let mut mismatched = 0;
    let mut total = std::time::Duration::ZERO;
    let mut failures = 0;
    for path in &paths {
        let started = Instant::now();
        let result = cull_frame(
            path,
            &detector,
            &settings,
            profile,
            &models,
            faces.as_ref(),
            quality.as_ref(),
        );
        let secs = started.elapsed().as_secs_f64();
        total += started.elapsed();
        match result {
            Ok(f) => {
                if let Err(e) = persist(&lock(&desktop.db), job, &f) {
                    println!("could not store {}: {e}", path.display());
                    failures += 1;
                    continue;
                }
                // The frame's own verdict, from its rating; and the reason its
                // best subject would be culled (that subject decides the frame).
                let verdict =
                    sharpness::verdict_for(f.rating, settings.sharp_at, settings.blurred_below);
                let best = f.subjects.iter().max_by(|a, b| a.rating.total_cmp(&b.rating));
                let exported = exported_names.contains(&path.to_string_lossy().to_lowercase());
                // The network's own answer, and the rest of each subject's
                // measurement, from one extra decode of the same preview.
                let (image, raw_found) = match raw_listing(path, &detector) {
                    Ok((image, found)) => (Some(image), found),
                    Err(e) => {
                        println!("could not list detections for {}: {e}", path.display());
                        (None, Vec::new())
                    }
                };
                let raw = match &image {
                    Some(image) => raw_text(&raw_found, image),
                    None => String::new(),
                };
                let focus = image
                    .as_ref()
                    .map(|image| remeasure(image, &f.subjects, &settings, profile, &models));
                // No subject to judge: what the fallback would have said had the
                // network's best box been allowed through the profile filter.
                let top_raw = match (&image, f.subjects.is_empty()) {
                    (Some(image), true) => raw_found
                        .iter()
                        .max_by(|a, b| a.1.total_cmp(&b.1))
                        .and_then(|(class, conf, bbox)| {
                            let region = if *class == detect::PERSON
                                || detect::PETS.contains(class)
                            {
                                "person"
                            } else {
                                "vehicle"
                            };
                            measure_box(image, *bbox, region, &settings, profile, &models)
                                .map(|m| format!("{}#{class}:{conf:.3}={:.4}", detect::class_name(*class), m.score))
                        }),
                    _ => None,
                };
                if let Some(focus) = &focus {
                    for (s, m) in f.subjects.iter().zip(focus) {
                        if let Some(m) = m {
                            if (m.score - s.sharpness).abs() > 1e-9 {
                                mismatched += 1;
                            }
                        }
                    }
                }
                let setter = frame_setter(profile, settings.include_people, &f.subjects);
                if let Some(i) = setter {
                    if (f.subjects[i].rating - f.rating).abs() > 1e-9 {
                        println!(
                            "note {}: subject {i} rates {:.4} but the frame rates {:.4}",
                            path.display(),
                            f.subjects[i].rating,
                            f.rating
                        );
                    }
                }
                for (i, s) in f.subjects.iter().enumerate() {
                    let edges = framing::assess(
                        Some(s.bbox),
                        f.size.0 as i64,
                        f.size.1 as i64,
                    );
                    let m = focus.as_ref().and_then(|v| v.get(i).cloned().flatten());
                    let area = ((s.bbox[2] - s.bbox[0]).max(0.0)
                        * (s.bbox[3] - s.bbox[1]).max(0.0))
                        / (f.size.0.max(1) as f64 * f.size.1.max(1) as f64);
                    subs.push(Subj {
                        file: path.to_string_lossy().into_owned(),
                        exported,
                        index: i,
                        class: s.class,
                        region: s.region_type,
                        conf: s.conf,
                        bbox: s.bbox,
                        frame: f.size,
                        fraction: area,
                        sharpness: s.sharpness,
                        learned: m.as_ref().map_or(s.heuristic != s.sharpness, |m| m.learned),
                        measured: m.as_ref().map_or(!s.features.is_empty(), |m| m.measured),
                        uncertain: m.as_ref().map_or(s.features.is_empty(), |m| m.uncertain),
                        background: m.as_ref().map_or(-1.0, |m| m.background),
                        panning: m.as_ref().map_or(s.panning, |m| m.panning),
                        partly: m.as_ref().is_some_and(|m| m.partly_sharp()),
                        bands: m.as_ref().map(|m| m.bands.clone()).unwrap_or_default(),
                        factor: edges.factor,
                        sides: edges.sides,
                        rating: s.rating,
                        stars: s.stars,
                        cull: s.cull_reason.clone(),
                        setter: setter == Some(i),
                    });
                }
                records.push(Rec {
                    path: path.to_string_lossy().into_owned(),
                    rating: f.rating,
                    whole: f.whole.unwrap_or(f64::NAN),
                    stars: f.stars,
                    verdict,
                    panning: f.subjects.iter().any(|s| s.panning),
                    cull: best.map(|s| s.cull_reason.clone()).unwrap_or_default(),
                    subjects: f.subjects.len(),
                    secs,
                    raw,
                    top_raw: top_raw.unwrap_or_default(),
                });
            }
            Err(e) => {
                failures += 1;
                println!("could not read {}: {e}", path.display());
                let _ = lock(&desktop.db).execute(
                    "UPDATE images SET status='error',error=? WHERE job_id=? AND path=?",
                    rusqlite::params![e, job, path.to_string_lossy()],
                );
            }
        }
    }

    // The app's own post-scan grouping and keeper pick (`desktop::regroup`,
    // which calls `passes::pick_of_pass`).
    desktop
        .regroup(job, settings.burst_gap)
        .expect("group the bursts and pick the keepers");

    // The frame's pick the way the review query reads it (`desktop.rs:389`).
    let rows = crate::desktop::rows(
        &lock(&desktop.db),
        "SELECT i.id, i.path, i.burst_key, i.rating, i.sharpness, \
         (SELECT count(*) FROM images b WHERE b.job_id=i.job_id AND b.burst_key=i.burst_key) AS burst_size, \
         COALESCE((SELECT max(d.burst_pick) FROM detections d WHERE d.image_id=i.id AND d.burst_pick=1), i.burst_pick, 0) AS pick \
         FROM images i WHERE i.job_id=? AND i.status='done' ORDER BY i.path",
        [job],
    )
    .expect("read the evaluated frames");

    let picked_bursts: HashSet<i64> = rows
        .iter()
        .filter(|r| r["pick"].as_i64() == Some(1))
        .filter_map(|r| r["burst_key"].as_i64())
        .collect();
    let by_path: std::collections::HashMap<String, &Rec> = records
        .iter()
        .map(|r| (r.path.to_lowercase(), r))
        .collect();

    let mut csv = String::from(
        "file,exported,rating,score,stars,verdict,panning,cull_reason,subjects,burst_key,burst_size,burst_pick,seconds,raw_detections,top_raw_subject_score\n",
    );
    // (path, exported, rating, stars, verdict, panning, cull, burst, size, pick)
    struct Row {
        file: String,
        exported: bool,
        rating: f64,
        stars: u8,
        verdict: &'static str,
        panning: bool,
        cull: String,
        burst: i64,
        size: usize,
        pick: bool,
    }
    let mut listed: Vec<Row> = Vec::new();
    for r in &rows {
        let file = r["path"].as_str().unwrap_or_default().to_string();
        let Some(rec) = by_path.get(&file.to_lowercase()) else {
            continue;
        };
        let burst = r["burst_key"].as_i64().unwrap_or(-1);
        let is_exported = exported_names.contains(&file.to_lowercase());
        let pick = r["pick"].as_i64() == Some(1);
        csv += &format!(
            "\"{}\",{},{:.4},{:.4},{},{},{},\"{}\",{},{},{},{},{:.3},\"{}\",\"{}\"\n",
            file.replace('"', "\"\""),
            u8::from(is_exported),
            rec.rating,
            rec.whole,
            rec.stars,
            rec.verdict,
            u8::from(rec.panning),
            rec.cull.replace('"', "\"\""),
            rec.subjects,
            burst,
            r["burst_size"].as_i64().unwrap_or(0),
            u8::from(pick),
            rec.secs,
            rec.raw.replace('"', "\"\""),
            rec.top_raw.replace('"', "\"\"")
        );
        listed.push(Row {
            file,
            exported: is_exported,
            rating: rec.rating,
            stars: rec.stars,
            verdict: rec.verdict,
            panning: rec.panning,
            cull: rec.cull.clone(),
            burst,
            size: r["burst_size"].as_i64().unwrap_or(0) as usize,
            pick,
        });
    }
    std::fs::write(out_dir.join("frames.csv"), &csv).expect("write frames.csv");

    let mut subject_csv = String::from(
        "file,exported,index,class,region,conf,x1,y1,x2,y2,frame_w,frame_h,box_fraction,sharpness,learned,measured,uncertain,background,panning,partly_sharp,band0,band1,band2,framing_factor,sides,rating,stars,cull_reason,set_frame_rating\n",
    );
    for s in &subs {
        subject_csv += &format!(
            "\"{}\",{},{},{},{},{:.4},{:.2},{:.2},{:.2},{:.2},{},{},{:.6},{:.4},{},{},{},{:.4},{},{},{},{},{},{:.4},{},{:.4},{},\"{}\",{}\n",
            s.file.replace('"', "\"\""),
            u8::from(s.exported),
            s.index,
            s.class,
            s.region,
            s.conf,
            s.bbox[0],
            s.bbox[1],
            s.bbox[2],
            s.bbox[3],
            s.frame.0,
            s.frame.1,
            s.fraction,
            s.sharpness,
            u8::from(s.learned),
            u8::from(s.measured),
            u8::from(s.uncertain),
            s.background,
            u8::from(s.panning),
            u8::from(s.partly),
            band(&s.bands, 0),
            band(&s.bands, 1),
            band(&s.bands, 2),
            s.factor,
            s.sides,
            s.rating,
            s.stars,
            s.cull.replace('"', "\"\""),
            u8::from(s.setter)
        );
    }
    std::fs::write(out_dir.join("subjects.csv"), &subject_csv).expect("write subjects.csv");
    println!(
        "subjects.csv: {} row(s); re-measured scores differing from the app's: {mismatched}",
        subs.len()
    );

    // --- summary -------------------------------------------------------------
    println!("\nshoot: {shoot}");
    println!(
        "frames scanned: {} of {} listed ({} failed) in {:.1} s, mean {:.2} s/frame",
        listed.len(),
        paths.len(),
        failures,
        total.as_secs_f64(),
        total.as_secs_f64() / listed.len().max(1) as f64
    );
    let mut sizes: std::collections::BTreeMap<usize, usize> = std::collections::BTreeMap::new();
    let mut bursts: HashSet<i64> = HashSet::new();
    for r in &listed {
        bursts.insert(r.burst);
        *sizes.entry(r.size).or_default() += 1;
    }
    println!(
        "bursts: {}, of which {} have a pick",
        bursts.len(),
        bursts.intersection(&picked_bursts).count()
    );
    println!(
        "burst-size histogram: {:?}",
        sizes.iter().collect::<Vec<_>>()
    );
    // Per-frame time: the slowest and fastest, so a mean is not the whole story.
    let mut times: Vec<f64> = records.iter().map(|r| r.secs).collect();
    times.sort_by(f64::total_cmp);
    if !times.is_empty() {
        println!(
            "per-frame time: min {:.2} s, median {:.2} s, max {:.2} s",
            times[0],
            times[times.len() / 2],
            times[times.len() - 1]
        );
    }

    let keepers: Vec<&Row> = listed.iter().filter(|r| r.exported).collect();
    println!("\nexported (human-confirmed keepers) in the run: {}", keepers.len());
    if keepers.is_empty() {
        return;
    }
    let mut stars: std::collections::BTreeMap<u8, usize> = std::collections::BTreeMap::new();
    let mut verdicts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for r in &keepers {
        *stars.entry(r.stars).or_default() += 1;
        *verdicts.entry(r.verdict).or_default() += 1;
    }
    println!("stars: {:?}", stars.iter().collect::<Vec<_>>());
    println!("verdict: {:?}", verdicts.iter().collect::<Vec<_>>());
    let rejected: Vec<&Row> = keepers.iter().copied().filter(|r| !r.cull.is_empty()).collect();
    println!(
        "with a cull reason (false rejects): {} ({:.0}%)",
        rejected.len(),
        100.0 * rejected.len() as f64 / keepers.len() as f64
    );
    for r in rejected.iter().take(10) {
        println!("    {} — {}", r.file, r.cull);
    }
    let panning = keepers.iter().filter(|r| r.panning).count();
    println!(
        "flagged panning: {panning} ({:.0}%)",
        100.0 * panning as f64 / keepers.len() as f64
    );
    let picks = keepers.iter().filter(|r| r.pick).count();
    println!(
        "their burst's pick: {picks} ({:.0}%)",
        100.0 * picks as f64 / keepers.len() as f64
    );
    let mut missed: Vec<&Row> = keepers.iter().copied().filter(|r| !r.pick).collect();
    missed.sort_by(|a, b| b.rating.total_cmp(&a.rating));
    if !missed.is_empty() {
        // The pick of that burst, to compare against.
        let pick_of: std::collections::HashMap<i64, &Row> = listed
            .iter()
            .filter(|r| r.pick)
            .map(|r| (r.burst, r))
            .collect();
        let unowned = missed
            .iter()
            .filter(|r| !pick_of.contains_key(&r.burst))
            .count();
        println!(
            "not the pick ({}), of which {unowned} sit in a burst where nothing was picked at all:",
            missed.len()
        );
        for r in missed.iter().take(10) {
            match pick_of.get(&r.burst) {
                Some(p) => println!(
                    "    {} {:.3} (burst {} of {}) -> {} {:.3}",
                    r.file,
                    r.rating,
                    r.burst,
                    r.size,
                    p.file,
                    p.rating
                ),
                None => println!(
                    "    {} {:.3} (burst {} of {}) -> no pick",
                    r.file, r.rating, r.burst, r.size
                ),
            }
        }
        if missed.len() > 10 {
            println!("    … {} more in frames.csv", missed.len() - 10);
        }
    }

    // Nothing beside the photographer's originals changed.
    for (dir, was) in &before {
        let now = snapshot(dir);
        assert_eq!(
            *was, now,
            "the shoot folder {} changed during the run",
            dir.display()
        );
    }
    println!(
        "\nsources untouched: {} folder(s), {} file(s) each with the same size and mtime as before",
        before.len(),
        before.iter().map(|(_, v)| v.len()).sum::<usize>()
    );
    println!("csv: {}", out_dir.join("frames.csv").display());
    println!("csv: {}", out_dir.join("subjects.csv").display());
}
