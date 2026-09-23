//! Registration plate detection and reading.
//!
//! Port of `conrod/plates.py`: the plate detector (open-image-models's
//! `yolo-v9-t-640-license-plate-end2end`, an end-to-end ONNX graph with NMS
//! baked in), its own fixed 640x640 letterbox, tiling over the native crop,
//! the plate OCR reader (fast-plate-ocr's `global-plates-mobile-vit-v2`),
//! and the pure text logic that turns raw OCR lines into a plate reading.
//!
//! `scan_regions` reads each plate the way `conrod/plates.py` does: general OCR
//! (`crate::ocr`) supplies the state name, roundel numbers and a candidate
//! registration, `interpret` picks among those lines, and the plate reader's
//! own text wins when it is more confident and plate-shaped.

use crate::detect::{open_session, Device};
use crate::imageops::{Filter, Rgb};
use crate::ocr::Ocr;
use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;

/// The plate detector's fixed square input edge.
const DET_SIZE: usize = 640;
/// Letterbox padding colour, as a fraction of full scale.
const DET_PAD: f32 = 114.0 / 255.0;

/// The OCR reader's alphabet: 9 slots, 37 classes each (`0-9A-Z_`).
const OCR_SLOTS: usize = 9;
const OCR_ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ_";
const OCR_PAD: u8 = b'_';
const OCR_W: usize = 140;
const OCR_H: usize = 70;

/// The plate-related settings from `conrod/config.py`, with its defaults.
#[derive(Debug, Clone)]
pub struct PlateOptions {
    pub plate_conf: f32,
    pub plate_reader: bool,
    pub plate_reader_min_conf: f32,
    pub plate_native_search: bool,
    pub plate_native_lower: f64,
    pub plate_tile_edge: usize,
    pub plate_tile_overlap: f64,
    pub plate_pad_x: f64,
    pub plate_pad_y: f64,
    pub plate_ocr_edge: usize,
    pub plate_min_len: usize,
    pub plate_max_len: usize,
    pub max_plates_per_vehicle: usize,
    /// Digits-only reads on a plate piece are kept as roundel numbers when this long.
    pub number_min_len: usize,
    pub number_max_len: usize,
}

