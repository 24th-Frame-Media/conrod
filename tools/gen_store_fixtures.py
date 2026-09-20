"""Record what the Python app's store writes into the native repository's rust/fixtures/.

From a worktree of this branch:  python tools/gen_store_fixtures.py <path to rust/fixtures>
(the repository root must be on sys.path, e.g. PYTHONPATH=.)
"""
import json, sqlite3, sys, tempfile
from pathlib import Path

from conrod import store

out = Path(sys.argv[1])
tmp = Path(tempfile.mkdtemp(prefix="conrod-store-fixture-"))

# --- a small library written the way the Python app writes one ---------------------------------
db = tmp / "library.db"
conn = store.connect(db)
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
conn.commit()
conn.close()
dump = sqlite3.connect(db)
sql = "\n".join(dump.iterdump()) + "\n"
dump.close()
(out / "python_library.sql").write_text(sql, encoding="utf-8", newline="\n")

# --- the schema of a fresh Python database ------------------------------------------------------
fresh = tmp / "fresh.db"
store.connect(fresh).close()
c = sqlite3.connect(fresh)
schema = {t: [list(r) for r in c.execute(f"PRAGMA table_info({t})")]
          for t in ("jobs", "images", "detections", "known_vehicles", "sharpness_labels")}
c.close()
(out / "python_schema.json").write_text(json.dumps(schema, indent=1), encoding="utf-8", newline="\n")
print("ok", len(sql), "bytes of SQL;", {t: len(v) for t, v in schema.items()})
