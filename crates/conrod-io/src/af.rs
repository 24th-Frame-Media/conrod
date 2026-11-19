//! The camera's autofocus area, as fractions of the upright frame.
//!
//! Each maker records the AF point in its own frame: Canon and Nikon as an
//! offset from the centre of an "AF image" canvas, Sony and Fuji as a pixel in
//! the image, Panasonic as a fraction of it. exiftool normalises the tag names
//! but not the geometry, so [`parse`] does the arithmetic and returns 0..1 of
//! the frame as it is displayed, the space the rest of Conrod measures a crop
//! in.
//!
//! None of this is needed for a scan to succeed: a frame with no AF metadata, a
//! body this module does not know, or a file cropped after capture simply has
//! no AF area, and [`read`] answers `None` for it rather than failing.

use crate::canon_af::CanonAf;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The camera's autofocus area, as fractions of the upright frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AfArea {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
    /// The camera's own AF mode number, when it reports one.
    pub mode: Option<u8>,
}

/// One exiftool `-n -json -G1` record's tags by bare name: `"Canon:AFAreaWidths"`
/// becomes `"AFAreaWidths"`. The group is not trusted, because the same tag can
/// appear in the makernote and again in a timed-metadata track; the first one to
/// carry a name wins.
struct Tags<'a> {
    values: HashMap<&'a str, &'a Value>,
}

impl<'a> Tags<'a> {
    fn new(record: &'a Value) -> Tags<'a> {
        let mut values = HashMap::new();
        if let Some(map) = record.as_object() {
            for (key, value) in map {
                let bare = key.rsplit(':').next().unwrap_or(key.as_str());
                values.entry(bare).or_insert(value);
            }
        }
        Tags { values }
    }

    fn get(&self, name: &str) -> Option<&'a Value> {
        self.values.get(name).copied()
    }

    /// The first number in a tag, whether it is a scalar, a `-n` string like
    /// `"2631 0 0"`, or a JSON array.
    fn first(&self, name: &str) -> Option<f64> {
        self.numbers(name).into_iter().next()
    }

    fn numbers(&self, name: &str) -> Vec<f64> {
        self.get(name).map(number_list).unwrap_or_default()
    }

