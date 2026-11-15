"""Contact sheet of the frames the exposure check found most blown.

`cargo test -p conrod-vision --release --test exposure_local -- --ignored`
writes the six worst previews and a manifest to target/exposure_check; this
compounds them into one JPEG so the verdicts can be eyeballed.

    python scripts/exposure_sheet.py [--dir DIR] [--out FILE]
"""

import argparse
import json
import sys
from pathlib import Path

from PIL import Image, ImageDraw

COLUMNS = 3
CELL = 640


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--dir", default=r"E:\code\photo\conrod\target\exposure_check"
    )
    parser.add_argument(
        "--out", default=r"E:\code\photo\conrod\target\exposure_check.jpg"
    )
    args = parser.parse_args()

    folder = Path(args.dir)
    manifest = folder / "sheet.json"
    if not manifest.exists():
        print(f"no {manifest}; run the ignored exposure_local test first")
        return 1
    entries = json.loads(manifest.read_text(encoding="utf-8"))
    if not entries:
        print("the manifest is empty")
        return 1

    rows = (len(entries) + COLUMNS - 1) // COLUMNS
    label_height = 28
    sheet = Image.new(
        "RGB", (COLUMNS * CELL, rows * (CELL + label_height)), (24, 24, 24)
    )
    draw = ImageDraw.Draw(sheet)
    for i, entry in enumerate(entries):
        image = Image.open(entry["image"]).convert("RGB")
        image.thumbnail((CELL, CELL), Image.LANCZOS)
        x = (i % COLUMNS) * CELL
        y = (i // COLUMNS) * (CELL + label_height)
        # Centre the preview in its cell.
        sheet.paste(
            image,
            (x + (CELL - image.width) // 2, y + (CELL - image.height) // 2),
        )
        draw.text((x + 6, y + CELL + 6), entry["label"], fill=(230, 230, 230))

    sheet.save(args.out, quality=88)
    print(f"wrote {args.out} ({sheet.width}x{sheet.height}) from {len(entries)} frames")
    return 0


if __name__ == "__main__":
    sys.exit(main())
