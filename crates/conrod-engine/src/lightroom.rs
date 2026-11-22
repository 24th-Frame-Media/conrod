//! The Lightroom Classic bridge: verdict packages out, catalog flags in.
//!
//! Folder layout, envelope and matching rules are the contract in
//! `docs/plans/lightroom-bridge.md`; the Lua half lives in
//! `lightroom-plugin/ConrodBridge.lrplugin` and reads exactly what
//! [`export`] writes. Neither side needs the other running: a package is
//! written atomically (`.json.tmp` then rename) and picked up later, so a
//! 3,000-frame album cannot time out the way a live bridge would.
//!
//! An import is a second opinion, never a command: it fills
//! `images.catalog_rating`, `catalog_pick` and `catalog_label` and leaves
//! conrod's own `stars`/`picked`/`rejected` alone.

use crate::desktop::{rows, Desktop, Result};
use crate::{err, lock, now};
use conrod_vision::sharpness;
use rusqlite::{params, Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::UNIX_EPOCH;

/// The envelope's format tag, shared with the plugin.
pub const FORMAT: &str = "conrod.lightroom.bridge";
/// The only version this reader knows. A greater one goes to `failed\`.
pub const VERSION: i64 = 1;
const TO_LIGHTROOM: &str = "to-lightroom";
const TO_CONROD: &str = "to-conrod";
const COLORS: [&str; 5] = ["red", "yellow", "green", "blue", "purple"];
/// Lightroom's catalog timestamps count seconds from 2001-01-01T00:00:00Z.
const LR_EPOCH_OFFSET: f64 = 978_307_200.0;
/// How far a fallback match's capture time may differ, in seconds.
const TIME_SLOP: f64 = 2.0;

// --- the envelope ------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Album {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_id: Option<i64>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub root: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Producer {
    pub app: String,
    pub version: String,
}

/// One photo of a package. `path` is required; every verdict field may be
/// null, which means "this package carries no value for it".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Photo {
    pub path: String,
    #[serde(default)]
    pub rating: Option<i64>,
    #[serde(default)]
    pub pick: Option<i64>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keywords: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stars: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub burst: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keeper: Option<bool>,
}

/// Photos stay as raw JSON so one malformed entry is skipped and counted
/// instead of aborting the package, as the contract requires.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub format: String,
    pub version: i64,
    pub direction: String,
    pub album: Album,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub producer: Option<Producer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    /// `to-lightroom` only: whether the photographer lets Conrod replace a
    /// hand-set star rating or colour label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overwrite: Option<Overwrite>,
    #[serde(default)]
    pub photos: Vec<Value>,
}

/// The two flags a package carries so the plug-in never overwrites a choice
/// the photographer made by hand.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Overwrite {
    pub rating: bool,
    pub label: bool,
}

/// Why a package must go to `failed\`: the same wording the plugin uses.
pub fn validate(value: &Value, direction: &str) -> Result<()> {
    let Some(object) = value.as_object() else {
        return Err("the package is not a JSON object".into());
    };
    if object.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(format!(
            "unknown format {:?} (expected {:?})",
            object
                .get("format")
                .map_or("null", |v| v.as_str().unwrap_or("null")),
            FORMAT
        ));
    }
    let Some(version) = object.get("version").and_then(Value::as_i64) else {
        return Err("version is missing or not an integer".into());
    };
    if version > VERSION {
        return Err(format!(
            "package version {version} is newer than conrod supports ({VERSION})"
        ));
    }
    if version < 1 {
        return Err(format!("package version {version} is not supported"));
    }
    if object.get("direction").and_then(Value::as_str) != Some(direction) {
        return Err(format!(
            "direction is {:?}, expected \"{direction}\"",
            object
                .get("direction")
                .map_or("null", |v| v.as_str().unwrap_or("null"))
        ));
    }
    if !object.get("photos").is_some_and(Value::is_array) {
        return Err("photos is missing or not an array".into());
    }
    Ok(())
}

// --- folders -----------------------------------------------------------------

/// `%USERPROFILE%\.conrod\bridge`, which is the app's own data root.
pub fn bridge_root(d: &Desktop) -> PathBuf {
    d.root.join("bridge")
}

fn folder(d: &Desktop, name: &str) -> PathBuf {
    bridge_root(d).join(name)
}

fn ensure_directory(path: &Path) -> Result<()> {
    fs::create_dir_all(path).map_err(|e| format!("cannot create {} ({e})", path.display()))
}

/// The plugin's `filePart`: a package name that is safe on Windows and short
/// enough to read in a dialog.
fn slug(text: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;
    for ch in text.chars() {
        let ch = if ch.is_alphanumeric() || matches!(ch, '-' | '.' | '_' | ' ') {
            ch
        } else {
            '-'
        };
        if ch == ' ' || ch == '-' {
            if last_dash {
                continue;
            }
            last_dash = true;
            out.push('-');
        } else {
            last_dash = false;
            out.push(ch);
        }
    }
    let out = out.trim_matches('-').to_string();
    let out = if out.is_empty() {
        "album".to_string()
    } else {
        out.chars().take(60).collect()
    };
    out
}

/// `<name>.json.tmp` then a rename, so a reader never sees a partial package.
fn write_package(dir: &Path, name: &str, bytes: &[u8]) -> Result<PathBuf> {
    ensure_directory(dir)?;
    let temporary = dir.join(format!("{name}.json.tmp"));
    let final_path = dir.join(format!("{name}.json"));
    fs::write(&temporary, bytes)
        .map_err(|e| format!("cannot write {} ({e})", temporary.display()))?;
    fs::rename(&temporary, &final_path)
        .map_err(|e| format!("cannot rename into {} ({e})", final_path.display()))?;
    Ok(final_path)
}

/// A target that does not exist yet: two packages consumed in the same second
/// must not overwrite each other.
fn unique_target(dir: &Path, base: &str) -> PathBuf {
    let mut candidate = dir.join(base);
    let mut counter = 1;
    while candidate.exists() {
        counter += 1;
        candidate = dir.join(format!("{base}-{counter}"));
    }
    candidate
}

/// Move a consumed package out of `in\` or `out\`. A failure keeps its reason
/// in a `.txt` beside it (never a `.json`, which a reader would pick up).
fn move_package(
    from: &Path,
    dir: &Path,
    stem: &str,
    suffix: &str,
    reason: Option<&str>,
) -> Result<()> {
    ensure_directory(dir)?;
    let target = unique_target(dir, &format!("{stem}.{}.{suffix}", now() as i64));
    fs::rename(from, &target).map_err(|e| format!("cannot move {} ({e})", from.display()))?;
    if let Some(reason) = reason {
        let note = dir.join(format!(
            "{}.txt",
            target
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "package".into())
        ));
        let _ = fs::write(note, format!("{reason}\n"));
    }
    Ok(())
}

// --- time --------------------------------------------------------------------

fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = i64::from(month);
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    (year, month as u32, day as u32)
}

fn iso_from_epoch(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as i64;
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

fn offset_seconds(text: &str) -> Option<i64> {
    let sign = if text.starts_with('-') { -1 } else { 1 };
    let (hours, minutes) = text.get(1..)?.split_once(':')?;
    Some(sign * (hours.parse::<i64>().ok()? * 3600 + minutes.parse::<i64>().ok()? * 60))
}

/// `2026-11-19T16:00:00Z` (and a numeric offset) as Unix seconds. Fractional
/// seconds are ignored: this only ages a package.
fn epoch_from_iso(text: &str) -> Option<f64> {
    let text = text.trim();
    let (date, clock) = text.split_once('T').or_else(|| text.split_once(' '))?;
    let mut parts = date.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: u32 = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    let clock = clock.trim_end_matches('Z');
    let (time, offset) = match clock.find(['+', '-']) {
        Some(index) => (&clock[..index], offset_seconds(&clock[index..])?),
        None => (clock, 0),
    };
    let mut time = time.split(':');
    let hours: i64 = time.next()?.parse().ok()?;
    let minutes: i64 = time.next().unwrap_or("0").parse().ok()?;
    let seconds: i64 = time
        .next()
        .unwrap_or("0")
        .split('.')
        .next()
        .unwrap_or("0")
        .parse()
        .ok()?;
    Some(
        (days_from_civil(year, month, day) * 86_400 + hours * 3600 + minutes * 60 + seconds
            - offset) as f64,
    )
}

fn modified_epoch(path: &Path) -> Option<f64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs_f64())
}

// --- matching ----------------------------------------------------------------

