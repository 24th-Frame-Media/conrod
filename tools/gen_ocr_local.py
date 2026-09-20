"""RapidOCR's behaviour on real vehicle crops, for the Rust OCR to check itself against.

    .venv/Scripts/python.exe tools/gen_ocr_local.py [--models DIR]   # rust/fixtures/ocr_local.json

Local-only (git-ignored, like the other *_local.json): it points at crops rebuilt
from the photographer's originals by tools/local_frames.py. RapidOCR is run with
the PP-OCRv4 files that ship in the Python release (--models, default: the 0.2.11
build in Downloads), because that is what a user's Python Conrod actually runs;
the venv's own package still carries v3. Everything after the engine
(`ocr_tokens`, `read_number`, `visible_text`) is Python's real code.
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "tools"))

import local_frames  # noqa: E402
from conrod import ocr  # noqa: E402
from conrod.config import Settings  # noqa: E402

DEFAULT_MODELS = local_frames.SHIPPED_OCR


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--models", type=Path, default=DEFAULT_MODELS)
    ap.add_argument("--job", type=int, default=15)
    ap.add_argument("--limit", type=int, default=120)
    args = ap.parse_args()

    local_frames.use_shipped_ocr(args.models)
    settings = Settings()
    rows = local_frames.materialise(local_frames.detections(args.job, args.limit), settings, need_preview=False)
    cases = []
    for r in rows:
        from PIL import Image

        with Image.open(r["crop_path"]) as im:
            im.load()
            raw, _ = ocr._engine(__import__("numpy").asarray(im.convert("RGB")))
            reading = ocr.read_number(im, settings)
            text = ocr.visible_text(im, settings)
        cases.append({
            "crop": r["crop_path"], "detection": r["id"],
            "stored_number": r["number"], "stored_source": r["number_source"], "stored_plate": r["plate"],
            "lines": [[t, float(s), [[float(x), float(y)] for x, y in b]] for b, t, s in (raw or [])],
            "number": [reading.number, reading.confidence], "text": text,
        })
    out = ROOT / "rust" / "fixtures" / "ocr_local.json"
    out.write_text(json.dumps({"models": "PP-OCRv4", "cases": cases}, indent=1) + "\n", encoding="utf-8")
    print(f"{out.relative_to(ROOT)}: {len(cases)} cases, {sum(1 for c in cases if c['number'][0])} with a number")


if __name__ == "__main__":
    main()
