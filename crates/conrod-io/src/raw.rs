//! A frame as the cull needs it: the camera's own full-size JPEG, which way up
//! it goes, and the tags that say which body took it and when.
//!
//! Replaces exiftool's preview extraction and tag read (`conrod/exif.py`).
//! Only the preview's byte range is read, not the whole RAW: on a slow card
//! that is a few MB a frame instead of thirty. Serial and capture time come
//! from rawler, which decodes the Canon makernotes; verified against exiftool.

use crate::tiff::{self, Tiff};
use conrod_core::bursts::{self, Tags};
use serde_json::Value;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub struct Frame {
    pub path: PathBuf,
    /// The embedded full-size JPEG (or the file itself, for a JPEG).
    pub preview: Vec<u8>,
    /// EXIF orientation of the frame: 1 upright, 3 upside down, 6 and 8 on
    /// their side. The preview is stored in sensor order and ignores it.
    pub orientation: u16,
    /// In exiftool's names, so conrod-core's bursts logic reads them as-is.
    pub tags: Tags,
    /// Shutter, aperture, ISO and friends: what tells a bracket from a burst.
    pub exposure: Exposure,
}

/// A frame's exposure settings, as the keep-together finder needs them.
///
/// Every field is optional, because the files genuinely vary: an exported JPEG
/// may carry none of it, and no Canon body writes focus distance to standard
/// EXIF -- it lives in per-model makernote arrays (see [`Exposure::focus_distance`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Exposure {
    /// Shutter time in seconds.
    pub exposure_time: Option<f64>,
    pub f_number: Option<f64>,
    pub iso: Option<u32>,
    /// Exposure compensation in EV, as dialled in, not as metered. An
    /// auto-exposure bracket leaves it at 0 on every frame and moves the
    /// shutter instead, so it is no substitute for `exposure_time`.
    pub exposure_bias: Option<f64>,
    pub focal_length: Option<f64>,
    /// Focus distance in metres, from EXIF's SubjectDistance. Canon writes
    /// 0xFFFFFFFF there and puts FocusDistanceUpper/Lower in its makernote's
    /// model-specific ShotInfo and CameraSettings arrays, which rawler does not
    /// decode, so on a Canon body this is normally `None`.
    pub focus_distance: Option<f64>,
    /// True when the body flagged the frame as one shot of an auto-exposure
    /// bracket. Also a Canon makernote array field, so normally `None`; the set
    /// finder works from the exposures themselves instead of trusting it.
    pub aeb: Option<bool>,
    /// The sub-second part of the capture time, as text.
    pub subsec: Option<String>,
    /// The lens as the file names it (EXIF `LensModel`). Carried here as well
    /// as in the frame's tags, so an old album can backfill its lens column
    /// from [`exposure_meta`] without decoding a preview.
    pub lens: Option<String>,
}

impl Exposure {
    /// Take `newer`'s values wherever it has them. The RAW reader is more
    /// trustworthy than the header walk, so it is applied second.
    fn overlay(&mut self, newer: Exposure) {
        macro_rules! take {
            ($($field:ident),*) => {$(
                if newer.$field.is_some() {
                    self.$field = newer.$field;
                }
            )*};
        }
        take!(
            exposure_time,
            f_number,
            iso,
            exposure_bias,
            focal_length,
            focus_distance,
            aeb,
            subsec,
            lens
        );
    }
}

/// A setting of zero or a non-finite value means "not recorded", not "zero".
fn recorded(value: Option<f64>) -> Option<f64> {
    value.filter(|v| v.is_finite() && *v > 0.0)
}

impl Frame {
    pub fn camera(&self, fallback: &str) -> String {
        bursts::camera_of(&self.tags, fallback)
    }

    pub fn taken(&self) -> Option<f64> {
        bursts::taken_at(&self.tags)
    }
}

pub type Result<T> = std::result::Result<T, String>;

