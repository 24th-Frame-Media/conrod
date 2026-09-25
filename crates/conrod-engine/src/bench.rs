//! The VLM bench: pick a handful of sharp and blurry frames, run one or more
//! provider/model pairs over them, and score each against a hand-typed truth.
//! Read-only -- nothing here writes to the database or a photograph.

use crate::desktop::{Desktop, Result};
use crate::lock;
use conrod_core::normalise;
use conrod_core::text::casefold;
use conrod_io::settings::Settings as IoSettings;
use conrod_io::vlm::{self, VehicleDescription, VlmClient};
use conrod_vision::imageops::Rgb;
use serde_json::{json, Value};
use std::time::Instant;

use crate::commands::{BenchItem, BenchRunArgs, BenchTruth};

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

pub fn pick(d: &Desktop) -> Result<Value> {
    let picks = conrod_store::bench_pick(&lock(&d.reader)).map_err(err)?;
    Ok(json!(picks
        .into_iter()
        .map(|(image_id, path, thumb, sharp)| json!({"imageId": image_id, "path": path, "thumb": thumb, "sharp": sharp}))
        .collect::<Vec<_>>()))
}

/// What the VLM sees: the largest vehicle cut from the raw's embedded
/// preview, the same pixels the scan crops from. Whole preview if no box.
fn load(d: &Desktop, path: &str) -> Result<Rgb> {
    let raw = conrod_io::raw::read(std::path::Path::new(path))?;
    let image = Rgb::decode_jpeg(&raw.preview, 1)?.orient(raw.orientation);
    let Some(b) = conrod_store::largest_vehicle_box(&lock(&d.reader), path).map_err(err)? else {
        return Ok(image);
    };
    let [x1, y1, x2, y2] = b.map(|v| v.max(0.0) as usize);
    let crop = image.crop(
        x1.min(image.width),
        y1.min(image.height),
        x2.min(image.width),
        y2.min(image.height),
    );
    Ok(if crop.width == 0 || crop.height == 0 {
        image
    } else {
        crop
    })
}

/// One image through the model set in Settings, answer returned and not stored.
pub fn describe_one(d: &Desktop, path: &str) -> Result<Value> {
    let io = IoSettings::from_core(&lock(&d.settings));
    let image = load(d, path)?;
    let client = VlmClient::new(std::sync::Arc::new(vlm::RealClock::default()));
    let v = vlm::describe(&client, &image, &io, false).map_err(err)?;
    Ok(json!({"make": v.make, "model": v.model, "colour": v.colour, "number": v.race_number}))
}

/// Casefold, trim, and (for the make) drop punctuation, then compare exactly.
/// A blank truth field is not scored at all.
fn field_score(truth: Option<&str>, answer: Option<&str>, is_make: bool) -> Option<f64> {
    let truth = truth.map(str::trim).filter(|s| !s.is_empty())?;
    let norm = |s: &str| -> String {
        let s = casefold(s.trim());
        if is_make {
            normalise::key(&s)
        } else {
            s
        }
    };
    let matched = answer.is_some_and(|a| norm(a) == norm(truth));
    Some(if matched { 1.0 } else { 0.0 })
}

/// The mean of whichever fields the truth actually claims.
fn item_score(truth: &BenchTruth, answer: &VehicleDescription) -> f64 {
    let scores: Vec<f64> = [
        field_score(truth.make.as_deref(), answer.make.as_deref(), true),
        field_score(truth.model.as_deref(), answer.model.as_deref(), false),
        field_score(truth.colour.as_deref(), answer.colour.as_deref(), false),
        field_score(
            truth.number.as_deref(),
            answer.race_number.as_deref(),
            false,
        ),
    ]
    .into_iter()
    .flatten()
    .collect();
    if scores.is_empty() {
        1.0
    } else {
        scores.iter().sum::<f64>() / scores.len() as f64
    }
}

/// The service with this id, set to run `model` and nothing else.
fn pin(
    base: &conrod_core::settings::Settings,
    id: &str,
    model: &str,
) -> Option<conrod_core::settings::VlmService> {
    let s = base.vlm_services.iter().find(|s| s.id == id)?;
    Some(conrod_core::settings::VlmService {
        model: model.to_string(),
        enabled: true,
        ..s.clone()
    })
}

struct Row {
    service_id: String,
    provider: String,
    model: String,
    score: f64,
    sharp_score: f64,
    blurry_score: f64,
    avg_secs: f64,
    errors: i64,
    answers: Vec<Value>,
}

