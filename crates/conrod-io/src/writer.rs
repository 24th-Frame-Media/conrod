//! Port of `conrod/writer.py`, plus the persistent-process half of
//! `conrod/exif.py` (`ExifTool`) that it needs to run against.
//!
//! RAW files get a sidecar (`.xmp` beside the frame), which is what
//! Lightroom and Bridge expect and leaves the original bytes untouched.
//! JPEGs get the keywords embedded, plus legacy IPTC so Photo Mechanic sees
//! them too.
//!
//! The argument lists exiftool is actually run with are pure functions of
//! their inputs -- no process, no filesystem -- so they can be checked
//! against `tools/gen_writer_fixtures.py`'s recording of the same lists
//! out of `writer.py` without exiftool anywhere in the test.

use std::fmt::Write as FmtWrite;
use std::fs::OpenOptions;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

use crate::settings::Settings;

/// `IMAGE_SUFFIXES`'s JPEG half, from `conrod/config.py`.
pub const JPEG_SUFFIXES: [&str; 2] = ["jpg", "jpeg"];

fn is_jpeg(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| JPEG_SUFFIXES.contains(&e.to_lowercase().as_str()))
}

/// Lightroom's convention is `IMG_1234.xmp`, not `IMG_1234.CR3.xmp`. Port of
/// `sidecar_for`.
pub fn sidecar_for(image: &Path) -> PathBuf {
    image.with_extension("xmp")
}

fn base_args() -> Vec<String> {
    vec![
        "-overwrite_original".into(),
        "-charset".into(),
        "filename=utf8".into(),
    ]
}

/// The keyword remove-then-add pass on `XMP-dc:Subject`,
/// `XMP-lr:HierarchicalSubject`, and (JPEG only) `IPTC:Keywords`. Port of
/// the tag-building half of `write_keywords`.
///
/// Delete each keyword before adding it. exiftool's `+=` appends
/// unconditionally -- `-api nodups` does not suppress that -- so writing a
/// shoot twice would otherwise stack every keyword a second time. Removing
/// first makes the write idempotent while leaving keywords this tool did
/// not add untouched.
pub fn keyword_args(target: &Path, keywords: &[String], is_jpeg: bool) -> Vec<String> {
    let mut tags = vec![
        "XMP-dc:Subject".to_string(),
        "XMP-lr:HierarchicalSubject".to_string(),
    ];
    if is_jpeg {
        // Photo Mechanic and older catalogues still read legacy IPTC.
        tags.push("IPTC:Keywords".to_string());
    }
    let mut args = base_args();
    for tag in &tags {
        args.extend(keywords.iter().map(|kw| format!("-{tag}-={kw}")));
    }
    for tag in &tags {
        args.extend(keywords.iter().map(|kw| format!("-{tag}+={kw}")));
    }
    args.push(path_str(target));
    args
}