pub fn read(path: &Path) -> Result<Frame> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let (preview, header) = match ext.as_str() {
        "cr3" => cr3(&mut file)?,
        "cr2" => cr2(&mut file)?,
        "jpg" | "jpeg" => {
            let mut all = Vec::new();
            file.read_to_end(&mut all).map_err(|e| e.to_string())?;
            let exif = jpeg_exif(&all).map(<[u8]>::to_vec).unwrap_or_default();
            (all, exif)
        }
        other => return Err(format!("{}: .{other} is not read yet", path.display())),
    };
    if !preview.starts_with(&[0xFF, 0xD8]) {
        return Err(format!("{}: no JPEG preview found", path.display()));
    }

    let mut tags = Tags::new();
    tags.insert(
        "SourceFile".into(),
        Value::String(path.display().to_string()),
    );
    let mut orientation = 1;
    if let Some(t) = Tiff::new(&header) {
        let ifd0 = t.first_ifd().map(|o| t.ifd(o)).unwrap_or_default();
        if let Some(model) = ifd0.get(&tiff::MODEL).and_then(tiff::Value::text) {
            tags.insert("Model".into(), Value::String(model.to_string()));
        }
        orientation = ifd0
            .get(&tiff::ORIENTATION)
            .and_then(tiff::Value::first)
            .map_or(1, |o| o as u16);
        // A JPEG carries its own EXIF; for RAW the richer source is below.
        if let Some(exif) = ifd0.get(&tiff::EXIF_IFD).and_then(tiff::Value::first) {
            let exif = t.ifd(exif as usize);
            for (tag, name) in [
                (tiff::DATE_TIME_ORIGINAL, "DateTimeOriginal"),
                (tiff::DATE_TIME_DIGITIZED, "DateTimeDigitized"),
                (tiff::SUB_SEC_TIME_ORIGINAL, "SubSecTimeOriginal"),
                (tiff::SUB_SEC_TIME, "SubSecTime"),
                (tiff::SUB_SEC_TIME_DIGITIZED, "SubSecTimeDigitized"),
                (tiff::BODY_SERIAL_NUMBER, "SerialNumber"),
                (tiff::LENS_MODEL, "LensModel"),
            ] {
                if let Some(v) = exif.get(&tag).and_then(tiff::Value::text) {
                    tags.insert(name.into(), Value::String(v.to_string()));
                }
            }
        }
        if !tags.contains_key("DateTimeOriginal") {
            if let Some(dt) = ifd0.get(&tiff::DATE_TIME).and_then(tiff::Value::text) {
                tags.insert("DateTimeOriginal".into(), Value::String(dt.to_string()));
            }
        }
    }
    let mut exposure = exposure_from_tiff(&header);
    if ext == "cr3" || ext == "cr2" {
        exposure.overlay(rawler_metadata(path, Some(&mut tags)));
    }
    Ok(Frame {
        path: path.to_path_buf(),
        preview,
        orientation,
        tags,
        exposure,
    })
}

/// Only the exposure settings and the lens, read from the same header the
/// preview walk touches: no decode and no image data, so it is cheap enough to
/// call across a whole scan (or a backfill of an old album).
pub fn exposure_meta(path: &Path) -> Result<Exposure> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mut file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let header = match ext.as_str() {
        "cr3" => cr3_header(&mut file)?,
        "cr2" => header_prefix(&mut file)?,
        // The TIFF inside the JPEG's APP1 segment is what the EXIF walk wants.
        "jpg" | "jpeg" => header_prefix(&mut file)
            .map(|head| jpeg_exif(&head).map(<[u8]>::to_vec).unwrap_or_default())?,
        other => return Err(format!("{}: .{other} is not read yet", path.display())),
    };
    let mut exposure = exposure_from_tiff(&header);
    if ext == "cr3" || ext == "cr2" {
        exposure.overlay(rawler_metadata(path, None));
    }
    Ok(exposure)
}

/// The first 64 KB of the file: IFD0 and the strings it points at sit inside.
fn header_prefix(file: &mut File) -> Result<Vec<u8>> {
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    read_at(file, 0, len.min(65_536) as usize)
}

