"""Shared helpers for the taste pipeline: cheap capture-time read + 512px preview extraction."""
import io
import re
import struct

from PIL import Image, ImageOps

Image.MAX_IMAGE_PIXELS = None
SOI = re.compile(rb'\xff\xd8\xff')
TIFF_HDR = re.compile(rb'(II\x2a\x00|MM\x00\x2a)')


def _ifd(buf, base, off, want, out, depth=0):
    bo = {b'II': '<', b'MM': '>'}[buf[base:base + 2]]
    if base + off + 2 > len(buf) or depth > 2:
        return
    n = struct.unpack(bo + 'H', buf[base + off:base + off + 2])[0]
    for i in range(n):
        e = base + off + 2 + 12 * i
        if e + 12 > len(buf):
            return
        tag, typ, cnt = struct.unpack(bo + 'HHI', buf[e:e + 8])
        if tag == 0x8769 and typ == 4 and cnt == 1:
            sub = struct.unpack(bo + 'I', buf[e + 8:e + 12])[0]
            _ifd(buf, base, sub, want, out, depth + 1)
            continue
        if tag not in want:
            continue
        if cnt <= 4:
            val = buf[e + 8:e + 8 + cnt]
        else:
            vo = struct.unpack(bo + 'I', buf[e + 8:e + 12])[0]
            val = buf[base + vo: base + vo + cnt]
        out[tag] = val.split(b'\x00')[0]


def _time_from_tiff(buf, base, out):
    bo = {b'II': '<', b'MM': '>'}.get(buf[base:base + 2])
    if not bo:
        return None
    try:
        off = struct.unpack(bo + 'I', buf[base + 4:base + 8])[0]
        tags = {}
        _ifd(buf, base, off, {0x0132, 0x9003, 0x9291, 0x9290}, tags)
    except Exception:
        return None
    raw = tags.get(0x9003) or tags.get(0x0132)
    if not raw:
        return None
    m = re.match(rb'(\d{4})[:\-](\d{2})[:\-](\d{2})[T ](\d{2}):(\d{2}):(\d{2})', raw)
    if not m:
        return None
    y, mo, d, h, mi, s = (int(x) for x in m.groups())
    sec = (((y * 372 + mo * 31 + d) * 24 + h) * 60 + mi) * 60 + s
    frac = 0.0
    for k in (0x9291, 0x9290):
        sv = tags.get(k)
        if sv:
            dg = ''.join(c for c in sv.decode('ascii', 'replace') if c.isdigit())
            if dg:
                frac = float('0.' + dg[:3])
                break
    return sec + frac


def head_time(path, nbytes=1 << 20):
    """Capture time (float seconds, arbitrary epoch) parsed from a 1 MB head read."""
    try:
        with open(path, 'rb') as f:
            buf = f.read(nbytes)
    except Exception:
        return None
    t = None
    hits = 0
    for m in TIFF_HDR.finditer(buf):
        t = _time_from_tiff(buf, m.start(), None)
        if t is not None:
            return t
        hits += 1
        if hits >= 4:
            break
    # CR3: preview JPEG EXIF may carry SubSec only; try any Exif APP1 elsewhere
    j = buf.find(b'Exif\x00\x00')
    if j > 0:
        t = _time_from_tiff(buf, j + 6, None)
    return t


def pick_preview(buf):
    cands = []
    i = 0
    while len(cands) < 8:
        m = SOI.search(buf, i)
        if not m:
            break
        i = m.start() + 3
        cands.append(m.start())
    best, best_area = None, 0
    for off in cands:
        try:
            im = Image.open(io.BytesIO(buf[off:]))
            im.load()
        except Exception:
            continue
        a = im.size[0] * im.size[1]
        if a > best_area:
            best, best_area = im, a
    return best


def preview_to_512(path, sizes=(4 << 20, 12 << 20, 0)):
    """Biggest decodable embedded JPEG -> EXIF-oriented RGB, 512 px long edge."""
    im = None
    for n in sizes:
        try:
            with open(path, 'rb') as f:
                buf = f.read() if n == 0 else f.read(n)
        except Exception:
            return None
        im = pick_preview(buf)
        if im is not None and max(im.size) >= 900:
            break
        if n == 0:
            break
    if im is None:
        return None
    try:
        im = ImageOps.exif_transpose(im) or im
        im = im.convert('RGB')
    except Exception:
        return None
    w, h = im.size
    sc = 512.0 / max(w, h)
    if sc < 1.0:
        im = im.resize((max(1, int(round(w * sc))), max(1, int(round(h * sc)))),
                       Image.BICUBIC)
    return im