/// Case-insensitive, one separator, no trailing space: the plugin normalises
/// exactly this way on Windows.
fn normalize_path(path: &str) -> String {
    let unified = path.trim().replace('/', "\\");
    let (prefix, rest) = match unified.strip_prefix("\\\\") {
        Some(rest) => ("\\\\", rest),
        None => ("", unified.as_str()),
    };
    let mut out = String::with_capacity(unified.len());
    out.push_str(prefix);
    for ch in rest.chars() {
        if ch == '\\' && out.ends_with('\\') {
            continue;
        }
        out.push(ch);
    }
    out.to_lowercase()
}

fn file_name(path: &str) -> String {
    path.rsplit(['\\', '/'])
        .next()
        .unwrap_or(path)
        .to_lowercase()
}

struct AlbumFrame {
    id: i64,
    path: String,
    name: String,
    taken_at: Option<f64>,
}

#[derive(Debug, PartialEq)]
enum FrameMatch {
    One(i64),
    Ambiguous,
    None,
}

/// The plan's two rules: exact normalised path first, then file name plus a
/// capture time within two seconds. A copy makes a path ambiguous, and only a
/// unique answer is applied -- nothing is guessed silently.
fn match_frame(frames: &[AlbumFrame], path: &str, taken_at: Option<f64>) -> FrameMatch {
    let key = normalize_path(path);
    let exact: Vec<&AlbumFrame> = frames.iter().filter(|f| f.path == key).collect();
    match exact.len() {
        1 => return FrameMatch::One(exact[0].id),
        0 => {}
        _ => return FrameMatch::Ambiguous,
    }
    let Some(taken_at) = taken_at else {
        return FrameMatch::None;
    };
    let name = file_name(&key);
    let hits: Vec<&AlbumFrame> = frames
        .iter()
        .filter(|f| f.name == name)
        .filter(|f| {
            f.taken_at
                .is_some_and(|t| (t - taken_at).abs() <= TIME_SLOP)
        })
        .collect();
    match hits.len() {
        1 => FrameMatch::One(hits[0].id),
        0 => FrameMatch::None,
        _ => FrameMatch::Ambiguous,
    }
}

fn album_frames(d: &Desktop, job: i64) -> Result<Vec<AlbumFrame>> {
    let rows = rows(
        &lock(&d.reader),
        "SELECT id,path,taken_at FROM images WHERE job_id=? AND COALESCE(copy_of,0)=0",
        [job],
    )?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            let id = row["id"].as_i64()?;
            let path = normalize_path(row["path"].as_str().unwrap_or_default());
            let name = file_name(&path);
            Some(AlbumFrame {
                id,
                path,
                name,
                taken_at: row["taken_at"].as_f64(),
            })
        })
        .collect())
}

// --- export ------------------------------------------------------------------

/// The effective keeper rule the review grid shows (`keepersOf`): a hand pick
/// wins its burst, otherwise the engine's suggested pick. A rejected frame is
/// never a keeper. `i` is the `images` alias.
const KEEPER: &str = "((COALESCE(i.picked,0)=1 AND COALESCE(i.rejected,0)=0)
    OR (COALESCE(i.rejected,0)=0
        AND NOT EXISTS (SELECT 1 FROM images p
                         WHERE p.job_id=i.job_id AND p.burst_key=i.burst_key
                           AND COALESCE(p.copy_of,0)=0 AND COALESCE(p.picked,0)=1
                           AND COALESCE(p.rejected,0)=0)
        AND (COALESCE(i.burst_pick,0)=1
             OR EXISTS (SELECT 1 FROM detections d
                         WHERE d.image_id=i.id AND COALESCE(d.burst_pick,0)=1))))";

/// Write the album's verdicts for the plugin to apply. Returns the package
/// name, its photo count and its absolute path.
pub fn export(d: &Desktop, job: i64) -> Result<Value> {
    crate::library::require_job(d, job)?;
    let settings = crate::operations::settings(d, job)?;
    let album = rows(
        &lock(&d.reader),
        "SELECT root,label FROM jobs WHERE id=?",
        [job],
    )?
    .pop()
    .unwrap_or(Value::Null);
    let root = album["root"].as_str().unwrap_or_default().to_string();
    let label = album["label"]
        .as_str()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            Path::new(&root)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .filter(|l| !l.is_empty());
    let frames = rows(
        &lock(&d.reader),
        "SELECT i.id,i.path,i.stars,i.rating,i.rating_verdict,i.rejected,i.picked,
                i.burst_key,i.picked AS hand_pick,
                COALESCE(i.stars,(SELECT MAX(d.stars) FROM detections d WHERE d.image_id=i.id)) AS manual_stars,
                COALESCE((SELECT MAX(d.burst_pick) FROM detections d
                           WHERE d.image_id=i.id AND COALESCE(d.burst_pick,0)=1), i.burst_pick) AS pick_flag,
                (SELECT COUNT(*) FROM detections d WHERE d.image_id=i.id) AS detections,
                (SELECT COUNT(*) FROM detections d
                  WHERE d.image_id=i.id AND COALESCE(d.cull_reason,'')='') AS live_detections,
                (SELECT COUNT(*) FROM detections d
                  WHERE d.image_id=i.id
                    AND COALESCE(d.region_type,'vehicle')='vehicle'
                    AND ((COALESCE(d.number,'')='' AND COALESCE(d.plate,'')='')
                         OR (d.group_agreement IS NOT NULL AND d.group_agreement < 0.5))) AS looks,
                CASE WHEN COALESCE(i.rejected,0)=1 THEN 1 ELSE 0 END AS hand_reject
           FROM images i
          WHERE i.job_id=? AND i.status='done' AND COALESCE(i.copy_of,0)=0
          ORDER BY i.path",
        [job],
    )?;

    // A hand pick suppresses the engine's suggestion for the whole burst; a
    // burstless frame is its own group, which the SQL above already gives it.
    let mut picked_bursts: HashSet<i64> = HashSet::new();
    for frame in &frames {
        if frame["hand_pick"].as_i64() == Some(1) && frame["hand_reject"].as_i64() != Some(1) {
            if let Some(burst) = frame["burst_key"].as_i64() {
                picked_bursts.insert(burst);
            }
        }
    }

    let prefix = {
        let prefix = settings.keyword_prefix.trim();
        if prefix.is_empty() {
            "Conrod".to_string()
        } else {
            prefix.to_string()
        }
    };
    let mut photos = Vec::with_capacity(frames.len());
    for frame in &frames {
        let manual_stars = frame["manual_stars"].as_i64();
        let auto_culled = frame["detections"].as_i64().unwrap_or(0) > 0
            && frame["live_detections"].as_i64().unwrap_or(0) == 0;
        let rejected = frame["hand_reject"].as_i64() == Some(1)
            || (settings.respect_culling && manual_stars.is_none() && auto_culled);
        let picked = frame["hand_pick"].as_i64() == Some(1);
        let burst = frame["burst_key"].as_i64();
        let suggested = frame["pick_flag"].as_i64() == Some(1);
        let auto_pick = !rejected
            && !picked
            && suggested
            && !burst.is_some_and(|key| picked_bursts.contains(&key));
        let keeper = picked || auto_pick;
        let stars = manual_stars.unwrap_or_else(|| {
            frame["rating"]
                .as_f64()
                .map_or(0, |rating| i64::from(sharpness::stars_for(rating)))
        });
        let mut keywords = Vec::new();
        if keeper {
            keywords.push(format!("{prefix} keeper"));
        }
        if rejected {
            keywords.push(format!("{prefix} cull"));
        }
        if let Some(burst) = burst {
            keywords.push(format!("{prefix} burst {burst}"));
        }
        // A pick flag lives in the catalog only, so a suggested keeper also
        // carries the photographer's chosen colour label; the plug-in sets
        // both, and the XMP sidecar carries the label for the no-plugin path.
        let pick_colour =
            crate::operations::pick_label_colour(&settings.pick_label).map(str::to_ascii_lowercase);
        let photo = Photo {
            path: frame["path"].as_str().unwrap_or_default().to_string(),
            rating: Some(stars),
            pick: Some(if rejected {
                -1
            } else if keeper {
                1
            } else {
                0
            }),
            label: if rejected {
                Some("red".into())
            } else if auto_pick {
                pick_colour.clone()
            } else if picked {
                Some("green".into())
            } else if frame["looks"].as_i64().unwrap_or(0) > 0 {
                Some("yellow".into())
            } else {
                None
            },
            keywords: Some(keywords),
            stars: Some(stars),
            verdict: frame["rating_verdict"].as_str().map(str::to_owned),
            burst,
            keeper: Some(keeper),
        };
        photos.push(serde_json::to_value(photo).map_err(err)?);
    }

    let envelope = Envelope {
        format: FORMAT.into(),
        version: VERSION,
        direction: TO_LIGHTROOM.into(),
        album: Album {
            job_id: Some(job),
            label: label.clone(),
            root: Some(root),
        },
        created_at: Some(iso_from_epoch(now())),
        producer: Some(Producer {
            app: "conrod".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        }),
        collection: label
            .clone()
            .map(|label| format!("Conrod keepers - {label}")),
        overwrite: Some(Overwrite {
            rating: settings.overwrite_rating,
            label: settings.overwrite_label,
        }),
        photos,
    };
    let directory = folder(d, "out");
    ensure_directory(&directory)?;
    let base = slug(label.as_deref().unwrap_or("album"));
    let stamp = now() as i64;
    let mut name = format!("{base}-{stamp}");
    let mut counter = 1;
    while directory.join(format!("{name}.json")).exists() {
        counter += 1;
        name = format!("{base}-{stamp}-{counter}");
    }
    let bytes = serde_json::to_vec_pretty(&envelope).map_err(err)?;
    let path = write_package(&directory, &name, &bytes)?;
    Ok(json!({
        "package": name,
        "photos": envelope.photos.len(),
        "path": path.to_string_lossy(),
    }))
}

