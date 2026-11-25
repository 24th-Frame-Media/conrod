//! A frame's own fault: nothing in it is in focus, or it is blown out or
//! crushed beyond what a RAW can give back.
//!
//! Every other rating here is *relative*: how sharp the sharpest edges are
//! against a floor taken from the patch itself, or against the frame's own best
//! tiles. On a frame with nothing in focus that floor sinks to the grain, the
//! grain reads as perfectly sharp edges, and the frame is praised for it (the
//! three smeared frames of an album's burst all rated five stars). The census
//! here is absolute instead: how many real edge pixels the frame holds anywhere
//! (`edgeblur::detail_at`), and how much of the subject, or of the whole frame,
//! sits at clipped white or clipped black (`exposure`).
//!
//! The pixels are read once, at scan or re-score, and only the three numbers are
//! stored (`images.detail_edges`, `white_clip`, `black_clip`). The verdict is
//! derived from them at rating time ([`judge`]), so a preset or a toggle
//! changes it without another read, and a scan, a re-score and a re-rate cannot
//! disagree about it. The frame's rating is capped by [`Flaw::cap`] and its
//! reason stored as `images.flaw`.
use conrod_core::profile::PresetPolicy;
use conrod_core::settings::Settings;
use conrod_vision::edgeblur;
use conrod_vision::exposure::Exposure;
use conrod_vision::imageops::Gray;
use rusqlite::{params, Connection};

/// Gradient (0..255 luma units, after the re-blur) a pixel must clear to count
/// as an edge at all. A well-lit sharp frame has hundreds of thousands of such
/// ridge pixels; grain and a smeared frame's slopes have none.
pub const DETAIL_GRADIENT: f32 = 10.0;
/// Widest an edge may be (native pixels at the 6000 px reference edge) and
/// still count as detail: the moderate scale's zero-score point, 4.5 px, less
/// a margin. Anything wider is smear.
pub const DETAIL_MAX_SIGMA: f64 = 4.0;
/// Ridge pixels (at the 2.5 MP analysis plane) under which a frame has no
/// in-focus detail anywhere. Measured on 3,000 real RAW previews: the sharpest-looking
/// delivered or kept frame has 13,000+; smeared frames have 0-130.
pub const DETAIL_MIN_EDGES: i64 = 500;
/// A subject's own box (aircraft against sky, a bird on a plain wall) saves a
/// frame whose whole-frame count is low when it holds this many sharp ridge
/// pixels: smeared frames drew at most 130 over the *whole* frame.
pub const SUBJECT_MIN_EDGES: i64 = 80;
/// Clipped-white share of the judged region (the subject, else the frame) at or
/// over which a frame is overexposed beyond recovery.
pub const WHITE_FLAW: f64 = 0.55;
/// Clipped-black share of the judged region at or over which a frame is
/// underexposed beyond recovery...
pub const BLACK_FLAW: f64 = 0.90;
/// ...provided the frame as a whole is crushed this much too, so a silhouette
/// against a bright sky, or a lit subject in a dark room, is never one.
pub const BLACK_FRAME_FLAW: f64 = 0.80;
/// What a blurred frame is rated at most: a one-star frame, under the
/// keeper bar by a wide margin.
pub const BLURRED_CAP: f64 = 0.25;
/// What a blown or crushed frame is rated at most.
pub const EXPOSURE_CAP: f64 = 0.40;

/// One fault a frame can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flaw {
    /// No in-focus detail anywhere in the frame.
    Blurred,
    /// Blown out beyond recovery.
    Overexposed,
    /// Crushed to black beyond recovery.
    Underexposed,
}

