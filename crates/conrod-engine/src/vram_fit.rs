//! Can the decision model and the vision model share this card?
//!
//! Identify always runs its GPU stages one after the other (review decision D2:
//! the Decision stage finishes every frame and burst, then Vision reads every
//! subject). The fit check only decides whether the two models may be resident
//! at the same time:
//!
//! - both fit: the album's seats shape the workers inside each stage;
//! - only one fits: the stages run serially by stage, each model released before
//!   the next loads (the plan's §5 fallback). The user may still raise seats;
//!   the fallback engages on the fit check alone.
//!
//! Estimates are deliberately conservative: documented file sizes plus a
//! context/compute allowance, raised by the real files when they are on disk.
//! A card that "fits" on paper can still spill, so the numbers err toward
//! serial.

use conrod_core::models;
use conrod_core::settings::Settings;
use conrod_decider::gating::{self, Quant, GIB};
use conrod_decider::vram;
use serde_json::{json, Value};

/// The 8-bit decision model: weights and projector for a typical request.
pub const DECIDER_INT8_BYTES: u64 = 48 * GIB / 10;
/// The 4-bit decision model: the same projector, smaller weights.
pub const DECIDER_INT4_BYTES: u64 = 26 * GIB / 10;
/// qwen3-vl:8b Q4_K_M plus its projector, with `num_ctx` 8192.
pub const VISION_BYTES: u64 = 6 * GIB;
/// Room a loaded model needs beyond its files: the KV cache and compute buffers.
const CONTEXT_ALLOWANCE: u64 = 4 * GIB / 10;

/// How the two identify model stages treat the card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Both models can be resident; the stages still run in order.
    Concurrent,
    /// One model at a time: each stage finishes its work and releases the card.
    Serial,
}

impl Mode {
    pub fn word(self) -> &'static str {
        match self {
            Mode::Concurrent => "concurrent",
            Mode::Serial => "serial",
        }
    }
}

/// One fit check: what is on the card and what identify would ask of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fit {
    /// Dedicated video memory of the card, bytes (`0` when none was found).
    pub card_bytes: u64,
    /// What the decision model needs, `0` when it is off or not wanted.
    pub decision_bytes: u64,
    /// What the local vision model needs, `0` when vision is off or not local.
    pub vision_bytes: u64,
    pub fits_both: bool,
    pub mode: Mode,
}

impl Fit {
    /// Both models run on this machine: the seats preference applies.
    pub fn both_local(&self) -> bool {
        self.decision_bytes > 0 && self.vision_bytes > 0
    }

    /// The wire shape the settings/bootstrap snapshot carries (B5 renders it).
    pub fn json(&self) -> Value {
        json!({
            "cardBytes": self.card_bytes,
            "decisionBytes": self.decision_bytes,
            "visionBytes": self.vision_bytes,
            "fitsBoth": self.fits_both,
            "mode": self.mode.word(),
        })
    }
}

/// The check itself, from numbers only (testable without a GPU).
pub fn fit(card_bytes: u64, decision_bytes: u64, vision_bytes: u64) -> Fit {
    let both = decision_bytes > 0 && vision_bytes > 0;
    let fits_both =
        !both || (card_bytes > 0 && decision_bytes.saturating_add(vision_bytes) <= card_bytes);
    Fit {
        card_bytes,
        decision_bytes,
        vision_bytes,
        fits_both,
        mode: if both && !fits_both {
            Mode::Serial
        } else {
            Mode::Concurrent
        },
    }
}

