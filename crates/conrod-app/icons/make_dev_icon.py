"""Generate the badged dev-mode Conrod icons from the production `conrod.png`.

The slate tile and its amber rod are untouched; a small amber DEV box is drawn
into the empty bottom-right corner, inside a dark ring. One file per size, with
three tiers so the signal survives down to 16 px (SPEC §7):

  >= 64 px  word + box   ("DEV" in the box)
  32-48 px  plain box
  < 32 px   plain box with a thicker ring (a "nick")

Run from the checkout root (or anywhere):  python crates/conrod-app/icons/make_dev_icon.py
Outputs live beside this script. Requires Pillow.
"""
from __future__ import annotations

import os
import struct
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFont

HERE = Path(__file__).resolve().parent
SRC = HERE / "conrod.png"

SIZES = [16, 24, 32, 48, 64, 128, 256, 512]
ICO_SIZES = [16, 24, 32, 48, 64, 128, 256]
AA = 4  # supersample factor for smooth edges at small sizes

RING = (12, 15, 19, 255)        # #0c0f13
GRAD_TOP = (242, 192, 98)        # #f2c062
GRAD_BOTTOM = (207, 145, 39)     # #cf9127
INK = (26, 20, 7, 255)           # #1a1407

# Geometry on a 256 grid. (ring bbox, ring radius, face bbox, face radius)
WORD = ((132, 196, 246, 246), 13, (136, 200, 242, 242), 9)
BOX = ((134, 198, 246, 246), 13, (138, 202, 240, 240), 9)
NICK = ((133, 197, 245, 245), 14, (138, 202, 240, 240), 9)

FONT_CANDIDATES = [
    r"C:\Windows\Fonts\segoeuib.ttf",
    "DejaVuSans-Bold.ttf",
]


def tier(S: int):
    if S >= 64:
        return WORD, True
    if S >= 32:
        return BOX, False
    return NICK, False


def scale(box, k):
    return tuple(round(v * k) for v in box)


def load_font(size: int):
    for path in FONT_CANDIDATES:
        try:
            return ImageFont.truetype(path, size)
        except OSError:
            continue
    return ImageFont.load_default()


def gradient_face(size, bbox, radius, k):
    """A rounded rectangle filled with a vertical amber gradient (top -> bottom)."""
    x0, y0, x1, y1 = bbox
    w, h = max(1, x1 - x0), max(1, y1 - y0)
    strip = Image.new("RGB", (1, h))
    px = strip.load()
    for yy in range(h):
        t = yy / (h - 1) if h > 1 else 0
        px[0, yy] = tuple(int(GRAD_TOP[i] + (GRAD_BOTTOM[i] - GRAD_TOP[i]) * t) for i in range(3))
    grad = strip.resize((w, h))
    layer = Image.new("RGBA", size, (0, 0, 0, 0))
    layer.paste(grad, (x0, y0))
    mask = Image.new("L", size, 0)
    ImageDraw.Draw(mask).rounded_rectangle(bbox, radius=max(1, round(radius * k)), fill=255)
    layer.putalpha(ImageChops.multiply(layer.getchannel("A"), mask))
    return layer


def draw_word(overlay, k, size):
    font = load_font(max(1, round(26 * k)))
    chars = "DEV"
    advs = [font.getlength(c) for c in chars]
    tracking = 3 * k
    total = sum(advs) + tracking * (len(chars) - 1)
    x = 189 * k - total / 2
    d = ImageDraw.Draw(overlay)
    for ch, adv in zip(chars, advs):
        d.text((x + adv / 2, 221 * k), ch, font=font, fill=INK, anchor="mm")
        x += adv + tracking


def make_icon(src: Image.Image, S: int) -> Image.Image:
    (ring_box, ring_r, face_box, face_r), with_word = tier(S)
    W = S * AA
    k = W / 256.0
    base = src.resize((W, W), Image.Resampling.LANCZOS).convert("RGBA")

    ring = Image.new("RGBA", (W, W), (0, 0, 0, 0))
    ImageDraw.Draw(ring).rounded_rectangle(
        scale(ring_box, k), radius=max(1, round(ring_r * k)), fill=RING,
    )
    face = gradient_face((W, W), scale(face_box, k), face_r, k)
    overlay = Image.alpha_composite(ring, face)
    if with_word:
        draw_word(overlay, k, (W, W))

    combined = Image.alpha_composite(base, overlay)
    return combined.resize((S, S), Image.Resampling.LANCZOS)


def build_ico(pngs: dict[int, bytes]) -> bytes:
    """A PNG-backed multi-resolution .ico (each entry keeps its own tier)."""
    count = len(ICO_SIZES)
    header = struct.pack("<HHH", 0, 1, count)
    offset = 6 + 16 * count
    entries = bytearray()
    blobs = bytearray()
    for S in ICO_SIZES:
        data = pngs[S]
        w = 0 if S >= 256 else S  # 0 encodes 256 in an ICONDIRENTRY
        entries += struct.pack("<BBBBHHII", w, w, 0, 0, 1, 32, len(data), offset)
        blobs += data
        offset += len(data)
    return header + bytes(entries) + bytes(blobs)


def main():
    src = Image.open(SRC).convert("RGBA")
    pngs: dict[int, bytes] = {}
    for S in SIZES:
        icon = make_icon(src, S)
        path = HERE / f"conrod-dev-{S}.png"
        icon.save(path, "PNG")
        with open(path, "rb") as f:
            pngs[S] = f.read()
        print(f"wrote {path.name} ({S}x{S})")

    # The task's singular filename + the runtime's include_bytes! target.
    (HERE / "conrod-dev.png").write_bytes(pngs[256])
    # conrod-dev-256.png already written by the loop above; keep it for main.rs.
    (HERE / "conrod-dev-256.png").write_bytes(pngs[256])
    (HERE / "conrod-dev.ico").write_bytes(build_ico(pngs))
    print("wrote conrod-dev.png, conrod-dev-256.png, conrod-dev.ico")


if __name__ == "__main__":
    main()
