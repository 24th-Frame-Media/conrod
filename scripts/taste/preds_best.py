"""Best K1+K2 head (DINOv2-small embh + 3 IQA, listwise CL, L2=0.1) -> per-frame scores.

Saves target/taste/preds_best.npy (one score per frame, LOO-shoot) and preds_best_meta.json.
"""
import json
import os
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.stdout.reconfigure(encoding='utf-8', errors='replace')
import eval_taste as ET
import eval_k2 as EK

TASTE = r'E:\code\photo\conrod\target\taste'
LAM = 0.1


def main():
    d = ET.load(['dinov2'])
    segs, by, y = d['segs'], d['by'], d['y']
    fid = [int(r['frame_id']) for r in d['frames']]
    cols, data = EK.load_iqa()
    Xi = EK.align(cols, data, fid)
    for j in range(Xi.shape[1]):
        c = Xi[:, j]
        c[np.isnan(c)] = np.nanmean(c)
    X = np.column_stack([ET.build(d, 'embh', ['dinov2']), ET.burst_centre(Xi, segs)])
    shoots = sorted(set(d['bshoot'].tolist()))
    idx = {}
    for k, s in enumerate(segs):
        idx.setdefault(d['bshoot'][k], []).append(k)
    preds = np.full(d['n'], np.nan)
    for ts in shoots:
        te_b = np.array(idx[ts])
        tr_b = np.array([k for s in shoots if s != ts for k in idx[s]])
        tr_rows = ET.rows_of(segs, tr_b)
        mu, sd = X[tr_rows].mean(0), X[tr_rows].std(0) + 1e-8
        Xs = (X - mu) / sd
        loc, loc_by = ET.to_local(segs, by, tr_b)
        w = ET.fit_cl(Xs[tr_rows], loc, loc_by, LAM)
        te_rows = ET.rows_of(segs, te_b)
        preds[te_rows] = Xs[te_rows] @ w
    np.save(os.path.join(TASTE, 'preds_best.npy'), preds)
    m = ET.eval_scores(preds, segs, by, np.arange(d['nb']))
    json.dump(dict(m, lam=LAM, design='dinov2|embh+IQA|CL'), open(
        os.path.join(TASTE, 'preds_best_meta.json'), 'w'), indent=1)
    print('best head LOO:', m)


if __name__ == '__main__':
    main()