// --- pending -----------------------------------------------------------------

fn package_summary(path: &Path) -> Value {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("package")
        .to_string();
    let parsed = fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    let album = parsed
        .as_ref()
        .and_then(|data| data["album"]["label"].as_str())
        .map(str::to_owned);
    let photos = parsed
        .as_ref()
        .and_then(|data| data["photos"].as_array().map(|list| list.len() as i64));
    let created = parsed
        .as_ref()
        .and_then(|data| data["created_at"].as_str())
        .and_then(epoch_from_iso)
        .or_else(|| modified_epoch(path));
    let age_s = created.map(|created| (now() - created).max(0.0).round() as i64);
    json!({"name": name, "album": album, "photos": photos, "age_s": age_s})
}

/// Packages waiting in `in\` (the default) or `out\`, newest first. `*.tmp`
/// and anything that is not `*.json` is ignored, as both sides promise.
pub fn pending(d: &Desktop, dir: Option<&str>) -> Result<Value> {
    let name = match dir.unwrap_or("in").to_ascii_lowercase().as_str() {
        "" | "in" => "in",
        "out" => "out",
        other => {
            return Err(format!(
                "lightroom_pending: unknown folder {other:?} (expected \"in\" or \"out\")"
            ))
        }
    };
    let directory = folder(d, name);
    let mut packages = Vec::new();
    if let Ok(entries) = fs::read_dir(&directory) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_json = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("json"));
            if is_json && path.is_file() {
                packages.push(package_summary(&path));
            }
        }
    }
    packages.sort_by(|a, b| {
        let age = |value: &Value| value["age_s"].as_i64().unwrap_or(i64::MAX);
        age(a)
            .cmp(&age(b))
            .then_with(|| a["name"].as_str().cmp(&b["name"].as_str()))
    });
    Ok(json!({ "packages": packages }))
}

// --- import ------------------------------------------------------------------

#[derive(Default)]
struct Counts {
    matched: i64,
    unmatched: i64,
    missing: i64,
    skipped: i64,
    failed: i64,
    left: i64,
}

fn read_package(path: &Path) -> Result<Envelope> {
    let text =
        fs::read_to_string(path).map_err(|e| format!("cannot read {} ({e})", path.display()))?;
    let value: Value = serde_json::from_str(&text).map_err(|e| format!("not JSON ({e})"))?;
    validate(&value, TO_CONROD)?;
    serde_json::from_value(value).map_err(|e| format!("unexpected package shape ({e})"))
}

fn valid_rating(rating: i64) -> bool {
    (0..=5).contains(&rating)
}

fn valid_label(label: &str) -> bool {
    COLORS.contains(&label.to_ascii_lowercase().as_str())
}

/// One row's verdict, applied only when it would change something: importing
/// the same package twice must leave the album exactly as it was.
const SET_FLAG: &str = "UPDATE images
    SET catalog_rating=?1, catalog_pick=?2, catalog_label=?3, catalog_at=?4
  WHERE id=?5
    AND (catalog_at IS NULL
         OR NOT (catalog_rating IS ?1 AND catalog_pick IS ?2 AND catalog_label IS ?3))";

fn apply_flag(
    tx: &Connection,
    image: i64,
    rating: Option<i64>,
    pick: Option<i64>,
    label: Option<&str>,
    at: f64,
) -> Result<()> {
    tx.execute(
        SET_FLAG,
        params![rating, pick, label.map(str::to_ascii_lowercase), at, image],
    )
    .map_err(err)?;
    Ok(())
}

/// Applies one package to the album; returns how many photos it matched.
fn apply_package(d: &Desktop, job: i64, envelope: &Envelope, counts: &mut Counts) -> Result<i64> {
    let before = counts.matched;
    let frames = album_frames(d, job)?;
    let db = lock(&d.db);
    let tx = db.unchecked_transaction().map_err(err)?;
    let at = now();
    for raw in &envelope.photos {
        let Ok(photo) = serde_json::from_value::<Photo>(raw.clone()) else {
            counts.skipped += 1;
            continue;
        };
        if photo.path.trim().is_empty()
            || photo.rating.is_some_and(|rating| !valid_rating(rating))
            || photo.pick.is_some_and(|pick| !(-1..=1).contains(&pick))
            || photo
                .label
                .as_deref()
                .is_some_and(|label| !valid_label(label))
        {
            counts.skipped += 1;
            continue;
        }
        match match_frame(&frames, &photo.path, None) {
            FrameMatch::One(image) => {
                apply_flag(
                    &tx,
                    image,
                    photo.rating,
                    photo.pick,
                    photo.label.as_deref(),
                    at,
                )?;
                counts.matched += 1;
            }
            // A duplicate path inside the album is as unusable as no match:
            // applying to either copy would be a guess.
            FrameMatch::Ambiguous | FrameMatch::None => counts.unmatched += 1,
        }
    }
    tx.commit().map_err(err)?;
    Ok(counts.matched - before)
}

fn import_packages(d: &Desktop, job: i64, counts: &mut Counts) -> Result<()> {
    let source = folder(d, "in");
    let done = folder(d, "done");
    let failed = folder(d, "failed");
    for directory in [&source, &done, &failed] {
        ensure_directory(directory)?;
    }
    let mut packages: Vec<PathBuf> = fs::read_dir(&source)
        .map_err(err)?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("json"))
        })
        .collect();
    // Deterministic order, so two imports of the same folder apply in the same
    // sequence and a test (or a support log) reads the same way.
    packages.sort();
    for path in packages {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("package")
            .to_string();
        let unmatched_before = counts.unmatched;
        match read_package(&path).and_then(|envelope| apply_package(d, job, &envelope, counts)) {
            // Nothing in it belongs to this album: it is another shoot's
            // package, so it stays in `in\` for that album's import instead
            // of being consumed here.
            Ok(0) if counts.unmatched > unmatched_before => counts.left += 1,
            Ok(_) => move_package(&path, &done, &stem, "done", None)?,
            Err(reason) => {
                counts.failed += 1;
                move_package(&path, &failed, &stem, "failed", Some(&reason))?;
            }
        }
    }
    Ok(())
}

fn missing_entries(d: &Desktop, job: i64) -> Result<i64> {
    let rows = rows(
        &lock(&d.reader),
        "SELECT COUNT(*) AS n FROM images
          WHERE job_id=? AND COALESCE(copy_of,0)=0 AND catalog_at IS NULL",
        [job],
    )?;
    Ok(rows.first().and_then(|row| row["n"].as_i64()).unwrap_or(0))
}

