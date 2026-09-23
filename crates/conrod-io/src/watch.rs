//! Watching a folder for frames that arrive after a scan started.
//!
//! The workflow: a card goes into the reader and the copy runs for twenty minutes
//! while the shoot is still being packed up. Nothing here scans anything. It
//! answers one question: which files in the folder are new *and finished being
//! written*. A 60 MB CR3 mid-copy exists, has a name, matches the extension filter
//! and is incomplete; opening it gets a truncated file. So a file is only offered
//! once its size and modification time have held still for [`SETTLE`].
//!
//! File-system events cannot say "finished writing", so this polls; the interval
//! is the caller's.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// How long a file must hold still: past the stalls of a card copy, short enough
/// to keep up with one.
pub const SETTLE: Duration = Duration::from_secs(20);
/// The shortest interval a watch will poll at.
pub const MIN_INTERVAL: Duration = Duration::from_secs(10);
/// The frames a scan reads (the engine's `EXTENSIONS`).
const EXTENSIONS: [&str; 4] = ["cr3", "cr2", "jpg", "jpeg"];

/// One spelling of a path, for comparing against what an album holds: Windows
/// hands the same file back as `D:\Work` and `d:/work`.
pub fn key(path: &Path) -> String {
    path.to_string_lossy().replace('/', "\\").to_lowercase()
}

/// What a file looked like the last time the folder was read.
#[derive(Debug, Clone, PartialEq)]
struct Sighting {
    size: u64,
    modified: Option<SystemTime>,
    /// When it last changed (or first appeared), on the caller's clock.
    since: Duration,
}

#[derive(Debug)]
pub struct Watcher {
    pub folder: PathBuf,
    pub recursive: bool,
    settle: Duration,
    seen: HashMap<String, Sighting>,
}

impl Watcher {
    pub fn new(folder: impl Into<PathBuf>, recursive: bool) -> Watcher {
        Watcher {
            folder: folder.into(),
            recursive,
            settle: SETTLE,
            seen: HashMap::new(),
        }
    }

