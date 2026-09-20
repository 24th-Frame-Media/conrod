//! Exercised against the photographer's own `conrod.db`, when one exists.
//! Local-only: skips (not fails) when there is no real database, which is
//! true for CI and for anyone else's checkout.
//!
//! The live file is never opened read-write: it's opened SQLITE_OPEN_READONLY
//! and `VACUUM INTO` copies a consistent snapshot (WAL content included) to a
//! throwaway temp file, which is what the rest of the test then drives
//! through the normal, read-write `connect()`.

use conrod_store::{list_jobs, pending_images, unread_detections};
use rusqlite::{Connection, OpenFlags};

#[test]
fn real_database_reads_cleanly() {
    let real = conrod_store::db_path();
    if !real.exists() {
        eprintln!("no real database at {}; skipping", real.display());
        return;
    }

    let dir = std::env::temp_dir().join(format!("conrod-store-real-db-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let copy = dir.join("conrod.db");
    if copy.exists() {
        std::fs::remove_file(&copy).unwrap();
    }

    {
        let readonly = Connection::open_with_flags(&real, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("open the live database read-only");
        readonly
            .execute("VACUUM INTO ?", [copy.to_str().unwrap()])
            .expect("VACUUM INTO the temp copy");
    }

    let conn = conrod_store::connect(Some(&copy)).expect("open the copy read-write");

    let jobs = list_jobs(&conn).unwrap();
    let mut image_total = 0usize;
    let mut detection_total = 0usize;
    for job in &jobs {
        // Every status a real shoot's images can be in, not just "pending".
        for status in ["pending", "detected", "written", "error"] {
            image_total += pending_images(&conn, job.id, status).unwrap().len();
        }
        detection_total += unread_detections(&conn, job.id).unwrap().len();
    }

    // A generic scan covers every detection row, not just the unread ones
    // `unread_detections` returns -- through the same row-mapping the library
    // functions use.
    let mut all_detections = 0usize;
    {
        let mut stmt = conn.prepare("SELECT * FROM detections").unwrap();
        let mut rows = stmt.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            conrod_store::detection_from_row(row).unwrap();
            all_detections += 1;
        }
    }
    let mut all_images = 0usize;
    {
        let mut stmt = conn.prepare("SELECT * FROM images").unwrap();
        let mut rows = stmt.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            conrod_store::image_from_row(row).unwrap();
            all_images += 1;
        }
    }

    eprintln!(
        "{} jobs, {all_images} images read ({image_total} matched a known status), \
         {all_detections} detections read ({detection_total} unread)",
        jobs.len()
    );
    assert!(all_images >= image_total);
    assert!(all_detections >= detection_total);
}
