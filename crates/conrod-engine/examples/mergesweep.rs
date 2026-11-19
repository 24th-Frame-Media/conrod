//! One-off diagnostic: how many time bursts the look-alike merge joins, for a
//! sweep of its two measured limits, on the albums that already have looks.
//! Reads the database READ-ONLY; writes nothing anywhere.
//!
//! cargo run --release -p conrod-engine --example mergesweep -- <db> [labels]
//!
//! The chosen pair is printed with the scale (`MERGE_GAP`, `MERGE_SCENE`) and
//! the album-109 frame pairs the user reported.

use conrod_core::bursts::{
    assign_bursts, look_distance, merge_bursts_with, Frame, MergeRules, MomentFrame,
};
use conrod_core::settings::Settings;
use rusqlite::Connection;
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};

/// One frame of an album, as the merge pass sees it.
struct Row {
    path: String,
    burst: i64,
    camera: u32,
    taken: Option<f64>,
    portrait: bool,
    look: Option<conrod_core::bursts::Look>,
    subject: Option<[f64; 4]>,
    exported: bool,
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let db_path = args.first().ok_or("db path")?;
    let labels_path = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| r"target\groundtruth\labels.json".into());
    let db = Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(err)?;
    let exported = exported_paths(&labels_path);

    let jobs: Vec<(i64, String)> = db
        .prepare("SELECT id,label FROM jobs ORDER BY id")
        .map_err(err)?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(err)?
        .filter_map(|r| r.ok())
        .collect();

    let mut all: Vec<Vec<Row>> = Vec::new();
    for (job, label) in jobs {
        let rows = album(&db, job, &exported)?;
        let with_look = rows.iter().filter(|r| r.look.is_some()).count();
        if with_look == 0 || rows.iter().all(|r| r.taken.is_none()) {
            println!("job {job} {label:?}: no looks or times, skipped");
            continue;
        }
        println!(
            "job {job} {label:?}: {} frames, {with_look} with a look, {} exported",
            rows.len(),
            rows.iter().filter(|r| r.exported).count()
        );
        all.push(rows);
    }

    println!(
        "\n{:>4} {:>6} | {:>7} {:>9} {:>8} {:>10} {:>10}",
        "gap", "scene", "frames", "groups", "joined", "two-sent", "one-sent"
    );
    for gap in [5.0, 10.0, 15.0] {
        for scene in [0.08, 0.10, 0.12, 0.15] {
            let rules = MergeRules {
                gap,
                scene,
                ..MergeRules::default()
            };
            let mut total = Tally::default();
            for rows in &all {
                total += sweep(rows, &rules);
            }
            println!(
                "{:>4.0} {:>6.2} | {:>7} {:>9} {:>8} {:>10} {:>10}",
                gap,
                scene,
                total.frames,
                total.groups,
                total.joined,
                total.two_sent,
                total.one_sent
            );
        }
    }

    // The pair the user reported, and the look distances behind the choice.
    if let Some(rows) = all
        .iter()
        .find(|rows| rows.iter().any(|r| r.path.contains("20261006_B1000_0202")))
    {
        let find = |name: &str| rows.iter().find(|r| r.path.contains(name));
        for pair in [("0198", "0199"), ("0201", "0202")] {
            let (a, b) = (find(pair.0), find(pair.1));
            if let (Some(a), Some(b)) = (a, b) {
                let scene = a
                    .look
                    .zip(b.look)
                    .map(|(a, b)| look_distance(&a.scene, &b.scene));
                let livery = a.look.zip(b.look).and_then(|(a, b)| {
                    a.subject.zip(b.subject).map(|(a, b)| look_distance(&a, &b))
                });
                let gap = b.taken.unwrap_or_default() - a.taken.unwrap_or_default();
                println!(
                    "\n{}..{}: {gap:.1} s apart, bursts {} vs {}, scene {:.3}, livery {:.3}, exported {}/{}",
                    pair.0,
                    pair.1,
                    a.burst,
                    b.burst,
                    scene.unwrap_or(f64::NAN),
                    livery.unwrap_or(f64::NAN),
                    a.exported,
                    b.exported
                );
            }
        }
        let keys: Vec<i64> = rows
            .iter()
            .filter(|r| {
                r.path.contains("0197")
                    || r.path.contains("0198")
                    || r.path.contains("0199")
                    || r.path.contains("0201")
                    || r.path.contains("0202")
            })
            .map(|r| r.burst)
            .collect();
        println!("time bursts of 0197..0202: {keys:?}");
        let frames: Vec<MomentFrame> = rows.iter().map(moment).collect();
        let merged = merge_bursts_with(&frames, &MergeRules::default());
        for needle in ["0198", "0199", "0201", "0202"] {
            if let Some(i) = rows.iter().position(|r| r.path.contains(needle)) {
                println!(
                    "{needle}: burst {} -> merged {}, exported {}",
                    frames[i].burst, merged[i], rows[i].exported
                );
            }
        }
    }
    Ok(())
}

