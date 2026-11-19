//! Check the native Canon AF reader against exiftool's boxes already in the DB.
//!
//! cargo run --release -p conrod-io --example af_validate -- ground.tsv [limit]
//!
//! `ground.tsv` has one frame per line, tab-separated: path, width, height,
//! x1, y1, x2, y2, mode, camera. The box and mode columns are empty where
//! exiftool found no AF area. Export it from the DB read-only, e.g.
//!
//!   python -c "import sqlite3,os;c=sqlite3.connect('file:'+os.path.expanduser(
//!   '~/.conrod/conrod.db').replace(chr(92),'/')+'?mode=ro',uri=True);
//!   [print(*['' if v is None else v for v in r],sep='\t') for r in c.execute(
//!   'select path,width,height,af_x1,af_y1,af_x2,af_y2,af_mode,camera
//!   from images where job_id=108')]" > ground.tsv
//!
//! The originals are only ever read.

use conrod_io::af::{self, AfArea};
use conrod_io::raw;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Instant;

struct Row {
    path: PathBuf,
    frame: (u32, u32),
    truth: Option<AfArea>,
    camera: String,
}

#[derive(Default)]
struct Tally {
    total: usize,
    truth: usize,
    exact: usize,
    close: usize,
    wrong: usize,
    missing: usize,
    no_truth: usize,
    extra: usize,
    mode_ok: usize,
    errors: usize,
}

fn parse_rows(text: &str) -> Vec<Row> {
    text.lines()
        .filter_map(|line| {
            let c: Vec<&str> = line.split('\t').collect();
            if c.len() < 9 {
                return None;
            }
            let num = |i: usize| c[i].parse::<f64>().ok();
            let truth = match (num(3), num(4), num(5), num(6)) {
                (Some(x1), Some(y1), Some(x2), Some(y2)) => Some(AfArea {
                    x1,
                    y1,
                    x2,
                    y2,
                    mode: c[7].parse().ok(),
                }),
                _ => None,
            };
            Some(Row {
                path: PathBuf::from(c[0]),
                frame: (c[1].parse().ok()?, c[2].parse().ok()?),
                truth,
                camera: c[8].to_string(),
            })
        })
        .collect()
}

fn iou(a: &AfArea, b: &AfArea) -> f64 {
    let w = (a.x2.min(b.x2) - a.x1.max(b.x1)).max(0.0);
    let h = (a.y2.min(b.y2) - a.y1.max(b.y1)).max(0.0);
    let inter = w * h;
    let union = (a.x2 - a.x1) * (a.y2 - a.y1) + (b.x2 - b.x1) * (b.y2 - b.y1) - inter;
    if union > 0.0 {
        inter / union
    } else {
        0.0
    }
}

fn exact(a: &AfArea, b: &AfArea) -> bool {
    [(a.x1, b.x1), (a.y1, b.y1), (a.x2, b.x2), (a.y2, b.y2)]
        .iter()
        .all(|(p, q)| (p - q).abs() <= 0.005)
}

fn show(name: &str, t: &Tally) {
    let pct = |n: usize, d: usize| {
        if d == 0 {
            f64::NAN
        } else {
            100.0 * n as f64 / d as f64
        }
    };
    println!(
        "{name:<34} n={:<5} truth={:<5} exact={:>6.2}% close={:>6.2}% wrong={:<4} missing={:<4} | no-truth={:<5} extra={:<4} | mode ok {:>6.2}% | read errors {}",
        t.total,
        t.truth,
        pct(t.exact, t.truth),
        pct(t.close, t.truth),
        t.wrong,
        t.missing,
        t.no_truth,
        t.extra,
        pct(t.mode_ok, t.truth),
        t.errors
    );
}