/// The footprint of a decider quant: its files plus the context allowance,
/// never less than the documented size.
pub fn decider_bytes(quant: Quant) -> u64 {
    let (files, documented) = match quant {
        Quant::Int8 => (
            [models::DECIDER_INT8, models::DECIDER_MMPROJ],
            DECIDER_INT8_BYTES,
        ),
        Quant::Int4 => (
            [models::DECIDER_INT4, models::DECIDER_MMPROJ],
            DECIDER_INT4_BYTES,
        ),
    };
    let mut total = 0u64;
    for name in files {
        let Some(path) = models::find(name) else {
            return documented;
        };
        let Ok(meta) = std::fs::metadata(path) else {
            return documented;
        };
        total = total.saturating_add(meta.len());
    }
    total.saturating_add(CONTEXT_ALLOWANCE).max(documented)
}

/// The vision service identify would call when it runs on this machine.
/// `None` for cloud, a remote or LAN Ollama server, or vision turned off: those
/// never compete for this card.
pub fn local_vision(settings: &Settings) -> Option<(String, String)> {
    if !settings.use_vlm {
        return None;
    }
    let local = |provider: &str, host: &str, model: &str| {
        let host = if host.trim().is_empty() {
            conrod_core::settings::DEFAULT_VLM_HOST
        } else {
            host.trim()
        };
        (provider.eq_ignore_ascii_case("ollama") && conrod_io::vlm::is_loopback_host(host))
            .then(|| (host.trim_end_matches('/').to_string(), model.to_string()))
    };
    if !settings.vlm_services.is_empty() {
        // Every enabled service can receive calls (failover / round-robin), so
        // any enabled loopback Ollama counts toward this card's VRAM even when
        // placed behind a cloud service. Every configured service off means
        // nothing is asked (no fallback to legacy single-provider fields).
        settings
            .vlm_services
            .iter()
            .filter(|s| s.enabled)
            .find_map(|s| local(&s.provider, &s.host, &s.model))
    } else {
        local(
            &settings.vlm_provider,
            &settings.vlm_host,
            &settings.vlm_model,
        )
    }
}

/// The live fit check for these settings: the card now, the quant the existing
/// auto gate would choose, and whether vision runs on this machine.
pub fn plan(settings: &Settings) -> Fit {
    let card_bytes = vram::query().map_or(0, |gpu| gpu.total);
    let decision_bytes = if !crate::decider::wants_decider(settings) || card_bytes == 0 {
        0
    } else {
        let forced = crate::decider::forced_quant(&settings.decider_quant);
        // The auto gate measures what the card can hold; its thresholds are the
        // ones an install already uses. A card too small for either quant still
        // reports the smallest footprint, so the warning has real numbers.
        let quant = gating::choose(card_bytes, forced).unwrap_or(Quant::Int4);
        decider_bytes(quant)
    };
    let vision_bytes = if local_vision(settings).is_some() {
        VISION_BYTES
    } else {
        0
    };
    fit(card_bytes, decision_bytes, vision_bytes)
}

/// The settings/bootstrap snapshot's `identifyVram` object.
pub fn snapshot(settings: &Settings) -> Value {
    plan(settings).json()
}

/// Workers inside each identify model stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seats {
    pub decision: usize,
    pub vision: usize,
}

/// The stage worker counts for a known both-local answer. The seats only apply
/// when both models run on this machine; otherwise today's identify count
/// shapes both stages, exactly as before the split.
pub fn seats_for(settings: &Settings, both_local: bool) -> Seats {
    if both_local {
        Seats {
            decision: settings.identify_decision_worker_count().max(1) as usize,
            vision: settings.identify_vision_worker_count().max(1) as usize,
        }
    } else {
        let workers = settings.identify_worker_count().max(1) as usize;
        Seats {
            decision: workers,
            vision: workers,
        }
    }
}

/// The live seats for these settings.
pub fn seats(settings: &Settings) -> Seats {
    seats_for(settings, plan(settings).both_local())
}

