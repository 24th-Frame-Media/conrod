"""Freeze `conrod/plates.py`'s behaviour for the Rust port to check itself
against.

    python tools/gen_plates_local.py              # rust/fixtures/plates_local.json (local-only)
    python tools/gen_plates_local.py --committed   # rust/fixtures/plates_text.json (committed)

Run with C:/Users/kapsikkum/.trackaction/venv/Scripts/python.exe; no pip
installs. The local fixture points at the photographer's own job database and
real vehicle crops, so it is git-ignored like `sharpness_local.json`; the
committed one exercises only the pure text logic (`_interpret`,
`_trim_to_format`, `looks_like_plate`) with synthetic inputs, same as
`tools/gen_golden.py`'s fixtures. Kept in its own script, rather than added to
gen_golden.py, so the two files never conflict on the same lines.
"""

from __future__ import annotations

import json
import random
import sqlite3
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT / "tools"))

from conrod import plates  # noqa: E402
from conrod.config import DB_PATH, Settings  # noqa: E402

OUT = ROOT / "rust" / "fixtures"


def _reading_dict(reading: "plates.PlateReading") -> dict:
    return {
        "text": reading.text, "state": reading.state,
        "confidence": reading.confidence, "candidates": reading.candidates,
    }


def plates_text_cases() -> dict:
    """Synthetic cases for the pure text logic. No model, no camera --
    just the issue-format regexes and the rules for picking a registration
    out of a plate crop's OCR lines."""
    settings = Settings()

    looks_like = [
        "FD23RS", "ABC123", "123ABC", "A1234", "731117J", "AB1", "",
        "abc123", "NSW", "12345678", "AB1234CD", "1A2B3C",
    ]
    trim = [
        "ELA93NG",    # a real Bathurst misread: a stray leading edge character
        "LA93NGX",    # trailing stray character
        "ABCDEFGH",   # nothing plausible either way
        "FD23RS",     # already valid, untouched
        "A",          # too short to ever trim into a match
        "12345",
    ]
    lines_cases = [
        [("FD23RS", 0.9)],
        [("New South Wales", 0.5), ("FD23RS", 0.92)],
        [("NSW", 0.4), ("ABC123", 0.6)],
        [("QUEENSLAND", 0.3), ("73111J", 0.88)],
        [("The Sunshine State", 0.2), ("XY", 0.5)],     # noise, then too short
        [("garbage!!", 0.9)],
        [("ABC1234567", 0.9)],                          # too long once stripped
        [("ELA93NG", 0.7)],                              # needs trimming to match
        [],
        [("Australia", 0.9), ("random word", 0.8)],
        [("VIC", 0.6), ("1ABC23", 0.5)],
        [("73111J", 0.5), ("FD23RS", 0.6)],              # two candidates: format wins
    ]
    return {
        "looks_like_plate": [
            {"token": t, "want": plates.looks_like_plate(t)} for t in looks_like
        ],
        "trim_to_format": [
            {"token": t, "want": plates._trim_to_format(t)} for t in trim
        ],
        "interpret": [
            {
                "lines": [list(line) for line in lines],
                "want": _reading_dict(plates._interpret(lines, settings)),
            }
            for lines in lines_cases
        ],
    }


def plates_local_cases(count: int = 150) -> dict:
    """Real vehicle crops from the job database, and what
    `plates.scan_regions` made of each -- the parity gate that counts.

    Default `Settings()`, not `Settings.load()`: the Rust port's own default
    options must line up with whatever generated this fixture, and a
    photographer's local settings.json would otherwise silently drift the
    two apart.
    """
    from PIL import Image

    import local_frames

    local_frames.use_shipped_ocr()
    settings = Settings()
    # Real crops rebuilt from the originals of the identified job (Python's own
    # crop cache is disposable); 70% with a plate read, the rest without.
    rows = local_frames.materialise(local_frames.detections(15, count, plated=0.7), settings)
    chosen = [(r["crop_path"], r["x1"], r["y1"], r["x2"], r["y2"], r["preview_path"], r["plate"])
              for r in rows]

    cases = []
    for crop_path, x1, y1, x2, y2, preview_path, plate_column in chosen:
        try:
            with Image.open(crop_path) as crop:
                crop.load()
                native = None
                if settings.plate_native_search:
                    with Image.open(preview_path) as frame:
                        frame.load()
                        # The raw detection box (not the padded crop_box):
                        # what pipeline._native_region cuts the vehicle from
                        # at full resolution.
                        native = frame.crop((int(x1), int(y1), int(x2), int(y2)))
                reading, numbers = plates.scan_regions(crop, settings, native=native)
        except Exception as exc:  # a corrupt or half-written crop; skip it
            print(f"skip {crop_path}: {exc}")
            continue
        cases.append({
            "crop": crop_path, "preview": preview_path,
            "box": [x1, y1, x2, y2], "plate_column": plate_column,
            "text": reading.text, "state": reading.state,
            "confidence": reading.confidence,
            "numbers": [[text, score] for text, score in numbers],
        })
    return {"cases": cases}


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    if "--committed" in sys.argv:
        path = OUT / "plates_text.json"
        path.write_text(
            json.dumps(plates_text_cases(), indent=1, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        print(f"{path.relative_to(ROOT)}  committed pure-text-logic fixture")
        return

    payload = plates_local_cases()
    path = OUT / "plates_local.json"
    path.write_text(json.dumps(payload), encoding="utf-8")
    with_text = sum(1 for c in payload["cases"] if c["text"])
    print(f"{path.relative_to(ROOT)}  {len(payload['cases'])} real crops "
          f"({with_text} read a plate) (local-only)")


if __name__ == "__main__":
    main()
