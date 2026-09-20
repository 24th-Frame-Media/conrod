"""Freeze Python's similarity embedding and colour swatch on real crops.

    python tools/gen_vision_local.py [--job 38 --job 39] [--count 100]

Writes rust/fixtures/vision_local.json (git-ignored: it names private files
and embeds real photographs' pixels): per crop, the DINOv2 embedding and the
dominant-colour hex Python computed for it, for the Rust port to be checked
against. Needs the model at ~/.conrod/models -- see conrod/similarity.py --
and skips (prints why, writes nothing) without it.
"""

from __future__ import annotations

import argparse
import json
import random
import sqlite3
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))

from conrod import colour as colour_mod  # noqa: E402
from conrod import similarity  # noqa: E402
from conrod.config import DB_PATH  # noqa: E402


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--job", type=int, action="append", dest="jobs", default=None)
    ap.add_argument("--count", type=int, default=100)
    args = ap.parse_args()
    jobs = tuple(args.jobs or (38, 39))

    if not similarity.is_ready():
        print("dinov2 model not present under ~/.conrod/models; skipping "
              "(see conrod/similarity.py)")
        return

    from PIL import Image

    db = sqlite3.connect(f"file:{DB_PATH}?mode=ro", uri=True)
    marks = ",".join("?" * len(jobs))
    rows = [r[0] for r in db.execute(
        f"""SELECT d.crop_path FROM detections d JOIN images i ON i.id = d.image_id
             WHERE i.job_id IN ({marks}) AND d.crop_path IS NOT NULL""", jobs)
        if Path(r[0]).exists()]
    rows = random.Random(13).sample(rows, min(args.count, len(rows)))

    cases = []
    for crop_path in rows:
        with Image.open(crop_path) as crop:
            crop.load()
            vector = similarity.embed(crop)
            if vector is None:
                continue
            swatch = colour_mod.dominant(crop)
        cases.append({"image": crop_path, "embedding": vector.tolist(), "colour": swatch})

    out = ROOT / "rust" / "fixtures" / "vision_local.json"
    out.write_text(json.dumps({"cases": cases}), encoding="utf-8")
    print(f"rust/fixtures/vision_local.json  {len(cases)} real crops (local-only)")


if __name__ == "__main__":
    main()
