"""Record what exiftool says about a sample of the photographer's own files.

    python tools/gen_raw_local.py [--job 15]

Writes rust/fixtures/raw_local.json (git-ignored: it names private files):
per file the camera identity and capture time as the scan stored them, and
the orientation, for conrod-io's reader to be checked against.
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

from conrod.config import DB_PATH  # noqa: E402
from conrod.exif import read_tags_many  # noqa: E402


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--job", type=int, default=15)
    args = ap.parse_args()
    db = sqlite3.connect(f"file:{DB_PATH}?mode=ro", uri=True)
    rows = [r for r in db.execute(
        "SELECT path, camera, taken_at FROM images WHERE job_id=?", (args.job,))
        if Path(r[0]).exists()]
    rng = random.Random(11)
    picks = []
    for ext, n in ((".cr3", 60), (".cr2", 40), (".jpg", 20)):
        pool = [r for r in rows if r[0].lower().endswith(ext)]
        picks += rng.sample(pool, min(n, len(pool)))

    tags = read_tags_many([Path(p) for p, _, _ in picks], ["Orientation#"])
    orientation = {str(Path(t.get("SourceFile", "")).resolve()).lower(): t.get("Orientation")
                   for t in tags}
    cases = []
    for path, camera, taken in picks:
        o = orientation.get(str(Path(path).resolve()).lower())
        cases.append({"path": path, "camera": camera, "taken": taken,
                      "orientation": int(o) if isinstance(o, (int, float)) else 1})
    out = ROOT / "rust" / "fixtures" / "raw_local.json"
    out.write_text(json.dumps({"cases": cases}, indent=1), encoding="utf-8")
    print(f"{out.relative_to(ROOT)}  {len(cases)} files (local-only)")


if __name__ == "__main__":
    main()
