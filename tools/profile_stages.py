"""Where does a scan's time go, stage by stage, on frames already in the DB.

    python tools/profile_stages.py [--job N] [--frames 12] [--crops 30]

Read-only against ~/.conrod/conrod.db, and the vision model is left out on
purpose (it is seconds a crop and already measured), so this is safe to run
next to a live scan -- though it shares the CPU with it, so absolute numbers
read a little high while a scan is going.
"""

from __future__ import annotations

import argparse
import random
import sqlite3
import statistics
import sys
import tempfile
import time
from collections import defaultdict
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from PIL import Image  # noqa: E402

from conrod import analyze as analyze_mod  # noqa: E402
from conrod import colour, detect, ocr, plates, sharpness, similarity  # noqa: E402
from conrod.config import DB_PATH, Settings  # noqa: E402
from conrod.exif import extract_previews, read_tags_many  # noqa: E402
from conrod.pipeline import _native_region  # noqa: E402

TIMES: dict[str, list[float]] = defaultdict(list)


def timed(name, fn):
    def wrapper(*args, **kwargs):
        start = time.perf_counter()
        try:
            return fn(*args, **kwargs)
        finally:
            TIMES[name].append(time.perf_counter() - start)
    return wrapper


def report(title: str) -> None:
    print(f"\n{title}\n{'stage':<24}{'n':>4}{'mean ms':>10}{'median':>9}{'p90':>9}")
    for name, values in TIMES.items():
        if len(values) > 1:            # the first call pays for loading a model
            values = values[1:]
        ms = sorted(v * 1000 for v in values)
        p90 = ms[min(len(ms) - 1, int(len(ms) * 0.9))]
        print(f"{name:<24}{len(ms):>4}{statistics.mean(ms):>10.1f}"
              f"{statistics.median(ms):>9.1f}{p90:>9.1f}")
    TIMES.clear()


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--job", type=int)
    ap.add_argument("--frames", type=int, default=12)
    ap.add_argument("--crops", type=int, default=30)
    args = ap.parse_args()

    db = sqlite3.connect(f"file:{DB_PATH}?mode=ro", uri=True, timeout=5)
    db.row_factory = sqlite3.Row
    job = args.job or db.execute("SELECT max(id) FROM jobs").fetchone()[0]
    rng = random.Random(0)

    settings = Settings.load()
    settings.use_vlm = False
    print(f"job {job}, workers={settings.analysis_workers}, "
          f"plate_native_search={settings.plate_native_search}")

    # --- exif: tag reads and preview extraction, on the original RAWs ---
    raws = [Path(r["path"]) for r in db.execute(
        "SELECT path FROM images WHERE job_id=?", (job,))]
    raws = [p for p in raws if p.exists()]
    if raws:
        for sample in (rng.sample(raws, min(40, len(raws))),):
            start = time.perf_counter()
            read_tags_many(sample, ["DateTimeOriginal", "Rating", "Label"], workers=4)
            TIMES["exif read (per file)"].append(
                (time.perf_counter() - start) / len(sample))
            TIMES["exif read (per file)"].append(TIMES["exif read (per file)"][-1])
        pick = rng.sample(raws, min(args.frames, len(raws)))
        with tempfile.TemporaryDirectory() as tmp:
            start = time.perf_counter()
            got = extract_previews(pick, Path(tmp))
            per = (time.perf_counter() - start) / max(1, len(pick))
            TIMES["preview extract (file)"] += [per, per]
            sizes = sum(p.stat().st_size for p in pick) / len(pick) / 1e6
            print(f"\nsample RAW size: {sizes:.1f} MB, previews returned: {len(got)}")
        report("exif")

    # --- frame stage: detection on a cached preview ---
    frames = [dict(r) for r in db.execute(
        "SELECT preview_path FROM images WHERE job_id=? AND preview_path IS NOT NULL",
        (job,))]
    frames = [f["preview_path"] for f in frames if Path(f["preview_path"]).exists()]
    detect.detect = timed("detect (frame)", detect.detect)
    for path in rng.sample(frames, min(args.frames, len(frames))):
        detect.detect(Path(path), settings)
    report("frame loop")

    # --- crop stage: each reader, then the sharpness measure ---
    rows = [dict(r) for r in db.execute(
        """SELECT d.crop_path, d.x1, d.y1, d.x2, d.y2, i.preview_path
             FROM detections d JOIN images i ON i.id = d.image_id
            WHERE i.job_id=? AND d.crop_path IS NOT NULL""", (job,))]
    rows = [r for r in rows if Path(r["crop_path"]).exists()]
    rows = rng.sample(rows, min(args.crops, len(rows)))

    plates.scan_regions = timed("plates.scan_regions", plates.scan_regions)
    ocr.read_number = timed("ocr.read_number", ocr.read_number)
    ocr.visible_text = timed("ocr.visible_text", ocr.visible_text)
    analyze_mod.plates, analyze_mod.ocr = plates, ocr
    native_of = timed("native region (decode)", _native_region)
    measure = timed("sharpness.measure", sharpness.measure)
    embed = timed("similarity.embed", similarity.embed)
    swatch = timed("colour.dominant", colour.dominant)
    whole = timed("analyze total (no VLM)", analyze_mod.analyze)

    for r in rows:
        native = None
        if r["preview_path"] and Path(r["preview_path"]).exists():
            native = native_of(r["preview_path"], (r["x1"], r["y1"], r["x2"], r["y2"]),
                               settings)
        with Image.open(r["crop_path"]) as crop:
            crop.load()
            whole(crop, settings, native=native)
            measure(crop)
            swatch(crop)
            embed(crop)
    report(f"per crop, {len(rows)} crops")


if __name__ == "__main__":
    main()