    /// The frames in the folder now, with the size and time the file system reports.
    fn list(&self) -> Vec<(PathBuf, u64, Option<SystemTime>)> {
        fn walk(dir: &Path, recursive: bool, out: &mut Vec<(PathBuf, u64, Option<SystemTime>)>) {
            // A folder that went away (an unplugged reader, a dropped share) is not
            // worth stopping a watch for: the next pass finds it again.
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if kind.is_dir() {
                    if recursive {
                        walk(&path, recursive, out);
                    }
                } else if path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
                {
                    if let Ok(meta) = entry.metadata() {
                        out.push((path, meta.len(), meta.modified().ok()));
                    }
                }
            }
        }
        let mut out = Vec::new();
        walk(&self.folder, self.recursive, &mut out);
        out
    }

    /// New, finished files. `known` is what the album already holds (as [`key`]s),
    /// so restarting does not offer the whole folder again. `now` is any monotonic
    /// clock the caller keeps (time since it started, say).
    pub fn poll(&mut self, known: &HashSet<String>, now: Duration) -> Vec<PathBuf> {
        let files = self.list();
        self.poll_files(files, known, now)
    }

    fn poll_files(
        &mut self,
        files: Vec<(PathBuf, u64, Option<SystemTime>)>,
        known: &HashSet<String>,
        now: Duration,
    ) -> Vec<PathBuf> {
        let mut ready = Vec::new();
        let mut present = HashSet::new();
        for (path, size, modified) in files {
            let k = key(&path);
            present.insert(k.clone());
            match self.seen.get_mut(&k) {
                // First sighting is never enough: even a finished file has to survive
                // one interval, which costs a pass and buys the guarantee.
                None => {
                    self.seen.insert(
                        k,
                        Sighting {
                            size,
                            modified,
                            since: now,
                        },
                    );
                }
                Some(s) if s.size != size || s.modified != modified => {
                    *s = Sighting {
                        size,
                        modified,
                        since: now,
                    }; // the clock restarts on every write
                }
                Some(s) if now.saturating_sub(s.since) < self.settle => {}
                Some(_) if known.contains(&k) => {}
                Some(_) => ready.push(path),
            }
        }
        // Forget files that have gone, so a watch left on a working folder all season
        // does not accumulate a record of every frame ever moved out of it. One that
        // comes back has to settle again, the right answer for a file being rewritten.
        self.seen.retain(|k, _| present.contains(k));
        ready.sort();
        ready
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(name: &str, size: u64) -> (PathBuf, u64, Option<SystemTime>) {
        (
            PathBuf::from(name),
            size,
            Some(SystemTime::UNIX_EPOCH + Duration::from_secs(size)),
        )
    }
    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    #[test]
    fn a_file_is_offered_only_after_it_has_held_still_for_the_settle_time() {
        let mut w = Watcher::new("D:/shoot", true);
        let none = HashSet::new();
        assert!(
            w.poll_files(vec![file("D:/shoot/a.cr3", 10)], &none, secs(0))
                .is_empty(),
            "first sighting"
        );
        assert!(
            w.poll_files(vec![file("D:/shoot/a.cr3", 10)], &none, secs(19))
                .is_empty(),
            "not settled yet"
        );
        assert_eq!(
            w.poll_files(vec![file("D:/shoot/a.cr3", 10)], &none, secs(20)),
            vec![PathBuf::from("D:/shoot/a.cr3")]
        );
    }

    #[test]
    fn a_file_still_being_written_never_settles() {
        let mut w = Watcher::new("D:/shoot", true);
        let none = HashSet::new();
        for (t, size) in [(0, 10), (15, 20), (30, 30), (45, 40)] {
            assert!(
                w.poll_files(vec![file("D:/shoot/b.cr3", size)], &none, secs(t))
                    .is_empty(),
                "t={t}"
            );
        }
        // it stops growing: settled 20 s after the last change
        assert!(w
            .poll_files(vec![file("D:/shoot/b.cr3", 40)], &none, secs(60))
            .is_empty());
        assert_eq!(
            w.poll_files(vec![file("D:/shoot/b.cr3", 40)], &none, secs(65))
                .len(),
            1
        );
    }

    #[test]
    fn what_the_album_holds_is_not_offered_and_spelling_does_not_matter() {
        let mut w = Watcher::new("D:/shoot", true);
        let known: HashSet<String> = [key(Path::new("d:\\SHOOT\\a.cr3"))].into();
        w.poll_files(
            vec![file("D:/shoot/a.cr3", 1), file("D:/shoot/c.cr3", 1)],
            &known,
            secs(0),
        );
        let ready = w.poll_files(
            vec![file("D:/shoot/a.cr3", 1), file("D:/shoot/c.cr3", 1)],
            &known,
            secs(30),
        );
        assert_eq!(ready, vec![PathBuf::from("D:/shoot/c.cr3")]);
    }

    #[test]
    fn a_file_that_goes_and_comes_back_has_to_settle_again() {
        let mut w = Watcher::new("D:/shoot", true);
        let none = HashSet::new();
        w.poll_files(vec![file("D:/shoot/d.cr3", 5)], &none, secs(0));
        w.poll_files(vec![], &none, secs(30)); // gone: forgotten
        assert!(w
            .poll_files(vec![file("D:/shoot/d.cr3", 5)], &none, secs(31))
            .is_empty());
        assert!(w
            .poll_files(vec![file("D:/shoot/d.cr3", 5)], &none, secs(40))
            .is_empty());
        assert_eq!(
            w.poll_files(vec![file("D:/shoot/d.cr3", 5)], &none, secs(52))
                .len(),
            1
        );
    }

    #[test]
    fn listing_a_real_folder_finds_frames_and_ignores_the_rest() {
        let root = std::env::temp_dir().join(format!("conrod-watch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("sub")).unwrap();
        for name in ["a.CR3", "b.jpg", "c.xmp", "sub/d.cr2", "sub/e.txt"] {
            std::fs::write(root.join(name), b"x").unwrap();
        }
        let names = |recursive| {
            let mut n: Vec<String> = Watcher::new(&root, recursive)
                .list()
                .iter()
                .map(|(p, _, _)| p.file_name().unwrap().to_string_lossy().into_owned())
                .collect();
            n.sort();
            n
        };
        assert_eq!(names(true), ["a.CR3", "b.jpg", "d.cr2"]);
        assert_eq!(names(false), ["a.CR3", "b.jpg"]);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
