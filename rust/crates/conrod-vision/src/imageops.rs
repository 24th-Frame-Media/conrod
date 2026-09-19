//! Greyscale images and the Pillow operations the sharpness measure depends on.
//!
//! The measure's thresholds were fitted against Pillow's output, so these are
//! ports of Pillow's C code (`Convert.c`, `Resample.c`) rather than "a bilinear
//! resize": same fixed-point arithmetic, same rounding, same pixels out.

/// An 8-bit greyscale image, row-major.
#[derive(Debug, Clone, PartialEq)]
pub struct Gray {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
}

impl Gray {
    pub fn new(width: usize, height: usize, data: Vec<u8>) -> Gray {
        assert_eq!(data.len(), width * height, "pixel count");
        Gray {
            width,
            height,
            data,
        }
    }

    /// `Image.convert("L")` from 8-bit RGB: ITU-R 601 luma in Pillow's
    /// 16-bit fixed point.
    pub fn from_rgb(width: usize, height: usize, rgb: &[u8]) -> Gray {
        let data = rgb
            .as_chunks::<3>()
            .0
            .iter()
            .map(|&[r, g, b]| {
                let (r, g, b) = (u32::from(r), u32::from(g), u32::from(b));
                ((r * 19595 + g * 38470 + b * 7471 + 0x8000) >> 16) as u8
            })
            .collect();
        Gray::new(width, height, data)
    }

    /// Decode a file (PNG or JPEG) and convert it to greyscale the way the
    /// Python side does: RGB first, then `convert("L")`. A greyscale file is
    /// taken as it is.
    pub fn open(path: &std::path::Path) -> image::ImageResult<Gray> {
        let img = image::open(path)?;
        Ok(match img {
            image::DynamicImage::ImageLuma8(g) => {
                let (w, h) = g.dimensions();
                Gray::new(w as usize, h as usize, g.into_raw())
            }
            other => {
                let rgb = other.to_rgb8();
                let (w, h) = rgb.dimensions();
                Gray::from_rgb(w as usize, h as usize, rgb.as_raw())
            }
        })
    }

    /// `Image.thumbnail((edge, edge), BILINEAR)`: shrink to fit, keep the
    /// aspect, never enlarge. Pillow's default `reducing_gap` of 2.0 means a
    /// shrink by four or more first box-averages, which only a whole frame
    /// ever triggers; crops are at most twice the working edge.
    pub fn thumbnail(&self, edge: usize) -> Gray {
        let Some((w, h)) = thumbnail_size(self.width, self.height, edge) else {
            return self.clone();
        };
        if (w, h) == (self.width, self.height) {
            return self.clone();
        }
        let factor_x = ((self.width as f64 / w as f64 / 2.0) as usize).max(1);
        let factor_y = ((self.height as f64 / h as f64 / 2.0) as usize).max(1);
        if factor_x > 1 || factor_y > 1 {
            // ponytail: Pillow also widens the reduce box by the filter's
            // support ("_get_safe_box"); this reduces the whole image and
            // carries the fractional box. Differs by rounding at the edge
            // only. Revisit if whole-frame scores drift from the fixtures.
            let reduced = self.reduce(factor_x, factor_y);
            let bx = self.width as f64 / factor_x as f64;
            let by = self.height as f64 / factor_y as f64;
            return reduced.resize_bilinear_box(w, h, [0.0, 0.0, bx, by]);
        }
        self.resize_bilinear_box(w, h, [0.0, 0.0, self.width as f64, self.height as f64])
    }

    /// Pillow's `Image.reduce`: the mean of each `fx` by `fy` block, the last
    /// partial block averaged over what it has.
    pub fn reduce(&self, fx: usize, fy: usize) -> Gray {
        let (w, h) = (self.width.div_ceil(fx), self.height.div_ceil(fy));
        let mut out = Vec::with_capacity(w * h);
        for by in 0..h {
            let (y0, y1) = (by * fy, ((by + 1) * fy).min(self.height));
            for bx in 0..w {
                let (x0, x1) = (bx * fx, ((bx + 1) * fx).min(self.width));
                let count = ((y1 - y0) * (x1 - x0)) as u64;
                let mut sum = 0u64;
                for y in y0..y1 {
                    for &v in &self.data[y * self.width + x0..y * self.width + x1] {
                        sum += u64::from(v);
                    }
                }
                out.push(((sum + count / 2) / count) as u8);
            }
        }
        Gray::new(w, h, out)
    }

