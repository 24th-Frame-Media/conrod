"""Heads + leave-one-shoot-out evaluation for the burst-taste experiment.

  python eval_taste.py clipb32 [dinov2 ...]

Heads:  LR  = logistic regression on burst-centred features
        CL  = listwise conditional logit (softmax over the burst), numpy Adam + L2
Baselines: random, sharpness (Laplacian variance), burst position, single features.
"""
import csv
import json
import os
import sys
from collections import defaultdict

import numpy as np

sys.stdout.reconfigure(encoding='utf-8', errors='replace')
TASTE = r'E:\code\photo\conrod\target\taste'

HC = ['sharp_lap', 'luma_mean', 'luma_std', 'clip_hi', 'clip_lo', 'sat_mean',
      'colourful', 'aspect']


# ---------------------------------------------------------------- data
def load(tags):
    frames = list(csv.DictReader(open(os.path.join(TASTE, 'frames.csv'), encoding='utf-8')))
    frames.sort(key=lambda r: int(r['frame_id']))
    feats = {}
    for r in csv.DictReader(open(os.path.join(TASTE, 'features.csv'), encoding='utf-8')):
        feats[int(r['frame_id'])] = [float(r[k]) for k in HC]
    n = len(frames)
    fid = np.array([int(r['frame_id']) for r in frames])
    H = np.array([feats[int(f)] for f in fid], dtype=np.float64)

    emb = {}
    for t in tags:
        e = np.load(os.path.join(TASTE, 'emb_%s.npy' % t))
        ids = np.load(os.path.join(TASTE, 'emb_%s_ids.npy' % t))
        pos = {int(x): i for i, x in enumerate(ids)}
        E = np.zeros((n, e.shape[1]), dtype=np.float64)
        for i, f in enumerate(fid):
            E[i] = e[pos[int(f)]]
        emb[t] = E

    bid = np.array([int(r['burst_id']) for r in frames])
    bmeta = {}
    for r in csv.DictReader(open(os.path.join(TASTE, 'bursts.csv'), encoding='utf-8')):
        bmeta[int(r['burst_id'])] = r['shoot']
    shoot = np.array([bmeta[int(r['burst_id'])] for r in frames])
    genre = np.array([r['genre'] for r in frames])
    client = np.array([r['client'] for r in frames])
    y = np.array([int(r['delivered']) for r in frames])
    segs = []
    i = 0
    while i < n:
        j = i
        while j + 1 < n and bid[j + 1] == bid[i]:
            j += 1
        segs.append((i, j))
        i = j + 1
    segs = np.array(segs)
    bshoot = np.array([shoot[a] for a, b in segs])
    bgenre = np.array([genre[a] for a, b in segs])
    bclient = np.array([client[a] for a, b in segs])
    by = np.array([int(np.argmax(y[a:b + 1])) for a, b in segs])
    return dict(frames=frames, H=H, emb=emb, segs=segs, shoot=shoot, genre=genre,
                client=client, y=y, bshoot=bshoot, bgenre=bgenre, bclient=bclient,
                by=by, n=n, nb=len(segs))


def burst_centre(X, segs):
    out = np.zeros_like(X, dtype=np.float64)
    for a, b in segs:
        out[a:b + 1] = X[a:b + 1] - X[a:b + 1].mean(axis=0)
    return out


def rank_features(d):
    """burst-relative (centred) handcrafted + within-burst ranks + position globals."""
    H = d['H']
    Hc = burst_centre(H, d['segs'])
    n = d['n']
    size = np.zeros(n)
    pos = np.zeros(n)
    for a, b in d['segs']:
        s = b - a + 1
        size[a:b + 1] = s
        pos[a:b + 1] = np.arange(s)
    ranks = []
    for k in range(len(HC)):
        r = np.zeros(n)
        for a, b in d['segs']:
            v = H[a:b + 1, k]
            r[a:b + 1] = np.argsort(np.argsort(v)) / max(1, len(v) - 1)
        ranks.append(r)
    cols = [pos / np.maximum(size - 1, 1), (pos == 0).astype(float),
            (pos == size - 1).astype(float), np.log1p(size) / 5.0]
    logH = Hc.copy()
    logH[:, 0] = np.log1p(np.maximum(H[:, 0], 0)) - np.log1p(np.maximum(H[:, 0], 0)).mean()
    return np.column_stack([logH] + ranks + cols)


