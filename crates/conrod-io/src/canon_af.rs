//! Canon's autofocus record, read straight from the makernote.
//!
//! exiftool's `AFAreaWidths`, `AFAreaXPositions` and friends are fields of
//! Canon's `AFInfo2` array (makernote tag 0x26, or 0x3c on the newest bodies).
//! The array is a run of 16-bit words, laid out as exiftool's `Canon::AFInfo2`
//! table says:
//!
//! | word       | field                                        |
//! |------------|----------------------------------------------|
//! | 0          | size of the record, in bytes                 |
//! | 1          | `AFAreaMode`                                 |
//! | 2          | `NumAFPoints`, N                             |
//! | 3          | `ValidAFPoints`                              |
//! | 4, 5       | `CanonImageWidth`, `CanonImageHeight`        |
//! | 6, 7       | `AFImageWidth`, `AFImageHeight`              |
//! | 8 + 0N     | `AFAreaWidths`, N signed words               |
//! | 8 + 1N     | `AFAreaHeights`                              |
//! | 8 + 2N     | `AFAreaXPositions`, from the AF image's centre |
//! | 8 + 3N     | `AFAreaYPositions`, y up                     |
//! | 8 + 4N     | `AFPointsInFocus`, ceil(N/16) bit words      |
//! | 8 + 4N + W | `AFPointsSelected`, the same                 |
//!
//! Every point is kept, because the bitmaps name a point by its index and a
//! grid's area is the union of the cells it names. The older `AFInfo` record
//! (tag 0x12) belongs to PowerShot bodies and carries a single area size, so it
//! is not read.

use crate::tiff::Tiff;

/// Canon makernote tags holding an `AFInfo2`-layout record, in preference order.
const AF_INFO2: u16 = 0x0026;
const AF_INFO3: u16 = 0x003c;

/// One AF point of the record: the size of its area and where its centre sits,
/// in the AF image's own pixels, y up from the canvas's centre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AfPoint {
    pub width: i16,
    pub height: i16,
    pub x: i16,
    pub y: i16,
}

/// A Canon `AFInfo2` record: the area of every AF point and the two bitmaps
/// that say which of them the camera focused on and selected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonAf {
    /// `AFAreaMode`, Canon's own number.
    pub mode: u16,
    /// `NumAFPoints`: how many entries each array has.
    pub points: u16,
    /// `ValidAFPoints`.
    pub valid: u16,
    /// `AFImageWidth` and `AFImageHeight`: the canvas the offsets are in.
    pub canvas: (u16, u16),
    /// One entry per AF point, in the camera's order.
    pub areas: Vec<AfPoint>,
    /// `AFPointsInFocus`: bit p is point p, the low bit of each word first, as
    /// exiftool's `DecodeBits` reads it. Empty when the record stops before the
    /// bitmap, as an older body's may.
    pub in_focus: Vec<u16>,
    /// `AFPointsSelected`, in the same form.
    pub selected: Vec<u16>,
}

impl CanonAf {
    /// The point at `index`, if the record has that many.
    pub fn point(&self, index: usize) -> Option<AfPoint> {
        self.areas.get(index).copied()
    }

    /// The point indices the camera says are in focus, in point order.
    pub fn in_focus(&self) -> impl Iterator<Item = usize> + '_ {
        points(self.in_focus.iter().copied(), self.points)
    }

    /// The point indices the camera says it selected, in point order.
    pub fn selected(&self) -> impl Iterator<Item = usize> + '_ {
        points(self.selected.iter().copied(), self.points)
    }

    /// The one point named in focus, or `None` when none or several are: only
    /// a single bit says which point the camera used.
    pub fn sole_focus(&self) -> Option<usize> {
        let mut bits = self.in_focus();
        let first = bits.next()?;
        bits.next().is_none().then_some(first)
    }

    /// True when the bitmap names several points in focus: a focus grid rather
    /// than one point. Where only the kind matters (a re-score reading the file
    /// again), this saves building the box.
    pub fn names_grid(&self) -> bool {
        self.in_focus().nth(1).is_some()
    }
}

/// The set bits of a bitmap, as point indices, stopping at `count` points (the
/// last word's padding bits are not points). Shared with [`crate::af`], which
/// reads the same bitmap as exiftool prints it.
pub(crate) fn points(words: impl Iterator<Item = u16>, count: u16) -> impl Iterator<Item = usize> {
    let count = usize::from(count);
    words
        .enumerate()
        .flat_map(|(word, bits)| {
            // The low bit first: exiftool's `DecodeBits` tests `1 << i`.
            (0..16).filter_map(move |bit| (bits & (1 << bit) != 0).then_some(word * 16 + bit))
        })
        .take_while(move |p| *p < count)
}