    fn text(&self, name: &str) -> Option<&'a str> {
        match self.get(name)? {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    /// `AFAreaMode` is a plain number in `-n` output, but comes as text on a
    /// body whose value exiftool prints as a name.
    fn mode(&self) -> Option<u8> {
        let mode = self.first("AFAreaMode")?;
        (0.0..=255.0).contains(&mode).then_some(mode as u8)
    }
}

/// All the numbers in a tag, however exiftool wrote it: a scalar, a
/// space-separated string (`-n` prints `AFAreaWidths` as one), or an array.
fn number_list(value: &Value) -> Vec<f64> {
    match value {
        Value::Number(n) => n.as_f64().into_iter().collect(),
        Value::String(s) => s
            .split_whitespace()
            .filter_map(|t| t.parse().ok())
            .collect(),
        Value::Array(items) => items.iter().flat_map(number_list).collect(),
        _ => Vec::new(),
    }
}

/// A box in a maker's own canvas: the canvas size, then the box in its pixels.
type PixelBox = (f64, f64, f64, f64, f64, f64);

/// The makers' readers, in the order tried when the record carries no `Make`
/// this module recognises.
const MAKERS: [Maker; 5] = [canon, nikon, sony, fuji, panasonic];

type Maker = fn(&Tags, (f64, f64)) -> Option<PixelBox>;

/// Parse one `exiftool -n -json -G1` record.
///
/// `frame` is the upright frame's size, in the same orientation as `orientation`
/// says it is displayed: on a frame turned on its side its aspect is the canvas's
/// transposed, which is why a portrait shot of the same sensor still matches.
pub fn parse(record: &Value, frame: (u32, u32), orientation: u16) -> Option<AfArea> {
    let (fw, fh) = (f64::from(frame.0), f64::from(frame.1));
    if fw <= 0.0 || fh <= 0.0 || !matches!(orientation, 0 | 1 | 3 | 6 | 8) {
        return None;
    }
    // The AF tags are in sensor order, so on a frame that is displayed on its
    // side the canvas must be compared with the sensor's own dimensions.
    let sensor = if matches!(orientation, 6 | 8) {
        (fh, fw)
    } else {
        (fw, fh)
    };
    let tags = Tags::new(record);
    // Canon's bitmap can name the point the camera used even when the record
    // lists a whole grid (`ValidAFPoints` 651 on an R7) or a nine-point zone:
    // one bit is one point, and the arrays give its cell.
    if let Some(area) = canon_bit(&tags, sensor, orientation) {
        return Some(area);
    }
    // A record naming more than one valid point is a zone or the body's whole
    // grid (`ValidAFPoints` 9 on a 1500D, 651 on an R7): its arrays list every
    // point in the zone, and the first is just the grid's first cell, not where
    // the camera focused. Every maker shares the arrays, so this is checked
    // before any reader: such a record names no area, the same answer as a zone
    // too large to be a point.
    if tags.first("ValidAFPoints").is_some_and(|n| n > 1.0) {
        return None;
    }
    // A named maker is tried first; the others follow so a wrong or missing
    // `Make` still lets the tags speak for themselves.
    let mut order = [0usize, 1, 2, 3, 4];
    if let Some(i) = tags.text("Make").and_then(maker_index) {
        order.rotate_left(i);
    }
    for i in order {
        let Some(pixels) = MAKERS[i](&tags, sensor) else {
            continue;
        };
        if let Some(area) = finish(pixels, sensor, tags.mode(), orientation) {
            return Some(area);
        }
    }
    None
}

/// Canon's `AFPointsInFocus` bitmap, when the record names exactly one point:
/// that point is the one the camera used, and the arrays give its cell. `None`
/// for no bit or several (a grid names no single point).
fn canon_bit(tags: &Tags, sensor: (f64, f64), orientation: u16) -> Option<AfArea> {
    let count = tags
        .first("NumAFPoints")
        .map(|n| n.clamp(0.0, f64::from(u16::MAX)) as u16)
        .unwrap_or(0);
    let words: Vec<u16> = tags
        .numbers("AFPointsInFocus")
        .into_iter()
        .map(|w| w as i64 as u16)
        .collect();
    let mut named = crate::canon_af::points(words.into_iter(), count);
    let one = named.next()?;
    if named.next().is_some() {
        return None;
    }
    finish(
        canon_point(tags, sensor, one)?,
        sensor,
        tags.mode(),
        orientation,
    )
}

/// A maker's pixel box, checked and turned into the upright frame's fractions.
/// `None` for a degenerate box or a canvas that is not the sensor's shape.
fn finish(
    (cw, ch, x1, y1, x2, y2): PixelBox,
    sensor: (f64, f64),
    mode: Option<u8>,
    orientation: u16,
) -> Option<AfArea> {
    if cw <= 0.0 || ch <= 0.0 || x2 <= x1 || y2 <= y1 || !aspect_matches(cw, ch, sensor) {
        return None;
    }
    to_upright(
        AfArea {
            x1: x1 / cw,
            y1: y1 / ch,
            x2: x2 / cw,
            y2: y2 / ch,
            mode,
        },
        orientation,
    )
}

/// The AF area of a Canon frame, from the record read out of its makernote
/// ([`crate::canon_af`]) instead of from exiftool. The same arithmetic as
/// [`parse`]'s Canon reader, so the two agree: a point's cell, centred at the
/// AF image's centre plus the offset with y flipped, as wide and high as the
/// point says.
///
/// The record's `AFPointsInFocus` bitmap names the point: exactly one bit is
/// the point the camera used, and it wins over `ValidAFPoints` (a 1500D writes
/// nine valid points and one focus bit). With no bit set, a record naming more
/// than one valid point is a zone or a whole grid whose arrays list every point
/// in it, so it names no area.
pub fn from_canon(af: &CanonAf, frame: (u32, u32), orientation: u16) -> Option<AfArea> {
    let (fw, fh) = (f64::from(frame.0), f64::from(frame.1));
    if fw <= 0.0 || fh <= 0.0 || !matches!(orientation, 0 | 1 | 3 | 6 | 8) {
        return None;
    }
    let sensor = if matches!(orientation, 6 | 8) {
        (fh, fw)
    } else {
        (fw, fh)
    };
    let mode = u8::try_from(af.mode).ok();
    let canvas = match (af.canvas.0, af.canvas.1) {
        (cw, ch) if cw > 0 && ch > 0 => (f64::from(cw), f64::from(ch)),
        _ => sensor,
    };
    // One bit names the point; none falls back to the first valid point, and a
    // zone of several valid points names none.
    let index = match af.sole_focus() {
        Some(one) => one,
        None if af.valid > 1 => return None,
        None => 0,
    };
    finish(
        point_box(canvas, af.point(index)?),
        sensor,
        mode,
        orientation,
    )
}

/// One AF point's cell, in the AF canvas's pixels.
fn point_box(canvas: (f64, f64), p: crate::canon_af::AfPoint) -> PixelBox {
    centred(
        canvas,
        canvas.0 / 2.0 + f64::from(p.x),
        canvas.1 / 2.0 - f64::from(p.y),
        f64::from(p.width),
        f64::from(p.height),
    )
}

fn maker_index(make: &str) -> Option<usize> {
    let make = make.to_ascii_uppercase();
    if make.contains("CANON") {
        Some(0)
    } else if make.contains("NIKON") {
        Some(1)
    } else if make.contains("SONY") {
        Some(2)
    } else if make.contains("FUJI") {
        Some(3)
    } else if make.contains("PANASONIC") || make.contains("LUMIX") {
        Some(4)
    } else {
        None
    }
}

/// Whether the AF canvas and the sensor frame agree closely enough for the
/// canvas's pixels to be read as the frame's.
///
/// More than 4% apart means the file was cropped after capture: the AF box
/// would land in the wrong place, so it is better to report no area. Either
/// orientation of the aspect passes, since a portrait shot of the same sensor
/// is the same rectangle turned over.
fn aspect_matches(cw: f64, ch: f64, sensor: (f64, f64)) -> bool {
    let canvas = cw / ch;
    let frame = sensor.0 / sensor.1;
    let close = |a: f64, b: f64| (a / b - 1.0).abs() <= 0.04;
    close(canvas, frame) || close(canvas, 1.0 / frame)
}

/// The AF canvas's size: the maker's own AF image, else the file's image
/// dimensions, else the frame itself.
fn canvas(tags: &Tags, sensor: (f64, f64)) -> (f64, f64) {
    let pair = |w: &str, h: &str| match (tags.first(w), tags.first(h)) {
        (Some(w), Some(h)) if w > 0.0 && h > 0.0 => Some((w, h)),
        _ => None,
    };
    pair("AFImageWidth", "AFImageHeight")
        .or_else(|| pair("ImageWidth", "ImageHeight"))
        .unwrap_or(sensor)
}

/// The first AF point's size, when the maker gives one.
fn area_size(tags: &Tags) -> Option<(f64, f64)> {
    let (w, h) = (tags.first("AFAreaWidths")?, tags.first("AFAreaHeights")?);
    (w > 0.0 && h > 0.0).then_some((w, h))
}

/// Two percent of the canvas's short side: the size assumed for a maker that
/// reports only a point.
fn small_square(canvas: (f64, f64)) -> (f64, f64) {
    let side = 0.02 * canvas.0.min(canvas.1);
    (side, side)
}

/// A box of `w` by `h` centred on (`cx`, `cy`) in the canvas's pixels.
fn centred(canvas: (f64, f64), cx: f64, cy: f64, w: f64, h: f64) -> PixelBox {
    (
        canvas.0,
        canvas.1,
        cx - w / 2.0,
        cy - h / 2.0,
        cx + w / 2.0,
        cy + h / 2.0,
    )
}

/// A point in image pixels, top-left origin, turned into a box.
///
/// A point at the exact top-left corner is how these bodies say "no point", and
/// no real AF area sits there, so it is dropped rather than boxed in the corner.
fn point(tags: &Tags, sensor: (f64, f64), x: f64, y: f64) -> Option<PixelBox> {
    if x <= 0.0 && y <= 0.0 {
        return None;
    }
    let canvas = canvas(tags, sensor);
    let (w, h) = area_size(tags).unwrap_or_else(|| small_square(canvas));
    Some(centred(canvas, x, y, w, h))
}

/// Canon: an offset from the AF canvas's centre, y upwards. Verified on an EOS
/// R7: with the minus, the AF centre fell inside the detected car on 67 of 70
/// frames (42 with the other sign). This is the first point, the one the
/// arrays' owner writes for a single-point record.
fn canon(tags: &Tags, sensor: (f64, f64)) -> Option<PixelBox> {
    canon_point(tags, sensor, 0)
}

/// Canon's `index`th AF point as a pixel box: the same arithmetic as [`canon`],
/// for the point a bitmap names.
fn canon_point(tags: &Tags, sensor: (f64, f64), index: usize) -> Option<PixelBox> {
    let at = |name: &str| tags.numbers(name).get(index).copied();
    let dx = at("AFAreaXPositions")?;
    let dy = at("AFAreaYPositions")?;
    let (w, h) = (at("AFAreaWidths")?, at("AFAreaHeights")?);
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let canvas = canvas(tags, sensor);
    Some(centred(
        canvas,
        canvas.0 / 2.0 + dx,
        canvas.1 / 2.0 - dy,
        w,
        h,
    ))
}

/// Nikon: the same centre-origin offsets as Canon.
fn nikon(tags: &Tags, sensor: (f64, f64)) -> Option<PixelBox> {
    let dx = tags.first("AFAreaXPositions")?;
    let dy = tags.first("AFAreaYPositions")?;
    let canvas = canvas(tags, sensor);
    let (w, h) = area_size(tags).unwrap_or_else(|| small_square(canvas));
    Some(centred(
        canvas,
        canvas.0 / 2.0 + dx,
        canvas.1 / 2.0 - dy,
        w,
        h,
    ))
}

/// Sony: `FocusLocation` is `w h x y` of the point in image pixels, top-left
/// origin. The point is what the AF area is built around; its own w and h are
/// ignored in favour of `AFAreaWidths`/`AFAreaHeights` when those exist.
fn sony(tags: &Tags, sensor: (f64, f64)) -> Option<PixelBox> {
    let location = tags.numbers("FocusLocation");
    if location.len() < 4 {
        return None;
    }
    point(tags, sensor, location[2], location[3])
}

/// Fuji: `FocusPixel` is `x y` in image pixels, top-left origin.
fn fuji(tags: &Tags, sensor: (f64, f64)) -> Option<PixelBox> {
    let pixel = tags.numbers("FocusPixel");
    if pixel.len() < 2 {
        return None;
    }
    point(tags, sensor, pixel[0], pixel[1])
}

/// Panasonic: `AFPointPosition` is already 0..1 of the image, top-left origin.
fn panasonic(tags: &Tags, sensor: (f64, f64)) -> Option<PixelBox> {
    let position = tags.numbers("AFPointPosition");
    if position.len() < 2 || !(0.0..=1.0).contains(&position[0]) {
        return None;
    }
    if !(0.0..=1.0).contains(&position[1]) {
        return None;
    }
    let canvas = canvas(tags, sensor);
    point(tags, sensor, position[0] * canvas.0, position[1] * canvas.1)
}

/// Put the box in the frame's displayed orientation, the way `Rgb::orient`
/// turns the preview: 3 is upside down, 6 a quarter turn clockwise, 8 a quarter
/// turn anticlockwise. The mirrored orientations (2, 4, 5, 7) keep no area: no
/// camera writes AF tags for them.
fn to_upright(area: AfArea, orientation: u16) -> Option<AfArea> {
    let (x1, y1, x2, y2) = (area.x1, area.y1, area.x2, area.y2);
    // Each arm swaps the corners too, so x1 < x2 and y1 < y2 still hold.
    let (x1, y1, x2, y2) = match orientation {
        1 | 0 => (x1, y1, x2, y2),
        3 => (1.0 - x2, 1.0 - y2, 1.0 - x1, 1.0 - y1),
        6 => (1.0 - y2, x1, 1.0 - y1, x2),
        8 => (y1, 1.0 - x2, y2, 1.0 - x1),
        _ => return None,
    };
    let clamp = |v: f64| v.clamp(0.0, 1.0);
    let (x1, y1, x2, y2) = (clamp(x1), clamp(y1), clamp(x2), clamp(y2));
    (x2 > x1 && y2 > y1).then_some(AfArea {
        x1,
        y1,
        x2,
        y2,
        mode: area.mode,
    })
}

/// Read every frame's AF area, one entry per input path in input order.
///
/// exiftool costs about 300ms to start and is Perl, so paths go 200 at a time.
/// A frame with no AF tags, a file that cannot be read, or an exiftool that will
/// not start all answer `None`: the AF area is an extra, never a reason for a
/// scan to fail. `frames` gives each path's upright size and orientation.
pub fn read(
    paths: &[PathBuf],
    exiftool: &Path,
    frames: &HashMap<PathBuf, (u32, u32, u16)>,
) -> Vec<(PathBuf, Option<AfArea>)> {
    let tags = [
        "-AFImageWidth",
        "-AFImageHeight",
        "-AFAreaWidths",
        "-AFAreaHeights",
        "-AFAreaXPositions",
        "-AFAreaYPositions",
        "-AFAreaMode",
        "-ValidAFPoints",
        "-NumAFPoints",
        "-AFPointsInFocus",
        "-AFPointsSelected",
        "-FocusLocation",
        "-FocusPixel",
        "-AFPointPosition",
        "-Make",
        "-ImageWidth",
        "-ImageHeight",
    ];
    let mut out = Vec::with_capacity(paths.len());
    for chunk in paths.chunks(200) {
        let records = read_chunk(exiftool, chunk, &tags);
        for path in chunk {
            let area = records
                .as_ref()
                .and_then(|records| records.get(&path_key(&path.to_string_lossy())))
                .and_then(|record| {
                    let &(w, h, orientation) = frames.get(path)?;
                    parse(record, (w, h), orientation)
                });
            out.push((path.clone(), area));
        }
    }
    out
}

/// One exiftool run, keyed the way [`read`] looks paths up.
fn read_chunk(exiftool: &Path, chunk: &[PathBuf], tags: &[&str]) -> Option<HashMap<String, Value>> {
    let mut cmd = Command::new(exiftool);
    cmd.args(["-n", "-json", "-G1", "-charset", "filename=utf8"])
        .args(tags)
        .args(chunk);
    // A windowed build has no console, so each exiftool spawn would otherwise
    // flash a black window.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd.output().ok()?;
    // exiftool exits non-zero for a file it could not read but still writes the
    // records for the rest, so only an empty stdout is fatal.
    if !out.status.success() && out.stdout.is_empty() {
        return None;
    }
    let records: Vec<Value> = serde_json::from_slice(&out.stdout).ok()?;
    Some(
        records
            .into_iter()
            .filter_map(|record| {
                let source = record.get("SourceFile")?.as_str()?;
                Some((path_key(source), record))
            })
            .collect(),
    )
}

/// exiftool returns `SourceFile` with forward slashes and in the input's case;
/// Windows paths are case-insensitive and a caller may write either slash.
fn path_key(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const R7_FRAME: (u32, u32) = (6960, 4640);

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    /// The R7's record as `exiftool -n -json -G1` writes it: the arrays are one
    /// space-separated string under `Canon:`, and again under `Track4:`.
    fn r7(widths: Value, heights: Value, xs: Value, ys: Value) -> Value {
        json!({
            "SourceFile": "D:/x/20261008_B1000_0001.CR3",
            "Canon:AFImageWidth": 6960,
            "Canon:AFImageHeight": 4640,
            "Track4:AFImageWidth": 6960,
            "Track4:AFImageHeight": 4640,
            "Canon:AFAreaWidths": widths,
            "Canon:AFAreaHeights": heights,
            "Canon:AFAreaXPositions": xs,
            "Canon:AFAreaYPositions": ys,
            "Canon:AFAreaMode": 9,
            "IFD0:Make": "Canon",
            "IFD0:ImageWidth": 6960,
            "IFD0:ImageHeight": 4640,
        })
    }

    #[test]
    fn canon_r7_area_lands_where_the_formula_says() {
        let record = r7(
            json!("2631 0 0"),
            json!("1285 0 0"),
            json!("183 0 0"),
            json!("-196 0 0"),
        );
        let area = parse(&record, R7_FRAME, 1).expect("an AF area");
        // cx = 6960/2 + 183, cy = 4640/2 - (-196), and the box is the area
        // centred there. (2631/2 = 1315.5, 1285/2 = 642.5.)
        assert!(
            near(area.x1, (3480.0 + 183.0 - 1315.5) / 6960.0),
            "{area:?}"
        );
        assert!(
            near(area.x2, (3480.0 + 183.0 + 1315.5) / 6960.0),
            "{area:?}"
        );
        assert!(near(area.y1, (2320.0 + 196.0 - 642.5) / 4640.0), "{area:?}");
        assert!(near(area.y2, (2320.0 + 196.0 + 642.5) / 4640.0), "{area:?}");
        assert_eq!(area.mode, Some(9));
        // The box is the AF area's size, not the canvas's.
        assert!(near(area.x2 - area.x1, 2631.0 / 6960.0));
        assert!(near(area.y2 - area.y1, 1285.0 / 4640.0));
    }

    #[test]
    fn orientation_6_rotates_the_area_clockwise() {
        let record = r7(
            json!("2631 0 0"),
            json!("1285 0 0"),
            json!("183 0 0"),
            json!("-196 0 0"),
        );
        let upright = parse(&record, R7_FRAME, 1).expect("an AF area");
        let turned = parse(&record, (4640, 6960), 6).expect("an AF area");
        // `Rgb::orient` sends source (x, y) to (h-1-y, x), so the box's width
        // and height swap and its top-left corner is the old bottom-left.
        assert!(near(turned.x1, 1.0 - upright.y2), "{turned:?}");
        assert!(near(turned.x2, 1.0 - upright.y1), "{turned:?}");
        assert!(near(turned.y1, upright.x1), "{turned:?}");
        assert!(near(turned.y2, upright.x2), "{turned:?}");
        // 1 - 0.6807.. = 0.3192.., and the width is the old height: 1285/4640,
        // which is 0.2769 of the turned frame's 0..1.
        assert!(near(turned.x2 - turned.x1, 1285.0 / 4640.0));
        assert!(near(turned.y2 - turned.y1, 2631.0 / 6960.0));
        assert_eq!(turned.mode, Some(9));
    }

    #[test]
    fn a_cropped_canvas_aspect_is_rejected() {
        // A 16:9 AF canvas on a 3:2 frame: the file was cropped after capture,
        // so the canvas's pixels no longer line up with the frame.
        let record = json!({
            "Canon:AFImageWidth": 3840,
            "Canon:AFImageHeight": 2160,
            "Canon:AFAreaWidths": "800 0",
            "Canon:AFAreaHeights": "400 0",
            "Canon:AFAreaXPositions": "0 0",
            "Canon:AFAreaYPositions": "0 0",
            "IFD0:Make": "Canon",
            "IFD0:ImageWidth": 3840,
            "IFD0:ImageHeight": 2160,
        });
        assert!(parse(&record, (6000, 4000), 1).is_none());
    }

    #[test]
    fn sony_focus_location_is_a_point_in_pixels() {
        let record = json!({
            "IFD0:Make": "SONY",
            "IFD0:ImageWidth": 6000,
            "IFD0:ImageHeight": 4000,
            "Sony:FocusLocation": "6000 4000 3000 2000",
        });
        let area = parse(&record, (6000, 4000), 1).expect("an AF area");
        assert!(near((area.x1 + area.x2) / 2.0, 0.5), "{area:?}");
        assert!(near((area.y1 + area.y2) / 2.0, 0.5), "{area:?}");
        // No AFAreaWidths/Heights, so 2% of the short side: 80 of 4000 pixels.
        assert!(near((area.x2 - area.x1) * 6000.0, 80.0), "{area:?}");
        assert!(near((area.y2 - area.y1) * 4000.0, 80.0), "{area:?}");
    }

    #[test]
    fn fuji_focus_pixel_is_a_point_in_pixels() {
        let record = json!({
            "IFD0:Make": "FUJIFILM",
            "IFD0:ImageWidth": 6000,
            "IFD0:ImageHeight": 4000,
            "FujiFilm:FocusPixel": "3000 1000",
        });
        let area = parse(&record, (6000, 4000), 1).expect("an AF area");
        assert!(near((area.x1 + area.x2) / 2.0, 0.5), "{area:?}");
        assert!(near((area.y1 + area.y2) / 2.0, 0.25), "{area:?}");
        assert!(near((area.x2 - area.x1) * 6000.0, 80.0), "{area:?}");
    }

    #[test]
    fn an_empty_record_has_no_area() {
        assert!(parse(&json!({}), (6000, 4000), 1).is_none());
        assert!(parse(&json!({ "IFD0:Make": "Canon" }), (6000, 4000), 1).is_none());
    }

    #[test]
    fn string_and_json_arrays_parse_alike() {
        let string = r7(
            json!("2631 0 0"),
            json!("1285 0 0"),
            json!("183 0 0"),
            json!("-196 0 0"),
        );
        let array = r7(
            json!([2631, 0, 0]),
            json!([1285, 0, 0]),
            json!([183, 0, 0]),
            json!([-196, 0, 0]),
        );
        assert!(parse(&string, R7_FRAME, 1).is_some());
        assert_eq!(parse(&string, R7_FRAME, 1), parse(&array, R7_FRAME, 1));
    }

    #[test]
    fn a_mirrored_orientation_has_no_area() {
        let record = r7(
            json!("2631 0 0"),
            json!("1285 0 0"),
            json!("183 0 0"),
            json!("-196 0 0"),
        );
        for orientation in [2u16, 4, 5, 7] {
            assert!(
                parse(&record, R7_FRAME, orientation).is_none(),
                "orientation {orientation}"
            );
        }
    }

    #[test]
    fn a_multi_point_record_is_a_zone_not_a_point() {
        // The 1500D's 9-point zone and the R7's 651-point grid: the arrays
        // list every point, so their first entry is the grid's first cell.
        // With no bitmap naming one of them there is no point to take.
        for valid in [9, 45, 651] {
            let record = json!({
                "Canon:AFImageWidth": 6960,
                "Canon:AFImageHeight": 4640,
                "Canon:AFAreaWidths": "187 187",
                "Canon:AFAreaHeights": "191 191",
                "Canon:AFAreaXPositions": "-3018 -2810",
                "Canon:AFAreaYPositions": "2120 2120",
                "Canon:NumAFPoints": 651,
                "Canon:ValidAFPoints": valid,
                "IFD0:Make": "Canon",
                "IFD0:ImageWidth": 6960,
                "IFD0:ImageHeight": 4640,
            });
            assert!(parse(&record, R7_FRAME, 1).is_none(), "valid {valid}");
        }
        // One valid point is the point the camera used.
        let single = r7(
            json!("445 0 0"),
            json!("631 0 0"),
            json!("-186 0 0"),
            json!("405 0 0"),
        );
        let mut with_valid = single.clone();
        with_valid["Canon:ValidAFPoints"] = json!(1);
        assert!(parse(&with_valid, R7_FRAME, 1).is_some());
    }

    /// A Canon record with `count` points whose areas start at the given cells:
    /// (width, height, x, y) per point, from the AF canvas's centre.
    fn canon_record(count: u16, cells: &[(i64, i64, i64, i64)], in_focus: &[u16]) -> Value {
        let n = usize::from(count);
        let column = |column: usize| -> String {
            (0..n)
                .map(|i| {
                    cells
                        .get(i)
                        .map_or(0, |c| [c.0, c.1, c.2, c.3][column])
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join(" ")
        };
        let words = (0..n.div_ceil(16))
            .map(|w| in_focus.get(w).copied().unwrap_or(0).to_string())
            .collect::<Vec<_>>()
            .join(" ");
        json!({
            "Canon:AFImageWidth": 6960,
            "Canon:AFImageHeight": 4640,
            "Canon:AFAreaWidths": column(0),
            "Canon:AFAreaHeights": column(1),
            "Canon:AFAreaXPositions": column(2),
            "Canon:AFAreaYPositions": column(3),
            "Canon:NumAFPoints": count,
            "Canon:AFAreaMode": 9,
            "Canon:AFPointsInFocus": words,
            "Canon:ValidAFPoints": count,
            "IFD0:Make": "Canon",
            "IFD0:ImageWidth": 6960,
            "IFD0:ImageHeight": 4640,
        })
    }

    #[test]
    fn an_in_focus_bit_names_the_point_even_in_a_full_grid() {
        // 651 points, all valid, and the one bit the 1500D writes on a
        // nine-point zone: the bit is the point the camera used.
        let mut cells = vec![(187, 191, 0, 0); 651];
        cells[0] = (187, 191, -3018, 2120);
        cells[130] = (445, 631, -186, 405);
        let mut focus = vec![0u16; 41];
        focus[8] = 1 << 2; // point 130
        let record = canon_record(651, &cells, &focus);
        let area = parse(&record, R7_FRAME, 1).expect("the point the camera used");
        // The same box the native reader builds from slot 130.
        let native = from_canon(
            &CanonAf {
                mode: 9,
                points: 651,
                valid: 651,
                canvas: (6960, 4640),
                areas: cells
                    .iter()
                    .map(|&(width, height, x, y)| crate::canon_af::AfPoint {
                        width: width as i16,
                        height: height as i16,
                        x: x as i16,
                        y: y as i16,
                    })
                    .collect(),
                in_focus: focus.clone(),
                selected: focus.clone(),
            },
            R7_FRAME,
            1,
        )
        .expect("a box");
        assert_eq!(area, native);
        assert!(near((area.x1 + area.x2) / 2.0, (3480.0 - 186.0) / 6960.0));
        assert!(near((area.y1 + area.y2) / 2.0, (2320.0 - 405.0) / 4640.0));
    }

    #[test]
    fn several_in_focus_bits_still_name_no_point() {
        // A grid's cells, taken together, name no single point: unchanged.
        let cells = [(100, 100, -1000, 500), (100, 100, -800, 500)];
        let record = canon_record(9, &cells, &[0b11]);
        assert_eq!(parse(&record, R7_FRAME, 1), None);
    }

    #[test]
    fn the_native_reader_takes_the_named_point() {
        let cell = |x| crate::canon_af::AfPoint {
            width: 445,
            height: 631,
            x,
            y: 405,
        };
        let mut af = CanonAf {
            mode: 9,
            points: 651,
            valid: 1,
            canvas: (6960, 4640),
            areas: vec![cell(-186), cell(500), cell(900)],
            in_focus: vec![0b10],
            selected: vec![0b10],
        };
        // The one bit wins over the first slot.
        let area = from_canon(&af, R7_FRAME, 1).expect("the named point");
        assert!(near((area.x1 + area.x2) / 2.0, (3480.0 + 500.0) / 6960.0));
        // No bit: a record with one valid point is that point.
        af.in_focus = vec![0];
        let first = from_canon(&af, R7_FRAME, 1).expect("the first point");
        assert!(near((first.x1 + first.x2) / 2.0, (3480.0 - 186.0) / 6960.0));
        // No bit and several valid points: a zone, so no area.
        af.valid = 651;
        assert_eq!(from_canon(&af, R7_FRAME, 1), None);
        // Several bits: still a grid, so still no point.
        af.in_focus = vec![0b110];
        assert_eq!(from_canon(&af, R7_FRAME, 1), None);
    }

    #[test]
    fn paths_match_across_slashes_and_case() {
        assert_eq!(path_key(r"D:\Shoots\A.CR3"), path_key("d:/shoots/a.cr3"));
    }

    /// A real R7 frame from the Bathurst 1000 shoot, with a car where the
    /// detector found it. Ignored because it needs this machine's exiftool and
    /// the read-only D: originals.
    #[test]
    #[ignore = "reads a real CR3 from the D: originals with this machine's exiftool"]
    fn real_canon_af() {
        let exe = Path::new(r"C:\Users\kapsikkum\AppData\Local\Programs\ExifTool\ExifTool.exe");
        let file = PathBuf::from(
            r"D:\24th Frame Media\02_Projects\2026-10-06_Bathurst-1000\01_RAW\2026-10-08_Thursday\20261008_B1000_4692.CR3",
        );
        if !exe.exists() || !file.exists() {
            eprintln!(
                "skipping: {} or {} is missing",
                exe.display(),
                file.display()
            );
            return;
        }
        // The frame's own upright size, as the preview decode reports it.
        let mut frames = HashMap::new();
        frames.insert(file.clone(), (6960, 4640, 1));
        let got = read(&[file], exe, &frames);
        let area = got[0].1.expect("the R7 writes an AF area");
        println!("{} -> {area:?}", got[0].0.display());
        assert!(
            area.x2 > area.x1 && area.y2 > area.y1,
            "empty area: {area:?}"
        );
        let (cx, cy) = ((area.x1 + area.x2) / 2.0, (area.y1 + area.y2) / 2.0);
        assert!((0.05..=0.75).contains(&cx), "x {cx} outside the car");
        assert!((0.3..=0.75).contains(&cy), "y {cy} outside the car");
    }
}