/// Shutter, aperture, ISO and the rest from the EXIF IFD of a TIFF header --
/// a CR2's IFD0, a CR3's CMT1 box, or a JPEG's APP1 segment.
fn exposure_from_tiff(header: &[u8]) -> Exposure {
    let mut out = Exposure::default();
    let Some(t) = Tiff::new(header) else {
        return out;
    };
    let Some(ifd0) = t.first_ifd().map(|o| t.ifd(o)) else {
        return out;
    };
    let Some(exif) = ifd0
        .get(&tiff::EXIF_IFD)
        .and_then(tiff::Value::first)
        .map(|o| t.ifd(o as usize))
    else {
        return out;
    };
    out.exposure_time = recorded(exif.get(&tiff::EXPOSURE_TIME).and_then(tiff::Value::ratio));
    out.f_number = recorded(exif.get(&tiff::FNUMBER).and_then(tiff::Value::ratio));
    out.iso = exif
        .get(&tiff::ISO)
        .and_then(tiff::Value::first)
        .filter(|v| *v > 0)
        .map(|v| v as u32);
    out.exposure_bias = exif
        .get(&tiff::EXPOSURE_BIAS)
        .and_then(tiff::Value::ratio)
        .filter(|v| v.is_finite());
    out.focal_length = recorded(exif.get(&tiff::FOCAL_LENGTH).and_then(tiff::Value::ratio));
    // 0 metres is a macro reading, but 65535 is the "unknown" sentinel.
    out.focus_distance = recorded(
        exif.get(&tiff::SUBJECT_DISTANCE)
            .and_then(tiff::Value::ratio),
    )
    .filter(|v| *v < 10_000.0);
    out.subsec = exif
        .get(&tiff::SUB_SEC_TIME_ORIGINAL)
        .and_then(tiff::Value::text)
        .map(str::to_string);
    out.lens = exif
        .get(&tiff::LENS_MODEL)
        .and_then(tiff::Value::text)
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string);
    out
}

/// Everything rawler decodes: the identity tags when a `tags` map is offered,
/// and the exposure settings either way. One decode, whichever the caller needs.
fn rawler_metadata(path: &Path, tags: Option<&mut Tags>) -> Exposure {
    let mut out = Exposure::default();
    let Ok(source) = rawler::rawsource::RawSource::new(path) else {
        return out;
    };
    let Ok(decoder) = rawler::get_decoder(&source) else {
        return out;
    };
    let Ok(meta) = decoder.raw_metadata(&source, &rawler::decoders::RawDecodeParams::default())
    else {
        return out;
    };
    let exif = meta.exif;
    if let Some(tags) = tags {
        for (name, value) in [
            ("SerialNumber", exif.serial_number.clone()),
            ("DateTimeOriginal", exif.date_time_original.clone()),
            ("SubSecTimeOriginal", exif.sub_sec_time_original.clone()),
            ("LensModel", exif.lens_model.clone()),
        ] {
            if let Some(v) = value {
                let v = v.trim_end_matches('\0').trim().to_string();
                if !v.is_empty() {
                    tags.insert(name.into(), Value::String(v));
                }
            }
        }
    }
    let ratio = |r: Option<rawler::formats::tiff::Rational>| {
        r.and_then(|r| (r.d != 0).then(|| f64::from(r.n) / f64::from(r.d)))
    };
    out.exposure_time = recorded(ratio(exif.exposure_time));
    out.f_number = recorded(ratio(exif.fnumber));
    out.iso = exif
        .iso_speed_ratings
        .map(u32::from)
        .or(exif.iso_speed)
        .filter(|v| *v > 0);
    out.exposure_bias = exif.exposure_bias.and_then(|r| {
        (r.d != 0)
            .then(|| f64::from(r.n) / f64::from(r.d))
            .filter(|v| v.is_finite())
    });
    out.focal_length = recorded(ratio(exif.focal_length));
    out.focus_distance = recorded(ratio(exif.subject_distance)).filter(|v| *v < 10_000.0);
    out.subsec = exif
        .sub_sec_time_original
        .map(|s| s.trim_end_matches('\0').trim().to_string())
        .filter(|s| !s.is_empty());
    out.lens = exif
        .lens_model
        .map(|s| s.trim_end_matches('\0').trim().to_string())
        .filter(|s| !s.is_empty());
    out
}

