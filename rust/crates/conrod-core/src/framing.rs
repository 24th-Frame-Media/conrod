//! Is the subject actually in the frame?
//!
//! Port of `conrod/framing.py`. A vehicle that runs off the edge of the
//! photograph is a worse picture than the same vehicle inside it, however sharp
//! the visible part is, so this is measured apart from focus and applied to the
//! rating afterwards. It is arithmetic on the detector's box, not a model.

/// How close to the edge still counts as touching it, as a share of the frame's
/// width or height. Detector boxes rarely land exactly on the boundary.
pub const EDGE_TOLERANCE: f64 = 0.004;

/// How hard clipping bites: one edge is a blemish, two are serious, three leave
/// a rating that will not survive the cull.
pub const PENALTY: f64 = 0.6;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Framing {
    /// How many frame edges the subject runs off.
    pub sides: u32,
    /// What to multiply the rating by.
    pub factor: f64,
}

impl Default for Framing {
    fn default() -> Self {
        Framing {
            sides: 0,
            factor: 1.0,
        }
    }
}

impl Framing {
    /// Enough of the subject is missing to be worth saying out loud.
    pub fn cut_off(&self) -> bool {
        self.sides >= 2
    }
}

/// How much of the subject the frame edge has taken. A missing box or a frame
/// with no size is "nothing clipped", never an error.
pub fn assess(bbox: Option<[f64; 4]>, frame_width: i64, frame_height: i64) -> Framing {
    let Some([x1, y1, x2, y2]) = bbox else {
        return Framing::default();
    };
    if frame_width <= 0 || frame_height <= 0 {
        return Framing::default();
    }
    let (width, height) = (frame_width as f64, frame_height as f64);
    let margin_x = width * EDGE_TOLERANCE;
    let margin_y = height * EDGE_TOLERANCE;
    let sides = [
        x1 <= margin_x,
        y1 <= margin_y,
        x2 >= width - margin_x,
        y2 >= height - margin_y,
    ]
    .iter()
    .filter(|&&touching| touching)
    .count() as u32;
    Framing {
        sides,
        factor: 1.0 - PENALTY * (f64::from(sides) / 4.0),
    }
}

/// What to tell someone looking at the card.
pub fn describe(framing: &Framing) -> String {
    match framing.sides {
        0 => String::new(),
        1 => "touches the frame edge".to_string(),
        n => format!("cut off on {n} edges"),
    }
}
