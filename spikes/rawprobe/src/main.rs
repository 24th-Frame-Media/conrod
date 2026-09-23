//! Spike C: can pure Rust replace exiftool for reading RAW files?
//!
//!     rawprobe <file>...        one JSON line per file
//!
//! For each file: the camera identity bursts.rs needs (model, serial), the
//! capture time with sub-seconds, orientation, the embedded preview's size,
//! and how long each step took. Compared against what exiftool stored in the
//! job database by tools/spike_rawprobe.py.

use rawler::decoders::RawDecodeParams;
use rawler::rawsource::RawSource;
use std::path::Path;
use std::time::Instant;

fn ms(since: Instant) -> f64 {
    since.elapsed().as_secs_f64() * 1000.0
}

fn json_str(value: &Option<String>) -> String {
    match value {
        Some(s) => format!("{:?}", s.trim_end_matches('\0').trim()),
        None => "null".into(),
    }
}

fn main() {
    let params = RawDecodeParams::default();
    for arg in std::env::args().skip(1) {
        let path = Path::new(&arg);
        let start = Instant::now();
        let source = match RawSource::new(path) {
            Ok(s) => s,
            Err(e) => {
                println!("{{\"path\":{arg:?},\"error\":{:?}}}", e.to_string());
                continue;
            }
        };
        let open_ms = ms(start);

        let start = Instant::now();
        let decoder = match rawler::get_decoder(&source) {
            Ok(d) => d,
            Err(e) => {
                println!("{{\"path\":{arg:?},\"error\":{:?}}}", e.to_string());
                continue;
            }
        };
        let meta = decoder.raw_metadata(&source, &params);
        let meta_ms = ms(start);

        let start = Instant::now();
        let preview = decoder.preview_image(&source, &params);
        let preview_ms = ms(start);
        let (pw, ph) = match &preview {
            Ok(Some(img)) => (img.width(), img.height()),
            _ => (0, 0),
        };

        match meta {
            Ok(m) => {
                let e = &m.exif;
                println!(
                    "{{\"path\":{arg:?},\"make\":{:?},\"model\":{:?},\"serial\":{},\"lens\":{},\
                     \"dto\":{},\"subsec\":{},\"orientation\":{},\"preview\":[{pw},{ph}],\
                     \"open_ms\":{open_ms:.2},\"meta_ms\":{meta_ms:.2},\"preview_ms\":{preview_ms:.2},\
                     \"preview_error\":{}}}",
                    m.make,
                    m.model,
                    json_str(&e.serial_number),
                    json_str(&e.lens_model),
                    json_str(&e.date_time_original),
                    json_str(&e.sub_sec_time_original),
                    e.orientation.map_or("null".into(), |o| o.to_string()),
                    match &preview {
                        Err(err) => format!("{:?}", err.to_string()),
                        _ => "null".into(),
                    },
                );
            }
            Err(err) => println!("{{\"path\":{arg:?},\"error\":{:?}}}", err.to_string()),
        }
    }
}
