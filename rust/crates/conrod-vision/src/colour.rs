//! Port of `conrod/colour.py`.
//!
//! The actual colour of a vehicle, sampled from the crop rather than taken
//! from a model's word for it. Most of a vehicle crop is not paint -- glass
//! and tyres are near-black, chrome and sunlight are near-white, the edges
//! are road, grass and sky -- so pixels are weighted towards the centre and
//! the biggest brightness or hue cluster wins, not the mean of everything.
//! See the Python module's docstring for the full reasoning.

use crate::imageops::{Filter, Rgb};

/// The middle of the box, as fractions (x1, y1, x2, y2): cars are
/// photographed side-on, so the crop's edges tend to be background.
const BODY_REGION: (f64, f64, f64, f64) = (0.15, 0.22, 0.85, 0.72);

const MIN_VALUE: i16 = 45; // below this is glass, tyre or shadow, not paint
const MAX_VALUE: i16 = 245; // above this is a specular highlight or blown sky
const MIN_KEPT: f32 = 0.04; // if filtering leaves less than this, it filtered too hard
const BAND_SHARE: f32 = 0.18; // a brightness band worth calling the paint

/// A hex colour representing the vehicle's paint, or `None`.
pub fn dominant(image: &Rgb) -> Option<String> {
    let (w, h) = (image.width, image.height);
    if w < 8 || h < 8 {
        return None;
    }
    let (x1f, y1f, x2f, y2f) = BODY_REGION;
    let (x1, y1, x2, y2) = (
        (w as f64 * x1f) as usize,
        (h as f64 * y1f) as usize,
        (w as f64 * x2f) as usize,
        (h as f64 * y2f) as usize,
    );
    let mut body = image.crop(x1, y1, x2, y2);
    if body.width.min(body.height) < 4 {
        body = image.clone();
    }
    let body = body.resize(64, 64, Filter::Lanczos);
    let hsv = body.to_hsv();
    let weight = centre_weight(64, 64);

    let value: Vec<i16> = hsv
        .data
        .as_chunks::<3>()
        .0
        .iter()
        .map(|p| i16::from(p[2]))
        .collect();
    let mut keep: Vec<usize> = (0..value.len())
        .filter(|&i| value[i] >= MIN_VALUE && value[i] <= MAX_VALUE)
        .collect();
    if (keep.len() as f32 / value.len() as f32) < MIN_KEPT {
        // A genuinely black or genuinely white car: nothing survives the
        // filter, so describe everything rather than invent a mid-tone.
        keep = (0..value.len()).collect();
    }

    let sat: Vec<i16> = keep
        .iter()
        .map(|&i| i16::from(hsv.data[i * 3 + 1]))
        .collect();
    let kept_weight: Vec<f32> = keep.iter().map(|&i| weight[i]).collect();

    if weighted_median(&sat, &kept_weight) < 40.0 {
        // Black, white, silver, grey. Hue is meaningless here; cluster by
        // brightness and take the biggest band, which is the paint.
        let kept_value: Vec<i16> = keep.iter().map(|&i| value[i]).collect();
        let kept_pixels: Vec<[u8; 3]> = keep.iter().map(|&i| pixel(&body, i)).collect();
        return Some(hex(biggest_band(&kept_pixels, &kept_value, &kept_weight)));
    }

    let colourful: Vec<usize> = (0..keep.len()).filter(|&j| sat[j] >= 40).collect();
    let coloured: Vec<[u8; 3]> = colourful.iter().map(|&j| pixel(&body, keep[j])).collect();
    let cw: Vec<f32> = colourful.iter().map(|&j| kept_weight[j]).collect();
    let hue: Vec<f32> = colourful
        .iter()
        .map(|&j| f32::from(hsv.data[keep[j] * 3]) / 256.0)
        .collect();

    // Hue is circular, so bin it and take the fullest bin with its neighbours
    // rather than a median that would put a red car halfway round the wheel.
    let bins = 18usize;
    let index: Vec<usize> = hue
        .iter()
        .map(|&h| ((h * bins as f32) as i32).clamp(0, bins as i32 - 1) as usize)
        .collect();
    let mut counts = vec![0f32; bins];
    for (&b, &w) in index.iter().zip(&cw) {
        counts[b] += w;
    }
    let peak = argmax(&counts);
    let near_bins = [(peak + bins - 1) % bins, peak, (peak + 1) % bins];
    let mut near: Vec<usize> = (0..index.len())
        .filter(|&j| near_bins.contains(&index[j]))
        .collect();
    if near.len() < 8 {
        near = (0..index.len()).collect();
    }
    let near_pixels: Vec<[u8; 3]> = near.iter().map(|&j| coloured[j]).collect();
    let near_weight: Vec<f32> = near.iter().map(|&j| cw[j]).collect();
    Some(hex(weighted_median_rgb(&near_pixels, &near_weight)))
}