/// Runs the identify stages in order: Decision finishes every unit, then on a
/// serial card `handoff` releases the decision model and vision cache, then
/// Vision loads and reads every subject. Propagates a Decision error without
/// touching `handoff` or `vision`, and returns `Ok((d, None))` when `stop` is
/// set before or during Decision (or during a Serial handoff).
pub(crate) fn try_run_in_order<D, V, E>(
    mode: Mode,
    stop: &std::sync::atomic::AtomicBool,
    decision: impl FnOnce() -> Result<D, E>,
    handoff: impl FnOnce(),
    vision: impl FnOnce() -> Result<V, E>,
) -> Result<(D, Option<V>), E> {
    let d = decision()?;
    if stop.load(std::sync::atomic::Ordering::Relaxed) {
        return Ok((d, None));
    }
    if mode == Mode::Serial {
        handoff();
    }
    if stop.load(std::sync::atomic::Ordering::Relaxed) {
        return Ok((d, None));
    }
    let v = vision()?;
    Ok((d, Some(v)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use conrod_core::settings::VlmService;

    #[test]
    fn both_models_fit_exactly_when_their_sizes_reach_the_card() {
        let card = 16 * GIB;
        let fits = fit(card, 5 * GIB, 6 * GIB);
        assert!(fits.fits_both);
        assert_eq!(fits.mode, Mode::Concurrent);
        assert!(fits.both_local());

        // One gigabyte short is the serial fallback, not a blocked run.
        let serial = fit(8 * GIB, DECIDER_INT8_BYTES, VISION_BYTES);
        assert!(!serial.fits_both);
        assert_eq!(serial.mode, Mode::Serial);
        assert!(serial.both_local());
    }

    #[test]
    fn only_one_local_model_is_never_a_fit_failure() {
        let fits = fit(0, 0, VISION_BYTES);
        assert!(fits.fits_both, "nothing competes with vision");
        assert_eq!(fits.mode, Mode::Concurrent);
        assert!(!fits.both_local());
        let cloud_only = fit(4 * GIB, DECIDER_INT8_BYTES, 0);
        assert!(cloud_only.fits_both);
        assert_eq!(cloud_only.mode, Mode::Concurrent);
    }

    #[test]
    fn a_card_with_no_room_for_both_says_serial_with_real_numbers() {
        let card = 8 * GIB;
        let serial = fit(card, DECIDER_INT8_BYTES, VISION_BYTES);
        assert_eq!(serial.card_bytes, card);
        assert_eq!(serial.decision_bytes, DECIDER_INT8_BYTES);
        assert!(
            !serial.fits_both,
            "4.8 GB + 6 GB does not fit a card that reports 8 GiB"
        );
        let wire = serial.json();
        assert_eq!(wire["cardBytes"], json!(card));
        assert_eq!(wire["decisionBytes"], json!(DECIDER_INT8_BYTES));
        assert_eq!(wire["visionBytes"], json!(VISION_BYTES));
        assert_eq!(wire["fitsBoth"], json!(false));
        assert_eq!(wire["mode"], json!("serial"));
    }

    #[test]
    fn the_contract_field_always_carries_the_five_keys() {
        let wire = fit(12 * GIB, DECIDER_INT4_BYTES, VISION_BYTES).json();
        for key in [
            "cardBytes",
            "decisionBytes",
            "visionBytes",
            "fitsBoth",
            "mode",
        ] {
            assert!(wire.get(key).is_some(), "missing {key}: {wire}");
        }
        assert_eq!(wire.as_object().unwrap().len(), 5);
        assert_eq!(wire["mode"], json!("concurrent"));
    }

    #[test]
    fn seats_apply_only_when_both_models_are_local_and_stay_within_the_cap() {
        let settings = Settings {
            analysis_workers: 3,
            identify_vision_workers: 2,
            identify_decision_workers: 1,
            ..Settings::default()
        };
        let both = seats_for(&settings, true);
        assert_eq!(
            both,
            Seats {
                decision: 1,
                vision: 2
            }
        );
        // A cloud vision service (or one model only) keeps today's count.
        let one = seats_for(&settings, false);
        assert_eq!(
            one,
            Seats {
                decision: 3,
                vision: 3
            }
        );
        // Nonsense and oversized seats clamp to the identify cap.
        let capped = Settings {
            identify_vision_workers: 40,
            identify_decision_workers: 0,
            ..settings
        };
        let seats = seats_for(&capped, true);
        assert_eq!(
            seats.vision,
            conrod_core::settings::worker_caps().identify as usize
        );
        assert_eq!(seats.decision, 3, "unset follows the identify count");
    }

    #[test]
    fn a_local_plain_http_ollama_service_is_local_and_a_cloud_one_is_not() {
        let mut settings = Settings {
            use_vlm: true,
            vlm_services: vec![VlmService {
                id: "local".into(),
                provider: "ollama".into(),
                model: "qwen3-vl:8b".into(),
                host: "http://127.0.0.1:11434".into(),
                enabled: true,
                ..VlmService::default()
            }],
            ..Settings::default()
        };
        assert_eq!(
            local_vision(&settings),
            Some(("http://127.0.0.1:11434".into(), "qwen3-vl:8b".into()))
        );
        settings.vlm_services[0].host = "https://box.example:11434".into();
        assert_eq!(local_vision(&settings), None, "a remote host is not local");
        settings.vlm_services[0].provider = "openrouter".into();
        assert_eq!(local_vision(&settings), None, "cloud vision is not local");
        // Vision off means nothing competes for the card.
        settings.vlm_services[0].provider = "ollama".into();
        settings.vlm_services[0].host = "http://127.0.0.1:11434".into();
        settings.use_vlm = false;
        assert_eq!(local_vision(&settings), None);
    }

    #[test]
    fn serial_order_finishes_every_decision_unit_before_the_vision_model_loads() {
        use std::sync::atomic::AtomicBool;

        let stop = AtomicBool::new(false);
        let log = std::cell::RefCell::new(Vec::new());
        let (decided, read) = try_run_in_order::<_, _, ()>(
            Mode::Serial,
            &stop,
            || {
                for frame in 0..4 {
                    log.borrow_mut().push(format!("decision:{frame}"));
                }
                Ok(4usize)
            },
            || {
                // The handoff: the decider goes first, then the vision host's
                // cached model, so the vision model loads onto an empty card.
                log.borrow_mut().push("unload-decider".into());
                log.borrow_mut().push("release-vision".into());
            },
            || {
                log.borrow_mut().push("vision-model-loads".into());
                for frame in 0..4 {
                    log.borrow_mut().push(format!("vision:{frame}"));
                }
                Ok(4usize)
            },
        )
        .unwrap();
        assert_eq!((decided, read), (4, Some(4)));
        let log = log.borrow();
        let unload = log.iter().position(|e| e == "unload-decider").unwrap();
        assert!(
            log[..unload].iter().all(|e| e.starts_with("decision:")),
            "every frame went through the first model: {log:?}"
        );
        assert_eq!(log[unload + 1], "release-vision");
        assert_eq!(log[unload + 2], "vision-model-loads");
        assert!(
            log[unload + 3..].iter().all(|e| e.starts_with("vision:")),
            "the second model only starts after the handoff: {log:?}"
        );
        // A card that holds both skips the handoff but keeps the order.
        let concurrent = std::cell::RefCell::new(Vec::new());
        try_run_in_order::<_, _, ()>(
            Mode::Concurrent,
            &stop,
            || {
                concurrent.borrow_mut().push("decision");
                Ok(())
            },
            || concurrent.borrow_mut().push("handoff"),
            || {
                concurrent.borrow_mut().push("vision");
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(*concurrent.borrow(), ["decision", "vision"]);
    }

    #[test]
    fn lan_ollama_is_not_local_vision_and_loopback_behind_cloud_is() {
        let lan_only = Settings {
            use_vlm: true,
            vlm_services: vec![VlmService {
                id: "lan".into(),
                provider: "ollama".into(),
                model: "qwen3-vl:8b".into(),
                host: "http://192.168.1.50:11434".into(),
                enabled: true,
                ..VlmService::default()
            }],
            ..Settings::default()
        };
        assert_eq!(
            local_vision(&lan_only),
            None,
            "an Ollama server on a separate LAN machine does not compete for this PC's VRAM"
        );

        let cloud_then_local = Settings {
            use_vlm: true,
            vlm_services: vec![
                VlmService {
                    id: "cloud".into(),
                    provider: "openrouter".into(),
                    model: "qwen/qwen3-vl".into(),
                    enabled: true,
                    ..VlmService::default()
                },
                VlmService {
                    id: "local".into(),
                    provider: "ollama".into(),
                    model: "qwen3-vl:8b".into(),
                    host: "http://127.0.0.1:11434/".into(),
                    enabled: true,
                    ..VlmService::default()
                },
            ],
            ..Settings::default()
        };
        assert_eq!(
            local_vision(&cloud_then_local),
            Some(("http://127.0.0.1:11434".into(), "qwen3-vl:8b".into())),
            "a loopback Ollama behind an enabled cloud service still counts toward local VRAM"
        );
    }

    #[test]
    fn try_run_in_order_short_circuits_on_decision_error_and_stop() {
        use std::sync::atomic::{AtomicBool, Ordering};

        // (a) Decision error skips both handoff and vision.
        let stop = AtomicBool::new(false);
        let log = std::cell::RefCell::new(Vec::new());
        let res: Result<(usize, Option<usize>), &'static str> = try_run_in_order(
            Mode::Serial,
            &stop,
            || {
                log.borrow_mut().push("decision");
                Err("boom")
            },
            || log.borrow_mut().push("handoff"),
            || {
                log.borrow_mut().push("vision");
                Ok(2)
            },
        );
        assert_eq!(res, Err("boom"));
        assert_eq!(*log.borrow(), ["decision"]);

        // (b1) Stop already set before decision finishes skips handoff and vision.
        let stop_before = AtomicBool::new(true);
        let log = std::cell::RefCell::new(Vec::new());
        let res: Result<(usize, Option<usize>), &'static str> = try_run_in_order(
            Mode::Serial,
            &stop_before,
            || {
                log.borrow_mut().push("decision");
                Ok(1)
            },
            || log.borrow_mut().push("handoff"),
            || {
                log.borrow_mut().push("vision");
                Ok(2)
            },
        );
        assert_eq!(res, Ok((1, None)));
        assert_eq!(*log.borrow(), ["decision"]);

        // (b2) Stop set during decision skips handoff and vision.
        let stop_during = AtomicBool::new(false);
        let log = std::cell::RefCell::new(Vec::new());
        let res: Result<(usize, Option<usize>), &'static str> = try_run_in_order(
            Mode::Serial,
            &stop_during,
            || {
                log.borrow_mut().push("decision");
                stop_during.store(true, Ordering::Relaxed);
                Ok(3)
            },
            || log.borrow_mut().push("handoff"),
            || {
                log.borrow_mut().push("vision");
                Ok(4)
            },
        );
        assert_eq!(res, Ok((3, None)));
        assert_eq!(*log.borrow(), ["decision"]);

        // Normal serial execution runs all three in order and returns Some(v).
        let go = AtomicBool::new(false);
        let log = std::cell::RefCell::new(Vec::new());
        let res: Result<(usize, Option<usize>), &'static str> = try_run_in_order(
            Mode::Serial,
            &go,
            || {
                log.borrow_mut().push("decision");
                Ok(5)
            },
            || log.borrow_mut().push("handoff"),
            || {
                log.borrow_mut().push("vision");
                Ok(6)
            },
        );
        assert_eq!(res, Ok((5, Some(6))));
        assert_eq!(*log.borrow(), ["decision", "handoff", "vision"]);
    }
}
