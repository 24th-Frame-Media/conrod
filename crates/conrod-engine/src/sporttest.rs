//! Real-data check: what the detector calls a bucking bull, whether it finds
//! the rider on it, and whether `sport::buck_score` ranks the peak of a ride
//! above the loading chute and the walk-out.
//!
//! `rtk cargo test -p conrod-engine --release rodeo_probe -- --ignored --nocapture`
//!
//! Every 15th frame of a real bull-ride shoot (numbers 1300-3061) is read the
//! way `cull_frame` reads it: the embedded preview, oriented, through the
//! detector's network pass on its own, with all 80 COCO classes at confidence
//! 0.25. The profile's class list and the app's confidence cut are exactly
//! what this is measuring, so neither is applied -- the question is which
//! class the animal arrives as, and no rule of the app may answer it.
//!
//! Writes, under `CONROD_RODEO_OUT` (default `E:\code\photo\conrod\target\rodeo`):
//! `rodeo.csv`, one row per sampled frame, and `frames/<stem>.jpg`, the same
//! preview with the orientation baked in, so the contact sheets a script makes
//! from them and this CSV draw boxes in the same coordinates. The originals
//! are opened read-only and the folder's names and sizes are compared before
//! and after the run.

use super::*;
use conrod_core::sport::{self, Det};
use std::collections::BTreeMap;
use std::io::Write;
use std::time::Instant;

const SHOOT: &str =
    r"D:\24th Frame Media\01_Clients\Neville Show Society\2026-11-14_Neville-Show-Bull-Ride\01_RAW";
/// The bull riding, by frame number: the show content before 1300 is skipped.
const FIRST: usize = 1300;
const LAST: usize = 3061;
const STEP: usize = 15;
/// Low enough that a half-hidden bull still produces a box.
const CONF: f32 = 0.25;

fn frame_path(n: usize) -> PathBuf {
    Path::new(SHOOT).join(format!("20261114_NEVSHOW-BULLRIDE_{n:04}.CR3"))
}

