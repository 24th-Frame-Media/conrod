//! Just enough TIFF to read an IFD: the handful of tags a cull needs from a
//! CR2's header, a CR3's CMT boxes or a JPEG's EXIF segment.

use std::collections::HashMap;

pub const MODEL: u16 = 0x0110;
pub const ORIENTATION: u16 = 0x0112;
pub const STRIP_OFFSETS: u16 = 0x0111;
pub const STRIP_BYTE_COUNTS: u16 = 0x0117;
pub const EXIF_IFD: u16 = 0x8769;
pub const DATE_TIME_ORIGINAL: u16 = 0x9003;
pub const SUB_SEC_TIME_ORIGINAL: u16 = 0x9291;
pub const BODY_SERIAL_NUMBER: u16 = 0xA431;
pub const LENS_MODEL: u16 = 0xA434;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Text(String),
    Numbers(Vec<u64>),
}

impl Value {
    pub fn text(&self) -> Option<&str> {
        match self {
            Value::Text(s) => Some(s),
            Value::Numbers(_) => None,
        }
    }

    pub fn first(&self) -> Option<u64> {
        match self {
            Value::Numbers(n) => n.first().copied(),
            Value::Text(_) => None,
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

    fn u16_at(&self, at: usize) -> Option<u16> {
        let b: [u8; 2] = self.buf.get(at..at + 2)?.try_into().ok()?;
        Some(if self.little {
            u16::from_le_bytes(b)
        } else {
            u16::from_be_bytes(b)
        })
    }

    fn u32_at(&self, at: usize) -> Option<u32> {
        let b: [u8; 4] = self.buf.get(at..at + 4)?.try_into().ok()?;
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
            let entry = offset + 2 + i * 12;
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
                _ => continue,
            };
            let at = if n * width <= 4 {
                entry + 8
            } else {
                match self.u32_at(entry + 8) {
                    Some(v) => v as usize,
                    None => continue,
                }
            };
            let value = match kind {
                2 => {
                    let Some(bytes) = self.buf.get(at..at + n) else {
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
                _ => Value::Numbers(
                    self.buf
                        .get(at..at + n)
                        .map(|b| b.iter().map(|&x| u64::from(x)).collect())
                        .unwrap_or_default(),
                ),
            };
            out.insert(tag, value);
        }
        out
    }
}
