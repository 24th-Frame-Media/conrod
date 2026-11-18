"""Extra ceiling numbers (CI on tolerant top-1) + a zoomed contact sheet of 4 misses."""
import csv
import json
import os
import sys
from collections import defaultdict

import numpy as np
from PIL import Image, ImageDraw

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.stdout.reconfigure(encoding='utf-8', errors='replace')
import eval_taste as ET

TASTE = r'E:\code\photo\conrod\target\taste'


def main():
    d = ET.load(['dinov2'])
    segs, by = d['segs'], d['by']
    E = d['emb']['dinov2']
    E = E / np.maximum(np.linalg.norm(E, axis=1, keepdims=True), 1e-9)
    preds = np.load(os.path.join(TASTE, 'preds_best.npy'))
    nb = d['nb']
    hits = {k: [] for k in ('adj1', 'cos95', 'cos97', 'cos99', 'adj_or_cos97')}
    for k, (a, b) in enumerate(segs):
        n = b - a + 1
        dr = int(by[k])
        cos = (E[a:b + 1] @ E[a:b + 1].T)[dr]
        adj = {dr} | ({dr - 1} if dr > 0 else set()) | ({dr + 1} if dr < n - 1 else set())
        pr = int(np.argmax(preds[a:b + 1]))
        hits['adj1'].append(float(pr in adj))
        for th in (0.95, 0.97, 0.99):
            s = set(np.where(cos >= th)[0].tolist()) | {dr}
            hits['cos%d' % (th * 100)].append(float(pr in s))
            if th == 0.97:
                hits['adj_or_cos97'].append(float(pr in (s | adj)))
    print('best head, tolerant top-1 with 95%% CI:')
    for k, v in hits.items():
        lo, hi = ET.boot_ci(v)
        print('  %-14s %.3f [%.3f, %.3f]' % (k, float(np.mean(v)), lo, hi))

    # zoomed sheet: 4 misses with the most frames shown
    rows = list(csv.DictReader(open(os.path.join(TASTE, 'frames.csv'), encoding='utf-8')))
    byb = defaultdict(list)
    for r in rows:
        byb[int(r['burst_id'])].append(r)
    sizes = np.array([b - a + 1 for a, b in segs])
    pick_rel = np.array([int(np.argmax(preds[a:b + 1])) for a, b in segs])
    miss = [k for k in range(nb) if pick_rel[k] != by[k]]
    want = [346, 141, 181, 689]
    sel = [k for k in want if k in miss][:4] or miss[:4]
    TW = 300
    rows_img = []
    for k in sel:
        fr = sorted(byb[k], key=lambda r: int(r['pos']))[:8]
        rows_img.append((k, fr))
    H = sum(320 for _ in rows_img)
    W = TW * max(len(fr) for _, fr in rows_img)
    sheet = Image.new('RGB', (W, H), 'black')
    dr_ = ImageDraw.Draw(sheet)
    y = 0
    for k, fr in rows_img:
        dr_.text((4, y + 2), 'burst %d  %s  size %d  delivered #%d  head picked #%d'
                 % (k, fr[0]['genre'], sizes[k], int(by[k]) + 1, pick_rel[k] + 1),
                 fill='white')
        for c, r in enumerate(fr):
            im = Image.open(os.path.join(TASTE, 'thumbs', '%05d.jpg' % int(r['frame_id'])))
            im.thumbnail((TW - 6, 300))
            sheet.paste(im, (c * TW + 3, y + 16))
            if c == int(by[k]):
                dr_.rectangle([c * TW + 2, y + 17, (c + 1) * TW - 4, y + 315],
                              outline='lime', width=5)
            if c == pick_rel[k]:
                dr_.rectangle([c * TW + 6, y + 21, (c + 1) * TW - 8, y + 311],
                              outline='orange', width=3)
        y += 320
    out = os.path.join(TASTE, 'ceiling_zoom.jpg')
    sheet.save(out, quality=90)
    print('wrote', out, sheet.size, 'bursts', sel)


if __name__ == '__main__':
    main()
