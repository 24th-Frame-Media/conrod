//! Round-trip coverage for the setters, plus proof that a database this
//! crate creates or touches stays fully usable by the Python app: the two
//! python-backed tests shell out to the photographer's own venv and skip
//! (rather than fail) when it is not present, since it is a dev-machine path.

use conrod_core::bursts::Frame;
use conrod_store::{
    add_detection, add_images, add_sharpness_label, clear_groups, create_job, cull_detection,
    delete_detection, get_detection, latest_job, list_detections, list_jobs, next_to_rate,
    pending_images, set_analysis, set_detection_group, set_detection_measurement,
    set_existing_marks, set_frame_origin, set_image_quality, set_image_result, set_job_status,
    set_number, set_quality, sharpness_label_counts, sharpness_labels, undo_sharpness_label,
    unread_detections, update_detection_review, update_image_review, Analysis,
};
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

const PYTHON: &str = "C:/Users/kapsikkum/.trackaction/venv/Scripts/python.exe";

fn python_available() -> bool {
    Path::new(PYTHON).exists()
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

/// A path under the system temp dir that no other test or process is using.
fn temp_db_path(label: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("conrod-store-test-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("{label}.db"))
}

fn fresh_conn(label: &str) -> Connection {
    conrod_store::connect(Some(&temp_db_path(label))).unwrap()
}

// --- pure Rust round trips (no Python needed) --------------------------------

#[test]
fn round_trip_job_and_images() {
    let conn = fresh_conn("job_and_images");
    let settings = serde_json::json!({"detect_conf": 0.25});
    let job_id = create_job(&conn, Path::new("C:/album"), None, &settings).unwrap();

    let job = latest_job(&conn).unwrap().unwrap();
    assert_eq!(job.id, job_id);
    // No label given -- defaults to the root folder's name, like store.py.
    assert_eq!(job.label.as_deref(), Some("album"));
    assert_eq!(job.status, "scanning");
    assert_eq!(job.settings_json, Some(settings));

    set_job_status(&conn, job_id, "done").unwrap();
    assert_eq!(latest_job(&conn).unwrap().unwrap().status, "done");

    add_images(
        &conn,
        job_id,
        &[
            PathBuf::from("C:/album/a.jpg"),
            PathBuf::from("C:/album/b.jpg"),
        ],
    )
    .unwrap();
    // INSERT OR IGNORE: a repeat of an existing (job_id, path) is a no-op.
    add_images(&conn, job_id, &[PathBuf::from("C:/album/a.jpg")]).unwrap();
    let pending = pending_images(&conn, job_id, "pending").unwrap();
    assert_eq!(pending.len(), 2);

    set_image_result(
        &conn,
        pending[0].id,
        "detected",
        Some("C:/album/preview/a.jpg"),
        Some(100),
        Some(80),
        None,
    )
    .unwrap();
    assert_eq!(pending_images(&conn, job_id, "pending").unwrap().len(), 1);
    assert_eq!(pending_images(&conn, job_id, "detected").unwrap().len(), 1);

    let summaries = list_jobs(&conn).unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].image_count, 2);
    assert_eq!(summaries[0].unfinished_count, 1);
}

