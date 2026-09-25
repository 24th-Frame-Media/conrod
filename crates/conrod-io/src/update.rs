//! The in-app updater: find a newer release, download its installer, verify it
//! against the release's `SHA256SUMS.txt`, and hand it to the platform.
//!
//! **Fail closed.** A release without a checksum for its installer is never
//! offered: the Python updater installed whatever it downloaded when the sums file
//! was missing, and that is the one thing a verifying updater must not do.
//!
//! Releases are tagged `v*`. What makes one an update is an installer:
//! `Conrod-<version>-win64-setup.exe`, a per-user NSIS installer that runs silently
//! with `/S`. The Python app's old zips (v0.8.0 and earlier) and the asset files of
//! `assets-v1` have none, so they are never offered.

use crate::assets;
use serde_json::Value;
use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const REPO_API: &str = "https://api.github.com/repos/kapsikkum/conrod";
const INSTALLER_SUFFIX: &str = "-win64-setup.exe";
const SUMS: &str = "SHA256SUMS.txt";

/// A dotted version with an optional pre-release, e.g. `1.0.0-beta.2`
/// (a leading `v` is ignored).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    core: [u64; 3],
    pre: Vec<Ident>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Ident {
    /// Numeric identifiers sort below textual ones (semver).
    Num(u64),
    Text(String),
}

impl Version {
    pub fn parse(text: &str) -> Option<Version> {
        let text = text.trim().trim_start_matches('v');
        let (core, pre) = match text.split_once('-') {
            Some((c, p)) => (c, Some(p)),
            None => (text, None),
        };
        let mut parts = core.split('.').map(|p| p.parse::<u64>());
        let core = [
            parts.next()?.ok()?,
            parts.next()?.ok()?,
            parts.next()?.ok()?,
        ];
        if parts.next().is_some() {
            return None;
        }
        let pre = pre
            .map(|p| {
                p.split('.')
                    .map(|i| {
                        i.parse()
                            .map_or_else(|_| Ident::Text(i.to_string()), Ident::Num)
                    })
                    .collect()
            })
            .unwrap_or_default();
        Some(Version { core, pre })
    }

    pub fn is_pre(&self) -> bool {
        !self.pre.is_empty()
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.core
            .cmp(&other.core)
            .then_with(|| match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater, // a release outranks its own pre-releases
                (false, true) => Ordering::Less,
                (false, false) => self.pre.cmp(&other.pre),
            })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.core[0], self.core[1], self.core[2])?;
        for (i, id) in self.pre.iter().enumerate() {
            let sep = if i == 0 { '-' } else { '.' };
            match id {
                Ident::Num(n) => write!(f, "{sep}{n}")?,
                Ident::Text(t) => write!(f, "{sep}{t}")?,
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Download {
    pub name: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: Version,
    pub tag: String,
    pub notes: String,
    pub installer: Download,
}

/// The hash `sha256sum` recorded for `name`: lines are `<hex>  <name>` or `<hex> *<name>`.
fn checksum_for(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.trim().split_once(char::is_whitespace)?;
        let file = file.trim().trim_start_matches('*');
        (file == name && hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()))
            .then(|| hash.to_lowercase())
    })
}