impl Default for PlateOptions {
    fn default() -> Self {
        PlateOptions {
            plate_conf: 0.35,
            plate_reader: true,
            plate_reader_min_conf: 0.75,
            plate_native_search: true,
            plate_native_lower: 0.55,
            plate_tile_edge: 1280,
            plate_tile_overlap: 0.25,
            plate_pad_x: 0.06,
            plate_pad_y: 0.18,
            plate_ocr_edge: 700,
            plate_min_len: 2,
            plate_max_len: 8,
            max_plates_per_vehicle: 2,
            number_min_len: 1,
            number_max_len: 4,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlateReading {
    pub text: Option<String>,
    pub state: Option<String>,
    pub confidence: f64,
    /// In the coordinates of whichever image (crop or native) it was found in.
    pub bbox: Option<[i64; 4]>,
    pub candidates: Vec<String>,
}

impl PlateReading {
    pub fn display(&self) -> String {
        let Some(text) = self.text.as_deref().filter(|t| !t.is_empty()) else {
            return String::new();
        };
        match self.state.as_deref().filter(|s| !s.is_empty()) {
            Some(state) => format!("{text} ({state})"),
            None => text.to_string(),
        }
    }
}

// --- the detector -----------------------------------------------------

pub struct PlateDetector {
    session: Session,
    pub device: &'static str,
}

impl PlateDetector {
    pub fn load(model: &Path, device: Device) -> Result<PlateDetector, String> {
        let (session, device) = open_session(model, device)?;
        Ok(PlateDetector { session, device })
    }

    /// One pass of the plate detector over one image. `conf` is applied
    /// directly rather than through the model's own default (0.25): every
    /// caller in this file passes `settings.plate_conf` (0.35), which is
    /// stricter, so the intermediate threshold changes nothing kept.
    pub fn detect_on(&mut self, image: &Rgb, conf: f32) -> Result<Vec<([i64; 4], f32)>, String> {
        let lb = letterbox(image);
        let tensor = Tensor::from_array(([1usize, 3, DET_SIZE, DET_SIZE], lb.tensor.clone()))
            .map_err(|e| e.to_string())?;
        let outputs = self
            .session
            .run(ort::inputs![tensor])
            .map_err(|e| e.to_string())?;
        let (shape, raw) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        Ok(decode(raw, shape[0] as usize, &lb, conf))
    }
}

struct Letterboxed {
    /// CHW, channel order R,G,B, 0..1 -- two BGR flips cancel out on the
    /// Python side (`_detect_on`'s slice reversal, then the model's own
    /// preprocess doing the same); letterboxing the RGB `Rgb` directly and
    /// skipping both reproduces the same tensor.
    tensor: Vec<f32>,
    r: f64,
    dw: f64,
    dh: f64,
}

/// open-image-models' YOLOv9 `letterbox`: resize to fit 640x640 keeping
/// aspect (always allowed to scale up), pad to exactly 640x640 with 114.
fn letterbox(frame: &Rgb) -> Letterboxed {
    let (fw, fh) = (frame.width, frame.height);
    let r = (DET_SIZE as f64 / fh as f64).min(DET_SIZE as f64 / fw as f64);
    let new_w = ((fw as f64 * r).round() as usize).min(DET_SIZE);
    let new_h = ((fh as f64 * r).round() as usize).min(DET_SIZE);
    let resized = if (new_w, new_h) == (fw, fh) {
        frame.clone()
    } else {
        frame.resize_cv2_linear(new_w, new_h)
    };
    let dw = (DET_SIZE as f64 - new_w as f64) / 2.0;
    let dh = (DET_SIZE as f64 - new_h as f64) / 2.0;
    let top = (dh - 0.1).round().max(0.0) as usize;
    let left = (dw - 0.1).round().max(0.0) as usize;
    let mut tensor = vec![DET_PAD; DET_SIZE * DET_SIZE * 3];
    for y in 0..new_h {
        for x in 0..new_w {
            let s = (y * new_w + x) * 3;
            let d = (y + top) * DET_SIZE + (x + left);
            for c in 0..3 {
                tensor[c * DET_SIZE * DET_SIZE + d] = f32::from(resized.data[s + c]) / 255.0;
            }
        }
    }
    Letterboxed { tensor, r, dw, dh }
}

/// `[N, 7]` = batch, x1, y1, x2, y2, class, score -> boxes in frame pixels.
/// NMS already ran inside the graph, so this is just unletterbox and filter.
fn decode(raw: &[f32], n: usize, lb: &Letterboxed, conf: f32) -> Vec<([i64; 4], f32)> {
    (0..n)
        .filter_map(|i| {
            let row = &raw[i * 7..i * 7 + 7];
            let score = row[6];
            if score < conf {
                return None;
            }
            // Truncating cast, matching Python's `int(...)` on each coordinate.
            let at = |v: f32, pad: f64| ((f64::from(v) - pad) / lb.r) as i64;
            let b = [
                at(row[1], lb.dw),
                at(row[2], lb.dh),
                at(row[3], lb.dw),
                at(row[4], lb.dh),
            ];
            Some((b, score))
        })
        .collect()
}

// --- tiling and merging --------------------------------------------------

/// Overlapping tile offsets covering `width` x `height`.
fn tile_offsets(width: usize, height: usize, edge: usize, overlap: f64) -> Vec<(usize, usize)> {
    let step = ((edge as f64 * (1.0 - overlap)) as usize).max(1);
    let starts = |extent: usize| -> Vec<usize> {
        if extent <= edge {
            return vec![0];
        }
        let mut out: Vec<usize> = (0..=extent - edge).step_by(step).collect();
        if out.last().copied().unwrap_or(0) + edge < extent {
            out.push(extent - edge);
        }
        out
    };
    starts(height)
        .into_iter()
        .flat_map(|oy| starts(width).into_iter().map(move |ox| (ox, oy)))
        .collect()
}

fn iou(a: [i64; 4], b: [i64; 4]) -> f64 {
    let ix = (a[2].min(b[2]) - a[0].max(b[0])).max(0);
    let iy = (a[3].min(b[3]) - a[1].max(b[1])).max(0);
    let inter = (ix * iy) as f64;
    if inter == 0.0 {
        return 0.0;
    }
    let union = ((a[2] - a[0]) * (a[3] - a[1]) + (b[2] - b[0]) * (b[3] - b[1])) as f64 - inter;
    if union != 0.0 {
        inter / union
    } else {
        0.0
    }
}

/// Keep the most confident box out of each overlapping cluster.
fn merge(mut found: Vec<([i64; 4], f32)>) -> Vec<([i64; 4], f32)> {
    found.sort_by(|a, b| b.1.total_cmp(&a.1));
    let mut kept: Vec<([i64; 4], f32)> = Vec::new();
    for (b, conf) in found {
        if kept.iter().all(|&(k, _)| iou(k, b) < 0.4) {
            kept.push((b, conf));
        }
    }
    kept
}

/// Locate plates and number roundels in a vehicle crop.
pub fn find_plates(
    image: &Rgb,
    opts: &PlateOptions,
    detector: &mut PlateDetector,
) -> Result<Vec<([i64; 4], f32)>, String> {
    let found = detector.detect_on(image, opts.plate_conf)?;
    Ok(merge(found)
        .into_iter()
        .take(opts.max_plates_per_vehicle)
        .collect())
}

/// Hunt for a small plate in the vehicle at full resolution, in overlapping
/// tiles over the lower part of the vehicle (see `conrod/plates.py` for why
/// only the lower part, and why tiling the original rather than downscaling).
pub fn find_plates_native(
    native: &Rgb,
    opts: &PlateOptions,
    detector: &mut PlateDetector,
) -> Result<Vec<([i64; 4], f32)>, String> {
    if !opts.plate_native_search {
        return Ok(Vec::new());
    }
    let top = (native.height as f64 * (1.0 - opts.plate_native_lower)) as usize;
    let region = native.crop(0, top, native.width, native.height);
    let mut found = Vec::new();
    for (ox, oy) in tile_offsets(
        region.width,
        region.height,
        opts.plate_tile_edge,
        opts.plate_tile_overlap,
    ) {
        let tile = region.crop(
            ox,
            oy,
            (ox + opts.plate_tile_edge).min(region.width),
            (oy + opts.plate_tile_edge).min(region.height),
        );
        for (b, conf) in detector.detect_on(&tile, opts.plate_conf)? {
            let (ox, oy, top) = (ox as i64, oy as i64, top as i64);
            found.push((
                [b[0] + ox, b[1] + oy + top, b[2] + ox, b[3] + oy + top],
                conf,
            ));
        }
    }
    Ok(merge(found)
        .into_iter()
        .take(opts.max_plates_per_vehicle)
        .collect())
}

/// Cut the plate out with a little margin and upscale it for OCR.
fn crop_plate(image: &Rgb, bbox: [i64; 4], opts: &PlateOptions) -> Rgb {
    let [x1, y1, x2, y2] = bbox;
    let pad_x = (x2 - x1) as f64 * opts.plate_pad_x;
    let pad_y = (y2 - y1) as f64 * opts.plate_pad_y;
    let cx1 = (((x1 as f64 - pad_x) as i64).max(0) as usize).min(image.width);
    let cy1 = (((y1 as f64 - pad_y) as i64).max(0) as usize).min(image.height);
    let cx2 = (((x2 as f64 + pad_x) as i64).max(0) as usize)
        .min(image.width)
        .max(cx1);
    let cy2 = (((y2 as f64 + pad_y) as i64).max(0) as usize)
        .min(image.height)
        .max(cy1);
    let crop = image.crop(cx1, cy1, cx2, cy2);
    if crop.width < 10 || crop.height < 6 {
        return crop;
    }
    let longest = crop.width.max(crop.height);
    if longest < opts.plate_ocr_edge {
        let scale = opts.plate_ocr_edge as f64 / longest as f64;
        let w = ((crop.width as f64 * scale) as i64).max(1) as usize;
        let h = ((crop.height as f64 * scale) as i64).max(1) as usize;
        return crop.resize(w, h, Filter::Lanczos);
    }
    crop
}

// --- the OCR reader --------------------------------------------------

pub struct PlateReader {
    session: Session,
    pub device: &'static str,
}

impl PlateReader {
    pub fn load(model: &Path, device: Device) -> Result<PlateReader, String> {
        let (session, device) = open_session(model, device)?;
        Ok(PlateReader { session, device })
    }

    /// Read the characters off a plate-shaped crop. `("", 0.0)` if unsure --
    /// the recogniser always returns *something*, so a confidence floor is
    /// not optional.
    pub fn read(&mut self, crop: &Rgb, min_conf: f32) -> Result<(String, f32), String> {
        let gray = crop.to_gray().resize_cv2_linear(OCR_W, OCR_H);
        let tensor = Tensor::from_array(([1usize, OCR_H, OCR_W, 1usize], gray.data))
            .map_err(|e| e.to_string())?;
        let outputs = self
            .session
            .run(ort::inputs![tensor])
            .map_err(|e| e.to_string())?;
        let (_, raw) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        let (plate, confidence) = decode_ocr(raw);
        if confidence < min_conf {
            return Ok((String::new(), 0.0));
        }
        let cleaned: String = plate
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_uppercase())
            .collect();
        Ok((cleaned, confidence))
    }
}

/// `[9*37]` -> argmax per slot, trailing pad trimmed, confidence = mean of
/// the max probability over ALL 9 slots (not just the kept ones).
fn decode_ocr(raw: &[f32]) -> (String, f32) {
    let alphabet_len = OCR_ALPHABET.len();
    let mut text = String::with_capacity(OCR_SLOTS);
    let mut sum = 0.0f32;
    for slot in 0..OCR_SLOTS {
        let row = &raw[slot * alphabet_len..(slot + 1) * alphabet_len];
        let (idx, best) =
            row.iter()
                .enumerate()
                .fold((0usize, f32::NEG_INFINITY), |(bi, bv), (i, &v)| {
                    if v > bv {
                        (i, v)
                    } else {
                        (bi, bv)
                    }
                });
        text.push(OCR_ALPHABET[idx] as char);
        sum += best;
    }
    let plate = text.trim_end_matches(OCR_PAD as char).to_string();
    (plate, sum / OCR_SLOTS as f32)
}

// --- putting it together ----------------------------------------------

/// Find and read the most convincing plate on a vehicle crop, and any
/// digits-only reads (competition-number roundels the plate detector also
/// boxes), best first.
///
/// Without `ocr` only the plate reader supplies text: `state` stays unset and
/// there are no roundel numbers.
pub fn scan_regions(
    image: &Rgb,
    native: Option<&Rgb>,
    opts: &PlateOptions,
    detector: &mut PlateDetector,
    reader: &mut PlateReader,
    ocr: Option<&Ocr>,
) -> Result<(PlateReading, Vec<(String, f64)>), String> {
    let mut candidates: Vec<(&Rgb, [i64; 4], f32)> = find_plates(image, opts, detector)?
        .into_iter()
        .map(|(b, c)| (image, b, c))
        .collect();
    if let Some(native) = native {
        candidates.extend(
            find_plates_native(native, opts, detector)?
                .into_iter()
                .map(|(b, c)| (native, b, c)),
        );
    }

    let mut best = PlateReading::default();
    let mut numbers: Vec<(String, f64)> = Vec::new();
    for (source, bbox, detection_conf) in candidates {
        let piece = crop_plate(source, bbox, opts);
        let weight = 0.5 + 0.5 * f64::from(detection_conf);
        // A failed OCR pass on one piece costs that piece's OCR lines, not the vehicle.
        let lines: Vec<(String, f64)> = ocr
            .and_then(|engine| engine.read(&piece).ok())
            .unwrap_or_default()
            .into_iter()
            .map(|t| (t.text, t.confidence))
            .collect();
        let mut reading = interpret(&lines, opts);
        if opts.plate_reader {
            let (text, text_conf) = reader.read(&piece, opts.plate_reader_min_conf)?;
            if !text.is_empty()
                && looks_like_plate(&text)
                && f64::from(text_conf) > reading.confidence
            {
                reading.text = Some(text);
                reading.confidence = f64::from(text_conf);
            }
        }
        reading.bbox = Some(bbox);
        // Weight the character read by how sure the detector was that it is a plate.
        reading.confidence *= weight;
        if reading.text.is_some() && reading.confidence > best.confidence {
            best = reading;
        }

        for (raw, score) in &lines {
            let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
            let alnum: String = raw.chars().filter(char::is_ascii_alphanumeric).collect();
            // Letters mixed with digits is a registration, not a number.
            if digits.is_empty()
                || digits != alnum
                || !(opts.number_min_len..=opts.number_max_len).contains(&digits.len())
            {
                continue;
            }
            numbers.push((digits, score * weight));
        }
    }
    if let Some(ref plate_text) = best.text {
        let clean_plate: String = plate_text
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
        numbers.retain(|(num, _)| !clean_plate.contains(num.as_str()));
    }
    numbers.retain(|(num, _)| {
        if num.len() > 3 {
            return false;
        }
        if let Ok(val) = num.parse::<i64>() {
            if (1900..=2099).contains(&val) {
                return false;
            }
        }
        true
    });
    numbers.sort_by(|a, b| b.1.total_cmp(&a.1));
    Ok((best, numbers))
}

// --- pure text logic: state badging, the issue formats, and interpretation ---

fn is_state_code(token: &str) -> bool {
    matches!(
        token,
        "NSW" | "VIC" | "QLD" | "SA" | "WA" | "TAS" | "NT" | "ACT"
    )
}

/// Australian state and territory names, as OCR tends to render them once
/// squashed together and uppercased.
fn state_hint(token: &str) -> Option<&'static str> {
    Some(match token {
        "NEWSOUTHWALES" => "NSW",
        "VICTORIA" => "VIC",
        "QUEENSLAND" => "QLD",
        "SOUTHAUSTRALIA" => "SA",
        "WESTERNAUSTRALIA" => "WA",
        "TASMANIA" => "TAS",
        "NORTHERNTERRITORY" => "NT",
        "AUSTRALIANCAPITALTERRITORY" => "ACT",
        _ => return None,
    })
}

/// Text that shows up on or beside plates and is never the registration.
fn is_plate_noise(token: &str) -> bool {
    matches!(
        token,
        "AUSTRALIA"
            | "THEFIRSTSTATE"
            | "SUNSHINESTATE"
            | "GARDENSTATE"
            | "THEPLACETOBE"
            | "HOLIDAYISLE"
            | "OUTBACKAUSTRALIA"
            | "THEFESTIVALSTATE"
            | "PREMIERSTATE"
            | "THEHEARTOFAUSTRALIA"
    )
}

/// A maximal run of same-class ASCII characters: `true` for a digit run.
type Run = (bool, usize);

fn char_runs(s: &str) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    for b in s.bytes() {
        let digit = b.is_ascii_digit();
        match out.last_mut() {
            Some(last) if last.0 == digit => last.1 += 1,
            _ => out.push((digit, 1)),
        }
    }
    out
}