#[test]
fn round_trip_detection_lifecycle() {
    let conn = fresh_conn("detection_lifecycle");
    let job_id = create_job(
        &conn,
        Path::new("C:/album"),
        Some("Album"),
        &serde_json::json!({}),
    )
    .unwrap();
    add_images(&conn, job_id, &[PathBuf::from("C:/album/a.jpg")]).unwrap();
    let image_id = pending_images(&conn, job_id, "pending").unwrap()[0].id;

    let det_id = add_detection(
        &conn,
        image_id,
        [1.0, 2.0, 3.0, 4.0],
        "car",
        0.9,
        "C:/album/crops/1.jpg",
    )
    .unwrap();
    set_number(&conn, det_id, Some("88"), "manual", 1.0).unwrap();

    let analysis = Analysis {
        race_number: Some("88".into()),
        number_source: Some("manual".into()),
        number_conf: Some(1.0),
        plate: Some("ABC123".into()),
        plate_state: Some("NSW".into()),
        plate_conf: Some(0.7),
        attributes: serde_json::json!({"make": "Holden", "model": "Commodore"}),
    };
    set_analysis(
        &conn,
        det_id,
        &analysis,
        Some("#ff0000"),
        Some(0.8),
        Some("sharp"),
    )
    .unwrap();
    set_quality(
        &conn, det_id, 0.8, "sharp", 0, 4.5, "good", false, "even", -1.0, false,
    )
    .unwrap();

    // number_source is set, so this detection has left the unread queue.
    assert!(unread_detections(&conn, job_id).unwrap().is_empty());

    let row = one_detection(&conn, det_id);
    assert_eq!(row.number.as_deref(), Some("88"));
    assert_eq!(row.plate.as_deref(), Some("ABC123"));
    assert_eq!(row.colour_hex.as_deref(), Some("#ff0000"));
    assert_eq!(row.attributes, Some(analysis.attributes.clone()));
    assert_eq!(row.rating_verdict.as_deref(), Some("good"));
    assert!(!row.rejected);

    cull_detection(&conn, det_id, "clipped", true).unwrap();
    let row = one_detection(&conn, det_id);
    assert!(row.rejected);
    assert_eq!(row.cull_reason.as_deref(), Some("clipped"));
    assert_eq!(row.uncertain, Some(1));

    conrod_store::set_embedding(&conn, det_id, "[0.1,0.2,0.3]").unwrap();
    assert_eq!(
        one_detection(&conn, det_id).embedding.as_deref(),
        Some("[0.1,0.2,0.3]")
    );
}

fn one_detection(conn: &Connection, det_id: i64) -> conrod_store::Detection {
    conn.query_row(
        "SELECT * FROM detections WHERE id=?",
        [det_id],
        conrod_store::detection_from_row,
    )
    .unwrap()
}

#[test]
fn frame_origin_and_marks_match_on_normalised_path() {
    let conn = fresh_conn("path_normalisation");
    let job_id = create_job(&conn, Path::new("C:/album"), None, &serde_json::json!({})).unwrap();
    // Stored with backslashes and mixed case, the way Windows hands them back.
    add_images(&conn, job_id, &[PathBuf::from(r"C:\album\A.JPG")]).unwrap();

    // exiftool and a plain folder walk both hand back forward slashes.
    let frame = Frame {
        path: "C:/album/a.jpg".to_string(),
        camera: "Canon-123".to_string(),
        taken: Some(1000.0),
        burst: 3,
    };
    let updated = set_frame_origin(&conn, job_id, std::slice::from_ref(&frame)).unwrap();
    assert_eq!(updated, 1);
    let img = pending_images(&conn, job_id, "pending").unwrap().remove(0);
    assert_eq!(img.camera.as_deref(), Some("Canon-123"));
    assert_eq!(img.burst_key, Some(3));
    assert_eq!(img.taken_at, Some(1000.0));

    let mut marks = HashMap::new();
    marks.insert(
        "c:/album/a.jpg".to_string(),
        (Some(4), Some(" Green ".to_string())),
    );
    // Not a real image in this job -- ignored, not an error.
    marks.insert("c:/album/missing.jpg".to_string(), (Some(2), None));
    let with_rating = set_existing_marks(&conn, job_id, &marks).unwrap();
    assert_eq!(with_rating, 1);
    let img = pending_images(&conn, job_id, "pending").unwrap().remove(0);
    assert_eq!(img.rating_in_file, Some(4));
    assert_eq!(img.label_in_file.as_deref(), Some("Green"));

    // A rating of 0 is "not rated", not a real one star -- must not stick.
    let mut zero = HashMap::new();
    zero.insert("c:/album/a.jpg".to_string(), (Some(0), None));
    assert_eq!(set_existing_marks(&conn, job_id, &zero).unwrap(), 0);
}