/// The newest release newer than `current`, from GitHub's release list. `sums` reads
/// the text at a URL (the release's `SHA256SUMS.txt`).
pub fn choose(
    releases: &Value,
    current: &Version,
    allow_pre: bool,
    sums: &mut dyn FnMut(&str) -> Result<String, String>,
) -> Result<Option<Release>, String> {
    let best = releases
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| !r["draft"].as_bool().unwrap_or(false))
        .filter(|r| allow_pre || !r["prerelease"].as_bool().unwrap_or(false))
        .filter_map(|r| {
            let tag = r["tag_name"].as_str()?;
            tag.starts_with('v').then_some(())?;
            r["assets"]
                .as_array()?
                .iter()
                .any(|a| {
                    a["name"]
                        .as_str()
                        .is_some_and(|n| n.ends_with(INSTALLER_SUFFIX))
                })
                .then_some(())?;
            Some((Version::parse(tag)?, r))
        })
        .filter(|(v, _)| v > current)
        .max_by(|a, b| a.0.cmp(&b.0));
    let Some((version, release)) = best else {
        return Ok(None);
    };
    let assets = release["assets"].as_array().cloned().unwrap_or_default();
    let asset = |pred: &dyn Fn(&str) -> bool| {
        assets
            .iter()
            .find(|a| a["name"].as_str().is_some_and(pred))
            .cloned()
    };
    let installer = asset(&|n| n.ends_with(INSTALLER_SUFFIX))
        .ok_or_else(|| format!("release {version} has no installer"))?;
    let sums_asset = asset(&|n| n == SUMS)
        .ok_or_else(|| format!("release {version} has no {SUMS}, so it is not offered"))?;
    let name = installer["name"].as_str().unwrap_or_default().to_string();
    let text = sums(
        sums_asset["browser_download_url"]
            .as_str()
            .unwrap_or_default(),
    )?;
    let sha256 = checksum_for(&text, &name).ok_or_else(|| {
        format!("release {version} has no checksum for {name}, so it is not offered")
    })?;
    Ok(Some(Release {
        tag: release["tag_name"].as_str().unwrap_or_default().to_string(),
        notes: release["body"].as_str().unwrap_or_default().to_string(),
        installer: Download {
            url: installer["browser_download_url"]
                .as_str()
                .unwrap_or_default()
                .to_string(),
            size: installer["size"].as_u64().unwrap_or(0),
            name,
            sha256,
        },
        version,
    }))
}

#[derive(Debug, Clone, PartialEq)]
pub struct AtomEntry {
    pub tag: String,
    pub title: String,
    pub content: String,
}

pub fn parse_atom_entries(xml: &str) -> Vec<AtomEntry> {
    let mut entries = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<entry>") {
        let entry_body = &rest[start + 7..];
        let end = match entry_body.find("</entry>") {
            Some(e) => e,
            None => break,
        };
        let block = &entry_body[..end];
        rest = &entry_body[end + 8..];

        let title = extract_tag_content(block, "title").unwrap_or_default();
        let content = extract_tag_content(block, "content").unwrap_or_default();
        let clean_notes = unescape_html(&content);
        if !title.is_empty() {
            entries.push(AtomEntry {
                tag: title.clone(),
                title,
                content: clean_notes,
            });
        }
    }
    entries
}

fn extract_tag_content(xml: &str, tag: &str) -> Option<String> {
    let open_pattern = format!("<{tag}");
    let close_pattern = format!("</{tag}>");
    let open_idx = xml.find(&open_pattern)?;
    let after_open = &xml[open_idx + open_pattern.len()..];
    let tag_end_idx = after_open.find('>')?;
    let content_start = &after_open[tag_end_idx + 1..];
    let close_idx = content_start.find(&close_pattern)?;
    Some(content_start[..close_idx].trim().to_string())
}