/// Share of the album's effective keepers that Lightroom also picked. The
/// shoot statistics card shows it as "matched your picks".
pub(crate) fn catalog_match(db: &Connection, job: i64) -> Result<Value> {
    let sql = format!(
        "SELECT COUNT(*) AS compared,
                COALESCE(SUM(CASE WHEN COALESCE(i.catalog_pick,0)=1 THEN 1 ELSE 0 END),0) AS matched
           FROM images i
          WHERE i.job_id=? AND i.status='done' AND COALESCE(i.copy_of,0)=0
            AND {KEEPER}"
    );
    let row = db
        .query_row(&sql, [job], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(err)?;
    let (compared, matched) = row;
    let percent = (compared > 0).then(|| (matched * 100 + compared / 2) / compared);
    Ok(json!({
        "matched": matched,
        "compared": compared,
        "percent": percent,
    }))
}

// --- a `.lrcat` is a SQLite catalog ------------------------------------------

/// Read-only and immutable: an import must never write to the photographer's
/// catalog, even while Lightroom has it open.
fn open_catalog(path: &Path) -> Result<Connection> {
    let encoded: String = path
        .to_string_lossy()
        .replace('\\', "/")
        .chars()
        .map(|ch| match ch {
            '%' => "%25".to_string(),
            '?' => "%3f".to_string(),
            '#' => "%23".to_string(),
            ' ' => "%20".to_string(),
            other => other.to_string(),
        })
        .collect();
    let uri = format!("file:{encoded}?mode=ro&immutable=1");
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_URI
        | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    Connection::open_with_flags(&uri, flags)
        .or_else(|_| {
            Connection::open_with_flags(
                path,
                OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
        })
        .map_err(|e| {
            format!(
                "lightroom_import: cannot open {} read-only ({e})",
                path.display()
            )
        })
}

struct CatalogEntry {
    path: String,
    rating: Option<i64>,
    pick: Option<i64>,
    label: Option<String>,
    taken_at: Option<f64>,
}

/// The catalog rows for the album's file names, masters only (the lowest id
/// wins, because a virtual copy shares its master's path).
fn catalog_entries(conn: &Connection, names: &HashSet<String>) -> Result<Vec<CatalogEntry>> {
    let names: Vec<&String> = names.iter().collect();
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    // A virtual copy has a `masterImage`; only the master is the file's flag.
    let masters_only = conn
        .prepare("PRAGMA table_info(Adobe_images)")
        .and_then(|mut statement| {
            let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
            Ok(columns
                .flatten()
                .any(|column| column.eq_ignore_ascii_case("masterImage")))
        })
        .unwrap_or(false);
    let masters = if masters_only {
        " AND i.masterImage IS NULL"
    } else {
        ""
    };
    for chunk in names.chunks(400) {
        let placeholders = vec!["?"; chunk.len()].join(",");
        let sql = format!(
            "SELECT COALESCE(f.idx_filename, f.baseName || '.' || f.extension, f.baseName) AS file_name,
                    r.absolutePath AS root_path,
                    fo.pathFromRoot AS folder_path,
                    i.rating AS rating,
                    i.pick AS pick,
                    i.colorLabels AS color_labels,
                    i.captureTime AS capture_time
               FROM Adobe_images i
               JOIN AgLibraryFile f ON f.id=i.rootFile
               JOIN AgLibraryFolder fo ON fo.id=f.folder
               JOIN AgLibraryRootFolder r ON r.id=fo.rootFolder
              WHERE lower(COALESCE(f.idx_filename, f.baseName || '.' || f.extension, f.baseName))
                    IN ({placeholders}){masters}
              ORDER BY i.id"
        );
        let mut statement = conn
            .prepare(&sql)
            .map_err(|e| format!("lightroom_import: this catalog cannot be read ({e})"))?;
        let mut found = statement
            .query_map(
                rusqlite::params_from_iter(chunk.iter().map(|name| name.as_str())),
                |row| {
                    let name: String = row.get(0)?;
                    let root: Option<String> = row.get(1)?;
                    let folder_path: Option<String> = row.get(2)?;
                    let rating: Option<f64> = row.get(3)?;
                    let pick: Option<f64> = row.get(4)?;
                    let label: Option<String> = row.get(5)?;
                    let taken: rusqlite::types::Value = row.get(6)?;
                    let mut combined = String::new();
                    combined.push_str(root.as_deref().unwrap_or_default());
                    combined.push('\\');
                    combined.push_str(folder_path.as_deref().unwrap_or_default());
                    combined.push('\\');
                    combined.push_str(&name);
                    Ok(CatalogEntry {
                        path: normalize_path(&combined),
                        rating: rating.map(|r| (r.round() as i64).clamp(0, 5)),
                        pick: pick.map(|p| {
                            if p < 0.0 {
                                -1
                            } else if p > 0.0 {
                                1
                            } else {
                                0
                            }
                        }),
                        label: label.as_deref().and_then(colour_label),
                        taken_at: capture_time(&taken),
                    })
                },
            )
            .map_err(|e| format!("lightroom_import: this catalog cannot be read ({e})"))?;
        for entry in found.by_ref() {
            let entry = entry
                .map_err(|e| format!("lightroom_import: this catalog cannot be read ({e})"))?;
            if seen.insert(entry.path.clone()) {
                entries.push(entry);
            }
        }
    }
    Ok(entries)
}

/// `Adobe_images.captureTime` is wall-clock text (`2026-10-06T10:23:45.12`, no
/// zone) in a real catalog, which is how `images.taken_at` is stored too (a
/// wall clock encoded as an epoch). A number is read as Lightroom's own epoch.
fn capture_time(value: &rusqlite::types::Value) -> Option<f64> {
    use rusqlite::types::Value as Sql;
    match value {
        Sql::Text(text) => epoch_from_iso(text),
        Sql::Real(seconds) => Some(seconds + LR_EPOCH_OFFSET),
        Sql::Integer(seconds) => Some(*seconds as f64 + LR_EPOCH_OFFSET),
        _ => None,
    }
}

/// Lightroom reports a colour label by name, `grey` or an empty string when it
/// has none. The first recognised name wins.
fn colour_label(text: &str) -> Option<String> {
    text.split(|ch: char| !ch.is_ascii_alphabetic())
        .map(str::to_ascii_lowercase)
        .find(|word| COLORS.contains(&word.as_str()))
}

fn import_catalog(d: &Desktop, job: i64, path: &Path, counts: &mut Counts) -> Result<()> {
    if !path.is_file() {
        return Err(format!(
            "lightroom_import: no catalog at {}",
            path.display()
        ));
    }
    let catalog = open_catalog(path)?;
    let frames = album_frames(d, job)?;
    let names: HashSet<String> = frames.iter().map(|frame| frame.name.clone()).collect();
    let entries = catalog_entries(&catalog, &names)?;
    let db = lock(&d.db);
    let tx = db.unchecked_transaction().map_err(err)?;
    let at = now();
    for entry in entries {
        match match_frame(&frames, &entry.path, entry.taken_at) {
            FrameMatch::One(image) => {
                apply_flag(
                    &tx,
                    image,
                    entry.rating,
                    entry.pick,
                    entry.label.as_deref(),
                    at,
                )?;
                counts.matched += 1;
            }
            FrameMatch::Ambiguous | FrameMatch::None => counts.unmatched += 1,
        }
    }
    tx.commit().map_err(err)
}

/// Apply what Lightroom said. `catalog` (an `.lrcat`) is the alternative to
/// the plugin's packages: the packages are the primary path, because they
/// work while Lightroom holds the catalog open.
pub fn import(d: &Desktop, job: i64, catalog: Option<&str>) -> Result<Value> {
    crate::library::require_job(d, job)?;
    let mut counts = Counts::default();
    match catalog.map(str::trim).filter(|path| !path.is_empty()) {
        Some(path) => import_catalog(d, job, Path::new(path), &mut counts)?,
        None => import_packages(d, job, &mut counts)?,
    }
    counts.missing = missing_entries(d, job)?;
    let matched = catalog_match(&lock(&d.reader), job)?;
    let percent = matched["percent"].clone();
    Ok(json!({
        "matched": counts.matched,
        "unmatched": counts.unmatched,
        "missing": counts.missing,
        "percent": percent,
        "skipped": counts.skipped,
        "failed": counts.failed,
        "left": counts.left,
    }))
}

// --- installing the plug-in --------------------------------------------------

/// The bundle Lightroom Classic loads from its Modules folder.
const PLUGIN_FOLDER: &str = "ConrodBridge.lrplugin";
/// Written inside our folder, so a later app knows which build put it there.
const VERSION_MARKER: &str = "conrod-version.txt";

/// Where the app bundled the plug-in (the installer's resources, or the repo
/// in a development build). Set once at start-up; tests pass their own path.
static PLUGIN_SOURCE: OnceLock<Option<PathBuf>> = OnceLock::new();

/// The shell tells the engine where the bundled plug-in is. Only the app does
/// this, which is what keeps start-up and Save from touching a test machine's
/// real Lightroom folder.
pub fn set_plugin_source(path: PathBuf) {
    let _ = PLUGIN_SOURCE.set(Some(path));
}

fn plugin_source() -> Option<PathBuf> {
    PLUGIN_SOURCE.get().cloned().flatten()
}

/// The folder the app should have bundled the plug-in in, on this build.
pub fn default_plugin_source() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../lightroom-plugin")
}

/// Lightroom Classic's Modules folder for this user.
pub fn modules_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let roaming = std::env::var_os("APPDATA")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("USERPROFILE")
                    .filter(|value| !value.is_empty())
                    .map(|home| PathBuf::from(home).join("AppData/Roaming"))
            })?;
        Some(roaming.join("Adobe").join("Lightroom").join("Modules"))
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME").filter(|value| !value.is_empty())?;
        Some(
            PathBuf::from(home)
                .join("Library")
                .join("Application Support")
                .join("Adobe")
                .join("Lightroom")
                .join("Modules"),
        )
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        None
    }
}

