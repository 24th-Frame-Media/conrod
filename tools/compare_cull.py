"""Rust cull vs a finished Python job: the plan's side-by-side gate.

    conrod-cli cull "<folder>" > rust.jsonl
    python tools/compare_cull.py rust.jsonl --job 38

Reads ~/.conrod/conrod.db read-only. Subjects are matched per frame by IoU
within the same class; sharpness and stars are compared on the matches.
Python stored no derived stars, so they are recomputed from `rating` with
sharpness.STAR_BANDS.
"""
import argparse
import json
import os
import sqlite3
import statistics
from pathlib import Path

STAR_BANDS = ((0.958, 5), (0.825, 4), (0.728, 3), (0.606, 2), (0.0, 1))


def stars_for(rating):
    return next((s for floor, s in STAR_BANDS if rating >= floor), 1)


def key(path):
    return os.path.normcase(os.path.normpath(path))


def iou(a, b):
    ix = max(0.0, min(a[2], b[2]) - max(a[0], b[0]))
    iy = max(0.0, min(a[3], b[3]) - max(a[1], b[1]))
    inter = ix * iy
    union = (a[2] - a[0]) * (a[3] - a[1]) + (b[2] - b[0]) * (b[3] - b[1]) - inter
    return inter / union if union else 0.0


def pct(xs, q):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, int(q * len(xs)))] if xs else float("nan")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("jsonl")
    ap.add_argument("--job", type=int, required=True)
    ap.add_argument("--db", default=str(Path.home() / ".conrod" / "conrod.db"))
    ap.add_argument("--classes", default="vehicle", help="Rust class group: vehicle|person")
    args = ap.parse_args()

    rust = {}
    for line in Path(args.jsonl).read_text(encoding="utf-8").splitlines():
        if line.startswith("{"):
            f = json.loads(line)
            rust[key(f["path"])] = f

    db = sqlite3.connect(f"file:{args.db}?mode=ro", uri=True)
    py = {}
    for path, iid in db.execute("SELECT path, id FROM images WHERE job_id=?", (args.job,)):
        py[key(path)] = {"id": iid, "subjects": []}
    by_id = {v["id"]: v for v in py.values()}
    for row in db.execute(
        "SELECT d.image_id, d.cls, d.x1, d.y1, d.x2, d.y2, d.conf, d.sharpness, d.rating, d.panning, d.rejected "
        "FROM detections d JOIN images i ON i.id = d.image_id WHERE i.job_id=?",
        (args.job,),
    ):
        by_id[row[0]]["subjects"].append(row[1:])

    both = sorted(rust.keys() & py.keys())
    print(f"frames: rust {len(rust)}, python {len(py)}, in both {len(both)}")
    py_person = sum(1 for k in py for s in py[k]["subjects"] if s[0] == "person")
    print(f"python person detections in job: {py_person} (0 means the job ran without people)")

    same_count = matched = m90 = rust_only = py_only = 0
    d_sharp, d_rating, d_stars_eq, d_pan_eq, d_rej_eq, n_cmp = [], [], 0, 0, 0, 0
    conf_gap = []
    for k in both:
        r = [s for s in rust[k]["vehicles"] if (s["class"] != "person") == (args.classes == "vehicle")]
        p = [s for s in py[k]["subjects"] if (s[0] != "person") == (args.classes == "vehicle")]
        same_count += len(r) == len(p)
        used = set()
        for s in r:
            best, bi = 0.0, None
            for i, q in enumerate(p):
                if i in used or q[0] != s["class"]:
                    continue
                v = iou(s["box"], q[1:5])
                if v > best:
                    best, bi = v, i
            if bi is None or best < 0.5:
                rust_only += 1
                continue
            used.add(bi)
            matched += 1
            m90 += best >= 0.9
            q = p[bi]
            d_sharp.append(abs(s["sharpness"] - q[6]))
            d_rating.append(abs(s["rating"] - q[7]))
            conf_gap.append(abs(s["conf"] - q[5]))
            n_cmp += 1
            d_stars_eq += s["stars"] == stars_for(q[7])
            d_pan_eq += bool(s["panning"]) == bool(q[8])
            d_rej_eq += bool(s["cull"]) == bool(q[9])
        py_only += len(p) - len(used)

    print(f"frames with equal subject count: {same_count}/{len(both)}")
    print(f"subjects matched IoU>=0.5: {matched} (>=0.9: {m90}); rust-only {rust_only}, python-only {py_only}")
    if n_cmp:
        for name, ds in (("sharpness", d_sharp), ("rating", d_rating)):
            print(
                f"|d {name}| p50 {pct(ds, .5):.4f}  p90 {pct(ds, .9):.4f}  "
                f"p99 {pct(ds, .99):.4f}  max {max(ds):.4f}  "
                f"within 0.005: {sum(x <= .005 for x in ds) / n_cmp:.1%}"
            )
        print(f"|d conf| median {statistics.median(conf_gap):.4f}")
        print(f"stars equal {d_stars_eq / n_cmp:.1%}, panning equal {d_pan_eq / n_cmp:.1%}, cull decision equal {d_rej_eq / n_cmp:.1%}")


if __name__ == "__main__":
    main()