#[derive(Default)]
struct Tally {
    frames: usize,
    groups: usize,
    /// Frames in a group of more than one *original* burst.
    joined: usize,
    /// Joined groups with two or more delivered frames.
    two_sent: usize,
    /// Joined groups with exactly one delivered frame.
    one_sent: usize,
}

impl std::ops::AddAssign for Tally {
    fn add_assign(&mut self, other: Tally) {
        self.frames += other.frames;
        self.groups += other.groups;
        self.joined += other.joined;
        self.two_sent += other.two_sent;
        self.one_sent += other.one_sent;
    }
}

fn sweep(rows: &[Row], rules: &MergeRules) -> Tally {
    let frames: Vec<MomentFrame> = rows.iter().map(moment).collect();
    let merged = merge_bursts_with(&frames, rules);
    let mut groups: HashMap<i64, Vec<usize>> = HashMap::new();
    for (i, key) in merged.iter().enumerate() {
        groups.entry(*key).or_default().push(i);
    }
    let mut tally = Tally {
        frames: rows.len(),
        ..Tally::default()
    };
    for members in groups.values() {
        tally.groups += 1;
        let bursts: HashSet<i64> = members.iter().map(|&i| frames[i].burst).collect();
        if bursts.len() < 2 {
            continue;
        }
        tally.joined += members.len();
        let sent = members.iter().filter(|&&i| rows[i].exported).count();
        if sent >= 2 {
            tally.two_sent += 1;
        } else if sent == 1 {
            tally.one_sent += 1;
        }
    }
    tally
}

fn moment(row: &Row) -> MomentFrame {
    MomentFrame {
        burst: row.burst,
        camera: row.camera,
        portrait: row.portrait,
        taken: row.taken,
        look: row.look,
        subject: row.subject,
        panning: false,
    }
}