fn modules_or(modules: Option<&Path>) -> Result<PathBuf> {
    modules
        .map(Path::to_path_buf)
        .or_else(modules_dir)
        .ok_or_else(|| {
            "Conrod does not know where Lightroom Classic keeps its plug-ins on this system"
                .to_string()
        })
}

/// Number groups of a version, so `1.0.0-beta.8` orders before `1.0.0-beta.9`
/// and both sit below a later number. Good enough to decide an update.
fn version_key(text: &str) -> Vec<u64> {
    text.trim()
        .trim_start_matches(['v', 'V'])
        .split(['.', '-', '+'])
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

fn bundled_is_newer(bundled: &str, installed: &str) -> bool {
    if bundled.trim() == installed.trim() {
        return false;
    }
    version_key(bundled) > version_key(installed)
}

fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to).map_err(|e| format!("cannot create {} ({e})", to.display()))?;
    let entries =
        fs::read_dir(from).map_err(|e| format!("cannot read {} ({e})", from.display()))?;
    for entry in entries.flatten() {
        let source = entry.path();
        let target = to.join(entry.file_name());
        let meta =
            fs::metadata(&source).map_err(|e| format!("cannot read {} ({e})", source.display()))?;
        if meta.is_dir() {
            copy_tree(&source, &target)?;
        } else {
            fs::copy(&source, &target)
                .map_err(|e| format!("cannot copy {} ({e})", source.display()))?;
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn lightroom_running() -> bool {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    std::process::Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq Lightroom.exe", "/NH"])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|out| {
            String::from_utf8_lossy(&out.stdout)
                .to_ascii_lowercase()
                .contains("lightroom.exe")
        })
        .unwrap_or(false)
}

