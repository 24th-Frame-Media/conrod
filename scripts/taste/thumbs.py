"""Pass B: 512 px previews + handcrafted per-frame features for the selected bursts.

Writes target/taste/thumbs/<frame_id>.jpg and target/taste/features.csv
"""
import csv
import os
import sys
import time
from concurrent.futures import ThreadPoolExecutor

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.stdout.reconfigure(encoding='utf-8', errors='replace')
import rawlib

TASTE = r'E:\code\photo\conrod\target\taste'
THUMBS = os.path.join(TASTE, 'thumbs')
os.makedirs(THUMBS, exist_ok=True)

LAP = None


def handcrafted(im):
    a = np.asarray(im.convert('RGB'), dtype=np.float32) / 255.0
    luma = 0.299 * a[..., 0] + 0.587 * a[..., 1] + 0.114 * a[..., 2]
    # 5-point laplacian variance (scale-fixed: all previews are 512 px long edge)
    lap = (luma[:-2, 1:-1] + luma[2:, 1:-1] + luma[1:-1, :-2] + luma[1:-1, 2:]
           - 4.0 * luma[1:-1, 1:-1])
    sharp = float(lap.var())
    mx = a.max(axis=2)
    mn = a.min(axis=2)
    sat = (mx - mn) / np.maximum(mx, 1e-6)
    colourfulness = float(np.sqrt(a[..., 0].std() ** 2 + a[..., 1].std() ** 2
                                   + a[..., 2].std() ** 2) + 0.3 * np.sqrt(
        (a[..., 0].mean() - a[..., 1].mean()) ** 2
        + (a[..., 0].mean() - a[..., 2].mean()) ** 2
        + (a[..., 1].mean() - a[..., 2].mean()) ** 2))
    return {
        'sharp_lap': sharp,
        'luma_mean': float(luma.mean()),
        'luma_std': float(luma.std()),
        'clip_hi': float((luma > 0.97).mean()),
        'clip_lo': float((luma < 0.02).mean()),
        'sat_mean': float(sat.mean()),
        'colourful': colourfulness,
        'aspect': a.shape[1] / a.shape[0],
    }


def work(row):
    fid, path = int(row['frame_id']), row['path']
    out = os.path.join(THUMBS, '%05d.jpg' % fid)
    t0 = time.time()
    if os.path.exists(out) and os.path.getsize(out) > 2000:
        from PIL import Image
        im = Image.open(out)
        im.load()
    else:
        im = rawlib.preview_to_512(path)
        if im is None:
            return fid, None, 0.0
        im.save(out, quality=92, optimize=False)
    feats = handcrafted(im)
    return fid, feats, (time.time() - t0)


def main():
    rows = list(csv.DictReader(open(os.path.join(TASTE, 'frames.csv'), encoding='utf-8')))
    print('frames', len(rows))
    res = {}
    t0 = time.time()
    times = []
    with ThreadPoolExecutor(max_workers=8) as ex:
        for i, (fid, feats, dt) in enumerate(ex.map(work, rows)):
            if feats is None:
                print('FAIL', rows[i]['path'])
                continue
            res[fid] = feats
            times.append(dt)
            if (i + 1) % 200 == 0:
                el = time.time() - t0
                print('  %d/%d  %.1fs elapsed  %.0f/1000 done  eta %.0f min'
                      % (i + 1, len(rows), el, i + 1, (len(rows) - i - 1) * el / (i + 1) / 60),
                      flush=True)
    keys = ['sharp_lap', 'luma_mean', 'luma_std', 'clip_hi', 'clip_lo', 'sat_mean',
            'colourful', 'aspect']
    with open(os.path.join(TASTE, 'features.csv'), 'w', newline='', encoding='utf-8') as f:
        w = csv.writer(f)
        w.writerow(['frame_id'] + keys)
        for fid in sorted(res):
            w.writerow([fid] + ['%.6f' % res[fid][k] for k in keys])
    print('ok %d frames, median %.3fs/frame (8 threads), total %.1f min'
          % (len(res), float(np.median(times)), (time.time() - t0) / 60))


if __name__ == '__main__':
    main()