const DIGIT: bool = true;
const LETTER: bool = false;

/// The common Australian issue formats, expressed as run shapes (class,
/// length range) rather than as regexes -- equivalent given the ASCII
/// alnum-only, already-uppercased tokens this is always called with.
///
/// ponytail: ASCII only, like `conrod_core::py::is_digit`; the source
/// regexes are themselves ASCII-literal (`[A-Z]`, `\d` on tokens already
/// filtered to `[A-Za-z0-9]`), so this is not a narrowing.
const FORMATS: &[&[(bool, std::ops::RangeInclusive<usize>)]] = &[
    &[(LETTER, 3..=3), (DIGIT, 2..=2), (LETTER, 1..=1)], // NSW current: FD23RS
    &[(LETTER, 3..=3), (DIGIT, 3..=3)],                  // widespread older issue
    &[(DIGIT, 3..=3), (LETTER, 3..=3)],                  // QLD older
    &[(LETTER, 1..=1), (DIGIT, 3..=3), (LETTER, 2..=2)], // VIC
    &[(LETTER, 2..=2), (DIGIT, 2..=2), (LETTER, 2..=2)],
    &[(DIGIT, 1..=3), (LETTER, 1..=3), (DIGIT, 1..=3)],
    &[(DIGIT, 4..=6), (LETTER, 1..=1)], // club and historic registration, e.g. 73111J
    &[(LETTER, 1..=1), (DIGIT, 4..=6)],
];

