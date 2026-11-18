//! What a frame looks like, in a few dozen bytes, from its cached thumbnail.
//!
//! Two colour histograms feed the moment splitter (`conrod_core::bursts`):
//!
//! * the **scene**: a hue x saturation + brightness histogram of the whole
//!   frame, so "the photographer turned from the track to the crowd" reads as a
//!   large distance;
//! * the **subject**: a hue histogram of the main subject's box (its inner 70%,
//!   which leaves most of the road and sky out), so "a different livery" reads
//!   as a large distance.
//!
//! They are deliberately crude and cheap. A hue histogram does not know a car
//! from its shadow, but it costs microseconds on a 420 px thumbnail, needs no
//! model, and stays put when the same car blurs or changes size. Distances are
//! computed by `conrod_core::bursts::look_distance`.

use crate::imageops::Rgb;
use conrod_core::bursts::{LOOK_SCENE_BINS, LOOK_SUBJECT_BINS};

/// The scene is sampled on this grid, whatever the thumbnail's size.
const SCENE_GRID: (usize, usize) = (84, 56);
/// Hue bins of the scene histogram (x3 saturation bands); the last four bins
/// are brightness.
const SCENE_HUES: usize = 8;
/// Hue bins of the subject histogram; three more hold the colourless pixels
/// (black, grey, white) by brightness.
const SUBJECT_HUES: usize = 12;
/// The share of the subject box, per side, that is read.
const SUBJECT_INNER: f64 = 0.7;
/// A pixel with less saturation or brightness than this has no hue worth
/// counting: it is road, shadow or white paint.
const MIN_SATURATION: f64 = 0.25;
const MIN_VALUE: f64 = 0.2;

/// Format version of [`pack`]; bump it when the descriptors change so older
/// albums are measured again.
pub const VERSION: u8 = 1;

fn hsv(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let (r, g, b) = (
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
    );
    let max = r.max(g).max(b);
    let delta = max - r.min(g).min(b);
    let sector = if delta <= 0.0 {
        0.0
    } else if max == r {
        ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    let saturation = if max > 0.0 { delta / max } else { 0.0 };
    (sector / 6.0, saturation, max)
}

fn quantise<const N: usize>(counts: [f64; N]) -> [u8; N] {
    let total: f64 = counts.iter().sum();
    let mut out = [0u8; N];
    if total > 0.0 {
        for (o, c) in out.iter_mut().zip(counts) {
            *o = (c / total * 255.0).round() as u8;
        }
    }
    out
}

/// The whole frame's colour layout: 8 hues x 3 saturation bands, then 4
/// brightness bands, each half of the weight.
pub fn scene(image: &Rgb) -> [u8; LOOK_SCENE_BINS] {
    let (cols, rows) = SCENE_GRID;
    let mut counts = [0f64; LOOK_SCENE_BINS];
    if image.width == 0 || image.height == 0 {
        return [0; LOOK_SCENE_BINS];
    }
    for gy in 0..rows {
        let y = (2 * gy + 1) * image.height / (2 * rows);
        for gx in 0..cols {
            let x = (2 * gx + 1) * image.width / (2 * cols);
            let at = (y * image.width + x) * 3;
            let (h, s, v) = hsv(image.data[at], image.data[at + 1], image.data[at + 2]);
            let hue = ((h * SCENE_HUES as f64) as usize).min(SCENE_HUES - 1);
            let band = if s < 0.25 {
                0
            } else if s < 0.6 {
                1
            } else {
                2
            };
            counts[hue * 3 + band] += 1.0;
            counts[SCENE_HUES * 3 + ((v * 4.0) as usize).min(3)] += 1.0;
        }
    }
    quantise(counts)
}

/// The main subject's livery: a soft hue histogram of the inner part of its box
/// (fractions of the frame), plus how much of it is dark, mid or light and
/// colourless. `None` when the box holds no pixel.
pub fn subject(image: &Rgb, bbox: [f64; 4]) -> Option<[u8; LOOK_SUBJECT_BINS]> {
    let (w, h) = (image.width as f64, image.height as f64);
    let (cx, cy) = ((bbox[0] + bbox[2]) / 2.0, (bbox[1] + bbox[3]) / 2.0);
    let (bw, bh) = ((bbox[2] - bbox[0]).abs(), (bbox[3] - bbox[1]).abs());
    let edge = |centre: f64, size: f64, extent: f64| {
        let lo = (((centre - size * SUBJECT_INNER / 2.0) * extent)
            .floor()
            .max(0.0)) as usize;
        let hi = ((centre + size * SUBJECT_INNER / 2.0) * extent)
            .ceil()
            .min(extent) as usize;
        (lo, hi.max(lo + 2).min(extent as usize))
    };
    let (x0, x1) = edge(cx, bw, w);
    let (y0, y1) = edge(cy, bh, h);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let mut counts = [0f64; LOOK_SUBJECT_BINS];
    for y in y0..y1 {
        for x in x0..x1 {
            let at = (y * image.width + x) * 3;
            let (hue, s, v) = hsv(image.data[at], image.data[at + 1], image.data[at + 2]);
            if s >= MIN_SATURATION && v >= MIN_VALUE {
                let position = hue * SUBJECT_HUES as f64;
                let low = position.floor();
                let fraction = position - low;
                let low = low as usize % SUBJECT_HUES;
                counts[low] += 1.0 - fraction;
                counts[(low + 1) % SUBJECT_HUES] += fraction;
            } else {
                counts[SUBJECT_HUES + usize::from(v >= 0.33) + usize::from(v >= 0.66)] += 1.0;
            }
        }
    }
    (counts.iter().sum::<f64>() > 0.0).then(|| quantise(counts))
}

/// A frame's look as it is stored: the box the subject histogram was taken
/// from (so a later re-detection is noticed) and both histograms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stored {
    pub bbox: Option<[u16; 4]>,
    pub scene: [u8; LOOK_SCENE_BINS],
    pub subject: Option<[u8; LOOK_SUBJECT_BINS]>,
}

