"""Contact sheet for the level/keystone check.

Reads `target/level_check.csv` and `target/level_frames/*.pgm`, written by
`cargo test --release -p conrod-io --test level_local -- --ignored`, and lays
the eight most tilted frames plus the four most level ones out with their
numbers so the sign and the size of the estimate can be seen against the
picture.

    E:\\code\\photo\\conrod\\.venv\\Scripts\\python.exe tools/level_check.py
"""

import csv
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont

TARGET = Path(r"E:\code\photo\conrod\target")
FRAMES = TARGET / "level_frames"
OUT = TARGET / "level_check.jpg"

COLUMNS = 4
TILE = 460
CAPTION = 46


def font(size: int):
    try:
        return ImageFont.load_default(size=size)
    except TypeError:  # Pillow < 10
        return ImageFont.load_default()


def load_rows():
    with (TARGET / "level_check.csv").open(newline="", encoding="utf-8") as f:
        rows = list(csv.DictReader(f))
    for row in rows:
        row["tilt"] = float(row["tilt_deg"])
        row["converge"] = float(row["converge_deg"])
    return rows


def main():
    rows = load_rows()
    level = [r for r in rows if r["verdict"] == "level"]
    tilted = sorted(rows, key=lambda r: -abs(r["tilt"]))[:8]
    most_level = sorted(level, key=lambda r: abs(r["tilt"]))[:4]
    picked = [(r, "most tilted") for r in tilted] + [(r, "most level") for r in most_level]

    tiles = []
    for row, kind in picked:
        image = Image.open(FRAMES / f"{int(row['index']):03}.pgm").convert("L")
        image.thumbnail((TILE, TILE))
        tile = Image.new("RGB", (TILE, TILE + CAPTION), "white")
        tile.paste(image.convert("RGB"), ((TILE - image.width) // 2, (TILE - image.height) // 2))
        draw = ImageDraw.Draw(tile)
        engage = "-" if row["converge"] != row["converge"] else f"{row['converge']:.1f}"
        caption = f"tilt {row['tilt']:+.2f}   conv {engage}   {row['verdict']}"
        draw.text((6, TILE + 2), caption, fill="black", font=font(18))
        draw.text(
            (6, TILE + 22),
            f"{kind}   conf {float(row['confidence']):.2f}   {row['file'][-9:]}",
            fill=(90, 90, 90),
            font=font(13),
        )
        tiles.append(tile)

    grid_rows = (len(tiles) + COLUMNS - 1) // COLUMNS
    sheet = Image.new(
        "RGB", (COLUMNS * TILE, grid_rows * (TILE + CAPTION)), (235, 235, 235)
    )
    for i, tile in enumerate(tiles):
        sheet.paste(tile, ((i % COLUMNS) * TILE, (i // COLUMNS) * (TILE + CAPTION)))
    sheet.save(OUT, quality=88)
    print(f"wrote {OUT} ({sheet.width}x{sheet.height})")
    for row, kind in picked:
        print(
            f"  {kind:12} {row['file'][-14:]:>14}  tilt {row['tilt']:+6.2f}  "
            f"conv {row['converge']:6.2f}  {row['verdict']}"
        )


if __name__ == "__main__":
    main()