/// Port of the rating half of `_write_verdict`.
///
/// `keep_rating` (create-only) skips writing where a rating is already
/// present -- except that a camera writes `Rating=0` meaning *unrated*, so
/// the `-if` condition treats `0` as absent too. That is what every
/// catalogue means by it, and what lets a photographer's own first pass
/// through Lightroom stand rather than being argued with.
pub fn rating_args(target: &Path, rating: i32, is_jpeg: bool, keep_rating: bool) -> Vec<String> {
    let mut args = base_args();
    if keep_rating {
        args.push("-if".into());
        args.push(r#"not $Rating or $Rating eq "0""#.into());
    }
    args.push(format!("-XMP:Rating={rating}"));
    if is_jpeg {
        args.push(format!("-EXIF:Rating={rating}"));
    }
    args.push(path_str(target));
    args
}

/// Port of the label half of `_write_verdict`. `-wm cg` is create-only:
/// exiftool writes the tag when it is absent and leaves it alone otherwise
/// -- a label has no "unset but present" value the way `Rating=0` does, so
/// that flag is the whole of create-only here.
pub fn label_args(target: &Path, label: &str, keep_label: bool) -> Vec<String> {
    let mut args = base_args();
    if keep_label {
        args.push("-wm".into());
        args.push("cg".into());
    }
    args.push(format!("-XMP:Label={label}"));
    args.push(path_str(target));
    args
}

/// Port of `_write_caption`.
///
/// A caption is a single value, so writing one replaces whatever the
/// photographer put there -- unlike keywords, which merge. `-wm cg`
/// (create-only) is therefore the default; only `overwrite_caption` turns
/// it off.
pub fn caption_args(
    target: &Path,
    caption: &str,
    is_jpeg: bool,
    overwrite_caption: bool,
) -> Vec<String> {
    let mut args = base_args();
    if !overwrite_caption {
        args.push("-wm".into());
        args.push("cg".into());
    }
    args.push(format!("-XMP-dc:Description={caption}"));
    if is_jpeg {
        args.push(format!("-IPTC:Caption-Abstract={caption}"));
    }
    args.push(path_str(target));
    args
}

/// Seed a new sidecar from the RAW file's own XMP block. Port of the
/// exiftool-invocation half of `_create_sidecar`.
pub fn create_sidecar_args(image: &Path, sidecar: &Path) -> Vec<String> {
    vec![
        "-charset".into(),
        "filename=utf8".into(),
        "-o".into(),
        path_str(sidecar),
        "-XMP:all".into(),
        path_str(image),
    ]
}

/// A hand-made empty XMP packet, written when a RAW carries no XMP block of
/// its own for `-o` to seed a sidecar from. Byte-for-byte `writer._EMPTY_XMP`.
pub const EMPTY_XMP: &str = "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n  <rdf:Description rdf:about=\"\"/>\n </rdf:RDF>\n</x:xmpmeta>\n<?xpacket end=\"w\"?>\n";

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// `[k for k in dict.fromkeys(keywords) if k]`: first-seen order, blanks
/// dropped, duplicates collapsed.
fn dedup_nonempty(keywords: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    keywords
        .iter()
        .filter(|k| !k.is_empty() && seen.insert(k.as_str()))
        .cloned()
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub struct WriteResult {
    pub path: PathBuf,
    pub target: PathBuf,
    pub keywords: Vec<String>,
    pub ok: bool,
    pub message: String,
}

/// A persistent exiftool process (`-stay_open True -@ -`). Port of
/// `exif.ExifTool`, minus the batch tag-reading helpers -- this crate's
/// writer only needs `execute`.
///
/// Spawning exiftool per file costs ~300ms on Windows because it is Perl,
/// which would dominate a run over a whole shoot; `-stay_open` amortises
/// that one process over every write.
pub struct ExifTool {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
}

const SENTINEL: &str = "{ready}";

impl ExifTool {
    pub fn spawn(executable: &str) -> io::Result<Self> {
        let mut cmd = Command::new(executable);
        cmd.args(["-stay_open", "True", "-@", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        // A windowed build has no console, so each exiftool spawn would
        // otherwise flash a black window.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take().expect("stdin was piped");
        let stdout = BufReader::new(child.stdout.take().expect("stdout was piped"));
        Ok(ExifTool {
            child,
            stdin,
            stdout,
        })
    }

    pub fn execute(&mut self, args: &[String]) -> io::Result<String> {
        for arg in args {
            writeln!(self.stdin, "{arg}")?;
        }
        writeln!(self.stdin, "-execute")?;
        self.stdin.flush()?;
        let mut out = String::new();
        loop {
            let mut line = String::new();
            let n = self.stdout.read_line(&mut line)?;
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "exiftool exited unexpectedly",
                ));
            }
            if line.trim_start().starts_with(SENTINEL) {
                break;
            }
            out.push_str(&line);
        }
        Ok(out)
    }
}

impl Drop for ExifTool {
    fn drop(&mut self) {
        let _ = writeln!(self.stdin, "-stay_open");
        let _ = writeln!(self.stdin, "False");
        let _ = self.stdin.flush();
        // Give exiftool a chance to exit on its own before killing it, the
        // way `proc.wait(timeout=10)` does in the Python.
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(20))
                }
                _ => {
                    let _ = self.child.kill();
                    break;
                }
            }
        }
    }
}

/// High-level writer that starts ExifTool only when a merge or embedded
/// write needs it. New RAW sidecars use the atomic Rust path and therefore
/// do not pay ExifTool's process startup cost.
pub struct MetadataWriter {
    executable: String,
    tool: Option<ExifTool>,
}