fn matches_any_format(token: &str) -> bool {
    if token.is_empty() || !token.bytes().all(|b| b.is_ascii_alphanumeric()) {
        return false;
    }
    let runs = char_runs(token);
    FORMATS.iter().any(|spec| {
        runs.len() == spec.len()
            && runs
                .iter()
                .zip(*spec)
                .all(|(&(d, len), (want, range))| d == *want && range.contains(&len))
    })
}

/// Used elsewhere to avoid mistaking a registration for a race number.
pub fn looks_like_plate(token: &str) -> bool {
    matches_any_format(&token.to_ascii_uppercase())
}

/// Drop a stray edge character when doing so reveals a valid plate.
///
/// OCR picks up mounting bolts and frame edges as characters. Only ever
/// removes one character, and only when it turns a non-matching token into
/// a matching one.
pub fn trim_to_format(token: &str) -> String {
    if matches_any_format(token) {
        return token.to_string();
    }
    let chars: Vec<char> = token.chars().collect();
    if chars.is_empty() {
        return token.to_string();
    }
    for candidate in [
        chars[1..].iter().collect::<String>(),
        chars[..chars.len() - 1].iter().collect::<String>(),
    ] {
        if candidate.chars().count() >= 5 && matches_any_format(&candidate) {
            return candidate;
        }
    }
    token.to_string()
}