#[test]
fn round_trip_sharpness_labels() {
    let conn = fresh_conn("sharpness_labels");
    let job_id = create_job(&conn, Path::new("C:/album"), None, &serde_json::json!({})).unwrap();
    add_images(&conn, job_id, &[PathBuf::from("C:/album/a.jpg")]).unwrap();
    let image_id = pending_images(&conn, job_id, "pending").unwrap()[0].id;
    let det_id = add_detection(
        &conn,
        image_id,
        [1.0, 2.0, 3.0, 4.0],
        "car",
        0.9,
        "C:/album/crops/1.jpg",
    )
    .unwrap();
    set_quality(
        &conn, det_id, 0.5, "soft", 0, 2.0, "fair", false, "even", -1.0, false,
    )
    .unwrap();

    let candidate = next_to_rate(&conn, 0, 0.0, 1.0, Some(job_id))
        .unwrap()
        .unwrap();
    assert_eq!(candidate.id, det_id);
    assert_eq!(candidate.frame, "C:/album/a.jpg");
    // A job that never ran this detection is not offered it.
    assert!(next_to_rate(&conn, 0, 0.0, 1.0, Some(job_id + 1))
        .unwrap()
        .is_none());

    add_sharpness_label(
        &conn,
        "C:/album/a.jpg",
        [1.0, 2.0, 3.0, 4.0],
        3,
        false,
        false,
        Some(&[0.1, 0.2]),
        1,
    )
    .unwrap();
    // Now labelled, so it drops out of the queue.
    assert!(next_to_rate(&conn, 0, 0.0, 1.0, Some(job_id))
        .unwrap()
        .is_none());

    let labels = sharpness_labels(&conn, 1).unwrap();
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0].stars, 3);
    assert_eq!(labels[0].features, Some(serde_json::json!([0.1, 0.2])));
    assert!(sharpness_labels(&conn, 2).unwrap().is_empty()); // wrong feature_version

    let counts = sharpness_label_counts(&conn).unwrap();
    assert_eq!(counts.get(&3), Some(&1));

    undo_sharpness_label(&conn).unwrap();
    assert!(sharpness_labels(&conn, 1).unwrap().is_empty());
}

#[test]
fn needs_review_sql_function_matches_python_logic() {
    let conn = fresh_conn("needs_review");
    let ask = |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };

    assert_eq!(
        ask("SELECT _needs_review(0.9, 1, 0, 0, 0.8)"),
        0,
        "reviewed wins"
    );
    assert_eq!(
        ask("SELECT _needs_review(0.9, 0, 1, 0, 0.8)"),
        0,
        "rejected, not uncertain"
    );
    assert_eq!(
        ask("SELECT _needs_review(0.9, 0, 1, 1, 0.8)"),
        1,
        "rejected but uncertain"
    );
    assert_eq!(
        ask("SELECT _needs_review(0.5, 0, 0, 0, 0.8)"),
        1,
        "below threshold"
    );
    assert_eq!(
        ask("SELECT _needs_review(NULL, 0, 0, 0, 0.8)"),
        1,
        "no number at all"
    );
    assert_eq!(
        ask("SELECT _needs_review(0.85, 0, 0, 0, NULL)"),
        0,
        "threshold NULL -> 0.8 default"
    );
}