impl MetadataWriter {
    /// Use the supplied ExifTool executable, usually a bundled absolute path.
    pub fn new(executable: impl Into<String>) -> Self {
        Self {
            executable: executable.into(),
            tool: None,
        }
    }

    /// Use `exiftool` resolved through the process PATH when a merge is first
    /// needed.
    pub fn from_path() -> Self {
        Self::new("exiftool")
    }

    pub fn write_keywords(
        &mut self,
        image: &Path,
        keywords: &[String],
        settings: &Settings,
        caption: Option<&str>,
        rating: Option<i32>,
        label: Option<&str>,
    ) -> io::Result<WriteResult> {
        let keywords = dedup_nonempty(keywords);
        let caption = caption.filter(|caption| !caption.is_empty());
        if keywords.is_empty() && rating.is_none() && label.is_none() {
            return Ok(WriteResult {
                path: image.to_path_buf(),
                target: image.to_path_buf(),
                keywords: vec![],
                ok: true,
                message: "no keywords".into(),
            });
        }

        if !is_jpeg(image) && settings.write_sidecar_for_raw {
            let sidecar = sidecar_for(image);
            if !sidecar.exists()
                && create_sidecar_fast(&sidecar, &keywords, caption, rating, label, settings)?
            {
                return Ok(WriteResult {
                    path: image.to_path_buf(),
                    target: sidecar,
                    keywords,
                    ok: true,
                    message: "created XMP sidecar".into(),
                });
            }
        }

        let executable = self.executable.clone();
        let tool = match self.tool.as_mut() {
            Some(tool) => tool,
            None => {
                self.tool = Some(ExifTool::spawn(&executable)?);
                self.tool.as_mut().expect("ExifTool was just inserted")
            }
        };
        write_keywords(tool, image, &keywords, settings, caption, rating, label)
    }
}

fn wrote_one(output: &str) -> bool {
    // ExifTool's summary is the only reliable result on the stay-open pipe;
    // warnings may precede it and the wording is plural even for one file.
    // Parse the count instead of depending on one exact capitalization or
    // on the old two literal strings.
    output.lines().any(|line| {
        let mut words = line.split_whitespace();
        let Some(count) = words.next().and_then(|word| word.parse::<u64>().ok()) else {
            return false;
        };
        let rest: Vec<_> = words.map(|word| word.to_ascii_lowercase()).collect();
        count > 0
            && rest
                .iter()
                .any(|word| word == "image" || word == "output" || word == "files")
            && rest
                .iter()
                .any(|word| word == "updated" || word == "created")
    })
}

/// Write one frame's keywords, and how good the frame is. Port of
/// `write_keywords`.
///
/// `rating` and `label` carry the cull's verdict into the catalogue, where
/// it can actually be acted on: stars to sort by and a colour to filter on,
/// rather than a number in a database only Conrod can read.
pub fn write_keywords(
    tool: &mut ExifTool,
    image: &Path,
    keywords: &[String],
    settings: &Settings,
    caption: Option<&str>,
    rating: Option<i32>,
    label: Option<&str>,
) -> io::Result<WriteResult> {
    let keywords = dedup_nonempty(keywords);
    let caption = caption.filter(|caption| !caption.is_empty());
    if keywords.is_empty() && rating.is_none() && label.is_none() {
        return Ok(WriteResult {
            path: image.to_path_buf(),
            target: image.to_path_buf(),
            keywords: vec![],
            ok: true,
            message: "no keywords".into(),
        });
    }

    let jpeg = is_jpeg(image);
    let target = if jpeg || !settings.write_sidecar_for_raw {
        image.to_path_buf()
    } else {
        let sidecar = sidecar_for(image);
        if !sidecar.exists() {
            match create_sidecar_fast(&sidecar, &keywords, caption, rating, label, settings) {
                Ok(true) => {
                    return Ok(WriteResult {
                        path: image.to_path_buf(),
                        target: sidecar,
                        keywords,
                        ok: true,
                        message: "created XMP sidecar".into(),
                    });
                }
                Ok(false) => {}
                Err(error) => return Err(error),
            }
        }
        sidecar
    };

    if keywords.is_empty() {
        // Nothing to keyword, but there is still a verdict to record: a
        // frame the cull dropped has no vehicle worth naming and is
        // exactly the one that needs to arrive in the catalogue marked
        // red. Running the keyword command with no keywords in it reported
        // failure for every such frame while the label was in fact written.
        let ok = write_verdict(tool, &target, rating, label, jpeg, settings)?;
        return Ok(WriteResult {
            path: image.to_path_buf(),
            target,
            keywords: vec![],
            ok,
            message: "rating only".into(),
        });
    }

    let output = tool.execute(&keyword_args(&target, &keywords, jpeg))?;
    let ok = wrote_one(&output);

    if let Some(caption) = caption {
        write_caption(tool, &target, caption, jpeg, settings)?;
    }
    if rating.is_some() || label.is_some() {
        write_verdict(tool, &target, rating, label, jpeg, settings)?;
    }
    Ok(WriteResult {
        path: image.to_path_buf(),
        target,
        keywords,
        ok,
        message: output.trim().to_string(),
    })
}

