"""Real vehicle crops for the local parity fixtures, rebuilt from the originals.

Python's crop/preview cache is disposable, so a fixture that points into it goes
stale. This re-extracts what a fixture needs, the way the scan did (embedded
preview, then `detect.cut` on the padded box), into a folder outside the repo
and OneDrive. Originals are only read. Run with the repo venv:

    .venv/Scripts/python.exe tools/local_frames.py --job 15 --limit 120
"""

from __future__ import annotations

import argparse
import os
import random
import sqlite3
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))

from conrod import detect, exif  # noqa: E402
from conrod.config import DB_PATH, Settings  # noqa: E402

SHIPPED_OCR = Path.home() / "Downloads/Conrod-0.2.11-win64/Conrod/_internal/rapidocr_onnxruntime/models"
FIXTURES = Path(os.environ.get("LOCALAPPDATA") or Path.home()) / "conrod-dev" / "fixtures"


def use_shipped_ocr(models: Path = SHIPPED_OCR) -> None:
    """Make conrod.ocr run RapidOCR on the PP-OCRv4 files the Python release
    bundles (the venv's own package still carries v3), i.e. what a user's
    Python Conrod actually does."""
    from rapidocr_onnxruntime import RapidOCR

    from conrod import ocr

    ocr._engine = RapidOCR(
        det_model_path=str(models / "ch_PP-OCRv4_det_infer.onnx"),
        rec_model_path=str(models / "ch_PP-OCRv4_rec_infer.onnx"),
        cls_model_path=str(models / "ch_ppocr_mobile_v2.0_cls_infer.onnx"),
    )


def detections(job: int, limit: int, seed: int = 11, plated: float = 0.5) -> list[dict]:
    """Vehicle detections of an identified job: `plated` share with a plate read
    (plate_conf set), the rest without one. Originals must still exist."""
    db = sqlite3.connect(f"file:{DB_PATH}?mode=ro", uri=True)
    db.row_factory = sqlite3.Row
    rows = [
        dict(r)
        for r in db.execute(
            """SELECT d.id, i.path, d.x1, d.y1, d.x2, d.y2, d.cls, d.conf, d.plate,
                      d.plate_conf, d.number, d.number_source, d.attributes
                 FROM detections d JOIN images i ON i.id = d.image_id
                WHERE i.job_id = ? AND d.rejected = 0 AND d.cls != 'person'""",
            (job,),
        )
    ]
    rows = [r for r in rows if Path(r["path"]).exists()]
    with_plate = [r for r in rows if r["plate_conf"]]
    other = [r for r in rows if not r["plate_conf"]]
    rng = random.Random(seed)
    take = min(len(with_plate), round(limit * plated))
    chosen = rng.sample(with_plate, take) + rng.sample(other, min(len(other), limit - take))
    rng.shuffle(chosen)
    return chosen


def materialise(dets: list[dict], settings: Settings | None = None,
                need_preview: bool = True) -> list[dict]:
    """Add `crop_path` (Python's analysis crop, JPEG q95) and `preview_path` to each.

    Existing crops are reused; `need_preview=False` then skips re-extracting
    the previews they came from (only the plate search's native region needs them).
    """
    from PIL import Image

    settings = settings or Settings()
    (FIXTURES / "crops").mkdir(parents=True, exist_ok=True)
    crop_of = lambda d: FIXTURES / "crops" / f"{d['id']}.jpg"  # noqa: E731
    todo = [d for d in dets if need_preview or not crop_of(d).exists()]
    previews = exif.extract_previews(sorted({Path(d["path"]) for d in todo}),
                                     FIXTURES / "previews") if todo else {}
    done = []
    for d in dets:
        out = crop_of(d)
        if not need_preview and out.exists():
            done.append({**d, "crop_path": str(out), "preview_path": None})
            continue
        preview = previews.get(Path(d["path"]))
        if not preview:
            continue
        box = (d["x1"], d["y1"], d["x2"], d["y2"])
        with Image.open(preview) as frame:
            frame.load()
            crop_box = detect.expand_box(box, frame.width, frame.height, settings)
            det = detect.Detection(box=box, crop_box=crop_box, cls=d["cls"], conf=d["conf"])
            crop = detect.cut(frame, det, settings)
        crop.save(out, "JPEG", quality=95)
        done.append({**d, "crop_path": str(out), "preview_path": str(preview)})
    return done


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--job", type=int, default=15)
    ap.add_argument("--limit", type=int, default=120)
    args = ap.parse_args()
    got = materialise(detections(args.job, args.limit))
    print(f"{len(got)} crops in {FIXTURES}")
