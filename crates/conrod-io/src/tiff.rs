//! Just enough TIFF to read an IFD: the handful of tags a cull needs from a
//! CR2's header, a CR3's CMT boxes or a JPEG's EXIF segment.

use std::collections::HashMap;

pub const MODEL: u16 = 0x0110;
pub const ORIENTATION: u16 = 0x0112;
pub const STRIP_OFFSETS: u16 = 0x0111;
pub const STRIP_BYTE_COUNTS: u16 = 0x0117;
pub const EXIF_IFD: u16 = 0x8769;
pub const DATE_TIME: u16 = 0x0132;
pub const DATE_TIME_ORIGINAL: u16 = 0x9003;
pub const DATE_TIME_DIGITIZED: u16 = 0x9004;
pub const SUB_SEC_TIME: u16 = 0x9290;
pub const SUB_SEC_TIME_ORIGINAL: u16 = 0x9291;
pub const SUB_SEC_TIME_DIGITIZED: u16 = 0x9292;
pub const BODY_SERIAL_NUMBER: u16 = 0xA431;
pub const LENS_MODEL: u16 = 0xA434;
pub const EXPOSURE_TIME: u16 = 0x829A;
pub const FNUMBER: u16 = 0x829D;
pub const ISO: u16 = 0x8827;
pub const EXPOSURE_BIAS: u16 = 0x9204;
pub const FOCAL_LENGTH: u16 = 0x920A;
/// EXIF "SubjectDistance", in metres. Canon bodies leave it unset and put the
/// focus distance in their own makernote arrays instead.
pub const SUBJECT_DISTANCE: u16 = 0x9206;

/// One RATIONAL as numerator over denominator. TIFF rationals are unsigned;
/// the signed variant is folded in here because only its value is wanted.
pub type Ratio = (i32, i32);

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Text(String),
    Numbers(Vec<u64>),
    Ratios(Vec<Ratio>),
}

impl Value {
    pub fn text(&self) -> Option<&str> {
        match self {
            Value::Text(s) => Some(s),
            _ => None,
        }
    }

    pub fn first(&self) -> Option<u64> {
        match self {
            Value::Numbers(n) => n.first().copied(),
            _ => None,
        }
    }

    /// The first RATIONAL as a float, or `None` when it is absent or its
    /// denominator is zero (some bodies write 0/0 for "unknown").
    pub fn ratio(&self) -> Option<f64> {
        match self {
            Value::Ratios(r) => r.first().and_then(|&(n, d)| {
                if d == 0 {
                    None
                } else {
                    Some(f64::from(n) / f64::from(d))
                }
            }),
            _ => None,
        }
    }
}

/// A TIFF structure held in memory: `buf` starts at the byte-order mark.
pub struct Tiff<'a> {
    buf: &'a [u8],
    little: bool,
}

impl<'a> Tiff<'a> {
    pub fn new(buf: &'a [u8]) -> Option<Tiff<'a>> {
        let little = match buf.get(0..2)? {
            b"II" => true,
            b"MM" => false,
            _ => return None,
        };
        Some(Tiff { buf, little })
    }

    /// `len` bytes at `at`, or `None` if they are not all in the buffer.
    /// Offsets and counts come from the file, so the sum is checked.
    fn slice(&self, at: usize, len: usize) -> Option<&'a [u8]> {
        self.buf.get(at..at.checked_add(len)?)
    }

    fn u16_at(&self, at: usize) -> Option<u16> {
        let b: [u8; 2] = self.slice(at, 2)?.try_into().ok()?;
        Some(if self.little {
            u16::from_le_bytes(b)
        } else {
            u16::from_be_bytes(b)
        })
    }

    fn u32_at(&self, at: usize) -> Option<u32> {
        let b: [u8; 4] = self.slice(at, 4)?.try_into().ok()?;
        Some(if self.little {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        })
    }

    /// Offset of the first IFD, from the header.
    pub fn first_ifd(&self) -> Option<usize> {
        self.u32_at(4).map(|v| v as usize)
    }