fn run_model(
    client: &VlmClient,
    settings: &IoSettings,
    items: &[BenchItem],
    sources: &[Result<Rgb>],
    on_photo: &mut dyn FnMut(usize),
) -> (f64, f64, i64, f64, Vec<Value>) {
    let mut sharp_scores = Vec::new();
    let mut blurry_scores = Vec::new();
    let mut errors = 0i64;
    let mut total_secs = 0.0;
    let mut answers = Vec::new();
    for (index, (item, src)) in items.iter().zip(sources).enumerate() {
        on_photo(index);
        let image = match src {
            Ok(image) => image,
            Err(e) => {
                errors += 1;
                answers.push(json!(format!("could not open image: {e}")));
                (if item.sharp {
                    &mut sharp_scores
                } else {
                    &mut blurry_scores
                })
                .push(0.0);
                continue;
            }
        };
        let start = Instant::now();
        let result = vlm::describe(client, image, settings, false);
        total_secs += start.elapsed().as_secs_f64();
        let (score, answer) = match result {
            Ok(d) => {
                let score = item_score(&item.truth, &d);
                (
                    score,
                    json!({"make": d.make, "model": d.model, "colour": d.colour, "number": d.race_number}),
                )
            }
            Err(e) => {
                errors += 1;
                (0.0, json!(e.to_string()))
            }
        };
        (if item.sharp {
            &mut sharp_scores
        } else {
            &mut blurry_scores
        })
        .push(score);
        answers.push(answer);
    }
    let mean = |v: &[f64]| {
        if v.is_empty() {
            0.0
        } else {
            v.iter().sum::<f64>() / v.len() as f64
        }
    };
    let avg_secs = if items.is_empty() {
        0.0
    } else {
        total_secs / items.len() as f64
    };
    (
        mean(&sharp_scores),
        mean(&blurry_scores),
        errors,
        avg_secs,
        answers,
    )
}