/// A box as the 16-bit fractions it is stored under.
pub fn quantise_box(bbox: [f64; 4]) -> [u16; 4] {
    bbox.map(|v| (v.clamp(0.0, 1.0) * 65535.0).round() as u16)
}

/// Measure one thumbnail. `bbox` is the main subject's box as fractions of the
/// frame, when the frame has one.
pub fn measure(image: &Rgb, bbox: Option<[f64; 4]>) -> Stored {
    Stored {
        bbox: bbox.map(quantise_box),
        scene: scene(image),
        subject: bbox.and_then(|b| subject(image, b)),
    }
}

/// Text for the `images.look` column: the version, which parts are present,
/// the box, then both histograms, as hex.
pub fn pack(stored: &Stored) -> String {
    let mut bytes = vec![
        VERSION,
        u8::from(stored.bbox.is_some()) | u8::from(stored.subject.is_some()) << 1,
    ];
    for v in stored.bbox.unwrap_or([0; 4]) {
        bytes.extend(v.to_le_bytes());
    }
    bytes.extend(stored.scene);
    bytes.extend(stored.subject.unwrap_or([0; LOOK_SUBJECT_BINS]));
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The inverse of [`pack`]; `None` for text of another version or length.
pub fn unpack(text: &str) -> Option<Stored> {
    if text.len() % 2 != 0 || !text.is_ascii() {
        return None;
    }
    let bytes: Vec<u8> = (0..text.len() / 2)
        .map(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16).ok())
        .collect::<Option<_>>()?;
    if bytes.len() != 2 + 8 + LOOK_SCENE_BINS + LOOK_SUBJECT_BINS || bytes[0] != VERSION {
        return None;
    }
    let flags = bytes[1];
    let mut bbox = [0u16; 4];
    for (i, v) in bbox.iter_mut().enumerate() {
        *v = u16::from_le_bytes([bytes[2 + 2 * i], bytes[3 + 2 * i]]);
    }
    let scene_at = 10;
    let subject_at = scene_at + LOOK_SCENE_BINS;
    Some(Stored {
        bbox: (flags & 1 != 0).then_some(bbox),
        scene: bytes[scene_at..subject_at].try_into().ok()?,
        subject: (flags & 2 != 0)
            .then(|| bytes[subject_at..].try_into().ok())
            .flatten(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use conrod_core::bursts::look_distance;

    /// A frame of grey road with a car-sized rectangle of one colour on it.
    fn frame_with_car(car: [u8; 3]) -> (Rgb, [f64; 4]) {
        let (w, h) = (420, 280);
        let mut data = Vec::with_capacity(w * h * 3);
        for y in 0..h {
            for x in 0..w {
                let inside = (150..270).contains(&x) && (110..170).contains(&y);
                data.extend(if inside { car } else { [90, 90, 92] });
            }
        }
        (
            Rgb::new(w, h, data),
            [150.0 / 420.0, 110.0 / 280.0, 270.0 / 420.0, 170.0 / 280.0],
        )
    }

    #[test]
    fn identical_crops_are_at_distance_zero() {
        let (a, bbox) = frame_with_car([200, 30, 30]);
        let (b, _) = frame_with_car([200, 30, 30]);
        let (x, y) = (subject(&a, bbox).unwrap(), subject(&b, bbox).unwrap());
        assert_eq!(look_distance(&x, &y), 0.0);
        assert_eq!(look_distance(&scene(&a), &scene(&b)), 0.0);
    }

    #[test]
    fn a_red_car_and_a_blue_car_are_far_apart() {
        let (red, bbox) = frame_with_car([200, 30, 30]);
        let (blue, _) = frame_with_car([30, 60, 200]);
        let d = look_distance(
            &subject(&red, bbox).unwrap(),
            &subject(&blue, bbox).unwrap(),
        );
        assert!(d > 0.9, "red vs blue crops: {d}");
    }

    #[test]
    fn the_same_car_in_other_light_stays_close() {
        let (lit, bbox) = frame_with_car([200, 30, 30]);
        let (dim, _) = frame_with_car([170, 24, 26]);
        let d = look_distance(&subject(&lit, bbox).unwrap(), &subject(&dim, bbox).unwrap());
        assert!(d < 0.15, "one car, two exposures: {d}");
    }

    #[test]
    fn a_colourless_car_reads_as_brightness_not_hue() {
        let (white, bbox) = frame_with_car([235, 235, 235]);
        let (black, _) = frame_with_car([20, 20, 22]);
        let (w, b) = (
            subject(&white, bbox).unwrap(),
            subject(&black, bbox).unwrap(),
        );
        assert!(w[..SUBJECT_HUES].iter().all(|&v| v == 0), "{w:?}");
        assert!(look_distance(&w, &b) > 0.9);
    }

    #[test]
    fn a_turn_to_another_scene_moves_the_scene_histogram() {
        let (track, _) = frame_with_car([200, 30, 30]);
        let green = Rgb::new(420, 280, [40, 160, 50].repeat(420 * 280));
        assert!(look_distance(&scene(&track), &scene(&green)) > 0.5);
    }

    #[test]
    fn a_box_with_no_pixels_has_no_subject() {
        let (a, _) = frame_with_car([200, 30, 30]);
        assert!(subject(&a, [0.5, 0.5, 0.5, 0.5]).is_some());
        let empty = Rgb::new(0, 0, Vec::new());
        assert!(subject(&empty, [0.1, 0.1, 0.5, 0.5]).is_none());
    }

    #[test]
    fn pack_unpack_round_trips_and_rejects_other_versions() {
        let (a, bbox) = frame_with_car([200, 30, 30]);
        let stored = measure(&a, Some(bbox));
        assert_eq!(unpack(&pack(&stored)), Some(stored));
        let bare = measure(&a, None);
        assert_eq!(bare.subject, None);
        assert_eq!(unpack(&pack(&bare)), Some(bare));
        let mut text = pack(&stored);
        text.replace_range(0..2, "09");
        assert_eq!(unpack(&text), None);
        assert_eq!(unpack("zz"), None);
        assert_eq!(unpack(""), None);
    }

    /// `cargo test --release -p conrod-vision --lib look -- --ignored --nocapture`:
    /// time the descriptors on the real thumbnails of an album and print the
    /// first few, to compare with the Python measurement.
    #[test]
    #[ignore = "needs ~/.conrod/cache/native thumbnails"]
    fn real_thumbnails_timing() {
        let dir = std::path::Path::new(r"C:\Users\kapsikkum\.conrod\cache\native");
        let Ok(read) = std::fs::read_dir(dir) else {
            eprintln!("no thumbnails in {}", dir.display());
            return;
        };
        let mut paths: Vec<_> = read.filter_map(|e| e.ok().map(|e| e.path())).collect();
        paths.sort();
        paths.retain(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("thumb-"))
        });
        paths.truncate(400);
        let started = std::time::Instant::now();
        let mut shown = 0;
        for path in &paths {
            let Ok(image) = Rgb::open(path) else { continue };
            let looked = measure(&image, Some([0.35, 0.4, 0.65, 0.7]));
            if shown < 3 {
                eprintln!("{} scene {:?}", path.display(), looked.scene);
                eprintln!("  subject {:?}", looked.subject);
                shown += 1;
            }
        }
        let ms = started.elapsed().as_secs_f64() * 1000.0 / paths.len().max(1) as f64;
        eprintln!(
            "{} thumbnails: {ms:.2} ms each, decode + both histograms, one thread",
            paths.len()
        );
    }
}