impl Flaw {
    /// The reason the frame stores (`images.flaw`) and the review shows.
    pub fn reason(self) -> &'static str {
        match self {
            Flaw::Blurred => "blurred",
            Flaw::Overexposed => "overexposed",
            Flaw::Underexposed => "underexposed",
        }
    }

    pub fn parse(reason: &str) -> Option<Flaw> {
        match reason {
            "blurred" => Some(Flaw::Blurred),
            "overexposed" => Some(Flaw::Overexposed),
            "underexposed" => Some(Flaw::Underexposed),
            _ => None,
        }
    }

    /// The most a frame with this fault can be rated.
    pub fn cap(self) -> f64 {
        match self {
            Flaw::Blurred => BLURRED_CAP,
            Flaw::Overexposed | Flaw::Underexposed => EXPOSURE_CAP,
        }
    }
}

/// What a read of one frame's pixels adds up to. `None` means not measured (a
/// frame scanned before the census existed, or one that could not be read):
/// never a fault.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Census {
    /// Strong edge pixels anywhere in the frame, at the 2.5 MP analysis plane.
    pub detail_edges: Option<i64>,
    /// Clipped-white share of the judged region.
    pub white_clip: Option<f64>,
    /// Clipped-black share of the judged region.
    pub black_clip: Option<f64>,
}

/// The region a frame's exposure is judged on, as fractions of the upright
/// frame: the camera's focus area when it is the frame's subject (`af`, already
/// fractions), else the largest of the frame's own subjects (`boxes`: the
/// non-bystander vehicles and people, in frame pixels), else `None` (the whole
/// frame). One function for the scan and a re-score, so they judge the same
/// region.
pub fn subject_box(
    af: Option<[f64; 4]>,
    boxes: impl IntoIterator<Item = [f64; 4]>,
    frame: (usize, usize),
) -> Option<[f64; 4]> {
    if af.is_some() {
        return af;
    }
    let (fw, fh) = (frame.0 as f64, frame.1 as f64);
    boxes
        .into_iter()
        .max_by(|a, b| {
            let area = |b: &[f64; 4]| (b[2] - b[0]) * (b[3] - b[1]);
            area(a).total_cmp(&area(b))
        })
        .map(|b| {
            [
                b[0].max(0.0) / fw,
                b[1].max(0.0) / fh,
                b[2].min(fw) / fw,
                b[3].min(fh) / fh,
            ]
        })
}

/// Read the frame once: its strong edges from `grey`, its clipped shares from
/// the exposure measure (which already holds the subject box it was given).
pub fn census(grey: &Gray, exposure: Option<&Exposure>, subject: Option<[f64; 4]>) -> Census {
    // The cap is stated at the 6000 px reference edge; here it is in this frame's pixels.
    let max_sigma =
        DETAIL_MAX_SIGMA * grey.width.max(grey.height) as f64 / edgeblur::REFERENCE_EDGE as f64;
    let detail = edgeblur::detail(grey, DETAIL_GRADIENT, max_sigma);
    // Edge length grows with linear size, so a smaller plane (a small preview)
    // is brought to the 2.5 MP plane the threshold is set on.
    let scale = if detail.plane > 0 {
        (edgeblur::ANALYSIS_MAX_PIXELS as f64 / detail.plane as f64)
            .sqrt()
            .max(1.0)
    } else {
        1.0
    };
    let mut edges = (detail.plane > 0).then(|| (detail.edges as f64 * scale).round() as i64);
    // A tiny sharp subject in a plain sky (a distant aircraft) draws too few
    // ridge pixels on the downscaled plane to count: read the subject's own box
    // at native size before calling the frame empty. A smeared subject draws
    // none there either.
    if edges.is_some_and(|n| n < DETAIL_MIN_EDGES) {
        if let Some([x0, y0, x1, y1]) = subject {
            let (fw, fh) = (grey.width as f64, grey.height as f64);
            let crop = crate::crop_gray(grey, [x0 * fw, y0 * fh, x1 * fw, y1 * fh]);
            if crop.width >= 16 && crop.height >= 16 {
                let own = edgeblur::detail(&crop, DETAIL_GRADIENT, max_sigma);
                if own.edges as i64 >= SUBJECT_MIN_EDGES {
                    edges = edges.map(|n| n.max(DETAIL_MIN_EDGES));
                }
            }
        }
    }
    Census {
        detail_edges: edges,
        white_clip: exposure.map(|e| e.white_clip),
        black_clip: exposure.map(|e| e.black_clip),
    }
}