    /// `Image.resize((w, h), BILINEAR, box)`: Pillow's separable convolution
    /// with a triangle filter widened by the scale, horizontal pass first into
    /// an 8-bit intermediate, 22-bit fixed-point coefficients.
    pub fn resize_bilinear_box(&self, w: usize, h: usize, bbox: [f64; 4]) -> Gray {
        let need_h = w != self.width || bbox[0] != 0.0 || bbox[2] != w as f64;
        let need_v = h != self.height || bbox[1] != 0.0 || bbox[3] != h as f64;
        let (bounds_h, kk_h, ksize_h) = coefficients(self.width, bbox[0], bbox[2], w);
        let (mut bounds_v, kk_v, ksize_v) = coefficients(self.height, bbox[1], bbox[3], h);

        let mut current = self.clone();
        if need_h {
            // Only the rows the vertical pass will read.
            let first = bounds_v[0].0;
            let last = bounds_v[h - 1].0 + bounds_v[h - 1].1;
            for b in &mut bounds_v {
                b.0 -= first;
            }
            let rows = last - first;
            let mut out = vec![0u8; w * rows];
            for y in 0..rows {
                let src = &self.data[(y + first) * self.width..(y + first + 1) * self.width];
                for (x, &(start, len)) in bounds_h.iter().enumerate() {
                    let k = &kk_h[x * ksize_h..x * ksize_h + len];
                    let mut ss: i64 = 1 << (PRECISION_BITS - 1);
                    for (i, &kv) in k.iter().enumerate() {
                        ss += i64::from(src[start + i]) * kv;
                    }
                    out[y * w + x] = clip8(ss);
                }
            }
            current = Gray::new(w, rows, out);
        }
        if need_v {
            let width = current.width;
            let mut out = vec![0u8; width * h];
            for (y, &(start, len)) in bounds_v.iter().enumerate() {
                let k = &kk_v[y * ksize_v..y * ksize_v + len];
                for x in 0..width {
                    let mut ss: i64 = 1 << (PRECISION_BITS - 1);
                    for (i, &kv) in k.iter().enumerate() {
                        ss += i64::from(current.data[(start + i) * width + x]) * kv;
                    }
                    out[y * width + x] = clip8(ss);
                }
            }
            current = Gray::new(width, h, out);
        }
        current
    }
}

const PRECISION_BITS: u32 = 32 - 8 - 2;

fn clip8(ss: i64) -> u8 {
    if ss >= (1i64 << PRECISION_BITS) << 8 {
        255
    } else if ss <= 0 {
        0
    } else {
        (ss >> PRECISION_BITS) as u8
    }
}

/// Pillow's `precompute_coeffs` + `normalize_coeffs_8bpc` for the bilinear
/// (triangle) filter: per output pixel, the first source index, how many
/// taps, and the fixed-point weights, `ksize` apart.
fn coefficients(
    in_size: usize,
    in0: f64,
    in1: f64,
    out_size: usize,
) -> (Vec<(usize, usize)>, Vec<i64>, usize) {
    let scale = (in1 - in0) / out_size as f64;
    let filterscale = scale.max(1.0);
    let support = filterscale; // the triangle's support is 1
    let ksize = support.ceil() as usize * 2 + 1;
    let mut bounds = Vec::with_capacity(out_size);
    let mut kk = vec![0i64; out_size * ksize];
    for xx in 0..out_size {
        let center = in0 + (xx as f64 + 0.5) * scale;
        let ss = 1.0 / filterscale;
        // C's (int) cast: truncation toward zero, then clamped.
        let xmin = ((center - support + 0.5) as i64).max(0) as usize;
        let xmax = ((center + support + 0.5) as i64).min(in_size as i64) as usize;
        let taps = xmax.saturating_sub(xmin);
        let mut weights: Vec<f64> = (0..taps)
            .map(|x| {
                let t = ((x + xmin) as f64 - center + 0.5) * ss;
                (1.0 - t.abs()).max(0.0)
            })
            .collect();
        let total: f64 = weights.iter().sum();
        if total != 0.0 {
            for w in &mut weights {
                *w /= total;
            }
        }
        for (x, w) in weights.iter().enumerate() {
            let fixed = w * f64::from(1u32 << PRECISION_BITS);
            kk[xx * ksize + x] = if *w < 0.0 {
                (fixed - 0.5) as i64
            } else {
                (fixed + 0.5) as i64
            };
        }
        bounds.push((xmin, taps));
    }
    (bounds, kk, ksize)
}

/// Pillow's thumbnail sizing: fit inside `edge` x `edge`, keep the aspect,
/// rounding each way to whichever integer keeps the aspect closest. `None`
/// when the image already fits.
pub fn thumbnail_size(width: usize, height: usize, edge: usize) -> Option<(usize, usize)> {
    let (mut x, mut y) = (edge as f64, edge as f64);
    if x >= width as f64 && y >= height as f64 {
        return None;
    }
    let aspect = width as f64 / height as f64;
    let round_aspect = |n: f64, key: &dyn Fn(f64) -> f64| -> f64 {
        let (lo, hi) = (n.floor(), n.ceil());
        // min() keeps the first of equals, so a tie goes to the floor.
        let pick = if key(hi) < key(lo) { hi } else { lo };
        pick.max(1.0)
    };
    if x / y >= aspect {
        let yy = y;
        x = round_aspect(y * aspect, &|n| (aspect - n / yy).abs());
    } else {
        let xx = x;
        y = round_aspect(x / aspect, &|n| {
            if n == 0.0 {
                0.0
            } else {
                (aspect - xx / n).abs()
            }
        });
    }
    Some((x as usize, y as usize))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn luma_matches_pillow_on_the_corners() {
        let g = Gray::from_rgb(4, 1, &[255, 255, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0]);
        // Pillow: white 255, black 0, red 76, green 150.
        assert_eq!(g.data, vec![255, 0, 76, 150]);
    }

    #[test]
    fn thumbnail_sizes_match_pillow() {
        assert_eq!(thumbnail_size(2048, 1365, 1024), Some((1024, 683)));
        assert_eq!(thumbnail_size(1365, 2048, 1024), Some((682, 1024)));
        assert_eq!(thumbnail_size(6960, 4640, 1024), Some((1024, 683)));
        assert_eq!(thumbnail_size(800, 600, 1024), None);
    }

    #[test]
    fn a_resize_to_the_same_size_changes_nothing() {
        let g = Gray::new(3, 2, vec![1, 2, 3, 4, 5, 6]);
        assert_eq!(g.resize_bilinear_box(3, 2, [0.0, 0.0, 3.0, 2.0]), g);
    }
}