fn read_at(file: &mut File, offset: u64, len: usize) -> Result<Vec<u8>> {
    let mut buf = vec![0u8; len];
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    file.read_exact(&mut buf).map_err(|e| e.to_string())?;
    Ok(buf)
}

// --- CR2: a TIFF whose first IFD is the full-size JPEG --------------------------

fn cr2(file: &mut File) -> Result<(Vec<u8>, Vec<u8>)> {
    // IFD0 and the strings it points at sit well inside the first 64 KB.
    let header = header_prefix(file)?;
    let t = Tiff::new(&header).ok_or("CR2: not a TIFF")?;
    let ifd0 = t.ifd(t.first_ifd().ok_or("CR2: no IFD")?);
    let offset = ifd0
        .get(&tiff::STRIP_OFFSETS)
        .and_then(tiff::Value::first)
        .ok_or("CR2: no preview offset")?;
    let size = ifd0
        .get(&tiff::STRIP_BYTE_COUNTS)
        .and_then(tiff::Value::first)
        .ok_or("CR2: no preview size")?;
    let preview = read_at(file, offset, size as usize)?;
    Ok((preview, header))
}

// --- CR3: ISO base media, previews as track samples -----------------------------

/// Child boxes of `buf`: (type, body). A `uuid` box's body starts after its
/// 16-byte user type.
fn boxes(buf: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while at + 8 <= buf.len() {
        let size32 = u32::from_be_bytes(buf[at..at + 4].try_into().unwrap()) as usize;
        let kind: [u8; 4] = buf[at + 4..at + 8].try_into().unwrap();
        let (header, size) = match size32 {
            0 => (8, buf.len() - at),
            1 if at + 16 <= buf.len() => (
                16,
                u64::from_be_bytes(buf[at + 8..at + 16].try_into().unwrap()) as usize,
            ),
            n => (8, n),
        };
        if size < header || at + size > buf.len() {
            break;
        }
        let skip = if &kind == b"uuid" { 16 } else { 0 };
        let body = buf.get(at + header + skip..at + size).unwrap_or(&[]);
        out.push((kind, body));
        at += size;
    }
    out
}

fn child<'a>(buf: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    boxes(buf)
        .into_iter()
        .find(|(k, _)| k == kind)
        .map(|(_, b)| b)
}

/// The first sample of a track: (file offset, size).
fn first_sample(trak: &[u8]) -> Option<(u64, u64)> {
    let stbl = child(child(child(trak, b"mdia")?, b"minf")?, b"stbl")?;
    let stsz = child(stbl, b"stsz")?;
    let fixed = u32::from_be_bytes(stsz.get(4..8)?.try_into().ok()?);
    let size = if fixed != 0 {
        fixed
    } else {
        u32::from_be_bytes(stsz.get(12..16)?.try_into().ok()?)
    };
    let offset = if let Some(co64) = child(stbl, b"co64") {
        u64::from_be_bytes(co64.get(8..16)?.try_into().ok()?)
    } else {
        u64::from(u32::from_be_bytes(
            child(stbl, b"stco")?.get(8..12)?.try_into().ok()?,
        ))
    };
    Some((offset, u64::from(size)))
}

/// The moov box, read whole: it holds every track's sample table and the EXIF
/// blocks, and is tens of KB rather than the file's megabytes.
fn cr3_moov(file: &mut File) -> Result<Vec<u8>> {
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    // Walk the top level by headers alone, so nothing but moov is read.
    let mut at = 0u64;
    loop {
        if at + 8 > len {
            return Err("CR3: no moov box".into());
        }
        let head = read_at(file, at, 16.min((len - at) as usize))?;
        let size32 = u32::from_be_bytes(head[0..4].try_into().unwrap());
        let size = match size32 {
            0 => len - at,
            1 => u64::from_be_bytes(head[8..16].try_into().unwrap()),
            n => u64::from(n),
        };
        if size < 8 {
            return Err("CR3: malformed box".into());
        }
        if &head[4..8] == b"moov" {
            let header = if size32 == 1 { 16 } else { 8 };
            return read_at(file, at + header, (size - header) as usize);
        }
        at += size;
    }
}