/// Put the cull's judgement where a photographer already looks for it.
/// Port of `_write_verdict`.
fn write_verdict(
    tool: &mut ExifTool,
    target: &Path,
    rating: Option<i32>,
    label: Option<&str>,
    is_jpeg: bool,
    settings: &Settings,
) -> io::Result<bool> {
    let keep_rating = !settings.overwrite_rating;
    let keep_label = !settings.overwrite_label;
    let mut wrote = false;

    if let Some(rating) = rating {
        if settings.write_rating {
            let out = tool.execute(&rating_args(target, rating, is_jpeg, keep_rating))?;
            wrote = wrote || wrote_one(&out);
        }
    }
    if let Some(label) = label {
        if settings.write_label {
            let out = tool.execute(&label_args(target, label, keep_label))?;
            wrote = wrote || wrote_one(&out);
        }
    }
    Ok(wrote)
}

/// Fill in a caption without destroying one the photographer wrote. Port of
/// `_write_caption`.
fn write_caption(
    tool: &mut ExifTool,
    target: &Path,
    caption: &str,
    is_jpeg: bool,
    settings: &Settings,
) -> io::Result<()> {
    tool.execute(&caption_args(
        target,
        caption,
        is_jpeg,
        settings.overwrite_caption,
    ))?;
    Ok(())
}

/// Create a new RAW sidecar without starting a metadata parser. `create_new`
/// makes the check and creation one filesystem operation: if another writer
/// wins the race, the caller falls through to ExifTool's merge path and this
/// function never truncates that existing sidecar.
fn create_sidecar_fast(
    sidecar: &Path,
    keywords: &[String],
    caption: Option<&str>,
    rating: Option<i32>,
    label: Option<&str>,
    settings: &Settings,
) -> io::Result<bool> {
    let mut file = match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(sidecar)
    {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Ok(false),
        Err(error) => return Err(error),
    };
    let packet = xmp_packet(
        keywords,
        caption,
        rating.filter(|_| settings.write_rating),
        label.filter(|_| settings.write_label),
    );
    if let Err(error) = file.write_all(packet.as_bytes()).and_then(|_| file.flush()) {
        let _ = std::fs::remove_file(sidecar);
        return Err(error);
    }
    Ok(true)
}

fn xml_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            // XML 1.0 cannot carry these control characters. ExifTool would
            // reject them too, so omit them rather than creating broken XMP.
            '\u{0}'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}' => {}
            _ => escaped.push(character),
        }
    }
    escaped
}