#[cfg(target_os = "macos")]
fn lightroom_running() -> bool {
    std::process::Command::new("pgrep")
        .args(["-x", "Lightroom"])
        .output()
        .is_ok_and(|out| out.status.success())
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn lightroom_running() -> bool {
    false
}

fn installed_version(target: &Path) -> Option<String> {
    if !target.join("Info.lua").is_file() {
        return None;
    }
    fs::read_to_string(target.join(VERSION_MARKER))
        .ok()
        .map(|text| text.trim().to_string())
        .filter(|text| !text.is_empty())
}

/// Where the plug-in stands, in the words the Settings card shows.
pub fn status(d: &Desktop, modules: Option<&Path>, source: Option<&Path>) -> Result<Value> {
    let modules = modules_or(modules)?;
    let target = modules.join(PLUGIN_FOLDER);
    let version = installed_version(&target);
    let source = source.map(Path::to_path_buf).or_else(plugin_source);
    let bundled = source.as_ref().is_some_and(|path| path.is_dir());
    let bundled_version = env!("CARGO_PKG_VERSION");
    let needs_update = bundled
        && version
            .as_deref()
            .is_some_and(|installed| bundled_is_newer(bundled_version, installed));
    Ok(json!({
        "installed": version.is_some(),
        "version": version,
        "bundled": bundled,
        "bundledVersion": bundled_version,
        "needsUpdate": needs_update,
        "modules": modules,
        "path": target,
        "running": lightroom_running(),
        "enabled": lock(&d.settings).lightroom_bridge,
    }))
}

/// Copy the bundled plug-in into Lightroom's Modules folder, replacing only our
/// own folder. A build already there at the same version is left alone.
pub fn install(modules: Option<&Path>, source: Option<&Path>) -> Result<Value> {
    let modules = modules_or(modules)?;
    let source = match source.map(Path::to_path_buf).or_else(plugin_source) {
        Some(path) => path,
        None => {
            return Err("the Conrod Bridge plug-in is not bundled with this build".into());
        }
    };
    if !source.is_dir() {
        return Err(format!(
            "the Conrod Bridge plug-in is missing from this build ({})",
            source.display()
        ));
    }
    let target = modules.join(PLUGIN_FOLDER);
    let version = env!("CARGO_PKG_VERSION");
    let updated = installed_version(&target).as_deref() != Some(version);
    if updated {
        if target.exists() {
            fs::remove_dir_all(&target)
                .map_err(|e| format!("cannot replace {} ({e})", target.display()))?;
        }
        copy_tree(&source, &target)?;
        let marker = target.join(VERSION_MARKER);
        fs::write(&marker, format!("{version}\n"))
            .map_err(|e| format!("cannot write {} ({e})", marker.display()))?;
    }
    Ok(json!({
        "installed": true,
        "updated": updated,
        "version": version,
        "path": target,
        "running": lightroom_running(),
    }))
}

/// Remove our folder from Lightroom's Modules folder, and nothing else.
pub fn remove(modules: Option<&Path>) -> Result<Value> {
    let modules = modules_or(modules)?;
    let target = modules.join(PLUGIN_FOLDER);
    let removed = target.is_dir();
    if removed {
        fs::remove_dir_all(&target)
            .map_err(|e| format!("cannot remove {} ({e})", target.display()))?;
    }
    Ok(json!({"installed": false, "removed": removed, "path": target}))
}

/// A real app start or Save keeps the installed copy in step with the setting.
/// Only runs when the shell configured a bundled source, so a test or the CLI
/// never touches the photographer's Lightroom folder.
pub fn sync(d: &Desktop) {
    let Some(modules) = modules_dir() else {
        return;
    };
    if plugin_source().is_none() {
        return;
    }
    if lock(&d.settings).lightroom_bridge {
        let _ = install(Some(&modules), None);
    } else {
        let _ = remove(Some(&modules));
    }
}

/// On start, update a stale plug-in when the bridge is on. A start never
/// removes anything.
pub fn sync_on_start(d: &Desktop) {
    if plugin_source().is_none() || !lock(&d.settings).lightroom_bridge {
        return;
    }
    let Some(modules) = modules_dir() else {
        return;
    };
    let _ = install(Some(&modules), None);
}

// --- tests -------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::Lib;

    fn frame(id: i64, path: &str, taken_at: Option<f64>) -> AlbumFrame {
        AlbumFrame {
            id,
            path: normalize_path(path),
            name: file_name(path),
            taken_at,
        }
    }

    fn write_package_json(lib: &Lib, name: &str, envelope: Value) -> PathBuf {
        let directory = lib.root.join("bridge").join("in");
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join(format!("{name}.json"));
        fs::write(&path, serde_json::to_vec(&envelope).unwrap()).unwrap();
        path
    }

    fn to_conrod(photos: Value) -> Value {
        json!({
            "format": FORMAT,
            "version": VERSION,
            "direction": TO_CONROD,
            "album": {"label": "Bathurst 1000", "root": "C:\\\\Photos"},
            "created_at": "2026-11-19T16:00:00Z",
            "producer": {"app": "conrod-bridge", "version": "1.0.0"},
            "photos": photos,
        })
    }

    #[test]
    fn the_envelope_round_trips_and_unknown_fields_are_ignored() {
        let photo = Photo {
            path: "C:\\Photos\\a.CR3".into(),
            rating: Some(5),
            pick: Some(1),
            label: Some("green".into()),
            keywords: Some(vec!["Conrod keeper".into()]),
            stars: Some(5),
            verdict: Some("sharp".into()),
            burst: Some(145),
            keeper: Some(true),
        };
        let envelope = Envelope {
            format: FORMAT.into(),
            version: VERSION,
            direction: TO_LIGHTROOM.into(),
            album: Album {
                job_id: Some(9),
                label: Some("Bathurst 1000".into()),
                root: Some("C:\\Photos".into()),
            },
            created_at: Some("2026-11-19T16:00:00Z".into()),
            producer: Some(Producer {
                app: "conrod".into(),
                version: "1.0.0-beta.8".into(),
            }),
            collection: Some("Conrod keepers - Bathurst 1000".into()),
            overwrite: Some(Overwrite {
                rating: false,
                label: false,
            }),
            photos: vec![serde_json::to_value(photo).unwrap()],
        };
        let text = serde_json::to_string(&envelope).unwrap();
        let mut value: Value = serde_json::from_str(&text).unwrap();
        value["future"] = json!({"ignored": true});
        validate(&value, TO_LIGHTROOM).unwrap();
        let back: Envelope = serde_json::from_value(value).unwrap();
        assert_eq!(back.format, FORMAT);
        assert_eq!(back.version, VERSION);
        assert_eq!(back.direction, TO_LIGHTROOM);
        assert_eq!(back.album.job_id, Some(9));
        assert_eq!(
            back.collection.as_deref(),
            Some("Conrod keepers - Bathurst 1000")
        );
        let photo: Photo = serde_json::from_value(back.photos[0].clone()).unwrap();
        assert_eq!((photo.rating, photo.pick), (Some(5), Some(1)));
        assert_eq!(photo.label.as_deref(), Some("green"));
        assert_eq!(photo.keywords.unwrap(), vec!["Conrod keeper"]);
        assert_eq!(
            (photo.stars, photo.burst, photo.keeper),
            (Some(5), Some(145), Some(true))
        );
    }

    #[test]
    fn a_newer_version_or_the_wrong_direction_is_refused() {
        let mut value = to_conrod(json!([]));
        validate(&value, TO_CONROD).unwrap();
        value["version"] = json!(2);
        assert!(validate(&value, TO_CONROD).unwrap_err().contains("newer"));
        value["version"] = json!(0);
        assert!(validate(&value, TO_CONROD)
            .unwrap_err()
            .contains("not supported"));
        value["version"] = json!("1");
        assert!(validate(&value, TO_CONROD).unwrap_err().contains("integer"));
        let mut value = to_conrod(json!([]));
        value["direction"] = json!("to-lightroom");
        assert!(validate(&value, TO_CONROD)
            .unwrap_err()
            .contains("direction"));
        let mut value = to_conrod(json!([]));
        value["format"] = json!("something.else");
        assert!(validate(&value, TO_CONROD).unwrap_err().contains("format"));
        let mut value = to_conrod(json!([]));
        value.as_object_mut().unwrap().remove("photos");
        assert!(validate(&value, TO_CONROD).unwrap_err().contains("photos"));
        assert!(validate(&json!([]), TO_CONROD)
            .unwrap_err()
            .contains("JSON object"));
    }

    #[test]
    fn matching_is_case_insensitive_and_falls_back_to_name_and_time() {
        let frames = vec![
            frame(1, "C:\\Photos\\A.CR3", Some(1000.0)),
            frame(2, "C:\\Photos\\B.CR3", Some(2000.0)),
            frame(3, "D:\\Copies\\A.CR3", Some(1000.0)),
        ];
        // A different case and the other separator still find the frame.
        assert_eq!(
            match_frame(&frames, "c:/photos/a.cr3 ", None),
            FrameMatch::One(1)
        );
        // Two frames share a file name; only the capture time tells them apart.
        assert_eq!(
            match_frame(&frames, "E:\\Other\\B.CR3", Some(2001.5)),
            FrameMatch::One(2)
        );
        assert_eq!(
            match_frame(&frames, "E:\\Other\\B.CR3", Some(2005.0)),
            FrameMatch::None
        );
        // A copy of A.CR3 matches by name too, so the path rule applies first.
        assert_eq!(
            match_frame(&frames, "C:\\Photos\\A.CR3", None),
            FrameMatch::One(1)
        );
        assert_eq!(
            match_frame(&frames, "C:\\Photos\\A.CR3", Some(1000.0)),
            FrameMatch::One(1)
        );
        // An ambiguous path is never guessed; neither is a name-only fallback.
        assert_eq!(
            match_frame(&frames, "C:\\Photos\\B.CR3", Some(2000.0)),
            FrameMatch::One(2)
        );
        assert_eq!(
            match_frame(&frames, "C:\\Photos\\Missing.CR3", Some(1000.0)),
            FrameMatch::None
        );
        let duplicates = vec![
            frame(1, "C:\\Photos\\A.CR3", None),
            frame(2, "c:\\photos\\a.cr3", None),
        ];
        assert_eq!(
            match_frame(&duplicates, "C:\\Photos\\A.CR3", None),
            FrameMatch::Ambiguous
        );
    }

    #[test]
    fn an_export_writes_a_versioned_package_the_plugin_can_read() {
        let lib = Lib::new("lightroom-export");
        let keeper = lib.frame("a.CR3", Some(1));
        let rejected = lib.frame("b.CR3", Some(1));
        let suggested = lib.frame("c.CR3", Some(3));
        lib.sql(
            "UPDATE images SET picked=1,stars=4 WHERE id=?",
            params![keeper],
        );
        lib.sql("UPDATE images SET rejected=1 WHERE id=?", params![rejected]);
        let suggestion = lib.detection(suggested, "vehicle", 0.9);
        lib.sql(
            "UPDATE detections SET burst_pick=1 WHERE id=?",
            params![suggestion],
        );

        let out = lib
            .run("lightroom_export", json!({"jobId": lib.job}))
            .unwrap();
        assert_eq!(out["photos"], 3);
        let name = out["package"].as_str().unwrap();
        let path = PathBuf::from(out["path"].as_str().unwrap());
        assert_eq!(
            path.file_name().unwrap().to_string_lossy(),
            format!("{name}.json")
        );
        assert!(path.is_file(), "the package is written");
        assert!(
            !lib.root
                .join("bridge/out")
                .join(format!("{name}.json.tmp"))
                .exists(),
            "the temporary file is renamed away"
        );

        let package: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        validate(&package, TO_LIGHTROOM).unwrap();
        assert_eq!(package["direction"], TO_LIGHTROOM);
        assert_eq!(package["album"]["job_id"], lib.job);
        assert_eq!(
            package["collection"], "Conrod keepers - Test",
            "the album label names the collection"
        );
        let photos = package["photos"].as_array().unwrap();
        assert_eq!(photos.len(), 3);
        assert_eq!(
            package["overwrite"],
            json!({"rating": false, "label": false})
        );
        let kept: Photo = serde_json::from_value(photos[0].clone()).unwrap();
        assert_eq!((kept.rating, kept.pick), (Some(4), Some(1)));
        assert_eq!(kept.label.as_deref(), Some("green"));
        assert_eq!(kept.keeper, Some(true));
        assert_eq!(kept.burst, Some(1));
        let keywords = kept.keywords.unwrap();
        assert!(
            keywords.contains(&"Conrod keeper".to_string()),
            "{keywords:?}"
        );
        assert!(
            keywords.contains(&"Conrod burst 1".to_string()),
            "{keywords:?}"
        );

        let dropped: Photo = serde_json::from_value(photos[1].clone()).unwrap();
        assert_eq!(dropped.pick, Some(-1));
        assert_eq!(dropped.label.as_deref(), Some("red"));
        assert_eq!(dropped.keeper, Some(false));
        assert!(dropped
            .keywords
            .unwrap()
            .contains(&"Conrod cull".to_string()));

        let auto: Photo = serde_json::from_value(photos[2].clone()).unwrap();
        assert_eq!(
            (auto.pick, auto.label.as_deref()),
            (Some(1), Some("blue")),
            "a suggested keeper is picked and carries the pick colour"
        );
        assert_eq!(auto.keeper, Some(true));
    }

    #[test]
    fn pending_lists_only_json_packages_newest_first() {
        let lib = Lib::new("lightroom-pending");
        let old = now() - 600.0;
        write_package_json(
            &lib,
            "old",
            to_conrod_with_created(json!([{"path": "C:\\Photos\\a.CR3"}]), &iso_from_epoch(old)),
        );
        let newer = now() - 60.0;
        write_package_json(
            &lib,
            "new",
            to_conrod_with_created(json!([]), &iso_from_epoch(newer)),
        );
        fs::write(
            lib.root.join("bridge/in/half-written.json.tmp"),
            b"{not json",
        )
        .unwrap();

        let out = lib.run("lightroom_pending", json!({"dir": "in"})).unwrap();
        let packages = out["packages"].as_array().unwrap();
        assert_eq!(packages.len(), 2, "the *.tmp file is ignored");
        assert_eq!(packages[0]["name"], "new");
        assert_eq!(packages[1]["name"], "old");
        assert_eq!(packages[1]["album"], "Bathurst 1000");
        assert_eq!(packages[1]["photos"], 1);
        let age = packages[1]["age_s"].as_i64().unwrap();
        assert!((590..=610).contains(&age), "age was {age}");

        assert!(lib.run("lightroom_pending", json!({"dir": "out"})).is_ok());
        assert!(lib
            .run("lightroom_pending", json!({"dir": "somewhere"}))
            .is_err());
    }

    fn to_conrod_with_created(photos: Value, created_at: &str) -> Value {
        let mut value = to_conrod(photos);
        value["created_at"] = json!(created_at);
        value
    }

    #[test]
    fn an_import_applies_the_flags_and_moves_the_package() {
        let lib = Lib::new("lightroom-import");
        let keeper = lib.frame("a.CR3", Some(1));
        let plain = lib.frame("b.CR3", Some(2));
        lib.sql("UPDATE images SET picked=1 WHERE id=?", params![keeper]);

        let package = write_package_json(
            &lib,
            "bathurst-1000-1",
            to_conrod(json!([
                // Forward slashes and the wrong case still match.
                {"path": lib.root.join("A.CR3").to_string_lossy().replace('\\', "/"), "rating": 5, "pick": 1, "label": "Green"},
                {"path": "C:\\Somewhere\\NotHere.CR3", "rating": 2, "pick": -1, "label": "red"},
            ])),
        );
        let out = lib
            .run("lightroom_import", json!({"jobId": lib.job}))
            .unwrap();
        assert_eq!(out["matched"], 1);
        assert_eq!(out["unmatched"], 1);
        assert_eq!(out["missing"], 1, "b.CR3 has no catalog entry");
        assert_eq!(out["percent"], 100, "the one keeper carries pick 1");
        assert_eq!(out["failed"], 0);
        assert_eq!(out["skipped"], 0);

        assert_eq!(
            lib.one::<i64>(
                "SELECT catalog_pick FROM images WHERE id=?",
                params![keeper]
            ),
            1
        );
        assert_eq!(
            lib.one::<i64>(
                "SELECT catalog_rating FROM images WHERE id=?",
                params![keeper]
            ),
            5
        );
        assert_eq!(
            lib.one::<String>(
                "SELECT catalog_label FROM images WHERE id=?",
                params![keeper]
            ),
            "green"
        );
        assert_eq!(
            lib.one::<Option<String>>(
                "SELECT catalog_rating FROM images WHERE id=?",
                params![plain]
            ),
            None
        );
        // Conrod's own verdict is untouched: the catalog is a second opinion.
        assert_eq!(
            lib.one::<i64>("SELECT picked FROM images WHERE id=?", params![keeper]),
            1
        );

        assert!(!package.exists(), "the package left the in folder");
        let done: Vec<_> = fs::read_dir(lib.root.join("bridge/done"))
            .unwrap()
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(done.len(), 1);
        assert!(done[0].starts_with("bathurst-1000-1."));
        assert!(done[0].ends_with(".done"));
    }

    #[test]
    fn importing_the_same_package_twice_changes_nothing() {
        let lib = Lib::new("lightroom-idempotent");
        let frame = lib.frame("a.CR3", Some(1));
        lib.sql("UPDATE images SET picked=1 WHERE id=?", params![frame]);
        let envelope = to_conrod(json!([
            {"path": lib.root.join("a.CR3").to_string_lossy(), "rating": 3, "pick": 1, "label": "blue"},
        ]));
        write_package_json(&lib, "first", envelope.clone());
        let first = lib
            .run("lightroom_import", json!({"jobId": lib.job}))
            .unwrap();
        let stamp: f64 = lib.one("SELECT catalog_at FROM images WHERE id=?", params![frame]);

        write_package_json(&lib, "second", envelope);
        let second = lib
            .run("lightroom_import", json!({"jobId": lib.job}))
            .unwrap();
        assert_eq!(first["matched"], second["matched"]);
        assert_eq!(first["missing"], second["missing"]);
        assert_eq!(first["percent"], second["percent"]);
        assert_eq!(
            lib.one::<f64>("SELECT catalog_at FROM images WHERE id=?", params![frame]),
            stamp,
            "an unchanged verdict is not rewritten"
        );
        assert_eq!(
            lib.one::<i64>(
                "SELECT catalog_rating FROM images WHERE id=?",
                params![frame]
            ),
            3
        );
    }

    #[test]
    fn a_package_for_another_album_stays_in_the_folder() {
        let lib = Lib::new("lightroom-foreign");
        lib.frame("a.CR3", None);
        let package = write_package_json(
            &lib,
            "elsewhere",
            to_conrod(json!([{"path": "C:\\Other\\Shoot\\x.CR3", "rating": 2}])),
        );
        let out = lib
            .run("lightroom_import", json!({"jobId": lib.job}))
            .unwrap();
        assert_eq!(
            (out["matched"].as_i64(), out["unmatched"].as_i64()),
            (Some(0), Some(1))
        );
        assert_eq!(out["left"], 1);
        assert!(package.exists(), "not consumed by an album it is not for");
        assert!(!lib
            .root
            .join("bridge/done")
            .read_dir()
            .unwrap()
            .any(|_| true));
    }

    #[test]
    fn an_unchanged_empty_verdict_still_counts_the_frame_as_catalogued() {
        let lib = Lib::new("lightroom-empty");
        let frame = lib.frame("a.CR3", None);
        write_package_json(
            &lib,
            "blank",
            to_conrod(json!([{"path": lib.root.join("a.CR3").to_string_lossy()}])),
        );
        let out = lib
            .run("lightroom_import", json!({"jobId": lib.job}))
            .unwrap();
        assert_eq!(
            (out["matched"].as_i64(), out["missing"].as_i64()),
            (Some(1), Some(0))
        );
        assert!(lib
            .one::<Option<f64>>("SELECT catalog_at FROM images WHERE id=?", params![frame])
            .is_some());
    }

    #[test]
    fn a_bad_package_goes_to_failed_with_its_reason() {
        let lib = Lib::new("lightroom-failed");
        fs::create_dir_all(lib.root.join("bridge/in")).unwrap();
        fs::write(
            lib.root.join("bridge/in/newer.json"),
            serde_json::to_vec(&json!({
                "format": FORMAT,
                "version": 2,
                "direction": TO_CONROD,
                "album": {},
                "photos": [],
            }))
            .unwrap(),
        )
        .unwrap();
        let out = lib
            .run("lightroom_import", json!({"jobId": lib.job}))
            .unwrap();
        assert_eq!(out["matched"], 0);
        assert_eq!(out["failed"], 1);
        let failed: Vec<_> = fs::read_dir(lib.root.join("bridge/failed"))
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .collect();
        assert_eq!(failed.len(), 2, "the package and its reason");
        assert!(failed
            .iter()
            .any(|path| path.extension().is_some_and(|e| e == "txt")));
    }

    #[test]
    fn a_catalog_gives_the_same_flags_and_is_never_written() {
        let lib = Lib::new("lightroom-lrcat");
        let exact = lib.frame("IMG_1.CR3", Some(1));
        let fallback = lib.frame("IMG_2.CR3", Some(2));
        lib.sql("UPDATE images SET picked=1 WHERE id=?", params![exact]);
        lib.sql(
            "UPDATE images SET taken_at=? WHERE id=?",
            params![epoch_from_iso("2026-10-06T10:23:45").unwrap(), fallback],
        );
        let catalog = lib.root.join("test.lrcat");
        {
            let conn = Connection::open(&catalog).unwrap();
            conn.execute_batch(&format!(
                "CREATE TABLE AgLibraryRootFolder(id INTEGER PRIMARY KEY, absolutePath TEXT, name TEXT);
                 CREATE TABLE AgLibraryFolder(id INTEGER PRIMARY KEY, pathFromRoot TEXT, rootFolder INTEGER);
                 CREATE TABLE AgLibraryFile(id INTEGER PRIMARY KEY, folder INTEGER, baseName TEXT, extension TEXT, idx_filename TEXT);
                 CREATE TABLE Adobe_images(id INTEGER PRIMARY KEY, rootFile INTEGER, rating REAL, pick INTEGER, colorLabels TEXT, captureTime TEXT, masterImage INTEGER);
                 INSERT INTO AgLibraryRootFolder VALUES(1, '{}', 'Photos');
                 INSERT INTO AgLibraryRootFolder VALUES(2, 'C:\\Elsewhere', 'Elsewhere');
                 INSERT INTO AgLibraryFolder VALUES(1, '', 1);
                 INSERT INTO AgLibraryFolder VALUES(2, '2026\\10', 2);
                 INSERT INTO AgLibraryFile VALUES(1, 1, 'IMG_1', 'CR3', 'IMG_1.CR3');
                 INSERT INTO AgLibraryFile VALUES(2, 2, 'IMG_2', 'CR3', 'IMG_2.CR3');
                 INSERT INTO Adobe_images VALUES(1, 1, 4.0, 1.0, 'Green', '2026-10-06T10:20:00', NULL);
                 INSERT INTO Adobe_images VALUES(3, 1, 1.0, 0.0, 'Red', '2026-10-06T10:20:00', 1);
                 INSERT INTO Adobe_images VALUES(2, 2, 2.0, 0.0, '', '2026-10-06T10:23:46.5', NULL);",
                lib.root.to_string_lossy().replace('\'', "''")
            ))
            .unwrap();
        }
        let before = fs::read(&catalog).unwrap();

        let out = lib
            .run(
                "lightroom_import",
                json!({"jobId": lib.job, "catalog": catalog.to_string_lossy()}),
            )
            .unwrap();
        assert_eq!(
            out["matched"], 2,
            "the path matches one, the name+time the other"
        );
        assert_eq!(out["unmatched"], 0);
        assert_eq!(out["missing"], 0);
        assert_eq!(out["percent"], 100);
        assert_eq!(
            lib.one::<i64>("SELECT catalog_pick FROM images WHERE id=?", params![exact]),
            1
        );
        assert_eq!(
            lib.one::<String>(
                "SELECT catalog_label FROM images WHERE id=?",
                params![exact]
            ),
            "green"
        );
        assert_eq!(
            lib.one::<i64>(
                "SELECT catalog_rating FROM images WHERE id=?",
                params![fallback]
            ),
            2
        );
        assert_eq!(
            lib.one::<Option<String>>(
                "SELECT catalog_label FROM images WHERE id=?",
                params![fallback]
            ),
            None
        );
        assert_eq!(
            fs::read(&catalog).unwrap(),
            before,
            "a catalog is opened read-only"
        );
    }

    #[test]
    fn a_bad_photo_entry_is_skipped_not_fatal() {
        let lib = Lib::new("lightroom-bad-photo");
        let frame = lib.frame("a.CR3", None);
        let mut envelope = to_conrod(json!([
            {"path": lib.root.join("a.CR3").to_string_lossy(), "rating": 3},
            {"rating": 5},
            {"path": lib.root.join("a.CR3").to_string_lossy(), "rating": 9},
        ]));
        envelope["photos"][1] = json!("not an object");
        write_package_json(&lib, "mixed", envelope);
        let out = lib
            .run("lightroom_import", json!({"jobId": lib.job}))
            .unwrap();
        assert_eq!(out["matched"], 1);
        assert_eq!(out["skipped"], 2);
        assert_eq!(out["failed"], 0, "a bad entry never fails the package");
        assert_eq!(
            lib.one::<i64>(
                "SELECT catalog_rating FROM images WHERE id=?",
                params![frame]
            ),
            3
        );
        assert!(lib
            .root
            .join("bridge/done")
            .read_dir()
            .unwrap()
            .next()
            .is_some());
    }

    #[test]
    fn the_catalog_match_counts_keepers_with_lightroom_picks() {
        let lib = Lib::new("lightroom-match");
        let picked = lib.frame("a.CR3", Some(1));
        let unpicked = lib.frame("b.CR3", Some(1));
        lib.sql("UPDATE images SET picked=1 WHERE id=?", params![picked]);
        lib.sql(
            "UPDATE images SET catalog_pick=1 WHERE id=?",
            params![picked],
        );
        lib.sql(
            "UPDATE images SET catalog_pick=0 WHERE id=?",
            params![unpicked],
        );
        let value = catalog_match(&lib.db(), lib.job).unwrap();
        assert_eq!(value["compared"], 1, "only one effective keeper");
        assert_eq!(value["matched"], 1);
        assert_eq!(value["percent"], 100);

        lib.sql(
            "UPDATE images SET catalog_pick=0 WHERE id=?",
            params![picked],
        );
        let value = catalog_match(&lib.db(), lib.job).unwrap();
        assert_eq!(value["percent"], 0);
    }

    #[test]
    fn a_time_is_read_and_written_as_the_package_spells_it() {
        assert_eq!(iso_from_epoch(0.0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_from_epoch(LR_EPOCH_OFFSET), "2001-01-01T00:00:00Z");
        let back = epoch_from_iso("2026-11-19T16:00:00Z").unwrap();
        assert_eq!(iso_from_epoch(back), "2026-11-19T16:00:00Z");
        assert_eq!(epoch_from_iso("1970-01-01T02:00:00+02:00").unwrap(), 0.0);
        assert_eq!(epoch_from_iso("1970-01-01T00:00:00.500Z").unwrap(), 0.0);
        assert_eq!(epoch_from_iso("not a time"), None);
    }

    fn plugin_source() -> PathBuf {
        default_plugin_source().join(PLUGIN_FOLDER)
    }

    #[test]
    fn the_plugin_installs_into_the_given_modules_folder_and_leaves_others_alone() {
        let lib = Lib::new("lightroom-plugin-install");
        let modules = lib.root.join("modules");
        let other = modules.join("SomeoneElse.lrplugin");
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join("Info.lua"), b"-- not ours").unwrap();
        let source = plugin_source();
        assert!(source.is_dir(), "the checkout carries the plug-in");

        let before = status(lib.d(), Some(&modules), Some(&source)).unwrap();
        assert_eq!(before["installed"], false);
        assert_eq!(before["needsUpdate"], false, "nothing installed yet");
        assert_eq!(before["bundled"], true);
        // The plug-in follows the Lightroom setting, whose default the
        // file-based change turned off; the test tracks the default instead
        // of hard-coding either way.
        let bridge_default = conrod_core::settings::Settings::default().lightroom_bridge;
        assert_eq!(
            before["enabled"], bridge_default,
            "status follows the Lightroom setting's default"
        );

        let first = install(Some(&modules), Some(&source)).unwrap();
        assert_eq!(
            (first["installed"].as_bool(), first["updated"].as_bool()),
            (Some(true), Some(true))
        );
        let target = modules.join(PLUGIN_FOLDER);
        assert!(target.join("Info.lua").is_file());
        assert!(target.join("ConrodBridge.lua").is_file());
        assert_eq!(
            fs::read_to_string(target.join(VERSION_MARKER))
                .unwrap()
                .trim(),
            env!("CARGO_PKG_VERSION")
        );
        assert!(
            other.join("Info.lua").is_file(),
            "another plug-in is untouched"
        );

        let again = install(Some(&modules), Some(&source)).unwrap();
        assert_eq!(
            again["updated"], false,
            "the same version is not copied again"
        );

        // An older marker is replaced, so the app updates a stale install.
        fs::write(target.join(VERSION_MARKER), b"0.0.1\n").unwrap();
        let after = status(lib.d(), Some(&modules), Some(&source)).unwrap();
        assert_eq!(after["installed"], true);
        assert_eq!(after["version"], "0.0.1");
        assert_eq!(after["needsUpdate"], true);
        let updated = install(Some(&modules), Some(&source)).unwrap();
        assert_eq!(updated["updated"], true);
        assert_eq!(
            status(lib.d(), Some(&modules), Some(&source)).unwrap()["needsUpdate"],
            false
        );

        let removed = remove(Some(&modules)).unwrap();
        assert_eq!(
            (removed["installed"].as_bool(), removed["removed"].as_bool()),
            (Some(false), Some(true))
        );
        assert!(!target.exists());
        assert!(other.join("Info.lua").is_file(), "only our folder went");
        assert_eq!(
            remove(Some(&modules)).unwrap()["removed"],
            false,
            "a second remove is a no-op"
        );
    }

    #[test]
    fn a_newer_version_orders_by_its_numbers() {
        assert!(bundled_is_newer("1.0.0", "0.9.9"));
        assert!(bundled_is_newer("1.0.0-beta.9", "1.0.0-beta.8"));
        assert!(!bundled_is_newer("1.0.0-beta.8", "1.0.0-beta.8"));
        assert!(!bundled_is_newer("0.9.0", "1.0.0"));
    }
}