pub fn run(d: &Desktop, args: &BenchRunArgs) -> Result<Value> {
    let base = lock(&d.settings).clone();
    let client = VlmClient::new(std::sync::Arc::new(vlm::RealClock::default()));
    // Shown in the status pill: which model on which server, and which photo.
    let photos = args.items.len();
    let task = d
        .hub
        .start("Benchmarking models", (photos * args.models.len()) as u64);
    task.detail(format!("loading {photos} photos"));
    let sources: Vec<Result<Rgb>> = args.items.iter().map(|it| load(d, &it.path)).collect();
    let mut rows: Vec<Row> = args
        .models
        .iter()
        .enumerate()
        .map(|(n, m)| {
            let Some(service) = pin(&base, &m.service_id, &m.model) else {
                return Row {
                    service_id: m.service_id.clone(),
                    provider: String::new(),
                    model: m.model.clone(),
                    score: 0.0,
                    sharp_score: 0.0,
                    blurry_score: 0.0,
                    avg_secs: 0.0,
                    errors: args.items.len().max(1) as i64,
                    answers: vec![json!(format!("unknown service {}", m.service_id))],
                };
            };
            let provider = service.provider.clone();
            let mut settings = base.clone();
            settings.vlm_provider = service.provider.clone();
            settings.vlm_model = service.model.clone();
            settings.vlm_host = service.host.clone();
            settings.vlm_api_key = service.api_key.clone();
            settings.vlm_services = vec![service];
            let io = IoSettings::from_core(&settings);
            let label = format!(
                "{} on {} ({}/{})",
                m.model,
                conrod_io::health::service_label(&provider, &settings.vlm_host),
                n + 1,
                args.models.len()
            );
            let (sharp_score, blurry_score, errors, avg_secs, answers) =
                run_model(&client, &io, &args.items, &sources, &mut |i| {
                    task.detail(format!("{label} · photo {}/{photos}", i + 1));
                    task.progress((n * photos + i) as u64, (photos * args.models.len()) as u64);
                });
            let n_sharp = args.items.iter().filter(|i| i.sharp).count();
            let n_blurry = args.items.len() - n_sharp;
            let total = n_sharp + n_blurry;
            let score = if total == 0 {
                0.0
            } else {
                (sharp_score * n_sharp as f64 + blurry_score * n_blurry as f64) / total as f64
            };
            Row {
                service_id: m.service_id.clone(),
                provider,
                model: m.model.clone(),
                score,
                sharp_score,
                blurry_score,
                avg_secs,
                errors,
                answers,
            }
        })
        .collect();
    task.finish();
    rows.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                a.avg_secs
                    .partial_cmp(&b.avg_secs)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });
    Ok(json!(rows
        .into_iter()
        .map(|r| json!({
            "serviceId": r.service_id,
            "provider": r.provider,
            "model": r.model,
            "score": r.score,
            "sharpScore": r.sharp_score,
            "blurryScore": r.blurry_score,
            "avgSecs": r.avg_secs,
            "errors": r.errors,
            "answers": r.answers,
        }))
        .collect::<Vec<_>>()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desc(make: &str, model: &str, colour: &str, number: &str) -> VehicleDescription {
        VehicleDescription {
            make: Some(make.into()),
            model: Some(model.into()),
            colour: Some(colour.into()),
            race_number: Some(number.into()),
            ..VehicleDescription::default()
        }
    }

    fn truth(make: &str, model: &str, colour: &str, number: &str) -> BenchTruth {
        BenchTruth {
            make: Some(make.into()),
            model: Some(model.into()),
            colour: Some(colour.into()),
            number: Some(number.into()),
        }
    }

    #[test]
    fn scoring_folds_case_and_normalises_the_make_and_ranks_by_score_then_speed() {
        // Exact match on every field.
        assert_eq!(
            item_score(
                &truth("Ford", "Falcon", "Blue", "77"),
                &desc("ford", "Falcon", "blue", "77")
            ),
            1.0
        );
        // A punctuation-only difference in the make still matches.
        assert_eq!(
            item_score(
                &truth("Mercedes-Benz", "C63", "Black", "5"),
                &desc("Mercedes Benz", "C63", "Black", "5")
            ),
            1.0
        );
        // Wrong model halves a 4-field score to 0.75 (3 of 4 right).
        assert_eq!(
            item_score(
                &truth("Ford", "Falcon", "Blue", "77"),
                &desc("Ford", "Mustang", "Blue", "77")
            ),
            0.75
        );
        // A blank truth field is skipped, not scored as wrong.
        let mut partial_truth = truth("Ford", "Falcon", "Blue", "77");
        partial_truth.colour = None;
        assert_eq!(
            item_score(&partial_truth, &desc("Ford", "Falcon", "Red", "77")),
            1.0
        );

        // Ranking: higher score wins; a tie goes to the faster model.
        let mut rows = [
            Row {
                service_id: String::new(),
                provider: "a".into(),
                model: "slow-perfect".into(),
                score: 1.0,
                sharp_score: 1.0,
                blurry_score: 1.0,
                avg_secs: 2.0,
                errors: 0,
                answers: vec![],
            },
            Row {
                service_id: String::new(),
                provider: "b".into(),
                model: "fast-perfect".into(),
                score: 1.0,
                sharp_score: 1.0,
                blurry_score: 1.0,
                avg_secs: 1.0,
                errors: 0,
                answers: vec![],
            },
            Row {
                service_id: String::new(),
                provider: "c".into(),
                model: "worse".into(),
                score: 0.5,
                sharp_score: 0.5,
                blurry_score: 0.5,
                avg_secs: 0.1,
                errors: 0,
                answers: vec![],
            },
        ];
        rows.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap()
                .then(a.avg_secs.partial_cmp(&b.avg_secs).unwrap())
        });
        assert_eq!(
            rows.iter().map(|r| r.model.as_str()).collect::<Vec<_>>(),
            vec!["fast-perfect", "slow-perfect", "worse"]
        );
    }

    #[test]
    fn pin_targets_the_exact_server_with_the_chosen_model() {
        use conrod_core::settings::{Settings, VlmService};
        let svc = |id: &str, host: &str| VlmService {
            id: id.into(),
            provider: "ollama".into(),
            model: "old".into(),
            host: host.into(),
            enabled: false,
            ..VlmService::default()
        };
        let base = Settings {
            vlm_services: vec![svc("a", "http://h1:11434"), svc("b", "http://h2:11434")],
            ..Settings::default()
        };
        let p = pin(&base, "b", "qwen").unwrap();
        assert_eq!(
            (p.id.as_str(), p.host.as_str(), p.model.as_str(), p.enabled),
            ("b", "http://h2:11434", "qwen", true)
        );
        assert!(pin(&base, "nope", "qwen").is_none());
    }
}