fn xmp_packet(
    keywords: &[String],
    caption: Option<&str>,
    rating: Option<i32>,
    label: Option<&str>,
) -> String {
    let mut packet =
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n"
            .to_string()
            + "<x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n"
            + " <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n"
            + "  <rdf:Description rdf:about=\"\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:lr=\"http://ns.adobe.com/lightroom/1.0/\" xmlns:xmp=\"http://ns.adobe.com/xap/1.0/\">\n";
    if !keywords.is_empty() {
        packet.push_str("   <dc:subject><rdf:Bag>\n");
        for keyword in keywords {
            let _ = writeln!(packet, "    <rdf:li>{}</rdf:li>", xml_escape(keyword));
        }
        packet.push_str("   </rdf:Bag></dc:subject>\n   <lr:hierarchicalSubject><rdf:Bag>\n");
        for keyword in keywords {
            let _ = writeln!(packet, "    <rdf:li>{}</rdf:li>", xml_escape(keyword));
        }
        packet.push_str("   </rdf:Bag></lr:hierarchicalSubject>\n");
    }
    if let Some(caption) = caption {
        packet.push_str("   <dc:description><rdf:Alt><rdf:li xml:lang=\"x-default\">");
        packet.push_str(&xml_escape(caption));
        packet.push_str("</rdf:li></rdf:Alt></dc:description>\n");
    }
    if let Some(rating) = rating {
        let _ = writeln!(packet, "   <xmp:Rating>{rating}</xmp:Rating>");
    }
    if let Some(label) = label {
        let _ = writeln!(packet, "   <xmp:Label>{}</xmp:Label>", xml_escape(label));
    }
    packet.push_str("  </rdf:Description>\n </rdf:RDF>\n</x:xmpmeta>\n<?xpacket end=\"w\"?>\n");
    packet
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exiftool_summary_accepts_success_variants() {
        assert!(wrote_one("Warning: harmless\n1 image files updated\n"));
        assert!(wrote_one("1 output files created\n"));
        assert!(wrote_one("1 IMAGE FILES UPDATED\n"));
        assert!(!wrote_one("0 image files updated\n"));
        assert!(!wrote_one("Error: nothing written\n"));
    }

    #[test]
    fn new_sidecar_is_atomic_and_escapes_xml() {
        let root = std::env::temp_dir().join(format!("conrod-io-writer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let sidecar = root.join("frame.xmp");
        let settings = Settings {
            write_rating: true,
            write_label: true,
            ..Settings::default()
        };
        let keywords = vec!["A&B <team>".to_string()];
        assert!(create_sidecar_fast(
            &sidecar,
            &keywords,
            Some("Caption \"quoted\""),
            Some(4),
            Some("Red & blue"),
            &settings,
        )
        .unwrap());
        let first = std::fs::read_to_string(&sidecar).unwrap();
        assert!(first.contains("A&amp;B &lt;team&gt;"));
        assert!(first.contains("Caption &quot;quoted&quot;"));
        assert!(first.contains("Red &amp; blue"));
        assert!(first.contains("<xmp:Rating>4</xmp:Rating>"));
        assert!(first.contains("<xmp:Label>Red &amp; blue</xmp:Label>"));
        assert!(!first.contains("<xmp:rating>"));
        assert!(!first.contains("<xmp:label>"));
        assert!(!create_sidecar_fast(
            &sidecar,
            &["replacement".to_string()],
            None,
            None,
            None,
            &settings,
        )
        .unwrap());
        assert_eq!(std::fs::read_to_string(&sidecar).unwrap(), first);
        let sidecar_arg = sidecar.to_string_lossy();
        if let Ok(output) = std::process::Command::new("exiftool")
            .args([
                "-json",
                "-XMP-dc:Subject",
                "-XMP-dc:Description",
                "-XMP:Label",
                sidecar_arg.as_ref(),
            ])
            .output()
        {
            assert!(output.status.success());
            let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(metadata[0]["Subject"], "A&B <team>");
            assert_eq!(metadata[0]["Description"], "Caption \"quoted\"");
            assert_eq!(metadata[0]["Label"], "Red & blue");
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn metadata_writer_keeps_exiftool_lazy_for_new_sidecars() {
        let root = std::env::temp_dir().join(format!("conrod-io-lazy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let image = root.join("frame.CR3");
        let mut writer = MetadataWriter::new("exiftool-that-does-not-exist");
        let result = writer
            .write_keywords(
                &image,
                &["vehicle".to_string()],
                &Settings::default(),
                None,
                None,
                None,
            )
            .unwrap();
        assert!(result.ok);
        assert!(sidecar_for(&image).exists());
        let _ = std::fs::remove_dir_all(root);
    }
}