/// The frame's fault, if any, from what was measured of it.
///
/// `frame_shadow` is the whole-frame share at or under the crushed-shadow level
/// (`images.shadow_clip`), the second witness of a crushed frame. The exposure
/// faults are the album's to switch off (`cull_exposure`) and the preset's to
/// excuse: dark shoots (night sky, a stage in blackout) are not underexposed,
/// white-studio and white-room shoots are not overexposed, and a condition
/// report keeps everything the lens saw. The blur fault follows `cull_blurred`.
/// `bracketed`: the frame belongs to a keep-together set (bracket, focus stack,
/// interval), which is meant to hold frames that look wrong one by one, so the
/// exposure fault never applies to it.
pub fn judge(
    census: &Census,
    frame_shadow: Option<f64>,
    cfg: &Settings,
    policy: &PresetPolicy,
    bracketed: bool,
) -> Option<Flaw> {
    if cfg.cull_exposure && !policy.deliver_most && !bracketed {
        if !policy.high_key_ok && census.white_clip.is_some_and(|w| w >= WHITE_FLAW) {
            return Some(Flaw::Overexposed);
        }
        if !policy.dark_ok
            && census.black_clip.is_some_and(|b| b >= BLACK_FLAW)
            && frame_shadow.is_some_and(|s| s >= BLACK_FRAME_FLAW)
        {
            return Some(Flaw::Underexposed);
        }
    }
    // A dark shoot's frame that is mostly black has a few pin-point stars or
    // lamps for detail, and that is the picture: it is not "blurred".
    let dark_picture = policy.dark_ok && census.black_clip.is_some_and(|b| b >= 0.5);
    if cfg.cull_blurred
        && !dark_picture
        && census
            .detail_edges
            .is_some_and(|edges| edges < DETAIL_MIN_EDGES)
    {
        return Some(Flaw::Blurred);
    }
    None
}