fn unescape_html(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

pub fn choose_from_atom(
    entries: &[AtomEntry],
    current: &Version,
    allow_pre: bool,
    owner_repo: &str,
    exists: &mut dyn FnMut(&str) -> bool,
    sums: &mut dyn FnMut(&str) -> Result<String, String>,
) -> Result<Option<Release>, String> {
    let best = entries
        .iter()
        .filter_map(|e| {
            let ver = Version::parse(&e.tag)?;
            if !allow_pre && ver.is_pre() {
                return None;
            }
            if &ver <= current {
                return None;
            }
            Some((ver, e))
        })
        .max_by(|a, b| a.0.cmp(&b.0));

    let Some((version, entry)) = best else {
        return Ok(None);
    };

    let tag = entry.tag.trim();
    let installer_name = format!("Conrod-{version}-win64-setup.exe");
    let sums_url = format!("https://github.com/{owner_repo}/releases/download/{tag}/{SUMS}");
    let installer_url =
        format!("https://github.com/{owner_repo}/releases/download/{tag}/{installer_name}");

    // The Atom feed only gives us a title to build these URLs from; unlike
    // the REST path's asset list, nothing here confirms the files actually
    // exist. Fail closed rather than offer a 404.
    if !exists(&installer_url) || !exists(&sums_url) {
        return Err(format!(
            "release {version} is missing its installer or {SUMS}, so it is not offered"
        ));
    }

    let text = sums(&sums_url)?;
    let sha256 = checksum_for(&text, &installer_name).ok_or_else(|| {
        format!("release {version} has no checksum for {installer_name}, so it is not offered")
    })?;

    Ok(Some(Release {
        version,
        tag: tag.to_string(),
        notes: entry.content.clone(),
        installer: Download {
            name: installer_name,
            url: installer_url,
            size: 0,
            sha256,
        },
    }))
}

#[derive(Clone)]
struct CacheEntry {
    checked_at: Instant,
    etag: Option<String>,
    release: Option<Release>,
}

static CACHE: Mutex<Option<CacheEntry>> = Mutex::new(None);
const CACHE_TTL_SECS: u64 = 600; // 10 minutes

pub fn owner_repo_from_api(api: &str) -> Option<(&str, &str)> {
    let trimmed = api.trim_end_matches('/');
    let suffix = trimmed.strip_prefix("https://api.github.com/repos/")?;
    let (owner, repo) = suffix.split_once('/')?;
    Some((owner, repo))
}

/// Does `url` resolve to something (any non-error status)? Used to confirm
/// an Atom-derived asset URL exists before offering it, since the feed gives
/// us only a title to build the URL from, unlike the REST path's asset list.
fn head_exists(url: &str) -> bool {
    ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(10)))
        .build()
        .new_agent()
        .head(url)
        .header("User-Agent", "conrod-updater")
        .call()
        .is_ok_and(|resp| resp.status().as_u16() < 400)
}

fn get_text(url: &str) -> Result<String, String> {
    let mut resp = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(30)))
        .build()
        .new_agent()
        .get(url)
        .header("User-Agent", "conrod-updater")
        .call()
        .map_err(|e| e.to_string())?;
    let status = resp.status().as_u16();
    if status >= 400 {
        return Err(format!("HTTP {status} fetching {url}"));
    }
    resp.body_mut().read_to_string().map_err(|e| e.to_string())
}

/// Ask GitHub (or `api`, a mirror) for a newer release than `current`.
pub fn latest(api: &str, current: &Version, allow_pre: bool) -> Result<Option<Release>, String> {
    latest_with_cache(api, current, allow_pre, false)
}

/// Ask GitHub for updates with 10-min cache, conditional ETag requests, and automatic fallback to releases.atom.
pub fn latest_with_cache(
    api: &str,
    current: &Version,
    allow_pre: bool,
    force: bool,
) -> Result<Option<Release>, String> {
    if !force {
        if let Ok(guard) = CACHE.lock() {
            if let Some(ref entry) = *guard {
                if entry.checked_at.elapsed() < Duration::from_secs(CACHE_TTL_SECS) {
                    return Ok(entry.release.clone());
                }
            }
        }
    }

    let cached_etag = CACHE
        .lock()
        .ok()
        .and_then(|g| g.as_ref().and_then(|e| e.etag.clone()));

    // 1. Try standard GitHub REST API with conditional ETag
    let api_url = format!("{}/releases?per_page=15", api.trim_end_matches('/'));
    let mut builder = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .new_agent()
        .get(&api_url)
        .header("User-Agent", "conrod-updater")
        .header("Accept", "application/vnd.github+json");

    if let Some(ref etag) = cached_etag {
        builder = builder.header("If-None-Match", etag);
    }

    let api_res = builder.call();

    match api_res {
        Ok(mut resp) => {
            let status = resp.status().as_u16();
            if status == 304 {
                // Not modified: refresh timestamp and reuse cached release
                if let Ok(mut guard) = CACHE.lock() {
                    if let Some(ref mut entry) = *guard {
                        entry.checked_at = Instant::now();
                        return Ok(entry.release.clone());
                    }
                }
            } else if status == 200 {
                let etag = resp
                    .headers()
                    .get("etag")
                    .and_then(|h| h.to_str().ok())
                    .map(String::from);
                let list: Value = resp.body_mut().read_json().map_err(|e| e.to_string())?;
                let found = choose(&list, current, allow_pre, &mut |u| get_text(u))?;
                if let Ok(mut guard) = CACHE.lock() {
                    *guard = Some(CacheEntry {
                        checked_at: Instant::now(),
                        etag,
                        release: found.clone(),
                    });
                }
                return Ok(found);
            }
            // If status is 403 (Rate limited) or anything else, fall through to Atom feed!
        }
        Err(_) => {
            // Network error on REST API: fall through to Atom feed!
        }
    }

    // 2. Fallback: Edge-cached Atom feed (Zero GitHub API rate limit)
    if let Some((owner, repo)) = owner_repo_from_api(api) {
        let atom_url = format!("https://github.com/{owner}/{repo}/releases.atom");
        if let Ok(xml) = get_text(&atom_url) {
            let entries = parse_atom_entries(&xml);
            let owner_repo = format!("{owner}/{repo}");
            if let Ok(found) = choose_from_atom(
                &entries,
                current,
                allow_pre,
                &owner_repo,
                &mut head_exists,
                &mut |u| get_text(u),
            ) {
                if let Ok(mut guard) = CACHE.lock() {
                    *guard = Some(CacheEntry {
                        checked_at: Instant::now(),
                        etag: None,
                        release: found.clone(),
                    });
                }
                return Ok(found);
            }
        }
    }

    // 3. If both failed, return an informative error
    Err("GitHub update check failed (API rate limit exceeded and fallback unreachable). Check your network or visit github.com/kapsikkum/conrod/releases".into())
}

