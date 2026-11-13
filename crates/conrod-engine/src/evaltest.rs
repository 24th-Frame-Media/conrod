//! Export recall: of the frames the photographer chose and delivered (a human
//! sharp keeper), how many does the app's own rating, cull and burst pick keep?
//!
//! `CONROD_EVAL_SHOOT="2026-10-08_Thursday" cargo test -p conrod-engine --release export_recall -- --ignored --nocapture`
//!
//! Env: `CONROD_EVAL_LABELS` (labels.json), `CONROD_EVAL_SHOOT` (required
//! substring of the shoot folder), `CONROD_EVAL_NEIGHBOURS` (12), `CONROD_EVAL_MAX`
//! (40 exported frames).
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

    // The same burst grouping `desktop::regroup` does, then the keeper pick
    // (`passes::pick_of_pass`), from the rows just stored.
    {
        let db = lock(&desktop.db);
        let images = crate::desktop::rows(
            &db,
            "SELECT id,path,camera,taken_at,rating FROM images WHERE job_id=? AND status='done'",
            [job],
        )
        .expect("read the scanned frames");
        let mut frames: Vec<conrod_core::bursts::Frame> = images
            .iter()
            .map(|v| conrod_core::bursts::Frame {
                path: v["path"].as_str().unwrap_or_default().into(),
                camera: v["camera"].as_str().unwrap_or_default().into(),
                taken: v["taken_at"].as_f64(),
                burst: 0,
            })
            .collect();
        let gap = if (settings.burst_gap - 4.0).abs() < f64::EPSILON || settings.burst_gap <= 0.0 {
            1.5
        } else {
            settings.burst_gap
        };
        conrod_core::bursts::assign_bursts(&mut frames, gap);
        let tx = db.unchecked_transaction().expect("group the bursts");
        for frame in frames {
            tx.execute(
                "UPDATE images SET burst_key=? WHERE job_id=? AND path=?",
                rusqlite::params![frame.burst, job, frame.path],
            )
            .expect("store the burst");
        }
        tx.execute(
            "UPDATE detections SET burst_pick=0 WHERE image_id IN(SELECT id FROM images WHERE job_id=?)",
            [job],
        )
        .expect("clear the picks");
        tx.execute(
            "UPDATE detections SET burst_pick=1 WHERE image_id IN (SELECT id FROM (SELECT id, ROW_NUMBER() OVER(PARTITION BY burst_key ORDER BY COALESCE(stars,rating*5,0) DESC, COALESCE(sharpness,0) DESC, id) AS rank FROM images WHERE job_id=? AND status='done' AND rejected=0) WHERE rank=1)",
            [job],
        )
        .expect("pick the keepers of the bursts");
        tx.commit().expect("commit the grouping");
    }
    let regrouped: Vec<i64> = crate::desktop::rows(
        &lock(&desktop.db),
        "SELECT DISTINCT image_id FROM detections WHERE burst_pick=1 AND image_id IN(SELECT id FROM images WHERE job_id=?)",
        [job],
    )
    .expect("read the regrouped picks")
    .iter()
    .filter_map(|v| v["image_id"].as_i64())
    .collect();
    let picked = crate::passes::pick_of_pass(
        &lock(&desktop.db),
        job,
        profile,
        settings.include_people,
        settings.use_quality_score,
    )
    .expect("pick the keepers");

    let rows = crate::desktop::rows(
        &lock(&desktop.db),
        "SELECT i.id, i.path, i.burst_key, i.rating, i.sharpness, \
         (SELECT count(*) FROM images b WHERE b.job_id=i.job_id AND b.burst_key=i.burst_key) AS burst_size, \
         COALESCE((SELECT max(COALESCE(d.burst_pick,0)) FROM detections d WHERE d.image_id=i.id),0) AS pick \
         FROM images i WHERE i.job_id=? AND i.status='done' ORDER BY i.path",
        [job],
    )
    .expect("read the evaluated frames");

    // A burst whose pick is still the regroup pick (no subject to rate) keeps it.
    let mut picked_bursts: HashSet<i64> = HashSet::new();
    for r in &rows {
        if r["pick"].as_i64() == Some(1) {
            if let Some(k) = r["burst_key"].as_i64() {
                picked_bursts.insert(k);
            }
        }
    }
    let by_path: std::collections::HashMap<String, &Rec> = records
        .iter()
        .map(|r| (r.path.to_lowercase(), r))
        .collect();
    let exported: HashSet<String> = exported
        .iter()
        .map(|(_, p)| p.to_string_lossy().to_lowercase())
        .collect();

    let mut csv = String::from(
        "file,exported,rating,score,stars,verdict,panning,cull_reason,subjects,burst_key,burst_size,burst_pick,seconds\n",
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
        let is_exported = exported.contains(&file.to_lowercase());
        let pick = r["pick"].as_i64() == Some(1)
            || (!picked_bursts.contains(&burst) && regrouped.contains(&r["id"].as_i64().unwrap_or(0)));
        csv += &format!(
            "\"{}\",{},{:.4},{:.4},{},{},{},\"{}\",{},{},{},{},{:.3}\n",
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
            rec.secs
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
    println!("bursts: {} ({picked})", bursts.len(), picked = picked["picks"]);
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
}