#[test]
fn review_group_and_frame_fields_round_trip() {
    let conn = fresh_conn("review_fields");
    let job_id = create_job(&conn, Path::new("C:/album"), None, &serde_json::json!({})).unwrap();
    add_images(&conn, job_id, &[PathBuf::from("C:/album/a.jpg")]).unwrap();
    let image = pending_images(&conn, job_id, "pending").unwrap().remove(0);
    set_image_quality(
        &conn,
        image.id,
        Some(0.3),
        Some(1.5),
        Some("soft"),
        Some(2),
        Some(true),
    )
    .unwrap();
    update_image_review(&conn, image.id, Some(4), Some(false)).unwrap();
    let image = conrod_store::get_image(&conn, image.id).unwrap().unwrap();
    assert_eq!(image.stars, Some(4));
    assert!(!image.rejected);

    let det_id = add_detection(
        &conn,
        image.id,
        [0.0, 0.0, 10.0, 10.0],
        "car",
        0.9,
        "crop.jpg",
    )
    .unwrap();
    set_detection_measurement(&conn, det_id, Some(&[0.2, 0.4]), Some(0.7), Some("vehicle"))
        .unwrap();
    set_detection_group(&conn, det_id, Some(7), Some(1), Some(1.0), Some("#fff")).unwrap();
    update_detection_review(
        &conn,
        det_id,
        Some(Some("12")),
        None,
        None,
        Some(false),
        Some(true),
        Some(Some(5)),
        Some(false),
    )
    .unwrap();
    let det = get_detection(&conn, det_id).unwrap().unwrap();
    assert_eq!(det.number.as_deref(), Some("12"));
    assert_eq!(det.group_key, Some(7));
    assert_eq!(det.features, Some(serde_json::json!([0.2, 0.4])));
    assert_eq!(det.stars, Some(5));
    assert_eq!(list_detections(&conn, job_id).unwrap().len(), 1);
    clear_groups(&conn, Some(job_id)).unwrap();
    assert_eq!(
        get_detection(&conn, det_id).unwrap().unwrap().group_key,
        None
    );
    assert!(delete_detection(&conn, det_id).unwrap());
    assert!(get_detection(&conn, det_id).unwrap().is_none());
}

// --- compatibility with the Python app ---------------------------------------