fn pixel(rgb: &Rgb, i: usize) -> [u8; 3] {
    [rgb.data[i * 3], rgb.data[i * 3 + 1], rgb.data[i * 3 + 2]]
}

fn argmax(values: &[f32]) -> usize {
    values
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Falls off towards the edges, where the background is.
fn centre_weight(w: usize, h: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(w * h);
    for y in 0..h {
        let yy = -1.0 + 2.0 * y as f64 / (h - 1) as f64;
        for x in 0..w {
            let xx = -1.0 + 2.0 * x as f64 / (w - 1) as f64;
            out.push((-(xx * xx + yy * yy) * 1.6).exp() as f32);
        }
    }
    out
}

fn biggest_band(pixels: &[[u8; 3]], value: &[i16], weight: &[f32]) -> [f64; 3] {
    let bands = 8usize;
    let index: Vec<usize> = value
        .iter()
        .map(|&v| ((v as f32 / 256.0 * bands as f32) as i32).clamp(0, bands as i32 - 1) as usize)
        .collect();
    let mut counts = vec![0f32; bands];
    for (&b, &w) in index.iter().zip(weight) {
        counts[b] += w;
    }
    let total: f32 = counts.iter().sum();
    // The brightest band that is a substantial part of the crop, not merely
    // the biggest one: a white ute photographed head-on is mostly windscreen,
    // grille and shadow, so the biggest band is dark and the answer came back
    // grey. A black car has no substantial bright band, so it still reads
    // black.
    let substantial: Vec<usize> = (0..bands)
        .filter(|&i| total > 0.0 && counts[i] / total >= BAND_SHARE)
        .collect();
    let peak = substantial
        .into_iter()
        .max()
        .unwrap_or_else(|| argmax(&counts));
    let near_bands = [peak.saturating_sub(1), peak, (peak + 1).min(bands - 1)];
    let mut near: Vec<usize> = (0..index.len())
        .filter(|&j| near_bands.contains(&index[j]))
        .collect();
    if near.len() < 8 {
        near = (0..index.len()).collect();
    }
    let near_pixels: Vec<[u8; 3]> = near.iter().map(|&j| pixels[j]).collect();
    let near_weight: Vec<f32> = near.iter().map(|&j| weight[j]).collect();
    weighted_median_rgb(&near_pixels, &near_weight)
}

/// `np.searchsorted(cumsum(weight), total / 2)`: the value where half the
/// weight lies on each side, falling back to an unweighted median when every
/// weight is zero, same as the Python side.
fn weighted_median(values: &[i16], weight: &[f32]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut order: Vec<usize> = (0..values.len()).collect();
    order.sort_by_key(|&i| values[i]);
    let total: f64 = weight.iter().map(|&w| f64::from(w)).sum();
    if total <= 0.0 {
        let mut v: Vec<i16> = values.to_vec();
        v.sort();
        let n = v.len();
        return if n % 2 == 1 {
            f64::from(v[n / 2])
        } else {
            (f64::from(v[n / 2 - 1]) + f64::from(v[n / 2])) / 2.0
        };
    }
    let half = total / 2.0;
    let mut cumulative = 0.0;
    for &i in &order {
        cumulative += f64::from(weight[i]);
        if cumulative >= half {
            return f64::from(values[i]);
        }
    }
    f64::from(values[*order.last().unwrap()])
}

fn weighted_median_rgb(pixels: &[[u8; 3]], weight: &[f32]) -> [f64; 3] {
    if pixels.is_empty() {
        return [0.0, 0.0, 0.0];
    }
    std::array::from_fn(|c| {
        let channel: Vec<i16> = pixels.iter().map(|p| i16::from(p[c])).collect();
        weighted_median(&channel, weight)
    })
}

fn hex(values: [f64; 3]) -> String {
    let byte = |v: f64| v.round().clamp(0.0, 255.0) as u8;
    format!(
        "#{:02x}{:02x}{:02x}",
        byte(values[0]),
        byte(values[1]),
        byte(values[2])
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: usize, h: usize, rgb: [u8; 3]) -> Rgb {
        Rgb::new(w, h, rgb.repeat(w * h))
    }

    #[test]
    fn too_small_is_none() {
        assert_eq!(dominant(&solid(4, 4, [200, 30, 30])), None);
    }

    #[test]
    fn a_solid_red_crop_reads_red() {
        assert_eq!(
            dominant(&solid(64, 64, [200, 30, 30])),
            Some("#c81e1e".to_string())
        );
    }

    #[test]
    fn a_solid_black_crop_reads_black() {
        assert_eq!(
            dominant(&solid(64, 64, [10, 10, 10])),
            Some("#0a0a0a".to_string())
        );
    }

    #[test]
    fn a_solid_white_crop_reads_white() {
        assert_eq!(
            dominant(&solid(64, 64, [250, 250, 250])),
            Some("#fafafa".to_string())
        );
    }
}
