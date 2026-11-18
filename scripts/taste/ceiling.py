"""A) Label-ceiling analysis: tolerant top-1, delivered-frame distinctness, miss contact sheet."""
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


def build_tolerant(segs, E, ths=(0.95, 0.97, 0.99)):
    """Per burst: tolerant index sets. E = L2-normalised embeddings (per frame)."""
    out = []
    for a, b in segs:
        n = b - a + 1
        emb = E[a:b + 1]
        cos = emb @ emb.T
        d = int(np.argmax(np.load(os.path.join(TASTE, 'preds_best.npy'))[a:b + 1] * 0
                          + 0) * 0) if False else None
        out.append({'n': n, 'cos': cos})
    return out


def main():
    d = ET.load(['dinov2'])
    segs, by, y = d['segs'], d['by'], d['y']
    E = d['emb']['dinov2']
    E = E / np.maximum(np.linalg.norm(E, axis=1, keepdims=True), 1e-9)
    preds = np.load(os.path.join(TASTE, 'preds_best.npy'))
    sharp = np.log1p(np.maximum(d['H'][:, 0], 0))
    size = np.array([b - a + 1 for a, b in segs])
    nb = d['nb']

    # per-burst cosine matrix + tolerant sets
    T = {'deliv': [], 'adj1': [], 'cos95': [], 'cos97': [], 'cos99': [],
         'adj1_or_cos97': []}
    maxcos = np.zeros(nb)
    for k, (a, b) in enumerate(segs):
        n = b - a + 1
        emb = E[a:b + 1]
        cos = emb @ emb.T
        drel = int(by[k])
        row = cos[drel]
        others = np.delete(row, drel)
        maxcos[k] = others.max() if len(others) else 1.0
        T['deliv'].append({drel})
        adj = {drel} | ({drel - 1} if drel > 0 else set()) | ({drel + 1} if drel < n - 1 else set())
        T['adj1'].append(adj)
        for th in (0.95, 0.97, 0.99):
            T['cos%d' % (th * 100)].append(set(np.where(row >= th)[0].tolist()) | {drel})
        T['adj1_or_cos97'].append(adj | set(np.where(row >= 0.97)[0].tolist()))

    scorers = {'random(exact)': None, 'sharpness': sharp, 'last-frame': None,
               'best head (dinov2+IQA CL)': preds}
    pos = np.zeros(d['n'])
    for k, (a, b) in enumerate(segs):
        pos[a:b + 1] = np.arange(b - a + 1)
    scorers['last-frame'] = pos.astype(float)

    print('=== tolerant top-1 (724 bursts, 95%% bootstrap CI) ===')
    print('%-26s %5s %7s %7s %7s %7s %7s %7s'
          % ('scorer', 'exact', 'adj+-1', 'cos95', 'cos97', 'cos99', 'adj|.97', 'meanT'))
    res = {}
    for name, sc in scorers.items():
        picks = []
        if name == 'random(exact)':
            picks = None
        else:
            for k, (a, b) in enumerate(segs):
                picks.append(a + int(np.argmax(sc[a:b + 1])))
        row = {}
        for key in ['deliv', 'adj1', 'cos95', 'cos97', 'cos99', 'adj1_or_cos97']:
            if picks is None:
                v = float(np.mean([len(T[key][k]) / size[k] for k in range(nb)]))
            else:
                v = float(np.mean([1.0 if (picks[k] - segs[k][0]) in T[key][k] else 0.0
                                   for k in range(nb)]))
            row[key] = v
        if picks is not None:
            ci = ET.boot_ci([1.0 if (picks[k] - segs[k][0]) in T['adj1_or_cos97'] else 0.0
                             for k in range(nb)])
            row['ci_adj_cos97'] = ci
            ci2 = ET.boot_ci([1.0 if (picks[k] - segs[k][0]) in T['cos97'] else 0.0
                              for k in range(nb)])
            row['ci_cos97'] = ci2
        row['mean_tolerant_size'] = float(np.mean([len(T['adj1_or_cos97'][k])
                                                   for k in range(nb)]))
        res[name] = row
        print('%-26s %5.3f %7.3f %7.3f %7.3f %7.3f %7.3f %7.1f'
              % (name, row['deliv'], row['adj1'], row['cos95'], row['cos97'],
                 row['cos99'], row['adj1_or_cos97'], row['mean_tolerant_size']))
    print('  best-head 95%% CI: cos97 %.3f-%.3f  adj|cos97 %.3f-%.3f'
          % (*res['best head (dinov2+IQA CL)']['ci_cos97'],
             *res['best head (dinov2+IQA CL)']['ci_adj_cos97']))

    # distinctness of the delivered frame
    print('\n=== delivered-frame distinctness (max DINOv2 cosine to burst-mates) ===')
    qs = np.percentile(maxcos, [10, 25, 50, 75, 90])
    print('  mean %.3f  p10 %.3f p25 %.3f median %.3f p75 %.3f p90 %.3f'
          % (maxcos.mean(), *qs))
    distinct = {}
    for th in (0.90, 0.95, 0.97, 0.99):
        distinct[th] = float((maxcos >= th).mean())
        print('  bursts where a burst-mate has cos >= %.2f : %.1f%%' % (th, 100 * distinct[th]))
    print('  (1 - that) = share of bursts where the delivered frame is unambiguous)')

    # exact top-1 conditioned on distinctness
    pick_rel = np.array([int(np.argmax(preds[a:b + 1])) for a, b in segs])
    hit = (pick_rel == by)
    print('\n=== best head exact top-1 by delivered-frame distinctness ===')
    cond = {}
    for lo, hi, lab in [(0.0, 0.95, 'max_cos < 0.95 (distinct)'),
                        (0.95, 0.97, '0.95-0.97'),
                        (0.97, 1.01, '>= 0.97 (near-duplicate)')]:
        m = (maxcos >= lo) & (maxcos < hi)
        if m.sum():
            cond[lab] = dict(n=int(m.sum()), top1=float(hit[m].mean()))
            print('  %-30s n=%3d  top-1 %.3f' % (lab, m.sum(), hit[m].mean()))

    json.dump({'tolerant': res, 'maxcos_percentiles': qs.tolist(),
               'maxcos_mean': float(maxcos.mean()),
               'share_near_dup': {str(k): v for k, v in distinct.items()},
               'conditioned_top1': cond},
              open(os.path.join(TASTE, 'ceiling.json'), 'w'), indent=1)

    # ---- contact sheet of misses
    rows = list(csv.DictReader(open(os.path.join(TASTE, 'frames.csv'), encoding='utf-8')))
    byb = defaultdict(list)
    for r in rows:
        byb[int(r['burst_id'])].append(r)
    misses = [k for k in range(nb) if not hit[k]]
    rng = np.random.default_rng(5)
    # spread the sample: small, medium, large bursts
    misses.sort(key=lambda k: size[k])
    idx = np.linspace(0, len(misses) - 1, 12).astype(int)
    sel = [misses[i] for i in idx]
    TW, TH = 210, 190
    PER = 6
    sheet = Image.new('RGB', (TW * PER, (TH + 18) * len(sel)), 'black')
    dr = ImageDraw.Draw(sheet)
    for r_i, k in enumerate(sel):
        a, b = segs[k]
        fr = sorted(byb[k], key=lambda r: int(r['pos']))
        d_rel = int(by[k])
        p_rel = int(np.argmax(preds[a:b + 1]))
        keep = list(range(max(0, d_rel - 2), min(len(fr), d_rel + 4)))[:PER]
        for j in range(PER - len(keep)):
            extra = [i for i in range(len(fr)) if i not in keep]
            if extra:
                keep.append(extra[0] if j == 0 else extra[-1])
        keep = sorted(set(keep))[:PER]
        for c, i in enumerate(keep):
            im = Image.open(os.path.join(TASTE, 'thumbs', '%05d.jpg'
                                         % int(fr[i]['frame_id'])))
            im.thumbnail((TW - 4, TH - 4))
            x = c * TW + (TW - im.size[0]) // 2
            sheet.paste(im, (x, r_i * (TH + 18) + 14))
            if i == d_rel:
                dr.rectangle([c * TW + 1, r_i * (TH + 18) + 15,
                              (c + 1) * TW - 2, r_i * (TH + 18) + 14 + TH - 2],
                             outline='lime', width=4)
                dr.text((c * TW + 5, r_i * (TH + 18) + 16), 'DELIVERED', fill='lime')
            if i == p_rel:
                dr.rectangle([c * TW + 3, r_i * (TH + 18) + 17,
                              (c + 1) * TW - 4, r_i * (TH + 18) + 12 + TH - 4],
                             outline='orange', width=2)
                dr.text((c * TW + 5, r_i * (TH + 18) + 30), 'head pick', fill='orange')
        dr.text((4, r_i * (TH + 18) + 2),
                'burst %d  %s  size %d  kept #%d of %d  picked #%d'
                % (k, fr[0]['genre'], len(fr), d_rel + 1, len(fr), p_rel + 1),
                fill='white')
    sheet.save(os.path.join(TASTE, 'ceiling_sheet.jpg'), quality=88)
    print('\nwrote ceiling_sheet.jpg', sheet.size, 'bursts', sel)


if __name__ == '__main__':
    main()