def build(d, kind, tags):
    parts = [burst_centre(d['emb'][t], d['segs']) for t in tags]
    if kind in ('embh', 'embhpos'):
        parts.append(rank_features(d))
        # typicality: cosine similarity to the burst-mean embedding (1 column per tower)
        for t in tags:
            E = d['emb'][t]
            mu = np.zeros_like(E)
            for a, b in d['segs']:
                mu[a:b + 1] = E[a:b + 1].mean(axis=0)
            num = (E * mu).sum(1)
            den = np.linalg.norm(E, axis=1) * np.linalg.norm(mu, axis=1) + 1e-8
            parts.append((num / den)[:, None])
    return np.column_stack(parts)


# ---------------------------------------------------------------- heads
def _adam(grad, w, st, t, lr):
    g = grad
    st['m'] = 0.9 * st['m'] + 0.1 * g
    st['v'] = 0.999 * st['v'] + 0.001 * g * g
    mh = st['m'] / (1 - 0.9 ** t)
    vh = st['v'] / (1 - 0.999 ** t)
    return w - lr * mh / (np.sqrt(vh) + 1e-8)


def fit_lr(X, y, lam, iters=400, lr=0.05):
    w = np.zeros(X.shape[1])
    st = dict(m=np.zeros_like(w), v=np.zeros_like(w))
    yf = y.astype(np.float64)
    n = max(1, len(y))
    for t in range(1, iters + 1):
        p = 1.0 / (1.0 + np.exp(-np.clip(X @ w, -30, 30)))
        w = _adam(X.T @ (p - yf) / n + lam * w, w, st, t, lr)
    return w


def group_softmax(z, gb, G):
    mx = np.full(G, -1e30)
    np.maximum.at(mx, gb, z)
    e = np.exp(z - mx[gb])
    s = np.zeros(G)
    np.add.at(s, gb, e)
    return e, s, e / s[gb]


def fit_cl(X, segs, byloc, lam, iters=400, lr=0.05):
    """Conditional logit: each burst is a softmax group; label = delivered frame."""
    n = X.shape[0]
    gb = np.zeros(n, dtype=np.int64)
    for k, (a, b) in enumerate(segs):
        gb[a:b + 1] = k
    G = len(segs)
    byloc = np.asarray(byloc, dtype=np.int64)
    w = np.zeros(X.shape[1])
    st = dict(m=np.zeros_like(w), v=np.zeros_like(w))
    for t in range(1, iters + 1):
        e, s, p = group_softmax(X @ w, gb, G)
        gt = np.zeros(n)
        gt[byloc] = 1.0
        w = _adam(X.T @ (p - gt) / G + lam * w, w, st, t, lr)
    return w


def to_local(segs, by, ks):
    """Local (offset) segment list + local delivered index for a subset of bursts."""
    loc, loc_by, m = [], [], 0
    for k in ks:
        a, b = segs[k]
        loc.append((m, m + b - a))
        loc_by.append(m + int(by[k]))
        m += b - a + 1
    return loc, loc_by


def rows_of(segs, ks):
    return np.concatenate([np.arange(segs[k][0], segs[k][1] + 1) for k in ks])


# ---------------------------------------------------------------- metrics
def eval_scores(scores, segs, by, which):
    t1 = t3 = 0
    rs = []
    for k in which:
        a, b = segs[k]
        s = scores[a:b + 1]
        order = np.argsort(-s, kind='stable')
        hit = np.where(order == by[k])[0]
        h = int(hit[0]) if len(hit) else 10 ** 6
        t1 += int(h == 0)
        t3 += int(h < 3)
        rs.append(h)
    rs = np.array(rs, dtype=float)
    return dict(top1=float(t1 / len(which)), top3=float(t3 / len(which)),
                mrr=float(np.mean(1.0 / (rs + 1))), n=int(len(which)))


def hits_from(scores, segs, by, which):
    return np.array([int(np.argmax(scores[segs[k][0]:segs[k][1] + 1]) == by[k])
                     for k in which], dtype=float)


def boot_ci(h, B=2000, seed=0):
    rng = np.random.default_rng(seed)
    n = len(h)
    idx = rng.integers(0, n, size=(B, n))
    m = np.asarray(h)[idx].mean(axis=1)
    return float(np.percentile(m, 2.5)), float(np.percentile(m, 97.5))