/// The `index`th 16-bit word of `bytes`, or `None` past the end.
fn word(bytes: &[u8], index: usize, little: bool) -> Option<u16> {
    let at = index.checked_mul(2)?;
    let pair: [u8; 2] = bytes.get(at..at.checked_add(2)?)?.try_into().ok()?;
    Some(if little {
        u16::from_le_bytes(pair)
    } else {
        u16::from_be_bytes(pair)
    })
}

/// The bitmap of `words` starting at word `base`, or empty when the record ends
/// first. A record that predates the bitmap still names its first point.
fn bitmap(at: &impl Fn(usize) -> Option<u16>, base: usize, words: usize) -> Vec<u16> {
    (0..words)
        .map(|k| at(base + k))
        .collect::<Option<Vec<u16>>>()
        .unwrap_or_default()
}

/// Decode an `AFInfo2` record. `None` when it is too short to hold every
/// point's area, however large its own point count claims to be.
pub fn parse_info2(bytes: &[u8], little: bool) -> Option<CanonAf> {
    let at = |i: usize| word(bytes, i, little);
    let points = at(2)?;
    if points == 0 {
        return None;
    }
    let n = usize::from(points);
    let signed = |i: usize| at(i).map(|w| w as i16);
    let mut areas = Vec::with_capacity(n.min(bytes.len() / 8));
    for i in 0..n {
        areas.push(AfPoint {
            width: signed(8 + i)?,
            height: signed(8 + n + i)?,
            x: signed(8 + 2 * n + i)?,
            y: signed(8 + 3 * n + i)?,
        });
    }
    let words = n.div_ceil(16);
    let base = 8 + 4 * n;
    Some(CanonAf {
        mode: at(1)?,
        points,
        valid: at(3)?,
        canvas: (at(6)?, at(7)?),
        areas,
        in_focus: bitmap(&at, base, words),
        selected: bitmap(&at, base + words, words),
    })
}

