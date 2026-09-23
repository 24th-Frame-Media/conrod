//! Reading the photographer's own files, when a list of them is available:
//! `fixtures/raw_local.json`, written by tools/gen_golden.py --raw,
//! holds paths with the camera and capture time exiftool recorded.

use conrod_io::raw;
use serde_json::Value;
use std::path::Path;

#[test]
fn camera_and_time_match_exiftool_when_available() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/raw_local.json");
    let Ok(text) = std::fs::read_to_string(&path) else {
        eprintln!("no local RAW fixture; run tools/gen_golden.py --raw");
        return;
    };
    let fixture: Value = serde_json::from_str(&text).unwrap();
    let cases = fixture["cases"].as_array().unwrap();
    let (mut ok, mut read) = (0, 0);
    for case in cases {
        let file = Path::new(case["path"].as_str().unwrap());
        let Ok(frame) = raw::read(file) else {
            panic!("could not read {}", file.display());
        };
        read += 1;
        assert!(
            frame.preview.len() > 100_000,
            "{}: preview too small",
            file.display()
        );
        assert_eq!(
            frame.orientation as u64,
            case["orientation"].as_u64().unwrap_or(1),
            "{}",
            file.display()
        );
        // Exported JPEGs were never described by the scan; nothing to compare.
        let Some(want_camera) = case["camera"].as_str() else {
            continue;
        };
        assert_eq!(frame.camera("fallback"), want_camera, "{}", file.display());
        assert_eq!(frame.taken(), case["taken"].as_f64(), "{}", file.display());
        ok += 1;
    }
    eprintln!("{read} files read; {ok} with a recorded camera matched exiftool on camera, capture time and orientation");
}