#[test]
fn rust_created_db_is_readable_by_python() {
    if !python_available() {
        eprintln!("no venv at {PYTHON}; skipping");
        return;
    }
    let db = temp_db_path("rust_to_python");
    let conn = conrod_store::connect(Some(&db)).unwrap();
    let job_id = create_job(
        &conn,
        Path::new("C:/shoot"),
        Some("My Shoot"),
        &serde_json::json!({"a": 1}),
    )
    .unwrap();
    add_images(
        &conn,
        job_id,
        &[
            PathBuf::from("C:/shoot/a.jpg"),
            PathBuf::from("C:/shoot/b.jpg"),
        ],
    )
    .unwrap();
    let imgs = pending_images(&conn, job_id, "pending").unwrap();
    let det_id = add_detection(
        &conn,
        imgs[0].id,
        [1.0, 2.0, 3.0, 4.0],
        "car",
        0.9,
        "C:/shoot/crops/1.jpg",
    )
    .unwrap();
    set_number(&conn, det_id, Some("42"), "manual", 0.99).unwrap();
    set_quality(
        &conn, det_id, 0.7, "sharp", 0, 4.0, "good", false, "even", -1.0, false,
    )
    .unwrap();
    let culled_id = add_detection(
        &conn,
        imgs[1].id,
        [0.0, 0.0, 1.0, 1.0],
        "car",
        0.5,
        "C:/shoot/crops/2.jpg",
    )
    .unwrap();
    cull_detection(&conn, culled_id, "blurred", false).unwrap();
    add_sharpness_label(
        &conn,
        &imgs[0].path,
        [1.0, 2.0, 3.0, 4.0],
        4,
        false,
        false,
        Some(&[0.1, 0.2]),
        1,
    )
    .unwrap();
    drop(conn);

    let script = r#"
import json, sys
from pathlib import Path
from conrod import store

conn = store.connect(Path(sys.argv[1]))
det_id, culled_id = int(sys.argv[2]), int(sys.argv[3])

job = store.latest_job(conn)
imgs = store.pending_images(conn, job["id"], "pending")
det = conn.execute("SELECT * FROM detections WHERE id=?", (det_id,)).fetchone()
culled = conn.execute("SELECT * FROM detections WHERE id=?", (culled_id,)).fetchone()
label = conn.execute(
    "SELECT stars, features FROM sharpness_labels WHERE path=?", (imgs[0]["path"],)
).fetchone()

print(json.dumps({
    "job_label": job["label"],
    "job_status": job["status"],
    "image_count": len(imgs),
    "det_number": det["number"],
    "det_number_conf": det["number_conf"],
    "det_sharpness": det["sharpness"],
    "det_rating_verdict": det["rating_verdict"],
    "culled_rejected": bool(culled["rejected"]),
    "culled_reason": culled["cull_reason"],
    "label_stars": label["stars"] if label else None,
    "label_features": json.loads(label["features"]) if label and label["features"] else None,
    "label_counts": store.sharpness_label_counts(conn),
}))
"#;
    let out = Command::new(PYTHON)
        .current_dir(repo_root())
        .args([
            "-c",
            script,
            db.to_str().unwrap(),
            &det_id.to_string(),
            &culled_id.to_string(),
        ])
        .output()
        .expect("run python");
    assert!(
        out.status.success(),
        "python failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let got: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    assert_eq!(got["job_label"], "My Shoot");
    assert_eq!(got["job_status"], "scanning");
    assert_eq!(got["image_count"], 2);
    assert_eq!(got["det_number"], "42");
    assert_eq!(got["det_number_conf"], 0.99);
    assert_eq!(got["det_sharpness"], 0.7);
    assert_eq!(got["det_rating_verdict"], "good");
    assert_eq!(got["culled_rejected"], true);
    assert_eq!(got["culled_reason"], "blurred");
    assert_eq!(got["label_stars"], 4);
    assert_eq!(got["label_features"], serde_json::json!([0.1, 0.2]));
    assert_eq!(got["label_counts"]["4"], 1);
}

#[test]
fn python_created_db_is_readable_by_rust() {
    if !python_available() {
        eprintln!("no venv at {PYTHON}; skipping");
        return;
    }
    let db = temp_db_path("python_to_rust");
    let script = r#"
import json, sys
from pathlib import Path
from conrod import store

conn = store.connect(Path(sys.argv[1]))
job_id = store.create_job(conn, Path("C:/shoot2"), "Py Shoot", {"b": 2})
store.add_images(conn, job_id, [Path("C:/shoot2/a.jpg"), Path("C:/shoot2/b.jpg")])
imgs = store.pending_images(conn, job_id, "pending")
det_id = store.add_detection(conn, imgs[0]["id"], (1.0, 2.0, 3.0, 4.0), "car", 0.8, "C:/shoot2/crops/1.jpg")
store.set_number(conn, det_id, "7", "ocr", 0.95)
store.set_quality(conn, det_id, sharpness=0.6, sharpness_verdict="soft", clipped=1,
                   rating=2.5, rating_verdict="fair", panning=True, sharp_end="left",
                   background=0.2, uncertain=True)
culled_id = store.add_detection(conn, imgs[1]["id"], (0.0, 0.0, 1.0, 1.0), "car", 0.4, "C:/shoot2/crops/2.jpg")
store.cull_detection(conn, culled_id, "no plate", uncertain=True)
store.add_sharpness_label(conn, imgs[0]["path"], (1.0, 2.0, 3.0, 4.0), stars=5, pan=False,
                           heur_pan=False, features=(0.3, 0.4), version=1)
conn.close()
print(json.dumps({"job_id": job_id, "det_id": det_id, "culled_id": culled_id}))
"#;
    let out = Command::new(PYTHON)
        .current_dir(repo_root())
        .args(["-c", script, db.to_str().unwrap()])
        .output()
        .expect("run python");
    assert!(
        out.status.success(),
        "python failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let job_id = meta["job_id"].as_i64().unwrap();
    let det_id = meta["det_id"].as_i64().unwrap();
    let culled_id = meta["culled_id"].as_i64().unwrap();

    let conn = conrod_store::connect(Some(&db)).unwrap();
    let job = latest_job(&conn).unwrap().unwrap();
    assert_eq!(job.id, job_id);
    assert_eq!(job.label.as_deref(), Some("Py Shoot"));

    let imgs = pending_images(&conn, job_id, "pending").unwrap();
    assert_eq!(imgs.len(), 2);

    // det_id has a number_source; only the culled one, with none, is unread.
    let unread = unread_detections(&conn, job_id).unwrap();
    assert_eq!(unread.len(), 1);
    assert_eq!(unread[0].id, culled_id);
    assert!(unread[0].rejected);
    assert_eq!(unread[0].cull_reason.as_deref(), Some("no plate"));
    assert_eq!(unread[0].uncertain, Some(1));

    let numbered = one_detection(&conn, det_id);
    assert_eq!(numbered.number.as_deref(), Some("7"));
    assert_eq!(numbered.sharpness, Some(0.6));
    assert_eq!(numbered.rating_verdict.as_deref(), Some("fair"));
    assert_eq!(numbered.panning, Some(1));

    let labels = sharpness_labels(&conn, 1).unwrap();
    assert_eq!(labels.len(), 1);
    assert_eq!(labels[0].stars, 5);
    assert_eq!(labels[0].features, Some(serde_json::json!([0.3, 0.4])));
}

#[test]
fn schema_matches_between_python_and_rust_created_dbs() {
    if !python_available() {
        eprintln!("no venv at {PYTHON}; skipping");
        return;
    }
    let rust_db = temp_db_path("schema_rust");
    conrod_store::connect(Some(&rust_db)).unwrap();

    let py_db = temp_db_path("schema_python");
    let out = Command::new(PYTHON)
        .current_dir(repo_root())
        .args([
            "-c",
            "import sys; from pathlib import Path; from conrod import store; store.connect(Path(sys.argv[1])).close()",
            py_db.to_str().unwrap(),
        ])
        .output()
        .expect("run python");
    assert!(
        out.status.success(),
        "python failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let rust_conn = Connection::open(&rust_db).unwrap();
    let py_conn = Connection::open(&py_db).unwrap();

    for table in [
        "jobs",
        "images",
        "detections",
        "known_vehicles",
        "sharpness_labels",
    ] {
        let rust_columns = table_info(&rust_conn, table);
        let python_columns = table_info(&py_conn, table);
        // Rust carries additive engine fields; every Python column must keep
        // the same ordinal/type/constraint so either implementation can read
        // a database created by the other.
        assert!(
            python_columns
                .iter()
                .all(|(_, name, kind, notnull, default, pk)| {
                    rust_columns.iter().any(
                        |(_, rust_name, rust_kind, rust_notnull, rust_default, rust_pk)| {
                            name == rust_name
                                && kind == rust_kind
                                && notnull == rust_notnull
                                && default == rust_default
                                && pk == rust_pk
                        },
                    )
                }),
            "Python columns missing or changed in Rust schema for {table}"
        );
    }

    assert!(table_info(&rust_conn, "images")
        .iter()
        .any(|(_, name, _, _, _, _)| name == "stars"));
    assert!(table_info(&rust_conn, "images")
        .iter()
        .any(|(_, name, _, _, _, _)| name == "rejected"));
    assert!(table_info(&rust_conn, "detections")
        .iter()
        .any(|(_, name, _, _, _, _)| name == "region_type"));
}

type ColumnRow = (i64, String, String, i64, Option<String>, i64);

fn table_info(conn: &Connection, table: &str) -> Vec<ColumnRow> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap();
    stmt.query_map([], |r| {
        Ok((
            r.get(0)?,
            r.get(1)?,
            r.get(2)?,
            r.get(3)?,
            r.get(4)?,
            r.get(5)?,
        ))
    })
    .unwrap()
    .collect::<rusqlite::Result<Vec<ColumnRow>>>()
    .unwrap()
}