/// The AF record in the Canon makernote IFD at `ifd` of `t`.
pub fn from_makernote(t: &Tiff<'_>, ifd: usize) -> Option<CanonAf> {
    [AF_INFO2, AF_INFO3].into_iter().find_map(|tag| {
        let (at, len) = t.entry_span(ifd, tag)?;
        parse_info2(t.bytes_at(at, len)?, t.is_little())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An AFInfo2 record of `points` entries whose areas are as given, with no
    /// bit set.
    fn record(points: u16, first: [i16; 4], little: bool) -> Vec<u8> {
        build(points, &[first], &[], &[], little)
    }

    /// `areas` fills the arrays in order; `in_focus` and `selected` are the
    /// bitmap words, in point order.
    fn build(
        points: u16,
        areas: &[[i16; 4]],
        in_focus: &[u16],
        selected: &[u16],
        little: bool,
    ) -> Vec<u8> {
        let n = usize::from(points);
        let words = n.div_ceil(16);
        let mut out = vec![0u16; 8 + 4 * n + 2 * words];
        out[0] = (out.len() * 2) as u16;
        out[1] = 9;
        out[2] = points;
        out[3] = 1;
        out[4] = 6960;
        out[5] = 4640;
        out[6] = 6960;
        out[7] = 4640;
        for (p, area) in areas.iter().enumerate().take(n) {
            for (k, v) in area.iter().enumerate() {
                out[8 + k * n + p] = *v as u16;
            }
        }
        for (k, v) in in_focus.iter().enumerate().take(words) {
            out[8 + 4 * n + k] = *v;
        }
        for (k, v) in selected.iter().enumerate().take(words) {
            out[8 + 4 * n + words + k] = *v;
        }
        out.into_iter()
            .flat_map(|w| {
                if little {
                    w.to_le_bytes()
                } else {
                    w.to_be_bytes()
                }
            })
            .collect()
    }

    #[test]
    fn every_point_and_both_bitmaps_are_read() {
        for little in [true, false] {
            let bytes = record(651, [2816, 936, 500, -66], little);
            let af = parse_info2(&bytes, little).expect("a record");
            assert_eq!(af.mode, 9);
            assert_eq!(af.points, 651);
            assert_eq!(af.valid, 1);
            assert_eq!(af.canvas, (6960, 4640));
            assert_eq!(af.areas.len(), 651);
            assert_eq!(
                af.areas[0],
                AfPoint {
                    width: 2816,
                    height: 936,
                    x: 500,
                    y: -66
                }
            );
            assert_eq!(
                af.areas[1],
                AfPoint {
                    width: 0,
                    height: 0,
                    x: 0,
                    y: 0
                }
            );
            assert_eq!(af.in_focus, vec![0; 41]);
            assert_eq!(af.selected, vec![0; 41]);
        }
    }

    /// Bit p is point p, the low bit of each word first: exiftool's own order.
    #[test]
    fn the_bitmaps_name_their_points() {
        let mut areas = [[0i16; 4]; 651];
        areas[0] = [10, 10, 0, 0];
        areas[17] = [10, 10, 1, 1];
        areas[650] = [10, 10, 2, 2];
        // Point 1 is bit 1 of word 0.
        let bytes = build(651, &areas, &[0b10], &[0b1], true);
        let mut af = parse_info2(&bytes, true).expect("a record");
        assert_eq!(af.in_focus().collect::<Vec<_>>(), vec![1]);
        assert_eq!(af.sole_focus(), Some(1));
        assert_eq!(af.selected().collect::<Vec<_>>(), vec![0]);
        // Point 17 is bit 1 of word 1; point 650 is bit 10 of word 40.
        af.in_focus = vec![0; 41];
        af.in_focus[1] = 0b10;
        assert_eq!(af.in_focus().collect::<Vec<_>>(), vec![17]);
        af.in_focus = vec![0; 41];
        af.in_focus[40] = 1 << 10;
        assert_eq!(af.in_focus().collect::<Vec<_>>(), vec![650]);
        af.in_focus[0] = (1 << 12) | 1;
        assert_eq!(af.in_focus().collect::<Vec<_>>(), vec![0, 12, 650]);
        assert_eq!(af.sole_focus(), None, "several bits name no single point");
        af.in_focus = vec![0; 41];
        af.in_focus[0] = 1 << 2;
        assert_eq!(af.sole_focus(), Some(2));
        // A word's padding bits past `NumAFPoints` are not points.
        af.in_focus[40] = 0xFFFF;
        assert_eq!(
            af.in_focus().collect::<Vec<_>>(),
            vec![2, 640, 641, 642, 643, 644, 645, 646, 647, 648, 649, 650]
        );
    }

    #[test]
    fn a_truncated_record_is_none() {
        let bytes = record(651, [2816, 936, 500, 66], true);
        // Every cut-off, down to nothing, is refused rather than panicking. The
        // last Y word is the last one needed, since every point is kept; the
        // bitmaps may be missing.
        let needed = 2 * (8 + 4 * 651);
        for len in 0..bytes.len() {
            let got = parse_info2(&bytes[..len], true);
            assert_eq!(got.is_some(), len >= needed, "len {len}");
            if let Some(af) = got {
                // Without the bitmap's words the set is empty, not a refusal.
                assert_eq!(af.in_focus.is_empty(), len < needed + 2 * 41);
            }
        }
    }

    #[test]
    fn an_absurd_point_count_is_none() {
        let mut bytes = record(9, [1, 2, 3, 4], true);
        // NumAFPoints claims 65535 entries in a 90-byte record.
        bytes[4..6].copy_from_slice(&u16::MAX.to_le_bytes());
        assert_eq!(parse_info2(&bytes, true), None);
    }

    #[test]
    fn zero_points_and_garbage_are_none() {
        assert_eq!(parse_info2(&record(0, [0; 4], true), true), None);
        assert_eq!(parse_info2(&[], true), None);
        assert_eq!(parse_info2(&[0xFF; 7], false), None);
        assert_eq!(parse_info2(&vec![0xFF; 4096], true), None);
    }

    /// A little-endian TIFF whose first IFD holds one UNDEFINED entry `tag`
    /// pointing at `payload`.
    fn tiff_with(tag: u16, payload: &[u8]) -> Vec<u8> {
        let mut out = b"II".to_vec();
        out.extend_from_slice(&42u16.to_le_bytes());
        out.extend_from_slice(&8u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&tag.to_le_bytes());
        out.extend_from_slice(&7u16.to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&26u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn a_makernote_ifd_yields_its_af_record() {
        let payload = record(7, [100, 50, -20, 30], true);
        for tag in [AF_INFO2, AF_INFO3] {
            let bytes = tiff_with(tag, &payload);
            let t = Tiff::new(&bytes).unwrap();
            let af = from_makernote(&t, 8).expect("a record");
            let first = af.point(0).expect("a first point");
            assert_eq!(
                (first.width, first.height, first.x, first.y),
                (100, 50, -20, 30)
            );
        }
        let other = tiff_with(0x0001, &payload);
        assert_eq!(from_makernote(&Tiff::new(&other).unwrap(), 8), None);
    }

    #[test]
    fn a_record_pointing_past_the_buffer_is_none() {
        let payload = record(7, [100, 50, -20, 30], true);
        let mut bytes = tiff_with(AF_INFO2, &payload);
        bytes.truncate(40);
        assert_eq!(from_makernote(&Tiff::new(&bytes).unwrap(), 8), None);
    }
}