/// Canon's metadata lives in a uuid box inside moov; CMT1 is IFD0.
fn cmt1(moov: &[u8]) -> Vec<u8> {
    let mut header = Vec::new();
    for (kind, body) in boxes(moov) {
        if &kind == b"uuid" {
            if let Some(cmt1) = child(body, b"CMT1") {
                header = cmt1.to_vec();
            }
        }
    }
    header
}

/// The CR3's CMT1 box: Canon's IFD0, carrying the EXIF fields and pointers.
fn cr3_header(file: &mut File) -> Result<Vec<u8>> {
    Ok(cmt1(&cr3_moov(file)?))
}

fn cr3(file: &mut File) -> Result<(Vec<u8>, Vec<u8>)> {
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    let moov = cr3_moov(file)?;
    let header = cmt1(&moov);

    // The full-size JPEG is the largest track whose sample is a JPEG.
    let mut best: Option<(u64, u64)> = None;
    for (kind, body) in boxes(&moov) {
        if &kind != b"trak" {
            continue;
        }
        let Some((offset, size)) = first_sample(body) else {
            continue;
        };
        if best.is_some_and(|(_, s)| s >= size) || offset + size > len {
            continue;
        }
        if read_at(file, offset, 2)? == [0xFF, 0xD8] {
            best = Some((offset, size));
        }
    }
    let (offset, size) = best.ok_or("CR3: no JPEG track")?;
    Ok((read_at(file, offset, size as usize)?, header))
}

// --- JPEG ---------------------------------------------------------------------

/// The TIFF inside a JPEG's APP1 "Exif" segment.
fn jpeg_exif(jpeg: &[u8]) -> Option<&[u8]> {
    let mut at = 2;
    while at + 4 <= jpeg.len() && jpeg[at] == 0xFF {
        let marker = jpeg[at + 1];
        let len = usize::from(u16::from_be_bytes([jpeg[at + 2], jpeg[at + 3]]));
        let body = jpeg.get(at + 4..at + 2 + len)?;
        if marker == 0xE1 && body.starts_with(b"Exif\0\0") {
            return Some(&body[6..]);
        }
        if marker == 0xDA {
            break; // image data: no metadata after this
        }
        at += 2 + len;
    }
    None
}

#[cfg(test)]
mod exposure_tests {
    use super::*;

