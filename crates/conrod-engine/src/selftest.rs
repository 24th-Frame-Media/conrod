//! `conrod selftest`: proves an installed build can do its job. Every model is
//! present and loads, a synthetic frame goes through the detector, sharpness and
//! OCR, ExifTool runs and a database opens. The exit status is the verdict, so CI
//! can run it against the binary it just built (`Conrod.exe --selftest`, or
//! `conrod-cli selftest`).

use conrod_core::models as m;
use conrod_core::tasks::TaskHub;
use conrod_vision::detect::{DetectOptions, Detector, Device};
use conrod_vision::imageops::Rgb;
use conrod_vision::{ocr, plates, sharpness};
use std::sync::atomic::AtomicBool;

type Check = Result<String, String>;

/// Run every check; the exit code (0 = all passed) and a line per check.
///
/// Models are not bundled with the installer, so a fresh checkout (and CI's
/// freshly-assembled release folder) may not have them yet: install whatever
/// is missing, via the same fail-closed path the app uses, before checking.
pub fn run() -> (i32, String) {
    let hub = TaskHub::new();
    let ids = crate::setup::everything();
    let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
    let install = crate::setup::ensure(&hub, &AtomicBool::new(false), &ids);
    let frame = synthetic(1280, 720);
    let checks: [(&str, Check); 7] = [
        ("model files", model_files()),
        ("detector runs", detector(&frame)),
        (
            "sharpness",
            Ok(format!(
                "{:.3}",
                sharpness::measure(&frame.to_gray(), None, None).score
            )),
        ),
        ("ocr runs", text(&frame)),
        ("plate models load", plate_models()),
        ("exiftool", exiftool()),
        ("database", database()),
    ];
    let mut failed = 0;
    let mut report = String::new();
    if let Err(e) = install {
        failed += 1;
        report += &format!("FAIL  install models: {e}\n");
    }
    for (name, result) in checks {
        match result {
            Ok(detail) => {
                report += &format!(
                    "ok    {name}: {detail}
"
                )
            }
            Err(e) => {
                failed += 1;
                report += &format!(
                    "FAIL  {name}: {e}
"
                );
            }
        }
    }
    (i32::from(failed > 0), report)
}

/// A textured gradient: enough structure for the detector and focus measure to chew on.
fn synthetic(width: usize, height: usize) -> Rgb {
    let mut data = Vec::with_capacity(width * height * 3);
    for y in 0..height {
        for x in 0..width {
            let texture = ((x * 7) ^ (y * 13)) as u8;
            data.extend([(x * 255 / width) as u8, (y * 255 / height) as u8, texture]);
        }
    }
    Rgb::new(width, height, data)
}

fn model_files() -> Check {
    let missing: Vec<&str> = [
        m::DETECTOR,
        m::FACES,
        m::SIMILARITY,
        m::PLATE_DETECTOR,
        m::PLATE_READER,
        m::OCR_DETECTOR,
    ]
    .into_iter()
    .filter(|name| m::find(name).is_none() && (*name != m::OCR_DETECTOR || m::ocr_dir().is_none()))
    .collect();
    if missing.is_empty() {
        Ok("all found".into())
    } else {
        Err(format!("not found: {}", missing.join(", ")))
    }
}

fn detector(frame: &Rgb) -> Check {
    let mut detector = Detector::load(&m::expected(m::DETECTOR), Device::Auto)?;
    let found = detector.detect(frame, &DetectOptions::default())?;
    Ok(format!("{}, {} detections", detector.device, found.len()))
}

fn text(frame: &Rgb) -> Check {
    let dir = m::ocr_dir().ok_or("PP-OCR models not found")?;
    let tokens = ocr::Ocr::load(&dir)?.read(frame)?;
    Ok(format!("{} tokens", tokens.len()))
}

fn plate_models() -> Check {
    plates::PlateDetector::load(&m::expected(m::PLATE_DETECTOR), Device::Cpu)?;
    plates::PlateReader::load(&m::expected(m::PLATE_READER), Device::Cpu)?;
    Ok("detector and reader".into())
}

fn exiftool() -> Check {
    let exe = m::exiftool().map_or_else(|| "exiftool".into(), |p| p.into_os_string());
    let out = std::process::Command::new(exe)
        .arg("-ver")
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err("exiftool -ver failed".into())
    }
}

fn database() -> Check {
    let dir = std::env::temp_dir().join(format!("conrod-selftest-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let opened = conrod_store::connect(Some(&dir.join("conrod.db")))
        .map(|_| "opens".to_string())
        .map_err(|e| e.to_string());
    let _ = std::fs::remove_dir_all(&dir);
    opened
}
