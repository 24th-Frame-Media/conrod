"""K2: score each no-reference IQA / aesthetic metric alone and as extra burst-centred
features for the best K1 head.

  python eval_k2.py [--head clipb32|dinov2] [--kind emb|embh]
"""
import json
import os
import sys
from collections import defaultdict

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.stdout.reconfigure(encoding='utf-8', errors='replace')
import eval_taste as ET

TASTE = r'E:\code\photo\conrod\target\taste'


def load_iqa():
    import csv
    p = os.path.join(TASTE, 'iqa_scores.csv')
    rows = list(csv.DictReader(open(p, encoding='utf-8')))
    cols = [c for c in rows[0] if c != 'frame_id']
    data = {c: {} for c in cols}
    for r in rows:
        for c in cols:
            v = r[c]
            data[c][int(r['frame_id'])] = float(v) if v not in ('', 'nan') else np.nan
    return cols, data


def align(cols, data, fid):
    X = np.zeros((len(fid), len(cols)))
    for j, c in enumerate(cols):
        for i, f in enumerate(fid):
            X[i, j] = data[c].get(int(f), np.nan)
    return X


def main(tags=('dinov2',), best_kind='emb', best_head='CL'):
    d = ET.load(tags)
    segs, by, y = d['segs'], d['by'], d['y']
    fid = [int(r['frame_id']) for r in d['frames']]
    cols, data = load_iqa()
    X_iqa = align(cols, data, fid)
    # impute column mean, then rank-normalise
    for j in range(X_iqa.shape[1]):
        c = X_iqa[:, j]
        c[np.isnan(c)] = np.nanmean(c)
    shoots = sorted(set(d['bshoot'].tolist()))
    bs_index = defaultdict(list)
    for k, s in enumerate(segs):
        bs_index[d['bshoot'][k]].append(k)
    bs_index = {s: np.array(v) for s, v in bs_index.items()}
    allb = np.arange(d['nb'])
    res = {'alone': {}, 'extra': {}, 'meta': {'metrics': cols,
                                              'frames': d['n'], 'bursts': d['nb']}}

    print('K2 SINGLE-METRIC BASELINES (%d frames, %d bursts)' % (d['n'], d['nb']))
    for j, c in enumerate(cols):
        s = X_iqa[:, j]
        m = ET.eval_scores(s, segs, by, allb)
        lo, hi = ET.boot_ci(ET.hits_from(s, segs, by, allb))
        res['alone'][c] = dict(m, ci=[lo, hi])
        print('  %-14s top1 %.3f [%.3f,%.3f]  top3 %.3f  mrr %.3f'
              % (c, m['top1'], lo, hi, m['top3'], m['mrr']))

    # ---- best K1 head + IQA as extra centred features
    Xb = ET.build(d, best_kind, list(tags))
    Xi = ET.burst_centre(X_iqa, segs)
    X = np.column_stack([Xb, Xi])
    htype = best_head
    grid = [3e-3, 1e-2, 3e-2, 1e-1]
    folds = np.array_split(np.array(shoots, dtype=object), 5)

    def train_score(Xs, tr_b, te_b, lam):
        tr_rows = ET.rows_of(segs, tr_b)
        te_rows = ET.rows_of(segs, te_b)
        if htype == 'LR':
            w = ET.fit_lr(Xs[tr_rows], y[tr_rows], lam)
        else:
            loc, loc_by = ET.to_local(segs, by, tr_b)
            w = ET.fit_cl(Xs[tr_rows], loc, loc_by, lam)
        return Xs[te_rows] @ w, te_rows

    best = (1e-2, -1.0)
    for lam in grid:
        hit = []
        for f in folds:
            tr_b = np.array([k for s in shoots if s not in set(f.tolist())
                             for k in bs_index[s]])
            te_b = np.array([k for s in f.tolist() for k in bs_index[s]])
            tr_rows = ET.rows_of(segs, tr_b)
            mu, sd = X[tr_rows].mean(0), X[tr_rows].std(0) + 1e-8
            Xs = (X - mu) / sd
            sc = np.zeros(d['n'])
            s, rows = train_score(Xs, tr_b, te_b, lam)
            sc[rows] = s
            hit.append(ET.hits_from(sc, segs, by, te_b))
        acc = float(np.mean(np.concatenate(hit)))
        print('  lam %.3g cv-top1 %.3f' % (lam, acc))
        if acc > best[1]:
            best = (lam, acc)
    lam = best[0]
    preds = np.full(d['n'], np.nan)
    for ts in shoots:
        te_b, tr_b = bs_index[ts], np.array([k for s in shoots if s != ts
                                             for k in bs_index[s]])
        tr_rows = ET.rows_of(segs, tr_b)
        mu, sd = X[tr_rows].mean(0), X[tr_rows].std(0) + 1e-8
        Xs = (X - mu) / sd
        s, rows = train_score(Xs, tr_b, te_b, lam)
        preds[rows] = s
    m = ET.eval_scores(preds, segs, by, allb)
    lo, hi = ET.boot_ci(ET.hits_from(preds, segs, by, allb))
    res['extra']['%s+%s+IQA' % (best_kind, htype)] = dict(m, ci=[lo, hi], l2=lam)
    print('\n%s/%s + burst-centred IQA: top1 %.3f [%.3f,%.3f] top3 %.3f mrr %.3f (lam %.3g)'
          % (best_kind, htype, m['top1'], lo, hi, m['top3'], m['mrr'], lam))
    genres = sorted(set(d['bgenre'].tolist()))
    row = {}
    for g in genres:
        which = np.where(d['bgenre'] == g)[0]
        mm = ET.eval_scores(preds, segs, by, which)
        rr = float(np.mean([1.0 / (segs[k][1] - segs[k][0] + 1) for k in which]))
        row[g] = [mm['top1'], mm['top3'], mm['n'], rr]
        print('   %-18s top1 %.3f  n %d  (random %.3f)' % (g, mm['top1'], mm['n'], rr))
    res['extra'][list(res['extra'])[0]]['per_genre'] = row
    out = os.path.join(TASTE, 'results_k2_%s.json' % best_kind)
    json.dump(res, open(out, 'w'), indent=1, default=float)
    print('wrote', out)


if __name__ == '__main__':
    args = sys.argv[1:]
    tags = [a for a in args if a in ('clipb32', 'dinov2', 'vitl14')] or ['dinov2']
    kinds = [a for a in args if a in ('emb', 'embh')] or ['emb']
    for k in kinds:
        main(tags, k, 'CL')
