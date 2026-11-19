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
//!
//! Only the first point of each array is kept, the one [`crate::af`] has always
//! used. The older `AFInfo` record (tag 0x12) belongs to PowerShot bodies and
//! carries a single area size, so it is not read.

use crate::tiff::Tiff;

/// Canon makernote tags holding an `AFInfo2`-layout record, in preference order.
const AF_INFO2: u16 = 0x0026;
const AF_INFO3: u16 = 0x003c;

/// The first AF point of a frame, in the AF image's own pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CanonAf {
    /// `AFAreaMode`, Canon's own number.
    pub mode: u16,
    /// `NumAFPoints`: how many entries each array has.
    pub points: u16,
    /// `ValidAFPoints`.
    pub valid: u16,
    /// `AFImageWidth` and `AFImageHeight`: the canvas the offsets are in.
    pub canvas: (u16, u16),
    pub width: i16,
    pub height: i16,
    /// Offset of the point's centre from the canvas's centre, to the right.
    pub x: i16,
    /// The same, upwards.
    pub y: i16,
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

/// Decode an `AFInfo2` record. `None` when it is too short to hold the first
/// point of each array, however large its own point count claims to be.
pub fn parse_info2(bytes: &[u8], little: bool) -> Option<CanonAf> {
    let at = |i: usize| word(bytes, i, little);
    let points = at(2)?;
    if points == 0 {
        return None;
    }
    let n = usize::from(points);
    let signed = |i: usize| at(i).map(|w| w as i16);
    Some(CanonAf {
        mode: at(1)?,
        points,
        valid: at(3)?,
        canvas: (at(6)?, at(7)?),
        width: signed(8)?,
        height: signed(8usize.checked_add(n)?)?,
        x: signed(8usize.checked_add(n.checked_mul(2)?)?)?,
        y: signed(8usize.checked_add(n.checked_mul(3)?)?)?,
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

    /// An AFInfo2 record of `points` entries whose first point is as given.
    fn record(points: u16, first: [i16; 4], little: bool) -> Vec<u8> {
        let mut words = vec![0u16; 8 + 4 * usize::from(points)];
        words[0] = (words.len() * 2) as u16;
        words[1] = 9;
        words[2] = points;
        words[3] = 1;
        words[4] = 6960;
        words[5] = 4640;
        words[6] = 6960;
        words[7] = 4640;
        if points > 0 {
            for (k, v) in first.into_iter().enumerate() {
                words[8 + k * usize::from(points)] = v as u16;
            }
        }
        words
            .into_iter()
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
    fn the_first_point_of_each_array_is_read() {
        for little in [true, false] {
            let bytes = record(651, [2816, 936, 500, -66], little);
            let af = parse_info2(&bytes, little).expect("a record");
            assert_eq!(
                af,
                CanonAf {
                    mode: 9,
                    points: 651,
                    valid: 1,
                    canvas: (6960, 4640),
                    width: 2816,
                    height: 936,
                    x: 500,
                    y: -66,
                }
            );
        }
    }

    #[test]
    fn a_truncated_record_is_none() {
        let bytes = record(651, [2816, 936, 500, 66], true);
        // Every cut-off, down to nothing, is refused rather than panicking.
        for len in 0..bytes.len() {
            let got = parse_info2(&bytes[..len], true);
            // The first Y word is the last one needed: word 8 + 3N.
            let needed = 2 * (8 + 3 * 651 + 1);
            assert_eq!(got.is_some(), len >= needed, "len {len}");
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
            assert_eq!((af.width, af.height, af.x, af.y), (100, 50, -20, 30));
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
