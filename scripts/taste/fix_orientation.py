"""Apply the RAW's EXIF Orientation to the cached 512 px thumbs (the embedded preview JPEGs
of CR3 files carry no Orientation tag, so we read it from the CMT1 IFD0 / CR2 IFD0).

Rewrites target/taste/thumbs/*.jpg in place and reports the orientation histogram.
"""
import csv
import os
import re
import struct
import sys
from collections import Counter
from concurrent.futures import ThreadPoolExecutor

from PIL import Image

sys.stdout.reconfigure(encoding='utf-8', errors='replace')
TASTE = r'E:\code\photo\conrod\target\taste'
TIFF_HDR = re.compile(rb'(II\x2a\x00|MM\x00\x2a)')
TR = {3: Image.ROTATE_180, 6: Image.ROTATE_270, 8: Image.ROTATE_90}


def orientation(path):
    try:
        with open(path, 'rb') as f:
            buf = f.read(1 << 20)
    except Exception:
        return 1
    for m in TIFF_HDR.finditer(buf):
        base = m.start()
        bo = {b'II': '<', b'MM': '>'}[buf[base:base + 2]]
        try:
            off = struct.unpack(bo + 'I', buf[base + 4:base + 8])[0]
            n = struct.unpack(bo + 'H', buf[base + off:base + off + 2])[0]
            for i in range(n):
                e = base + off + 2 + 12 * i
                tag, typ, cnt = struct.unpack(bo + 'HHI', buf[e:e + 8])
                if tag == 0x0112:
                    return struct.unpack(bo + 'H', buf[e + 8:e + 10])[0]
        except Exception:
            continue
    return 1


def work(r):
    fid = int(r['frame_id'])
    p = os.path.join(TASTE, 'thumbs', '%05d.jpg' % fid)
    o = orientation(r['path'])
    if o in TR:
        im = Image.open(p)
        im = im.transpose(TR[o])
        im.save(p, quality=92)
    return o


def main():
    rows = list(csv.DictReader(open(os.path.join(TASTE, 'frames.csv'), encoding='utf-8')))
    with ThreadPoolExecutor(max_workers=16) as ex:
        hist = list(ex.map(work, rows))
    c = Counter(hist)
    print('orientation histogram:', dict(c), 'rotated %.1f%%'
          % (100.0 * sum(v for k, v in c.items() if k != 1) / len(hist)))
    with open(os.path.join(TASTE, 'orientation.csv'), 'w', newline='', encoding='utf-8') as f:
        w = csv.writer(f)
        w.writerow(['frame_id', 'orientation'])
        for r, o in zip(rows, hist):
            w.writerow([r['frame_id'], o])


if __name__ == '__main__':
    main()