# ---------------------------------------------------------------- main
def main(tags):
    d = load(tags)
    segs, by, y = d['segs'], d['by'], d['y']
    shoots = sorted(set(d['bshoot'].tolist()))
    bs_index = defaultdict(list)
    for k, s in enumerate(segs):
        bs_index[d['bshoot'][k]].append(k)
    bs_index = {s: np.array(v) for s, v in bs_index.items()}
    allb = np.arange(d['nb'])
    print('frames %d  bursts %d  shoots %d  mean size %.2f'
          % (d['n'], d['nb'], len(shoots), np.mean([b - a + 1 for a, b in segs])))

    sharp = d['H'][:, 0]
    size = np.zeros(d['n'])
    pos = np.zeros(d['n'])
    for a, b in segs:
        size[a:b + 1] = b - a + 1
        pos[a:b + 1] = np.arange(b - a + 1)
    base_scores = {
        'sharpness (Laplacian)': np.log1p(np.maximum(sharp, 0)),
        'last frame in burst': pos.astype(float),
        'first frame in burst': -pos.astype(float),
        'brightest (luma mean)': d['H'][:, 1],
        'least clipped highlights': -d['H'][:, 3],
        'most colourful': d['H'][:, 6],
    }
    rnd1 = float(np.mean([1.0 / (b - a + 1) for a, b in segs]))
    rnd3 = float(np.mean([min(3, b - a + 1) / (b - a + 1) for a, b in segs]))
    res = {'meta': dict(frames=d['n'], bursts=d['nb'], shoots=len(shoots),
                        mean_size=float(np.mean([b - a + 1 for a, b in segs])),
                        random_top1=rnd1, random_top3=rnd3),
           'baselines': {}, 'heads': {}, 'per_genre': {}, 'curve': {}}

    print('\nBASELINES')
    for name, sc in base_scores.items():
        m = eval_scores(sc, segs, by, allb)
        lo, hi = boot_ci(hits_from(sc, segs, by, allb))
        res['baselines'][name] = dict(m, ci=[lo, hi])
        print('  %-24s top1 %.3f [%.3f,%.3f]  top3 %.3f  mrr %.3f'
              % (name, m['top1'], lo, hi, m['top3'], m['mrr']))
    print('  %-24s top1 %.3f          top3 %.3f' % ('random', rnd1, rnd3))

    heads = {}
    for t in tags:
        for kind in ('emb', 'embh'):
            heads['%s|%s|LR' % (t, kind)] = ('LR', t, kind)
            heads['%s|%s|CL' % (t, kind)] = ('CL', t, kind)

    def train_score(htype, Xs, tr_b, te_b, lam):
        tr_rows = rows_of(segs, tr_b)
        te_rows = rows_of(segs, te_b)
        if htype == 'LR':
            w = fit_lr(Xs[tr_rows], y[tr_rows], lam)
        else:
            loc, loc_by = to_local(segs, by, tr_b)
            w = fit_cl(Xs[tr_rows], loc, loc_by, lam)
        return Xs[te_rows] @ w, te_rows

    # ---- L2 via 5 grouped folds (shoots as groups)
    grid = [3e-3, 1e-2, 3e-2, 1e-1]
    folds = np.array_split(np.array(shoots, dtype=object), 5)
    lam_choice = {}
    print('\nL2 SELECTION (5 grouped folds)')
    for name, (htype, t, kind) in heads.items():
        X = build(d, kind, [t])
        best = (1e-2, -1.0)
        for lam in grid:
            hit = []
            for f in folds:
                tr_b = np.array([k for s in shoots if s not in set(f.tolist())
                                 for k in bs_index[s]])
                te_b = np.array([k for s in f.tolist() for k in bs_index[s]])
                mu, sd = X[rows_of(segs, tr_b)].mean(0), X[rows_of(segs, tr_b)].std(0) + 1e-8
                Xs = (X - mu) / sd
                sc = np.zeros(d['n'])
                s, rows = train_score(htype, Xs, tr_b, te_b, lam)
                sc[rows] = s
                hit.append(hits_from(sc, segs, by, te_b))
            acc = float(np.mean(np.concatenate(hit)))
            if acc > best[1]:
                best = (lam, acc)
        lam_choice[name] = best
        print('  %-34s lam %-7.3g cv-top1 %.3f' % (name, best[0], best[1]))

    # ---- leave-one-shoot-out
    print('\nLEAVE-ONE-SHOOT-OUT')
    preds = {k: np.full(d['n'], np.nan) for k in heads}
    Xs_cache = {}
    for name, (htype, t, kind) in heads.items():
        Xs_cache[name] = build(d, kind, [t])
    for si, ts in enumerate(shoots):
        te_b = bs_index[ts]
        tr_b = np.array([k for s in shoots if s != ts for k in bs_index[s]])
        tr_rows = rows_of(segs, tr_b)
        for name, (htype, t, kind) in heads.items():
            X = Xs_cache[name]
            mu, sd = X[tr_rows].mean(0), X[tr_rows].std(0) + 1e-8
            Xs = (X - mu) / sd
            s, rows = train_score(htype, Xs, tr_b, te_b, lam_choice[name][0])
            preds[name][rows] = s
        if (si + 1) % 5 == 0:
            print('  fold %d/%d' % (si + 1, len(shoots)), flush=True)

    for name in heads:
        m = eval_scores(preds[name], segs, by, allb)
        lo, hi = boot_ci(hits_from(preds[name], segs, by, allb))
        res['heads'][name] = dict(m, ci=[lo, hi])
        print('  %-34s top1 %.3f [%.3f,%.3f]  top3 %.3f  mrr %.3f'
              % (name, m['top1'], lo, hi, m['top3'], m['mrr']))

    print('\nPER GENRE (top1/n)')
    genres = sorted(set(d['bgenre'].tolist()))
    names = list(base_scores) + list(heads)
    print('  %-34s' % 'head' + ''.join('%15s' % g[:14] for g in genres) + '%15s' % 'random')
    for name in names:
        sc = base_scores[name] if name in base_scores else preds[name]
        row = {}
        for g in genres:
            which = np.where(d['bgenre'] == g)[0]
            mm = eval_scores(sc, segs, by, which)
            rr = float(np.mean([1.0 / (segs[k][1] - segs[k][0] + 1) for k in which]))
            row[g] = [mm['top1'], mm['top3'], mm['n'], rr]
        res['per_genre'][name] = row
        print('  %-34s' % name + ''.join('%8.3f/%-6d' % (row[g][0], row[g][2])
                                         for g in genres))
    print('  %-34s' % 'random' + ''.join('%8.3f/%-6d' % (res['per_genre'][names[0]][g][3],
                                                         res['per_genre'][names[0]][g][2])
                                         for g in genres))

    # ---- learning curve on the best head
    best_name = max(heads, key=lambda k: res['heads'][k]['top1'])
    htype, t, kind = heads[best_name]
    X = Xs_cache[best_name]
    print('\nLEARNING CURVE (%s)' % best_name)
    rng = np.random.default_rng(3)
    for frac, reps in [(0.1, 3), (0.3, 3), (1.0, 1)]:
        a1, a3 = [], []
        for rep in range(reps):
            h1, h3, tot = [], [], 0
            for ts in shoots:
                te_b = bs_index[ts]
                pool = [s for s in shoots if s != ts]
                k = max(2, int(round(frac * len(pool))))
                pick = np.sort(rng.choice(len(pool), size=min(k, len(pool)), replace=False))
                tr_b = np.array([x for i in pick for x in bs_index[pool[i]]])
                tr_rows = rows_of(segs, tr_b)
                mu, sd = X[tr_rows].mean(0), X[tr_rows].std(0) + 1e-8
                Xs = (X - mu) / sd
                s, rows = train_score(htype, Xs, tr_b, te_b, lam_choice[best_name][0])
                sc = np.zeros(d['n'])
                sc[rows] = s
                mm = eval_scores(sc, segs, by, te_b)
                h1.append(mm['top1'] * len(te_b))
                h3.append(mm['top3'] * len(te_b))
                tot += len(te_b)
            a1.append(sum(h1) / tot)
            a3.append(sum(h3) / tot)
        res['curve']['%.0f%%' % (frac * 100)] = dict(
            top1=float(np.mean(a1)), top3=float(np.mean(a3)),
            top1_sd=float(np.std(a1)))
        print('  %4.0f%% of shoots: top1 %.3f +-%.3f  top3 %.3f'
              % (frac * 100, np.mean(a1), np.std(a1), np.mean(a3)))

    res['meta']['best_head'] = best_name
    res['meta']['l2'] = {k: v[0] for k, v in lam_choice.items()}
    tm = {}
    for tg in tags:
        p = os.path.join(TASTE, 'emb_%s.json' % tg)
        if os.path.exists(p):
            tm[tg] = json.load(open(p))['ms_per_frame']
    res['meta']['ms_per_frame'] = tm
    out = os.path.join(TASTE, 'results_%s.json' % '_'.join(tags))
    json.dump(res, open(out, 'w'), indent=1, default=float)
    print('\nwrote', out)


if __name__ == '__main__':
    main(sys.argv[1:])