/// Download the installer into `into`, verified against the release's checksum.
pub fn download(
    release: &Release,
    into: &Path,
    stop: &AtomicBool,
    progress: &mut dyn FnMut(u64, u64),
) -> Result<PathBuf, String> {
    std::fs::create_dir_all(into).map_err(|e| e.to_string())?;
    let (part, target) = (
        into.join(format!("{}.part", release.installer.name)),
        into.join(&release.installer.name),
    );
    let d = &release.installer;
    match assets::fetch(&d.url, &part, &d.sha256, d.size, stop, progress) {
        Ok(()) => std::fs::rename(&part, &target)
            .map(|()| target)
            .map_err(|e| e.to_string()),
        Err(e) => {
            let _ = std::fs::remove_file(&part);
            Err(e)
        }
    }
}

/// Whether this exe was put there by the installer (which leaves `uninstall.exe`
/// beside it) rather than unpacked from the portable zip; only the former can be
/// updated by running a newer installer.
pub fn installed_by_installer(exe: &Path) -> bool {
    exe.parent()
        .is_some_and(|dir| dir.join("uninstall.exe").is_file())
}

/// The PowerShell that waits for this process to exit, runs the installer silently, and
/// starts the app again whatever happened (an installer that failed leaves the old version,
/// which is still worth opening); a failure is written to `log`. Waiting is what makes the
/// swap deterministic: Windows will not replace the executable of a running process.
/// Single quotes in paths are doubled, so a path cannot end the string.
fn restart_script(pid: u32, installer: &Path, app: &Path, log: &Path) -> String {
    let q = |p: &Path| p.to_string_lossy().replace('\'', "''");
    format!(
        "$ErrorActionPreference = 'Stop'; \
         try {{ Wait-Process -Id {pid} -Timeout 60 -ErrorAction SilentlyContinue; \
         Start-Process -FilePath '{}' -ArgumentList '/S' -Wait }} \
         catch {{ $_ | Out-File -FilePath '{}' -Append }} \
         finally {{ Start-Process -FilePath '{}' }}",
        q(installer),
        q(log),
        q(app)
    )
}

/// Run the installer in a detached process and return; the caller then exits so the
/// installer can replace the files.
#[cfg(windows)]
pub fn apply(installer: &Path, app: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    use std::process::Stdio;
    // A hidden console, not DETACHED_PROCESS: PowerShell's `Start-Process -Wait` fails
    // without one, and CREATE_NO_WINDOW is ignored when combined with DETACHED_PROCESS.
    // A new process group keeps this app's Ctrl-C from reaching it.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command"])
        .arg(restart_script(
            std::process::id(),
            installer,
            app,
            &installer.with_extension("log"),
        ))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("could not start the installer: {e}"))
}