/// Pick the registration out of the several strings a plate crop's general
/// OCR carries (a state name, a tourism slogan, and the plate itself).
pub fn interpret(lines: &[(String, f64)], opts: &PlateOptions) -> PlateReading {
    let mut reading = PlateReading::default();
    let mut best_score = 0.0f64;

    for (raw, score) in lines {
        let token: String = raw
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_uppercase())
            .collect();
        if token.is_empty() {
            continue;
        }
        if is_state_code(&token) {
            reading.state = Some(token);
            continue;
        }
        if let Some(code) = state_hint(&token) {
            reading.state = Some(code.to_string());
            continue;
        }
        if is_plate_noise(&token) {
            continue;
        }
        let len = token.chars().count();
        if len < opts.plate_min_len || len > opts.plate_max_len {
            continue;
        }
        let all_letters = token.bytes().all(|b| b.is_ascii_alphabetic());
        let all_digits = token.bytes().all(|b| b.is_ascii_digit());
        if (all_letters || all_digits) && !matches_any_format(&token) {
            continue;
        }

        let token = trim_to_format(&token);
        reading.candidates.push(token.clone());
        let mut weight = *score;
        if matches_any_format(&token) {
            weight = (weight + 0.2).min(1.0);
        }
        if weight > best_score {
            best_score = weight;
            reading.text = Some(token);
            reading.confidence = weight;
        }
    }

    reading
}