fn album(db: &Connection, job: i64, exported: &HashSet<String>) -> Result<Vec<Row>, String> {
    let mut settings = Settings::default();
    if let Ok(stored) = db.query_row(
        "SELECT COALESCE(settings_json,'') FROM jobs WHERE id=?1",
        [job],
        |r| r.get::<_, String>(0),
    ) {
        if let Ok(map) = serde_json::from_str::<Map<String, Value>>(&stored) {
            settings = settings.apply(&map);
        }
    }
    let gap = settings.burst_gap.clamp(0.1, 60.0);
    // The detector's main subject per frame (bystanders aside: a frame the
    // camera's focus area is the subject of falls back to its AF box below).
    let mut main: HashMap<i64, (f64, [f64; 4], String)> = HashMap::new();
    for v in db
        .prepare(
            "SELECT d.image_id,d.x1,d.y1,d.x2,d.y2,COALESCE(d.region_type,'vehicle') AS region,\
             COALESCE(d.bystander,0) AS bystander \
             FROM detections d JOIN images i ON i.id=d.image_id WHERE i.job_id=?1",
        )
        .map_err(err)?
        .query_map([job], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                [
                    r.get::<_, f64>(1)?,
                    r.get::<_, f64>(2)?,
                    r.get::<_, f64>(3)?,
                    r.get::<_, f64>(4)?,
                ],
                r.get::<_, String>(5)?,
                r.get::<_, i64>(6)?,
            ))
        })
        .map_err(err)?
        .filter_map(|r| r.ok())
    {
        let (id, bbox, region, bystander) = v;
        if bystander == 1 {
            continue;
        }
        if !matches!(region.as_str(), "vehicle" | "person") {
            continue;
        }
        let area = (bbox[2] - bbox[0]) * (bbox[3] - bbox[1]);
        let slot = main.entry(id).or_insert((area, bbox, region.clone()));
        if area > slot.0 {
            *slot = (area, bbox, region);
        }
    }
    type AlbumImage = (i64, String, Option<f64>, f64, f64, String, Option<String>);
    let mut images: Vec<AlbumImage> = db
        .prepare(
            "SELECT id,path,taken_at,width,height,camera,look FROM images \
             WHERE job_id=?1 AND status='done' AND COALESCE(copy_of,0)=0 ORDER BY path",
        )
        .map_err(err)?
        .query_map([job], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get::<_, Option<f64>>(3)?.unwrap_or_default(),
                r.get::<_, Option<f64>>(4)?.unwrap_or_default(),
                r.get::<_, Option<String>>(5)?.unwrap_or_default(),
                r.get(6)?,
            ))
        })
        .map_err(err)?
        .filter_map(|r| r.ok())
        .collect();
    images.sort_by(|a, b| a.1.cmp(&b.1));
    let mut frames: Vec<Frame> = images
        .iter()
        .map(|v| Frame {
            path: v.1.clone(),
            camera: v.5.clone(),
            taken: v.2,
            burst: 0,
        })
        .collect();
    assign_bursts(&mut frames, gap);
    let mut camera_ids: HashMap<String, u32> = HashMap::new();
    let mut next_camera = 0u32;
    Ok(images
        .iter()
        .zip(frames)
        .map(|(v, f)| {
            let (id, path, taken, w, h, camera, look) = v;
            let camera = *camera_ids.entry(camera.clone()).or_insert_with(|| {
                next_camera += 1;
                next_camera - 1
            });
            let subject = match (main.get(id), *w, *h) {
                (Some((_, bbox, _)), w, h) if w > 0.0 && h > 0.0 => {
                    Some([bbox[0] / w, bbox[1] / h, bbox[2] / w, bbox[3] / h])
                }
                _ => af_box(db, *id),
            };
            let vehicle = main.get(id).is_some_and(|m| m.2 == "vehicle");
            Row {
                path: path.clone(),
                burst: i64::from(f.burst),
                camera,
                taken: *taken,
                portrait: *h > *w,
                look: look
                    .as_deref()
                    .and_then(conrod_vision::look::unpack)
                    .map(|stored| conrod_core::bursts::Look {
                        scene: stored.scene,
                        subject: stored.subject,
                        vehicle,
                    }),
                subject,
                exported: exported.contains(&normalise(path)),
            }
        })
        .collect())
}

fn af_box(db: &Connection, id: i64) -> Option<[f64; 4]> {
    db.query_row(
        "SELECT af_x1,af_y1,af_x2,af_y2 FROM images WHERE id=?1 AND af_x1 IS NOT NULL",
        [id],
        |r| Ok([r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?]),
    )
    .ok()
}

fn exported_paths(path: &str) -> HashSet<String> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    value["frames"]
        .as_array()
        .map(|frames| {
            frames
                .iter()
                .filter(|f| f["exported"].as_bool() == Some(true))
                .filter_map(|f| f["path"].as_str().map(normalise))
                .collect()
        })
        .unwrap_or_default()
}

fn normalise(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}
