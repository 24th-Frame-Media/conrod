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
    if ext == "cr3" || ext == "cr2" {
        raw_metadata(path, &mut tags);
    }
    Ok(Frame {
        path: path.to_path_buf(),
        preview,
        orientation,
        tags,
    })
}

/// Serial, capture time and lens from rawler, which reads Canon makernotes.
fn raw_metadata(path: &Path, tags: &mut Tags) {
    let Ok(source) = rawler::rawsource::RawSource::new(path) else {
        return;
    };
    let Ok(decoder) = rawler::get_decoder(&source) else {
        return;
    };
    let Ok(meta) = decoder.raw_metadata(&source, &rawler::decoders::RawDecodeParams::default())
    else {
        return;
    };
    let exif = meta.exif;
    for (name, value) in [
        ("SerialNumber", exif.serial_number),
        ("DateTimeOriginal", exif.date_time_original),
        ("SubSecTimeOriginal", exif.sub_sec_time_original),
        ("LensModel", exif.lens_model),
    ] {
        if let Some(v) = value {
            let v = v.trim_end_matches('\0').trim().to_string();
            if !v.is_empty() {
                tags.insert(name.into(), Value::String(v));
            }
        }
    }
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
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    let header = read_at(file, 0, len.min(65_536) as usize)?;
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

fn cr3(file: &mut File) -> Result<(Vec<u8>, Vec<u8>)> {
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    // Walk the top level by headers alone until moov, which holds every
    // track's sample table and the EXIF blocks, and read just that.
    let mut at = 0u64;
    let moov = loop {
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
            break read_at(file, at + header, (size - header) as usize)?;
        }
        at += size;
    };

    // Canon's metadata lives in a uuid box inside moov; CMT1 is IFD0.
    let mut header = Vec::new();
    for (kind, body) in boxes(&moov) {
        if &kind == b"uuid" {
            if let Some(cmt1) = child(body, b"CMT1") {
                header = cmt1.to_vec();
            }
        }
    }

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
