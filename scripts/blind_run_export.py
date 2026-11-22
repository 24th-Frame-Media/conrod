"""Export the blind-run picks from the Conrod DB, read-only, to target/blind_run/."""
import csv
import json
import os
import sqlite3

DB = os.path.expanduser(r"~\.conrod\conrod.db")
OUT = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "target", "blind_run"
)
JOB = 106
os.makedirs(OUT, exist_ok=True)

db = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)
db.row_factory = sqlite3.Row

frames = db.execute(
    """
    SELECT i.id, i.path, i.burst_key, i.burst_pick, i.picked, i.rejected, i.stars,
           i.rating, i.sharpness, i.exposure_verdict, i.highlight_clip, i.shadow_clip,
           i.dark_frame, i.eye_open_min, i.smile_max, i.buck_score, i.set_id, i.set_kind,
           i.thumb_path,
           (SELECT max(d.burst_pick) FROM detections d WHERE d.image_id = i.id AND d.burst_pick = 1) AS det_pick,
           (SELECT group_concat(DISTINCT d.cls) FROM detections d WHERE d.image_id = i.id) AS classes
    FROM images i WHERE i.job_id = ? ORDER BY i.path
    """,
    (JOB,),
)
rows = [dict(r) for r in frames]
for r in rows:
    r["keeper"] = int(bool(r["picked"]) or bool(r["burst_pick"]) or bool(r["det_pick"]))

bursts = {}
for r in rows:
    b = str(r["burst_key"])
    bursts.setdefault(b, []).append(r)

with open(os.path.join(OUT, "picks.csv"), "w", newline="", encoding="utf-8") as f:
    w = csv.writer(f)
    w.writerow(
        ["file", "burst", "keeper", "engine_pick", "rating", "sharpness", "stars",
         "culled", "cull_reason", "exposure", "highlight_clip", "shadow_clip", "dark_frame",
         "eye_open_min", "smile_max", "buck_score", "set_kind", "classes"]
    )
    for r in rows:
        w.writerow([
            os.path.basename(r["path"]), r["burst_key"], r["keeper"], int(bool(r["det_pick"]) or bool(r["burst_pick"])),
            round(r["rating"] or 0, 4), round(r["sharpness"] or 0, 4), r["stars"],
            r["rejected"], "", r["exposure_verdict"],
            r["highlight_clip"], r["shadow_clip"], r["dark_frame"],
            r["eye_open_min"], r["smile_max"], r["buck_score"], r["set_kind"], r["classes"],
        ])

keepers = [r for r in rows if r["keeper"]]
multi = {b: v for b, v in bursts.items() if len(v) > 1}
summary = {
    "job": JOB,
    "frames": len(rows),
    "bursts": len(bursts),
    "multi_frame_bursts": len(multi),
    "singletons": len(bursts) - len(multi),
    "mean_burst": round(len(rows) / max(1, len(bursts)), 2),
    "keepers": len(keepers),
    "keep_rate": round(len(keepers) / max(1, len(rows)), 3),
    "bursts_with_pick": sum(1 for v in multi.values() if any(r["keeper"] for r in v)),
    "rejected": sum(1 for r in rows if r["rejected"]),
    "stars_set": sum(1 for r in rows if r["stars"] is not None),
    "exposure": {k: sum(1 for r in rows if r["exposure_verdict"] == k) for k in ("ok", "highlights", "shadows")},
    "sets": sum(1 for r in rows if r["set_kind"]),
    "rating_mean": round(sum((r["rating"] or 0) for r in rows) / len(rows), 3),
    "sharpness_mean": round(sum((r["sharpness"] or 0) for r in rows) / len(rows), 3),
    "eyes_measured": sum(1 for r in rows if r["eye_open_min"] is not None),
    "buck_scored": sum(1 for r in rows if r["buck_score"] is not None),
}
with open(os.path.join(OUT, "summary.json"), "w", encoding="utf-8") as f:
    json.dump(summary, f, indent=2)

# a contact sheet of 24 keeper picks, using the app's own thumbnails
from PIL import Image, ImageDraw  # noqa: E402

picked = [r for r in rows if r["keeper"] and r["thumb_path"] and os.path.exists(r["thumb_path"])]
step = max(1, len(picked) // 24)
sample = picked[::step][:24]
cell, cols = 300, 6
sheet = Image.new("RGB", (cell * cols, cell * ((len(sample) + cols - 1) // cols)), (18, 18, 20))
draw = ImageDraw.Draw(sheet)
for i, r in enumerate(sample):
    im = Image.open(r["thumb_path"]).convert("RGB")
    im.thumbnail((cell, cell - 18))
    x, y = (i % cols) * cell, (i // cols) * cell
    sheet.paste(im, (x + (cell - im.width) // 2, y))
    draw.text((x + 6, y + cell - 16), f"{os.path.basename(r['path'])[-12:]}  r{r['rating']:.2f}", fill=(220, 220, 220))
sheet.save(os.path.join(OUT, "keepers.jpg"), quality=88)
print(json.dumps(summary, indent=2))