/// Judge every frame of `job` from its stored census and store the verdict as
/// `images.flaw`: the re-rate half of the rule, run before a re-score, a rescan
/// or a feature toggle rates the frames. A frame never measured keeps no flaw.
pub(crate) fn refresh(conn: &Connection, job: i64, cfg: &Settings) -> Result<(), String> {
    let policy = conrod_core::profile::ShootPreset::parse(&cfg.scan_profile).policy();
    let mut found = conn
        .prepare(
            "SELECT id, detail_edges, white_clip, black_clip, shadow_clip, flaw, \
             set_id IS NOT NULL FROM images WHERE job_id=? AND status='done'",
        )
        .map_err(|e| e.to_string())?;
    type Row = (i64, Census, Option<f64>, Option<String>, bool);
    let rows: Vec<Row> = found
        .query_map([job], |r| {
            Ok((
                r.get(0)?,
                Census {
                    detail_edges: r.get(1)?,
                    white_clip: r.get(2)?,
                    black_clip: r.get(3)?,
                },
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    drop(found);
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    for (id, census, shadow, stored, bracketed) in rows {
        let flaw = judge(&census, shadow, cfg, &policy, bracketed).map(Flaw::reason);
        if flaw != stored.as_deref() {
            tx.execute("UPDATE images SET flaw=? WHERE id=?", params![flaw, id])
                .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())
}

/// Cap the stored rating of every flawed frame of `job` at its flaw's cap: the
/// last word on a frame's rating, whichever rule (the detector's subjects, the
/// camera's focus area, the time-burst rescue) wrote the rating before it.
pub(crate) fn cap_ratings(conn: &Connection, job: i64) -> Result<(), String> {
    conn.execute(
        "UPDATE images SET rating=min(rating, CASE flaw WHEN 'blurred' THEN ?2 \
         WHEN 'overexposed' THEN ?3 WHEN 'underexposed' THEN ?3 END) \
         WHERE job_id=?1 AND flaw IN ('blurred','overexposed','underexposed') \
         AND rating IS NOT NULL AND NOT (set_id IS NOT NULL AND flaw<>'blurred')",
        params![job, BLURRED_CAP, EXPOSURE_CAP],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use conrod_core::profile::ShootPreset;
    use conrod_vision::imageops::Rgb;

    fn cfg() -> Settings {
        Settings::default()
    }

    fn policy(preset: ShootPreset) -> PresetPolicy {
        preset.policy()
    }

    fn counted(edges: Option<i64>, white: f64, black: f64) -> Census {
        Census {
            detail_edges: edges,
            white_clip: Some(white),
            black_clip: Some(black),
        }
    }

    #[test]
    fn a_frame_with_no_edges_anywhere_is_blurred_and_a_sharp_one_is_not() {
        let motorsport = policy(ShootPreset::Motorsport);
        let smeared = counted(Some(0), 0.0, 0.0);
        assert_eq!(
            judge(&smeared, Some(0.0), &cfg(), &motorsport, false),
            Some(Flaw::Blurred)
        );
        let sharp = counted(Some(40_000), 0.0, 0.0);
        assert_eq!(judge(&sharp, Some(0.0), &cfg(), &motorsport, false), None);
        // Never measured is never a fault.
        let unread = Census::default();
        assert_eq!(judge(&unread, None, &cfg(), &motorsport, false), None);
    }

    #[test]
    fn a_blurred_frame_is_capped_under_the_keeper_bar_and_the_toggle_turns_it_off() {
        assert!(Flaw::Blurred.cap() < conrod_vision::sharpness::STAR_BANDS[3].0);
        assert!(Flaw::Overexposed.cap() < conrod_vision::sharpness::KEEPER_MIN_RATING);
        assert!(Flaw::Underexposed.cap() < conrod_vision::sharpness::KEEPER_MIN_RATING);
        let mut off = cfg();
        off.cull_blurred = false;
        assert_eq!(
            judge(
                &counted(Some(0), 0.0, 0.0),
                None,
                &off,
                &policy(ShootPreset::Motorsport),
                false
            ),
            None
        );
    }

    #[test]
    fn a_blown_or_crushed_frame_is_flagged_and_the_album_may_switch_it_off() {
        let mix = policy(ShootPreset::Mix);
        let blown = counted(Some(40_000), 0.9, 0.0);
        assert_eq!(
            judge(&blown, Some(0.0), &cfg(), &mix, false),
            Some(Flaw::Overexposed)
        );
        let crushed = counted(Some(40_000), 0.0, 0.97);
        assert_eq!(
            judge(&crushed, Some(0.95), &cfg(), &mix, false),
            Some(Flaw::Underexposed)
        );
        let mut off = cfg();
        off.cull_exposure = false;
        assert_eq!(judge(&blown, Some(0.0), &off, &mix, false), None);
        assert_eq!(judge(&crushed, Some(0.95), &off, &mix, false), None);
        // A modest clip is not unrecoverable.
        let bright = counted(Some(40_000), 0.2, 0.3);
        assert_eq!(judge(&bright, Some(0.3), &cfg(), &mix, false), None);
    }

    #[test]
    fn a_silhouette_or_a_lit_subject_in_a_dark_room_is_not_underexposed() {
        let mix = policy(ShootPreset::Mix);
        // The subject box is black but the frame is not: a silhouette.
        let silhouette = counted(Some(40_000), 0.0, 0.97);
        assert_eq!(judge(&silhouette, Some(0.3), &cfg(), &mix, false), None);
    }

    #[test]
    fn a_dark_shoot_is_not_underexposed_and_a_high_key_shoot_is_not_overexposed() {
        let crushed = counted(Some(40_000), 0.0, 0.97);
        let blown = counted(Some(40_000), 0.9, 0.0);
        for dark in [
            ShootPreset::Astro,
            ShootPreset::Concert,
            ShootPreset::EventShows,
        ] {
            assert_eq!(
                judge(&crushed, Some(0.95), &cfg(), &policy(dark), false),
                None,
                "{dark:?}"
            );
        }
        for bright in [
            ShootPreset::PropertyListing,
            ShootPreset::PropertyReport,
            ShootPreset::AutoDealer,
            ShootPreset::Wedding,
        ] {
            assert_eq!(
                judge(&blown, Some(0.0), &cfg(), &policy(bright), false),
                None,
                "{bright:?}"
            );
        }
        // The dark preset still flags a blown frame, and the bright one a crushed one.
        assert_eq!(
            judge(
                &blown,
                Some(0.0),
                &cfg(),
                &policy(ShootPreset::Astro),
                false
            ),
            Some(Flaw::Overexposed)
        );
        assert_eq!(
            judge(
                &crushed,
                Some(0.95),
                &cfg(),
                &policy(ShootPreset::PropertyListing),
                false
            ),
            Some(Flaw::Underexposed)
        );
    }

    #[test]
    fn a_condition_report_and_a_bracket_member_keep_their_exposure() {
        let blown = counted(Some(40_000), 0.9, 0.0);
        assert_eq!(
            judge(
                &blown,
                Some(0.0),
                &cfg(),
                &policy(ShootPreset::PropertyReport),
                false
            ),
            None
        );
        assert_eq!(
            judge(&blown, Some(0.0), &cfg(), &policy(ShootPreset::Mix), true),
            None
        );
        // A bracket member that is smeared is still smeared.
        assert_eq!(
            judge(
                &counted(Some(0), 0.0, 0.0),
                None,
                &cfg(),
                &policy(ShootPreset::Mix),
                true
            ),
            Some(Flaw::Blurred)
        );
    }

    #[test]
    fn pin_points_of_light_on_a_dark_sky_are_the_picture_not_a_blur() {
        let stars = counted(Some(10), 0.0, 0.97);
        assert_eq!(
            judge(
                &stars,
                Some(0.95),
                &cfg(),
                &policy(ShootPreset::Astro),
                false
            ),
            None
        );
        // The same on a shoot that is not about the dark is a crushed frame.
        assert_eq!(
            judge(&stars, Some(0.95), &cfg(), &policy(ShootPreset::Mix), false),
            Some(Flaw::Underexposed)
        );
    }

    fn grey_scene(w: usize, h: usize, pixel: impl Fn(usize, usize) -> u8) -> Gray {
        let mut data = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                data.push(pixel(x, y));
            }
        }
        Gray::new(w, h, data)
    }

    fn noise(x: usize, y: usize, amplitude: u32) -> u8 {
        let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA6B);
        h ^= h >> 15;
        h = h.wrapping_mul(0x2C1B_3C6D);
        h ^= h >> 12;
        (128 + (h % (2 * amplitude + 1)) as i32 - amplitude as i32) as u8
    }

    #[test]
    fn grain_alone_draws_no_edges_but_a_small_sharp_subject_in_plain_sky_does() {
        // ISO-style grain on a flat frame: no strong edge anywhere.
        let grain = grey_scene(3000, 2000, |x, y| noise(x, y, 6));
        let c = census(&grain, None, None);
        assert!(c.detail_edges.unwrap() < DETAIL_MIN_EDGES, "{c:?}");
        // A small hard-edged subject on a plain sky with the same grain.
        let aircraft = |x: usize, y: usize| {
            if (1400..1460).contains(&x) && (900..930).contains(&y) {
                40
            } else {
                noise(x, y, 3)
            }
        };
        let sky = grey_scene(3000, 2000, aircraft);
        let seen = census(
            &sky,
            None,
            Some([
                1380.0 / 3000.0,
                880.0 / 2000.0,
                1480.0 / 3000.0,
                950.0 / 2000.0,
            ]),
        );
        assert!(
            seen.detail_edges.unwrap() >= DETAIL_MIN_EDGES,
            "a sharp subject is detail: {seen:?}"
        );
    }

    #[test]
    fn a_smeared_frame_is_blurred_and_a_sharp_pan_and_bokeh_portrait_are_not() {
        let block = |x: usize, y: usize| -> u8 {
            let mut h =
                ((x / 32) as u32).wrapping_mul(73856093) ^ ((y / 32) as u32).wrapping_mul(19349663);
            h ^= h >> 13;
            h = h.wrapping_mul(2654435761);
            if h & 1 == 0 {
                70
            } else {
                190
            }
        };
        let mix = policy(ShootPreset::Mix);
        // Sharp blocks everywhere.
        let sharp = grey_scene(3000, 2000, block);
        let c = census(&sharp, None, None);
        assert_eq!(judge(&c, None, &cfg(), &mix, false), None, "{c:?}");
        // The same frame smeared by hand: a 90 px box blur along both axes.
        let smear = |src: &Gray, along_x: bool| -> Gray {
            let len = 91usize;
            grey_scene(src.width, src.height, |x, y| {
                let mut acc = 0u32;
                for i in 0..len {
                    let o = i as isize - (len / 2) as isize;
                    let (sx, sy) = if along_x {
                        (
                            (x as isize + o).clamp(0, src.width as isize - 1) as usize,
                            y,
                        )
                    } else {
                        (
                            x,
                            (y as isize + o).clamp(0, src.height as isize - 1) as usize,
                        )
                    };
                    acc += u32::from(src.data[sy * src.width + sx]);
                }
                (acc / len as u32) as u8
            })
        };
        let smeared = smear(&smear(&sharp, true), false);
        let c = census(&smeared, None, None);
        assert_eq!(
            judge(&c, None, &cfg(), &mix, false),
            Some(Flaw::Blurred),
            "{c:?}"
        );
        // A pan: a sharp car, a background streaked along x only.
        let streak = smear(&smear(&sharp, true), true);
        let pan = grey_scene(3000, 2000, |x, y| {
            if (800..1800).contains(&x) && (600..1300).contains(&y) {
                sharp.data[y * 3000 + x]
            } else {
                streak.data[y * 3000 + x]
            }
        });
        let c = census(&pan, None, None);
        assert_eq!(judge(&c, None, &cfg(), &mix, false), None, "{c:?}");
        // Bokeh: a sharp subject on a smooth, defocused background.
        let bokeh = grey_scene(3000, 2000, |x, y| {
            if (800..1800).contains(&x) && (600..1300).contains(&y) {
                sharp.data[y * 3000 + x]
            } else {
                128
            }
        });
        let c = census(&bokeh, None, None);
        assert_eq!(judge(&c, None, &cfg(), &mix, false), None, "{c:?}");
    }

    #[test]
    fn a_blown_frame_is_read_from_the_pixels() {
        let white = Rgb::new(600, 400, vec![255; 600 * 400 * 3]);
        let black = Rgb::new(600, 400, vec![0; 600 * 400 * 3]);
        let blown = conrod_vision::exposure::measure(&white, None);
        let crushed = conrod_vision::exposure::measure(&black, None);
        let mix = policy(ShootPreset::Mix);
        let grey = |rgb: &Rgb| rgb.to_gray();
        let c = census(&grey(&white), Some(&blown), None);
        assert_eq!(
            judge(&c, Some(blown.shadow_clip), &cfg(), &mix, false),
            Some(Flaw::Overexposed)
        );
        let c = census(&grey(&black), Some(&crushed), None);
        assert_eq!(
            judge(&c, Some(crushed.shadow_clip), &cfg(), &mix, false),
            Some(Flaw::Underexposed)
        );
    }
}