fn main() {
    let mut args = std::env::args().skip(1);
    let tsv = args.next().expect("usage: af_validate ground.tsv [limit]");
    let limit: usize = args
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(usize::MAX);
    let text = std::fs::read_to_string(&tsv).expect("read ground truth");
    let rows: Vec<Row> = parse_rows(&text).into_iter().take(limit).collect();
    println!("{} frames", rows.len());

    let mut overall = Tally::default();
    let mut by_camera: BTreeMap<String, Tally> = BTreeMap::new();
    let mut by_mode: BTreeMap<String, Tally> = BTreeMap::new();
    let mut shown = 0;
    let mut spent = 0.0;
    let mut timed = 0usize;
    for row in &rows {
        let started = Instant::now();
        let header = raw::af_header(&row.path);
        spent += started.elapsed().as_secs_f64();
        timed += 1;
        let (native, err) = match header {
            Ok(h) => (
                h.af.and_then(|af| af::from_canon(&af, row.frame, h.orientation)),
                false,
            ),
            Err(_) => (None, true),
        };
        let mode_key = row
            .truth
            .map_or("none".to_string(), |t| format!("{:?}", t.mode));
        for tally in [
            &mut overall,
            by_camera.entry(row.camera.clone()).or_default(),
            by_mode.entry(mode_key.clone()).or_default(),
        ] {
            tally.total += 1;
            tally.errors += usize::from(err);
            match (&row.truth, &native) {
                (Some(t), Some(n)) => {
                    tally.truth += 1;
                    let close = iou(t, n) >= 0.9;
                    tally.exact += usize::from(exact(t, n));
                    tally.close += usize::from(close);
                    tally.wrong += usize::from(!close);
                    tally.mode_ok += usize::from(t.mode == n.mode);
                }
                (Some(_), None) => {
                    tally.truth += 1;
                    tally.missing += 1;
                }
                (None, Some(_)) => tally.extra += 1,
                (None, None) => tally.no_truth += 1,
            }
        }
        let bad = match (&row.truth, &native) {
            (Some(t), Some(n)) => iou(t, n) < 0.9,
            (Some(_), None) => true,
            _ => false,
        };
        if bad && shown < 15 {
            shown += 1;
            println!(
                "MISMATCH {}\n  exiftool {:?}\n  native   {:?}",
                row.path.display(),
                row.truth,
                native
            );
        }
    }
    // Rows the DB has no box for are checked against a live exiftool run, a
    // sample of `EXIFTOOL_SAMPLE` of them (default 0, off). `EXIFTOOL` names it.
    let sample: usize = std::env::var("EXIFTOOL_SAMPLE")
        .ok()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    if let (Ok(exe), true) = (std::env::var("EXIFTOOL"), sample > 0) {
        let blank: Vec<&Row> = rows.iter().filter(|r| r.truth.is_none()).collect();
        let step = (blank.len() / sample).max(1);
        let picked: Vec<&Row> = blank.into_iter().step_by(step).take(sample).collect();
        let mut frames = std::collections::HashMap::new();
        let mut native = Vec::new();
        for row in &picked {
            let Ok(h) = raw::af_header(&row.path) else {
                continue;
            };
            frames.insert(row.path.clone(), (row.frame.0, row.frame.1, h.orientation));
            native.push((
                row.path.clone(),
                h.af.and_then(|a| af::from_canon(&a, row.frame, h.orientation)),
            ));
        }
        let paths: Vec<PathBuf> = native.iter().map(|(p, _)| p.clone()).collect();
        let started = Instant::now();
        let theirs = af::read(&paths, std::path::Path::new(&exe), &frames);
        let per = 1000.0 * started.elapsed().as_secs_f64() / paths.len().max(1) as f64;
        let (mut both, mut agree, mut only_native, mut only_exiftool, mut neither) =
            (0, 0, 0, 0, 0);
        for ((path, n), (_, e)) in native.iter().zip(&theirs) {
            match (n, e) {
                (Some(n), Some(e)) => {
                    both += 1;
                    if iou(n, e) >= 0.9 {
                        agree += 1;
                    } else {
                        println!(
                            "DISAGREE {}\n  exiftool {e:?}\n  native   {n:?}",
                            path.display()
                        );
                    }
                }
                (Some(_), None) => only_native += 1,
                (None, Some(_)) => only_exiftool += 1,
                (None, None) => neither += 1,
            }
        }
        println!(
            "\nlive exiftool on {} frames the DB has no box for: both={both} (IoU>=0.9: {agree}) only-native={only_native} only-exiftool={only_exiftool} neither={neither}; exiftool {per:.1} ms/frame (chunks of 200)",
            paths.len()
        );
    }
    println!();
    show("ALL", &overall);
    for (camera, tally) in &by_camera {
        show(&format!("camera {camera}"), tally);
    }
    for (mode, tally) in &by_mode {
        show(&format!("af_mode {mode}"), tally);
    }
    println!(
        "\nnative read: {:.2} ms/frame over {} frames (first read of each file, header only)",
        1000.0 * spent / timed.max(1) as f64,
        timed
    );
}