    /// Every entry of the IFD at `offset` that is ASCII or an unsigned
    /// integer; other types are not needed and are skipped.
    pub fn ifd(&self, offset: usize) -> HashMap<u16, Value> {
        let mut out = HashMap::new();
        let Some(count) = self.u16_at(offset) else {
            return out;
        };
        for i in 0..usize::from(count) {
            let Some(entry) = offset.checked_add(2 + i * 12) else {
                break;
            };
            let (Some(tag), Some(kind), Some(n)) = (
                self.u16_at(entry),
                self.u16_at(entry + 2),
                self.u32_at(entry + 4),
            ) else {
                break;
            };
            let n = n as usize;
            let width = match kind {
                1 | 2 | 7 => 1,
                3 => 2,
                4 => 4,
                5 | 10 => 8,
                _ => continue,
            };
            let at = if n.saturating_mul(width) <= 4 {
                entry + 8
            } else {
                match self.u32_at(entry + 8) {
                    Some(v) => v as usize,
                    None => continue,
                }
            };
            // A count larger than the buffer can hold would only spin through
            // reads that all miss.
            let n = if matches!(kind, 3..=5 | 10) {
                n.min(self.buf.len().saturating_sub(at) / width)
            } else {
                n
            };
            let value = match kind {
                2 => {
                    let Some(bytes) = self.slice(at, n) else {
                        continue;
                    };
                    let text = String::from_utf8_lossy(bytes);
                    Value::Text(text.trim_end_matches('\0').trim().to_string())
                }
                3 => Value::Numbers(
                    (0..n)
                        .filter_map(|k| self.u16_at(at + k * 2))
                        .map(u64::from)
                        .collect(),
                ),
                4 => Value::Numbers(
                    (0..n)
                        .filter_map(|k| self.u32_at(at + k * 4))
                        .map(u64::from)
                        .collect(),
                ),
                // RATIONAL: two u32 per value, numerator first.
                5 => Value::Ratios(
                    (0..n)
                        .filter_map(|k| {
                            let (num, den) =
                                (self.u32_at(at + k * 8)?, self.u32_at(at + k * 8 + 4)?);
                            Some((num as i32, den as i32))
                        })
                        .collect(),
                ),
                // SRATIONAL: the same, signed, which is how exposure bias is
                // written (negative for underexposure).
                10 => Value::Ratios(
                    (0..n)
                        .filter_map(|k| {
                            let (num, den) =
                                (self.u32_at(at + k * 8)?, self.u32_at(at + k * 8 + 4)?);
                            Some((num as i32, den as i32))
                        })
                        .collect(),
                ),
                _ => Value::Numbers(
                    self.slice(at, n)
                        .map(|b| b.iter().map(|&x| u64::from(x)).collect())
                        .unwrap_or_default(),
                ),
            };
            out.insert(tag, value);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Little-endian TIFF whose IFD0 holds one entry of `kind` with `count`
    /// values at `value` (an offset when they do not fit inline).
    fn one_entry(kind: u16, count: u32, value: u32) -> Vec<u8> {
        let mut out = b"II".to_vec();
        out.extend_from_slice(&42u16.to_le_bytes());
        out.extend_from_slice(&8u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&ORIENTATION.to_le_bytes());
        out.extend_from_slice(&kind.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&value.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out
    }

    #[test]
    fn a_count_far_past_the_buffer_does_not_spin_or_panic() {
        for kind in [1, 2, 3, 4, 5, 7, 10] {
            for value in [0, 26, u32::MAX] {
                let bytes = one_entry(kind, u32::MAX, value);
                let t = Tiff::new(&bytes).unwrap();
                // Returns at once, whatever it finds.
                let _ = t.ifd(8);
            }
        }
    }

    #[test]
    fn an_ifd_offset_at_the_end_of_the_address_space_is_empty() {
        let bytes = one_entry(3, 1, 1);
        let t = Tiff::new(&bytes).unwrap();
        assert!(t.ifd(usize::MAX).is_empty());
        assert!(t.ifd(usize::MAX - 3).is_empty());
    }

    #[test]
    fn a_short_value_is_still_read() {
        let bytes = one_entry(3, 1, 6);
        let t = Tiff::new(&bytes).unwrap();
        assert_eq!(t.ifd(8).get(&ORIENTATION).and_then(Value::first), Some(6));
    }
}
