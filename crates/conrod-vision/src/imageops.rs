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
            // ponytail: reduces the whole image and carries the fractional
            // box rather than widening it by the filter's support, so edge
            // pixels round slightly differently. Revisit if whole-frame
            // scores drift from the fixtures.
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
        self.resize_box(w, h, bbox, Filter::Bilinear)
    }

    /// `Image.resize((w, h), filter)` over the whole image.
    pub fn resize(&self, w: usize, h: usize, filter: Filter) -> Gray {
        self.resize_box(
            w,
            h,
            [0.0, 0.0, self.width as f64, self.height as f64],
            filter,
        )
    }

    /// Pillow's separable resample for any of its convolution filters.
    pub fn resize_box(&self, w: usize, h: usize, bbox: [f64; 4], filter: Filter) -> Gray {
        let need_h = w != self.width || bbox[0] != 0.0 || bbox[2] != w as f64;
        let need_v = h != self.height || bbox[1] != 0.0 || bbox[3] != h as f64;
        let (bounds_h, kk_h, ksize_h) = coefficients(self.width, bbox[0], bbox[2], w, filter);
        let (mut bounds_v, kk_v, ksize_v) = coefficients(self.height, bbox[1], bbox[3], h, filter);

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

    /// `cv2.resize(..., INTER_LINEAR)` on a single channel.
    pub fn resize_cv2_linear(&self, dw: usize, dh: usize) -> Gray {
        let xs = cv2_linear_taps(dw, self.width);
        let ys = cv2_linear_taps(dh, self.height);
        let row = |y: usize| -> Vec<i32> {
            let src = &self.data[y * self.width..(y + 1) * self.width];
            xs.iter()
                .map(|&(s0, s1, a0, a1)| i32::from(src[s0]) * a0 + i32::from(src[s1]) * a1)
                .collect()
        };
        let mut out = Vec::with_capacity(dw * dh);
        for &(s0, s1, b0, b1) in &ys {
            let (r0, r1) = (row(s0), row(s1));
            for (&v0, &v1) in r0.iter().zip(&r1) {
                let v = (((b0 * (v0 >> 4)) >> 16) + ((b1 * (v1 >> 4)) >> 16) + 2) >> 2;
                out.push(v.clamp(0, 255) as u8);
            }
        }
        Gray::new(dw, dh, out)
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

/// Pillow's convolution filters, as `Resample.c` defines them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    Bilinear,
    Lanczos,
}

impl Filter {
    fn support(self) -> f64 {
        match self {
            Filter::Bilinear => 1.0,
            Filter::Lanczos => 3.0,
        }
    }

    fn weight(self, x: f64) -> f64 {
        let sinc = |x: f64| {
            if x == 0.0 {
                1.0
            } else {
                (x * std::f64::consts::PI).sin() / (x * std::f64::consts::PI)
            }
        };
        match self {
            Filter::Bilinear => (1.0 - x.abs()).max(0.0),
            Filter::Lanczos if (-3.0..3.0).contains(&x) => sinc(x) * sinc(x / 3.0),
            Filter::Lanczos => 0.0,
        }
    }
}

/// Pillow's `precompute_coeffs` + `normalize_coeffs_8bpc`: per output pixel,
/// the first source index, how many taps, and the fixed-point weights,
/// `ksize` apart.
fn coefficients(
    in_size: usize,
    in0: f64,
    in1: f64,
    out_size: usize,
    filter: Filter,
) -> (Vec<(usize, usize)>, Vec<i64>, usize) {
    let scale = (in1 - in0) / out_size as f64;
    let filterscale = scale.max(1.0);
    let support = filter.support() * filterscale;
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
            .map(|x| filter.weight(((x + xmin) as f64 - center + 0.5) * ss))
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
    fn crop_is_total() {
        let img = Rgb::new(4, 3, vec![7; 4 * 3 * 3]);
        assert_eq!(
            (img.crop(1, 1, 3, 2).width, img.crop(1, 1, 3, 2).height),
            (2, 1)
        );
        // Outside, inverted and past-the-edge regions are empty rather than a panic.
        for (x0, y0, x1, y1) in [(5, 5, 9, 9), (3, 2, 1, 1), (2, 2, 2, 9), (0, 0, 9, 9)] {
            let c = img.crop(x0, y0, x1, y1);
            assert_eq!(c.data.len(), c.width * c.height * 3);
        }
        assert_eq!(img.crop(5, 5, 9, 9).width * img.crop(5, 5, 9, 9).height, 0);
    }

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

/// An 8-bit RGB image, row-major, interleaved.
#[derive(Debug, Clone, PartialEq)]
pub struct Rgb {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
}

impl Rgb {
    pub fn new(width: usize, height: usize, data: Vec<u8>) -> Rgb {
        assert_eq!(data.len(), width * height * 3, "pixel count");
        Rgb {
            width,
            height,
            data,
        }
    }

    /// Decode an image as RGB, matching Pillow's `convert("RGB")`.
    pub fn open(path: &std::path::Path) -> image::ImageResult<Rgb> {
        let img = image::open(path)?.to_rgb8();
        let (w, h) = img.dimensions();
        Ok(Rgb::new(w as usize, h as usize, img.into_raw()))
    }

    /// Decode a JPEG at full size, or scaled in the DCT by 1/2, 1/4 or 1/8.
    pub fn decode_jpeg(bytes: &[u8], denominator: u16) -> Result<Rgb, String> {
        let mut decoder = jpeg_decoder::Decoder::new(bytes);
        decoder.read_info().map_err(|e| e.to_string())?;
        let info = decoder.info().ok_or("no JPEG header")?;
        if denominator > 1 {
            decoder
                .scale(info.width / denominator, info.height / denominator)
                .map_err(|e| e.to_string())?;
        }
        let pixels = decoder.decode().map_err(|e| e.to_string())?;
        let info = decoder.info().ok_or("no JPEG header")?;
        let (w, h) = (usize::from(info.width), usize::from(info.height));
        match info.pixel_format {
            jpeg_decoder::PixelFormat::RGB24 => Ok(Rgb::new(w, h, pixels)),
            jpeg_decoder::PixelFormat::L8 => Ok(Rgb::new(
                w,
                h,
                pixels.iter().flat_map(|&v| [v, v, v]).collect(),
            )),
            other => Err(format!("unsupported JPEG pixel format {other:?}")),
        }
    }

    pub fn to_gray(&self) -> Gray {
        Gray::from_rgb(self.width, self.height, &self.data)
    }

    /// Pillow's 8-bit `RGB.convert("HSV")`, used by the paint swatch.
    pub fn to_hsv(&self) -> Rgb {
        let mut data = vec![0u8; self.data.len()];
        for (px, out) in self
            .data
            .as_chunks::<3>()
            .0
            .iter()
            .zip(data.as_chunks_mut::<3>().0.iter_mut())
        {
            let (r, g, b) = (px[0], px[1], px[2]);
            let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
            let delta = mx - mn;
            let (h, s) = if delta == 0 {
                (0u8, 0u8)
            } else {
                let s = (255u32 * u32::from(delta) / u32::from(mx)) as u8;
                let d = f64::from(delta);
                let mut h6 = if r == mx {
                    (f64::from(g) - f64::from(b)) / d
                } else if g == mx {
                    2.0 + (f64::from(b) - f64::from(r)) / d
                } else {
                    4.0 + (f64::from(r) - f64::from(g)) / d
                };
                if h6 < 0.0 {
                    h6 += 6.0;
                }
                ((h6 * 42.5) as u8, s)
            };
            out[0] = h;
            out[1] = s;
            out[2] = mx;
        }
        Rgb::new(self.width, self.height, data)
    }

    /// Turn a frame upright per its EXIF orientation, the way the Python
    /// scan baked it into the cached preview (3, 6 and 8; mirrored values are
    /// left alone, as they were).
    pub fn orient(self, orientation: u16) -> Rgb {
        let (w, h) = (self.width, self.height);
        let (nw, nh) = match orientation {
            3 => (w, h),
            6 | 8 => (h, w),
            _ => return self,
        };
        let mut out = Vec::with_capacity(self.data.len());
        for y in 0..nh {
            for x in 0..nw {
                // Pillow's ROTATE_180 for 3, ROTATE_270 (clockwise) for 6,
                // ROTATE_90 for 8.
                let (sx, sy) = match orientation {
                    3 => (w - 1 - x, h - 1 - y),
                    6 => (y, h - 1 - x),
                    _ => (w - 1 - y, x),
                };
                let at = (sy * w + sx) * 3;
                out.extend_from_slice(&self.data[at..at + 3]);
            }
        }
        Rgb::new(nw, nh, out)
    }

    /// The region, clamped to the image; empty (zero width or height) when it
    /// falls outside it or is inverted, never a panic.
    pub fn crop(&self, x0: usize, y0: usize, x1: usize, y1: usize) -> Rgb {
        let (x1, y1) = (x1.min(self.width), y1.min(self.height));
        let (x0, y0) = (x0.min(x1), y0.min(y1));
        let mut out = Vec::with_capacity((x1 - x0) * (y1 - y0) * 3);
        for y in y0..y1 {
            out.extend_from_slice(&self.data[(y * self.width + x0) * 3..(y * self.width + x1) * 3]);
        }
        Rgb::new(x1 - x0, y1 - y0, out)
    }

    /// Pillow's resample on each channel: its RGB path runs the same
    /// arithmetic per band.
    pub fn resize(&self, w: usize, h: usize, filter: Filter) -> Rgb {
        let bands: Vec<Gray> = (0..3)
            .map(|c| {
                let band = self.data.iter().skip(c).step_by(3).copied().collect();
                Gray::new(self.width, self.height, band).resize(w, h, filter)
            })
            .collect();
        let mut out = Vec::with_capacity(w * h * 3);
        for i in 0..w * h {
            out.extend([bands[0].data[i], bands[1].data[i], bands[2].data[i]]);
        }
        Rgb::new(w, h, out)
    }

    /// `cv2.resize(..., INTER_LINEAR)` on 8-bit data, as it runs on x86: two
    /// taps per axis in 11-bit fixed point, and the vectorised vertical pass's
    /// own rounding -- what ultralytics' letterbox feeds the detector.
    pub fn resize_cv2_linear(&self, dw: usize, dh: usize) -> Rgb {
        let xs = cv2_linear_taps(dw, self.width);
        let ys = cv2_linear_taps(dh, self.height);
        // Horizontal pass into 32-bit sums, one row at a time as cv2 does.
        let row = |y: usize| -> Vec<i32> {
            let src = &self.data[y * self.width * 3..(y + 1) * self.width * 3];
            let mut out = Vec::with_capacity(dw * 3);
            for &(s0, s1, a0, a1) in &xs {
                for c in 0..3 {
                    out.push(i32::from(src[s0 * 3 + c]) * a0 + i32::from(src[s1 * 3 + c]) * a1);
                }
            }
            out
        };
        let mut out = Vec::with_capacity(dw * dh * 3);
        for &(s0, s1, b0, b1) in &ys {
            let (r0, r1) = (row(s0), row(s1));
            for (&v0, &v1) in r0.iter().zip(&r1) {
                let v = (((b0 * (v0 >> 4)) >> 16) + ((b1 * (v1 >> 4)) >> 16) + 2) >> 2;
                out.push(v.clamp(0, 255) as u8);
            }
        }
        Rgb::new(dw, dh, out)
    }
}

/// OpenCV's two-tap 11-bit interpolation coefficients shared by RGB and gray.
fn cv2_linear_taps(dst: usize, src: usize) -> Vec<(usize, usize, i32, i32)> {
    let scale = 1.0 / (dst as f64 / src as f64);
    (0..dst)
        .map(|d| {
            let f = ((d as f64 + 0.5) * scale - 0.5) as f32;
            let mut s = f.floor() as isize;
            let mut f = f - s as f32;
            if s < 0 {
                f = 0.0;
                s = 0;
            }
            let mut s = s as usize;
            if s >= src - 1 {
                f = 0.0;
                s = src - 1;
            }
            let a0 = ((1.0 - f) * 2048.0).round() as i32;
            let a1 = (f * 2048.0).round() as i32;
            (s, (s + 1).min(src - 1), a0, a1)
        })
        .collect()
}