/// Names and sizes of everything in the folder: equal before and after means
/// nothing was written beside the photographer's originals.
fn snapshot(dir: &Path) -> Vec<(String, u64)> {
    let mut out: Vec<(String, u64)> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| {
                    (
                        e.file_name().to_string_lossy().into_owned(),
                        e.metadata().map_or(0, |m| m.len()),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// The oriented preview as a plain JPEG, so Pillow draws on the same pixels
/// the boxes were measured on.
fn save_preview(image: &Rgb, path: &Path) -> Result<(), String> {
    let buffer = image::ImageBuffer::<image::Rgb<u8>, Vec<u8>>::from_raw(
        image.width as u32,
        image.height as u32,
        image.data.clone(),
    )
    .ok_or("preview buffer does not match its size")?;
    buffer.save(path).map_err(|e| e.to_string())
}

/// COCO names for the classes this probe reports on; anything else by id.
fn coco_name(id: usize) -> &'static str {
    match id {
        sport::PERSON => "person",
        14 => "bird",
        15 => "cat",
        16 => "dog",
        17 => "horse",
        18 => "sheep",
        19 => "cow",
        20 => "elephant",
        21 => "bear",
        22 => "zebra",
        23 => "giraffe",
        sport::SPORTS_BALL => "ball",
        _ => "other",
    }
}

/// `class#id:conf@area` for each box, the form `evaltest` uses.
fn describe(boxes: &[Det], frame_area: f64) -> String {
    boxes
        .iter()
        .map(|(class, conf, b)| {
            let area = ((b[2] - b[0]).max(0.0) * (b[3] - b[1]).max(0.0)) / frame_area;
            format!("{}#{class}:{conf:.3}@{area:.3}", coco_name(*class))
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// Every box the network produced, in frame pixels, for a drawing script:
/// `class:conf:x1:y1:x2:y2` entries joined by `;`.
fn boxes_text(boxes: &[Det]) -> String {
    boxes
        .iter()
        .map(|(class, conf, b)| {
            format!(
                "{class}:{conf:.3}:{:.1}:{:.1}:{:.1}:{:.1}",
                b[0], b[1], b[2], b[3]
            )
        })
        .collect::<Vec<_>>()
        .join(";")
}

struct Rec {
    file: String,
    number: usize,
    size: (usize, usize),
    persons: usize,
    animals: usize,
    balls: usize,
    /// Every box, for the CSV's `boxes` column.
    all: Vec<Det>,
    animals_text: String,
    /// (class, confidence, height/width) of the most confident animal box.
    best_animal: Option<(usize, f32, f64)>,
    ball_in_play: bool,
    rider: Option<sport::Rider>,
    buck: Option<f64>,
    subject: Option<sport::RodeoSubject>,
}

impl Rec {
    fn buck_text(&self) -> String {
        self.buck.map(|b| format!("{b:.3}")).unwrap_or_default()
    }
}

#[test]
#[ignore]
fn rodeo_probe() {
    let out_dir = PathBuf::from(
        std::env::var("CONROD_RODEO_OUT")
            .unwrap_or_else(|_| String::from(r"E:\code\photo\conrod\target\rodeo")),
    );
    // The guard: everything this test writes goes to a folder called rodeo.
    assert!(
        out_dir.to_string_lossy().replace('\\', "/").contains("/rodeo"),
        "refusing to write to {}",
        out_dir.display()
    );
    let frames_dir = out_dir.join("frames");
    std::fs::create_dir_all(&frames_dir).expect("create the output folder");

    let before = snapshot(Path::new(SHOOT));
    assert!(!before.is_empty(), "no frames in {SHOOT}");

    let mut detector = Detector::load(&detector_model(), Device::Auto).expect("load the detector");
    println!("device {}", detector.device);
    let options = DetectOptions {
        classes: (0..80).collect(),
        conf: CONF,
        ..DetectOptions::default()
    };

    let mut recs: Vec<Rec> = Vec::new();
    let mut missing = 0;
    let mut failures = 0;
    let started = Instant::now();
    for n in (FIRST..=LAST).step_by(STEP) {
        let path = frame_path(n);
        if !path.is_file() {
            missing += 1;
            continue;
        }
        let image = match raw::read(&path) {
            Ok(frame) => match Rgb::decode_jpeg(&frame.preview, 1)
                .map(|image| image.orient(frame.orientation))
            {
                Ok(image) => image,
                Err(e) => {
                    println!("could not decode {}: {e}", path.display());
                    failures += 1;
                    continue;
                }
            },
            Err(e) => {
                println!("could not read {}: {e}", path.display());
                failures += 1;
                continue;
            }
        };
        let found = match detector.raw(&image, &options) {
            Ok(found) => found,
            Err(e) => {
                println!("could not detect {}: {e}", path.display());
                failures += 1;
                continue;
            }
        };
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if let Err(e) = save_preview(&image, &frames_dir.join(format!("{stem}.jpg"))) {
            println!("could not write the preview of {}: {e}", path.display());
        }

        let persons: Vec<Det> = found
            .iter()
            .filter(|(class, _, _)| *class == sport::PERSON)
            .copied()
            .collect();
        let animals: Vec<Det> = found
            .iter()
            .filter(|(class, _, _)| sport::ANIMALS.contains(class))
            .copied()
            .collect();
        let balls: Vec<Det> = found
            .iter()
            .filter(|(class, _, _)| *class == sport::SPORTS_BALL)
            .copied()
            .collect();
        let frame_area = (image.width.max(1) * image.height.max(1)) as f64;
        let best_animal = animals
            .iter()
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(class, conf, b)| {
                let (w, h) = (b[2] - b[0], b[3] - b[1]);
                (*class, *conf, if w > 0.0 { h / w } else { f64::NAN })
            });
        let rider = sport::rider_on_animal(&persons, &animals);
        let buck = rider.map(|r| {
            sport::buck_score(&r, animals[r.animal].2, persons[r.person].2)
        });
        recs.push(Rec {
            file: path.to_string_lossy().into_owned(),
            number: n,
            size: (image.width, image.height),
            persons: persons.len(),
            animals: animals.len(),
            balls: balls.len(),
            all: found,
            animals_text: describe(&animals, frame_area),
            best_animal,
            ball_in_play: sport::ball_in_play(&persons, &balls),
            rider,
            buck,
            subject: sport::rodeo_subject(&persons, &animals),
        });
    }

    // --- csv -----------------------------------------------------------------
    let mut csv = String::from(
        "file,number,frame_w,frame_h,persons,animals,balls,animal_classes,best_animal_id,best_animal_class,best_animal_conf,best_animal_aspect,rider,rider_overlap,rider_above,rider_offset,buck_score,subject_kind,subject_x1,subject_y1,subject_x2,subject_y2,boxes\n",
    );
    for r in &recs {
        let animal = |f: fn(&(usize, f32, f64)) -> f64| {
            r.best_animal
                .as_ref()
                .map(f)
                .map(|v| format!("{v:.3}"))
                .unwrap_or_default()
        };
        let subject_box = r.subject.map(|s| s.bbox());
        csv += &format!(
            "\"{}\",{},{},{},{},{},{},\"{}\",{},{},{},{},{},{:.4},{},{:.4},{},\"{}\",{},{},{},{},\"{}\"\n",
            r.file.replace('"', "\"\""),
            r.number,
            r.size.0,
            r.size.1,
            r.persons,
            r.animals,
            r.balls,
            r.animals_text.replace('"', "\"\""),
            r.best_animal
                .map(|(class, _, _)| class.to_string())
                .unwrap_or_default(),
            r.best_animal
                .map(|(class, _, _)| coco_name(class).to_string())
                .unwrap_or_default(),
            animal(|a| f64::from(a.1)),
            animal(|a| a.2),
            u8::from(r.rider.is_some()),
            r.rider.map_or(0.0, |x| x.overlap),
            r.rider.map(|x| u8::from(x.rider_above)).unwrap_or(0),
            r.rider.map_or(0.0, |x| x.rider_offset),
            r.buck_text(),
            r.subject.map(|s| s.kind()).unwrap_or(""),
            subject_box.map_or(String::new(), |b| format!("{:.1}", b[0])),
            subject_box.map_or(String::new(), |b| format!("{:.1}", b[1])),
            subject_box.map_or(String::new(), |b| format!("{:.1}", b[2])),
            subject_box.map_or(String::new(), |b| format!("{:.1}", b[3])),
            boxes_text(&r.all),
        );
    }
    let csv_path = out_dir.join("rodeo.csv");
    std::fs::write(&csv_path, &csv).expect("write rodeo.csv");

    // --- summary -------------------------------------------------------------
    println!(
        "\nframes sampled: {} of {} ({} missing, {} failed) in {:.1} s, {:.2} s/frame",
        recs.len(),
        (FIRST..=LAST).step_by(STEP).count(),
        missing,
        failures,
        started.elapsed().as_secs_f64(),
        started.elapsed().as_secs_f64() / recs.len().max(1) as f64
    );
    // Which class the animal arrives as, over every frame with an animal box.
    let mut classes: BTreeMap<(usize, &'static str), (usize, f64, f64)> = BTreeMap::new();
    for r in &recs {
        if let Some((class, conf, _)) = r.best_animal {
            let entry = classes.entry((class, coco_name(class))).or_default();
            entry.0 += 1;
            entry.1 += f64::from(conf);
            entry.2 = entry.2.max(f64::from(conf));
        }
    }
    println!("\nbest animal class per frame (frames, mean conf, max conf):");
    for ((id, name), (count, sum, max)) in &classes {
        println!(
            "    {name}#{id:<3} {count:>3} ({:.0}%)  mean {:.3}  max {:.3}",
            *count as f64 * 100.0 / recs.len().max(1) as f64,
            sum / *count as f64,
            max
        );
    }
    // Every animal box, not just the most confident one per frame.
    let mut boxes: BTreeMap<(usize, &'static str), (usize, f64)> = BTreeMap::new();
    for r in &recs {
        for (class, conf, _) in r.all.iter().filter(|(c, _, _)| sport::ANIMALS.contains(c)) {
            let entry = boxes.entry((*class, coco_name(*class))).or_default();
            entry.0 += 1;
            entry.1 = entry.1.max(f64::from(*conf));
        }
    }
    println!("every animal box, by class (boxes, max conf):");
    for ((id, name), (count, max)) in &boxes {
        println!("    {name}#{id:<3} {count:>4}  max {max:.3}");
    }

    let with_animal = recs.iter().filter(|r| r.animals > 0).count();
    let with_person = recs.iter().filter(|r| r.persons > 0).count();
    let riders = recs.iter().filter(|r| r.rider.is_some()).count();
    println!(
        "\nframes with an animal box: {with_animal}; with a person box: {with_person}; \
         with a rider: {riders} ({:.0}% of the animal frames)",
        100.0 * riders as f64 / with_animal.max(1) as f64
    );
    let above: Vec<&Rec> = recs
        .iter()
        .filter(|r| r.rider.is_some_and(|x| !x.rider_above))
        .collect();
    if !above.is_empty() {
        println!("riders not above the animal: {}", above.len());
    }

    let mut scored: Vec<&Rec> = recs.iter().filter(|r| r.buck.is_some()).collect();
    scored.sort_by(|a, b| b.buck.unwrap().total_cmp(&a.buck.unwrap()));
    println!("\nbuck_score, highest first:");
    for r in scored.iter().take(10) {
        let rider = r.rider.expect("scored frames have a rider");
        println!(
            "    {:.3}  n{} aspect {:.2} offset {:.2} overlap {:.2} — {}",
            r.buck.unwrap(),
            r.number,
            rider.animal_aspect,
            rider.rider_offset,
            rider.overlap,
            r.animals_text
        );
    }
    println!("lowest:");
    for r in scored.iter().rev().take(10) {
        let rider = r.rider.expect("scored frames have a rider");
        println!(
            "    {:.3}  n{} aspect {:.2} offset {:.2} overlap {:.2}",
            r.buck.unwrap(),
            r.number,
            rider.animal_aspect,
            rider.rider_offset,
            rider.overlap
        );
    }
    if !scored.is_empty() {
        let scores: Vec<f64> = scored.iter().map(|r| r.buck.unwrap()).collect();
        println!(
            "\nbuck_score over {} riders: min {:.3}, median {:.3}, max {:.3}",
            scores.len(),
            scores.last().unwrap(),
            scores[scores.len() / 2],
            scores[0]
        );
        let weak = scores.iter().filter(|s| **s < 0.5).count();
        println!("below 0.5: {weak} ({:.0}%)", 100.0 * weak as f64 / scores.len() as f64);
    }
    let kinds: BTreeMap<&str, usize> = recs
        .iter()
        .fold(BTreeMap::new(), |mut acc, r| {
            *acc.entry(r.subject.map_or("none", |s| s.kind())).or_default() += 1;
            acc
        });
    println!("decisive subject: {kinds:?}");
    if !recs.is_empty() {
        println!(
            "frames of {} x {} px; balls found: {} (in play: {})",
            recs[0].size.0,
            recs[0].size.1,
            recs.iter().map(|r| r.balls).sum::<usize>(),
            recs.iter().filter(|r| r.ball_in_play).count()
        );
    }

    // Nothing beside the photographer's originals changed.
    assert_eq!(before, snapshot(Path::new(SHOOT)), "the shoot folder changed");
    println!(
        "\nsource untouched: {} files, same names and sizes as before",
        before.len()
    );
    println!("csv: {}", csv_path.display());
    println!("previews: {}", frames_dir.display());
}

/// Writes a small README beside the CSV so the next reader knows how the
/// sheets were made and what the columns mean.
#[test]
#[ignore]
fn rodeo_notes() {
    let out = PathBuf::from(
        std::env::var("CONROD_RODEO_OUT")
            .unwrap_or_else(|_| String::from(r"E:\code\photo\conrod\target\rodeo")),
    );
    assert!(
        out.to_string_lossy().replace('\\', "/").contains("/rodeo"),
        "refusing to write to {}",
        out.display()
    );
    std::fs::create_dir_all(&out).expect("create the output folder");
    let mut f = std::fs::File::create(out.join("README.txt")).expect("write README.txt");
    let _ = writeln!(
        f,
        "rodeo.csv: one row per sampled RAW (every {STEP}th, {FIRST}-{LAST}), network pass\n\
         with all 80 COCO classes at conf >= {CONF}; boxes are in oriented preview pixels.\n\
         buck_score is conrod_core::sport's heuristic, not a measurement."
    );
}
