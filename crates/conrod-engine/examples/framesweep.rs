//! One-off sweep: the whole-frame detail census and clip shares of many RAWs,
//! for choosing the "nothing in focus" and "unrecoverable exposure" gates.
//! Not part of the app.
//!
//! cargo run --release -p conrod-engine --example framesweep -- <paths.txt> <out.csv>
use conrod_io::raw;
use conrod_vision::edgeblur;
use conrod_vision::imageops::Rgb;
use rayon::prelude::*;
use std::io::Write;

const GRADS: [(f32, f64); 4] = [(10.0, 1e9), (10.0, 4.0), (10.0, 2.0), (20.0, 4.0)];

fn shares(image: &Rgb, region: [f64; 4]) -> (f64, f64, f64, f64) {
    let (w, h) = (image.width, image.height);
    let (x0, x1) = (
        (region[0] * w as f64) as usize,
        (region[2] * w as f64) as usize,
    );
    let (y0, y1) = (
        (region[1] * h as f64) as usize,
        (region[3] * h as f64) as usize,
    );
    let step = ((x1 - x0).max(1) / 600).max(1);
    let (mut n, mut white, mut black, mut blackish) = (0usize, 0usize, 0usize, 0usize);
    let mut luma = 0f64;
    let mut y = y0;
    while y < y1.min(h) {
        let mut x = x0;
        while x < x1.min(w) {
            let at = (y * w + x) * 3;
            let (r, g, b) = (image.data[at], image.data[at + 1], image.data[at + 2]);
            if r.min(g).min(b) >= 253 {
                white += 1;
            }
            if r.max(g).max(b) <= 6 {
                black += 1;
            }
            if r.max(g).max(b) <= 16 {
                blackish += 1;
            }
            luma += 0.299 * f64::from(r) + 0.587 * f64::from(g) + 0.114 * f64::from(b);
            n += 1;
            x += step;
        }
        y += step;
    }
    let n = n.max(1) as f64;
    (
        white as f64 / n,
        black as f64 / n,
        blackish as f64 / n,
        luma / n / 255.0,
    )
}

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let list = std::fs::read_to_string(&args[0]).map_err(|e| e.to_string())?;
    let paths: Vec<&str> = list.lines().filter(|l| !l.trim().is_empty()).collect();
    let rows: Vec<String> = paths
        .par_iter()
        .map(|path| {
            let started = std::time::Instant::now();
            let frame = match raw::read(std::path::Path::new(path)) {
                Ok(f) => f,
                Err(e) => return format!("{path},ERR,{e}"),
            };
            let image = match Rgb::decode_jpeg(&frame.preview, 1) {
                Ok(i) => i.orient(frame.orientation),
                Err(e) => return format!("{path},ERR,{e}"),
            };
            let grey = image.to_gray();
            let detail: Vec<_> = GRADS.iter().map(|&(g, s)| edgeblur::detail(&grey, g, s * grey.width.max(grey.height) as f64 / edgeblur::REFERENCE_EDGE as f64)).collect();
            let (w, b, bb, m) = shares(&image, [0.0, 0.0, 1.0, 1.0]);
            let (cw, cb, cbb, cm) = shares(&image, [0.25, 0.25, 0.75, 0.75]);
            let counts: Vec<String> = detail.iter().map(|d| d.edges.to_string()).collect();
            format!(
                "{path},{},{},{:.3},{},{w:.4},{b:.4},{bb:.4},{m:.4},{cw:.4},{cb:.4},{cbb:.4},{cm:.4},{}",
                grey.width,
                grey.height,
                detail[0].noise,
                counts.join(";"),
                started.elapsed().as_millis()
            )
        })
        .collect();
    let mut out = std::fs::File::create(&args[1]).map_err(|e| e.to_string())?;
    writeln!(out, "path,w,h,noise,edges6_10_14_20_30_45,white,black,blackish,mean,cwhite,cblack,cblackish,cmean,ms")
        .map_err(|e| e.to_string())?;
    for row in rows {
        writeln!(out, "{row}").map_err(|e| e.to_string())?;
    }
    Ok(())
}