    /// A little-endian TIFF: IFD0, an EXIF IFD with one of each exposure tag,
    /// and the rationals stored past the entries -- the layout a CR2's header
    /// and a JPEG's APP1 segment both use.
    fn exposure_tiff() -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"II");
        out.extend_from_slice(&42u16.to_le_bytes());
        out.extend_from_slice(&8u32.to_le_bytes()); // IFD0 at 8
        out.extend_from_slice(&1u16.to_le_bytes()); // one entry: EXIF pointer
        out.extend_from_slice(&tiff::EXIF_IFD.to_le_bytes());
        out.extend_from_slice(&4u16.to_le_bytes()); // LONG
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&26u32.to_le_bytes()); // EXIF IFD at 26
        out.extend_from_slice(&0u32.to_le_bytes()); // no next IFD
        assert_eq!(out.len(), 26);

        // Six entries, each one value; values past the table at 104.
        out.extend_from_slice(&6u16.to_le_bytes());
        for (tag, kind, value) in [
            (tiff::EXPOSURE_TIME, 5u16, 104u32), // 1/1000
            (tiff::FNUMBER, 5, 112),             // 28/10
            (tiff::ISO, 3, 400),                 // inline SHORT
            (tiff::EXPOSURE_BIAS, 10, 120),      // -2/1
            (tiff::FOCAL_LENGTH, 5, 128),        // 35/1
            (tiff::SUBJECT_DISTANCE, 5, 136),    // 15/10
        ] {
            out.extend_from_slice(&tag.to_le_bytes());
            out.extend_from_slice(&kind.to_le_bytes());
            out.extend_from_slice(&1u32.to_le_bytes());
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.extend_from_slice(&0u32.to_le_bytes()); // no next IFD
        assert_eq!(out.len(), 104);

        for pair in [
            (1u32, 1000u32),
            (28, 10),
            (-2i32 as u32, 1),
            (35, 1),
            (15, 10),
        ] {
            out.extend_from_slice(&pair.0.to_le_bytes());
            out.extend_from_slice(&pair.1.to_le_bytes());
        }
        out
    }

    #[test]
    fn every_exposure_tag_reads_back_at_its_real_value() {
        let got = exposure_from_tiff(&exposure_tiff());
        assert_eq!(got.exposure_time, Some(1.0 / 1000.0));
        assert_eq!(got.f_number, Some(2.8));
        assert_eq!(got.iso, Some(400));
        assert_eq!(got.exposure_bias, Some(-2.0));
        assert_eq!(got.focal_length, Some(35.0));
        assert_eq!(got.focus_distance, Some(1.5));
        assert_eq!(got.subsec, None);
    }

    #[test]
    fn a_zero_denominator_is_not_a_reading() {
        // Some bodies write 0/0 where they mean "not recorded".
        let mut bytes = exposure_tiff();
        let at = 104 + 8; // f-number's numerator, denominator pair
        bytes[at..at + 8].copy_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]);
        let got = exposure_from_tiff(&bytes);
        assert_eq!(got.f_number, None);
        assert!(got.exposure_time.is_some());
    }

    #[test]
    fn a_jpeg_app1_segment_yields_the_same_fields() {
        let tiff = exposure_tiff();
        let mut jpeg = vec![0xFF, 0xD8];
        let len = u16::try_from(tiff.len() + 8).unwrap();
        jpeg.extend_from_slice(&[0xFF, 0xE1]);
        jpeg.extend_from_slice(&len.to_be_bytes());
        jpeg.extend_from_slice(b"Exif\0\0");
        jpeg.extend_from_slice(&tiff);
        let exif = jpeg_exif(&jpeg).expect("APP1 segment");
        assert_eq!(exposure_from_tiff(exif).focal_length, Some(35.0));
    }

    /// IFD0 pointing at an EXIF IFD with one ASCII entry, `LensModel`.
    fn lens_tiff(lens: &str) -> Vec<u8> {
        let text = format!("{lens}\0");
        let mut out = Vec::new();
        out.extend_from_slice(b"II");
        out.extend_from_slice(&42u16.to_le_bytes());
        out.extend_from_slice(&8u32.to_le_bytes()); // IFD0 at 8
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&tiff::EXIF_IFD.to_le_bytes());
        out.extend_from_slice(&4u16.to_le_bytes());
        out.extend_from_slice(&1u32.to_le_bytes());
        out.extend_from_slice(&26u32.to_le_bytes()); // EXIF IFD at 26
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&tiff::LENS_MODEL.to_le_bytes());
        out.extend_from_slice(&2u16.to_le_bytes()); // ASCII
        out.extend_from_slice(&(text.len() as u32).to_le_bytes());
        out.extend_from_slice(&44u32.to_le_bytes()); // text past the table
        out.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(out.len(), 44);
        out.extend_from_slice(text.as_bytes());
        out
    }

    #[test]
    fn the_lens_model_is_read_from_the_exif_ifd() {
        let got = exposure_from_tiff(&lens_tiff("EF100-400mm F4.5-5.6L IS II USM"));
        assert_eq!(
            got.lens.as_deref(),
            Some("EF100-400mm F4.5-5.6L IS II USM")
        );
        // A header with a lens still has no other reading recorded.
        assert_eq!(got.iso, None);
        assert_eq!(got.exposure_time, None);
    }
}