#[cfg(not(windows))]
pub fn apply(_installer: &Path, _app: &Path) -> Result<(), String> {
    Err("updating in place is only supported on Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const HASH: &str = "aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11aa11";

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    fn release(tag: &str, pre: bool, assets: &[&str]) -> Value {
        json!({
            "tag_name": tag, "prerelease": pre, "draft": false, "body": format!("notes for {tag}"),
            "assets": assets.iter().map(|n| json!({"name": n, "size": 42, "browser_download_url": format!("https://x/{tag}/{n}")})).collect::<Vec<_>>()
        })
    }

    fn sums_for(name: &str) -> impl FnMut(&str) -> Result<String, String> + '_ {
        move |_| Ok(format!("{HASH}  {name}\n"))
    }

    #[test]
    fn versions_order_the_way_semver_does() {
        assert!(v("0.1.0") > v("0.1.0-beta.2"));
        assert!(v("0.1.0-beta.10") > v("0.1.0-beta.2"));
        assert!(v("0.1.0-beta.2") > v("0.1.0-alpha.9"));
        assert!(v("0.2.0-beta.1") > v("0.1.9"));
        assert_eq!(v("v1.2.3-rc.1").to_string(), "1.2.3-rc.1");
        assert_eq!(Version::parse("nonsense"), None);
        assert_eq!(Version::parse("1.2"), None);
    }

    #[test]
    fn the_newest_native_release_is_chosen_and_the_python_ones_are_ignored() {
        let name = "Conrod-0.1.0-beta.3-win64-setup.exe";
        let list = json!([
            release("v0.9.9", false, &["Conrod-0.9.9-win64.zip", SUMS]),
            release(
                "v0.1.0-beta.2",
                true,
                &["Conrod-0.1.0-beta.2-win64-setup.exe", SUMS]
            ),
            release("v0.1.0-beta.3", true, &[name, SUMS]),
        ]);
        let found = choose(&list, &v("0.1.0-beta.1"), true, &mut sums_for(name))
            .unwrap()
            .unwrap();
        assert_eq!(found.version.to_string(), "0.1.0-beta.3");
        assert_eq!(found.installer.name, name);
        assert_eq!(found.installer.sha256, HASH);
        assert!(found.notes.contains("beta.3"));
        // up to date, or the only newer one is a pre-release the user did not ask for
        assert_eq!(
            choose(&list, &v("0.1.0-beta.3"), true, &mut sums_for(name)).unwrap(),
            None
        );
        assert_eq!(
            choose(&list, &v("0.0.1"), false, &mut sums_for(name)).unwrap(),
            None
        );
    }

    #[test]
    fn only_releases_with_an_installer_are_updates() {
        // Native releases are `v1.0.0`, beside the Python line's `v0.8.x` zips (no
        // installer, and older) and the assets-v1 pre-release.
        let name = "Conrod-1.0.0-win64-setup.exe";
        let list = json!([
            release("v0.8.0", false, &["Conrod-0.8.0-win64.zip", SUMS]),
            release("assets-v1", true, &["yolo11s-960.onnx"]),
            release(
                "v1.0.0-beta.2",
                true,
                &["Conrod-1.0.0-beta.2-win64-setup.exe", SUMS]
            ),
            release("v1.0.0", false, &[name, SUMS]),
        ]);
        let found = choose(&list, &v("1.0.0-beta.1"), true, &mut sums_for(name))
            .unwrap()
            .unwrap();
        assert_eq!(found.tag, "v1.0.0");
        assert_eq!(found.version.to_string(), "1.0.0");
        // a beta of the native app is not offered the Python line, however it is tagged
        assert_eq!(
            choose(&list, &v("1.0.0"), true, &mut sums_for(name)).unwrap(),
            None
        );
    }

    #[test]
    fn a_release_without_a_checksum_is_never_offered() {
        let name = "Conrod-0.2.0-win64-setup.exe";
        let no_sums_file = json!([release("v0.2.0", false, &[name])]);
        assert!(
            choose(&no_sums_file, &v("0.1.0"), false, &mut sums_for(name))
                .unwrap_err()
                .contains("SHA256SUMS")
        );

        let list = json!([release("v0.2.0", false, &[name, SUMS])]);
        // the sums file exists but does not mention the installer
        let err = choose(&list, &v("0.1.0"), false, &mut |_| {
            Ok(format!("{HASH}  something-else.zip\n"))
        })
        .unwrap_err();
        assert!(err.contains("no checksum"), "{err}");
        // a malformed hash is no checksum either
        let err = choose(&list, &v("0.1.0"), false, &mut |_| {
            Ok(format!("nothex  {name}\n"))
        })
        .unwrap_err();
        assert!(err.contains("no checksum"), "{err}");
    }

    #[test]
    fn checksums_read_both_sha256sum_spellings() {
        let text = format!("{HASH}  a.exe\n{} *b.exe\n", HASH.to_uppercase());
        assert_eq!(checksum_for(&text, "a.exe").as_deref(), Some(HASH));
        assert_eq!(checksum_for(&text, "b.exe").as_deref(), Some(HASH));
        assert_eq!(checksum_for(&text, "c.exe"), None);
    }

    #[test]
    fn the_restart_script_waits_for_the_app_quotes_paths_and_always_relaunches() {
        let script = restart_script(
            4242,
            Path::new("C:/tmp/it's here/setup.exe"),
            Path::new("C:/Apps/Conrod.exe"),
            Path::new("C:/tmp/it's here/setup.log"),
        );
        assert!(script.contains("Wait-Process -Id 4242 "), "{script}");
        assert!(
            script.contains(
                "Start-Process -FilePath 'C:/tmp/it''s here/setup.exe' -ArgumentList '/S' -Wait"
            ),
            "{script}"
        );
        assert!(
            script.contains("-FilePath 'C:/tmp/it''s here/setup.log' -Append"),
            "{script}"
        );
        assert!(
            script.ends_with("finally { Start-Process -FilePath 'C:/Apps/Conrod.exe' }"),
            "the app must come back even when the installer failed: {script}"
        );
    }

    #[test]
    fn atom_feed_parses_and_chooses_latest_verified_release() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <title>v1.0.0-beta.4</title>
    <content type="html">&lt;h2&gt;Conrod 1.0.0-beta.4&lt;/h2&gt;&lt;p&gt;Bug fixes &amp; performance&lt;/p&gt;</content>
  </entry>
  <entry>
    <title>v1.0.0-beta.3</title>
    <content type="html">&lt;h2&gt;Conrod 1.0.0-beta.3&lt;/h2&gt;</content>
  </entry>
</feed>"#;
        let entries = parse_atom_entries(xml);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].tag, "v1.0.0-beta.4");
        assert!(entries[0].content.contains("Bug fixes & performance"));

        let installer_name = "Conrod-1.0.0-beta.4-win64-setup.exe";
        let mut mock_sums = sums_for(installer_name);
        let found = choose_from_atom(
            &entries,
            &v("1.0.0-beta.3"),
            true,
            "kapsikkum/conrod",
            &mut |_| true,
            &mut mock_sums,
        )
        .unwrap()
        .unwrap();

        assert_eq!(found.version.to_string(), "1.0.0-beta.4");
        assert_eq!(found.tag, "v1.0.0-beta.4");
        assert_eq!(found.installer.name, installer_name);
        assert_eq!(found.installer.sha256, HASH);
        assert_eq!(
            found.installer.url,
            format!("https://github.com/kapsikkum/conrod/releases/download/v1.0.0-beta.4/{installer_name}")
        );
    }

    #[test]
    fn atom_feed_fails_closed_when_asset_missing() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <title>v1.0.0-beta.4</title>
    <content type="html">notes</content>
  </entry>
</feed>"#;
        let entries = parse_atom_entries(xml);
        let mut mock_sums = sums_for("Conrod-1.0.0-beta.4-win64-setup.exe");
        let err = choose_from_atom(
            &entries,
            &v("1.0.0-beta.3"),
            true,
            "kapsikkum/conrod",
            &mut |_| false,
            &mut mock_sums,
        )
        .unwrap_err();
        assert!(err.contains("not offered"));
    }
}
