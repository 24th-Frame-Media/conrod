"""Train the shipped taste head: DINOv2-small CLS + per-frame handcrafted features.

The experiment (E:\\code\\photo\\conrod\\target\\taste) found its best in-burst ranker
with burst-centred features: position, within-burst ranks, the cosine to the burst
mean, and two no-reference IQA models. None of that survives into Conrod's
`score(&Rgb) -> Option<f32>` surface, which sees one frame at a time, so this head
is trained on **absolute** per-frame features only:

  * 384 DINOv2-small CLS dims (L2-normalised, as emb_dinov2.npy stores them)
  * log1p(Laplacian variance), luma mean, luma std, clipped-highlight fraction,
    clipped-shadow fraction, mean saturation, colourfulness, aspect

Dropped against the experiment, and why: burst position, within-burst ranks and
the burst-mean cosine (no burst context at scoring time), and the MUSIQ +
CLIP-IQA+ features (260 MB of extra ONNX and 320 ms/frame; the tower must stay
under 100 MB). Everything is standardised globally; the head is a listwise
conditional logit (softmax over each burst), so its per-frame logit reproduces
exactly the in-burst ordering the experiment evaluated.

  python scripts/taste/train_head.py [--lam 0.1] [--iters 400]

Reads the experiment's cached embeddings/features; recomputes DINOv2 embeddings
from the 512 px thumbs (batch 8) only when emb_dinov2.npy is missing.
Writes %USERPROFILE%\\.conrod\\models\\taste-head.json.
"""
import csv
import json
import os
import sys
import time
from collections import defaultdict

import numpy as np

sys.stdout.reconfigure(encoding='utf-8', errors='replace')

TASTE = r'E:\code\photo\conrod\target\taste'
THUMBS = os.path.join(TASTE, 'thumbs')
MODELS = os.path.join(os.environ.get('USERPROFILE', os.path.expanduser('~')), '.conrod', 'models')
HEAD = os.path.join(MODELS, 'taste-head.json')
TOWER = 'dinov2-small-tower.onnx'

# The absolute handcrafted columns, in feature_order. sharp_lap goes in as
# log1p; the rest are already bounded or small.
HC = ['sharp_lap', 'luma_mean', 'luma_std', 'clip_hi', 'clip_lo', 'sat_mean',
      'colourful', 'aspect']
LOG1P = {'sharp_lap'}

# Field-of-view DINOv2 preprocessing, as embed.py / similarity.rs: squash to
# 224x224 (no centre crop), ImageNet mean/std.
DINO_MEAN = (0.485, 0.456, 0.406)
DINO_STD = (0.229, 0.224, 0.225)
# Xenova's export writes the same weights, but the experiment used this one.
DINO_REPO = 'onnx-community--dinov2-small'


# ---------------------------------------------------------------- data
def frames():
    rows = list(csv.DictReader(open(os.path.join(TASTE, 'frames.csv'), encoding='utf-8')))
    rows.sort(key=lambda r: int(r['frame_id']))
    return rows


def handcrafted():
    out = {}
    for r in csv.DictReader(open(os.path.join(TASTE, 'features.csv'), encoding='utf-8')):
        vals = []
        for k in HC:
            v = float(r[k])
            vals.append(float(np.log1p(max(v, 0.0))) if k in LOG1P else v)
        out[int(r['frame_id'])] = vals
    return out


def tower_path():
    import glob
    hits = glob.glob(os.path.join(TASTE, 'models', '_hf', 'models--' + DINO_REPO,
                                  'snapshots', '*', 'onnx', 'model.onnx'))
    if not hits:
        raise SystemExit('missing DINOv2 ONNX under %s/models/_hf' % TASTE)
    return hits[0]


def recompute(fid):
    """The embeddings the experiment cached; only needed if the .npy is gone."""
    import onnxruntime as ort
    from PIL import Image
    so = ort.SessionOptions()
    so.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL
    so.intra_op_num_threads = 10
    sess = ort.InferenceSession(tower_path(), sess_options=so,
                                providers=['CPUExecutionProvider'])
    iname = sess.get_inputs()[0].name
    files = [os.path.join(THUMBS, '%05d.jpg' % f) for f in fid]
    out = np.zeros((len(files), 384), dtype=np.float32)
    t0 = time.time()
    for i in range(0, len(files), 8):
        x = []
        for p in files[i:i + 8]:
            im = Image.open(p).convert('RGB').resize((224, 224), Image.BICUBIC)
            a = np.asarray(im, dtype=np.float32) / 255.0
            x.append(np.transpose((a - DINO_MEAN) / DINO_STD, (2, 0, 1)))
        y = sess.run(['last_hidden_state'], {iname: np.stack(x)})[0][:, 0, :]
        out[i:i + len(x)] = y
        if (i // 8) % 80 == 0:
            print('  embedded %d/%d  %.0f ms/frame' % (min(len(files), i + 8), len(files),
                                                       1000 * (time.time() - t0) / max(1, min(len(files), i + 8))),
                  flush=True)
    out /= np.maximum(np.linalg.norm(out, axis=1, keepdims=True), 1e-8)
    return out


def load():
    rows = frames()
    fid = np.array([int(r['frame_id']) for r in rows])
    bid = np.array([int(r['burst_id']) for r in rows])
    bshoot = {}
    for r in csv.DictReader(open(os.path.join(TASTE, 'bursts.csv'), encoding='utf-8')):
        bshoot[int(r['burst_id'])] = r['shoot']
    shoot = np.array([bshoot[int(b)] for b in bid])
    delivered = np.array([int(r['delivered']) for r in rows])

    H = handcrafted()
    F = np.array([H[int(f)] for f in fid], dtype=np.float64)

    emb_file = os.path.join(TASTE, 'emb_dinov2.npy')
    ids_file = os.path.join(TASTE, 'emb_dinov2_ids.npy')
    if os.path.exists(emb_file) and os.path.exists(ids_file):
        E = np.load(emb_file)
        pos = {int(x): i for i, x in enumerate(np.load(ids_file))}
        E = E[[pos[int(f)] for f in fid]].astype(np.float64)
        print('embeddings: cached (%d x %d)' % E.shape)
    else:
        print('embeddings: recomputing from 512 px thumbs')
        E = recompute(fid).astype(np.float64)

    # Contiguous bursts -> segment offsets. frames.csv is grouped by burst_id.
    segs, i = [], 0
    while i < len(rows):
        j = i
        while j + 1 < len(rows) and bid[j + 1] == bid[i]:
            j += 1
        segs.append((i, j))
        i = j + 1
    by = np.array([int(np.argmax(delivered[a:b + 1])) for a, b in segs])
    bshoot = np.array([shoot[a] for a, b in segs])
    X = np.column_stack([E, F])
    return dict(X=X, segs=segs, by=by, shoot=shoot, bshoot=bshoot,
                n=len(rows), nb=len(segs), emb=E, F=F)


# ---------------------------------------------------------------- head
def _adam(grad, w, st, t, lr):
    st['m'] = 0.9 * st['m'] + 0.1 * grad
    st['v'] = 0.999 * st['v'] + 0.001 * grad * grad
    return w - lr * (st['m'] / (1 - 0.9 ** t)) / (np.sqrt(st['v'] / (1 - 0.999 ** t)) + 1e-8)


def group_softmax(z, gb, G):
    mx = np.full(G, -1e30)
    np.maximum.at(mx, gb, z)
    e = np.exp(z - mx[gb])
    s = np.zeros(G)
    np.add.at(s, gb, e)
    return e, s, e / s[gb]


def fit_cl(X, segs, by_local, lam, iters=400, lr=0.05):
    n = X.shape[0]
    gb = np.zeros(n, dtype=np.int64)
    for k, (a, b) in enumerate(segs):
        gb[a:b + 1] = k
    G = len(segs)
    w = np.zeros(X.shape[1])
    st = dict(m=np.zeros_like(w), v=np.zeros_like(w))
    gt = np.zeros(n)
    gt[np.asarray(by_local, dtype=np.int64)] = 1.0
    for t in range(1, iters + 1):
        _, _, p = group_softmax(X @ w, gb, G)
        w = _adam(X.T @ (p - gt) / G + lam * w, w, st, t, lr)
    return w


def to_local(segs, by, ks):
    loc, loc_by, m = [], [], 0
    for k in ks:
        a, b = segs[k]
        loc.append((m, m + b - a))
        loc_by.append(m + int(by[k]))
        m += b - a + 1
    return loc, loc_by


def rows_of(segs, ks):
    return np.concatenate([np.arange(segs[k][0], segs[k][1] + 1) for k in ks])


def hits(scores, segs, by, which):
    return np.array([int(np.argmax(scores[segs[k][0]:segs[k][1] + 1]) == by[k])
                     for k in which], dtype=float)


def evaluate(scores, segs, by, which):
    t1 = t3 = 0
    ranks = []
    for k in which:
        s = scores[segs[k][0]:segs[k][1] + 1]
        order = np.argsort(-s, kind='stable')
        h = int(np.where(order == by[k])[0][0])
        t1 += h == 0
        t3 += h < 3
        ranks.append(h)
    return dict(top1=t1 / len(which), top3=t3 / len(which),
                mrr=float(np.mean(1.0 / (np.array(ranks) + 1))), n=len(which))


def loo(d, X, lam, centre=False):
    segs, by, bshoot = d['segs'], d['by'], d['bshoot']
    shoots = sorted(set(bshoot.tolist()))
    scores = np.full(d['n'], np.nan)
    for ts in shoots:
        te = np.array([k for k in range(d['nb']) if bshoot[k] == ts])
        tr = np.array([k for k in range(d['nb']) if bshoot[k] != ts])
        Xr = X.copy()
        if centre:
            for a, b in segs:
                Xr[a:b + 1] -= Xr[a:b + 1].mean(axis=0)
        tr_rows, te_rows = rows_of(segs, tr), rows_of(segs, te)
        mu, sd = Xr[tr_rows].mean(0), Xr[tr_rows].std(0) + 1e-8
        loc, loc_by = to_local(segs, by, tr)
        w = fit_cl((Xr[tr_rows] - mu) / sd, loc, loc_by, lam)
        scores[te_rows] = ((Xr[te_rows] - mu) / sd) @ w
    return scores


def main():
    lam = 0.1
    iters = 400
    for i, a in enumerate(sys.argv):
        if a == '--lam':
            lam = float(sys.argv[i + 1])
        if a == '--iters':
            iters = int(sys.argv[i + 1])
    d = load()
    segs, by, bshoot = d['segs'], d['by'], d['bshoot']
    shoots = sorted(set(bshoot.tolist()))
    allb = np.arange(d['nb'])
    print('%d frames  %d bursts  %d shoots  mean burst %.2f'
          % (d['n'], d['nb'], len(shoots), np.mean([b - a + 1 for a, b in segs])))
    print('features: 384 DINOv2 CLS + %s' % ', '.join(HC))

    # Sharpness and random baselines on exactly these bursts.
    sharp = np.log1p(np.maximum(d['F'][:, 0], 0))
    rnd1 = float(np.mean([1.0 / (b - a + 1) for a, b in segs]))
    m = evaluate(sharp, segs, by, allb)
    print('\nBASELINES')
    print('  %-26s top1 %.3f  top3 %.3f  mrr %.3f' % ('Laplacian sharpness', m['top1'], m['top3'], m['mrr']))
    print('  %-26s top1 %.3f  top3 %.3f' % ('random', rnd1,
          float(np.mean([min(3, b - a + 1) / (b - a + 1) for a, b in segs]))))

    # Same head on the experiment's burst-centred embedding: should land at the
    # report's dinov2-s emb CL (0.253 / 0.599), which proves the training code
    # reproduces the experiment before it is applied to absolute features.
    ref = loo(d, d['emb'], lam, centre=True)
    m = evaluate(ref, segs, by, allb)
    print('\nREFERENCE (burst-centred emb, CL)')
    print('  %-26s top1 %.3f  top3 %.3f  mrr %.3f   (report: 0.253 / 0.599)'
          % ('dinov2-s emb CL', m['top1'], m['top3'], m['mrr']))

    scores = loo(d, d['X'], lam)
    m = evaluate(scores, segs, by, allb)
    print('\nSHIPPED HEAD (absolute emb + handcrafted, CL, lam %.3g)' % lam)
    print('  %-26s top1 %.3f  top3 %.3f  mrr %.3f   leave-one-shoot-out, %d bursts'
          % ('taste (this build)', m['top1'], m['top3'], m['mrr'], d['nb']))
    print('  report best (embh + IQA)   top1 0.265  top3 0.637  (not shippable: burst context + IQA)')

    # Fit the shipped weights on everything, with the same global standardisation.
    mu, sd = d['X'].mean(0), d['X'].std(0) + 1e-8
    loc, loc_by = to_local(segs, by, allb)
    w = fit_cl((d['X'] - mu) / sd, loc, loc_by, lam, iters=iters)
    z = ((d['X'] - mu) / sd) @ w
    print('  train-set top1 %.3f  logit range %.2f..%.2f  sigmoid %.3f..%.3f'
          % (evaluate(z, segs, by, allb)['top1'], z.min(), z.max(),
             1 / (1 + np.exp(-z.max())), 1 / (1 + np.exp(-z.min()))))

    os.makedirs(MODELS, exist_ok=True)
    order = ['emb%d' % i for i in range(d['emb'].shape[1])] + HC
    doc = {
        'version': 1,
        'design': 'dinov2-s absolute emb+handcrafted, listwise conditional logit',
        'tower': TOWER,
        'tower_preprocess': 'squash-224 ImageNet mean/std, CLS token, L2-normalised',
        'trained_on': int(d['n']),
        'bursts': int(d['nb']),
        'shoots': int(len(shoots)),
        'lam': lam,
        'feature_order': order,
        'mean': [float(v) for v in mu],
        'std': [float(v) for v in sd],
        'weights': [float(v) for v in w],
        'bias': 0.0,
        'squash': 'sigmoid',
        'loo': {k: float(v) for k, v in m.items()},
        'dropped': ['burst position', 'within-burst ranks', 'burst-mean cosine',
                    'MUSIQ', 'CLIP-IQA+'],
        'note': 'Score is a within-burst ranking signal, not a keeper pick.',
    }
    with open(HEAD, 'w', encoding='utf-8') as f:
        json.dump(doc, f, indent=1)
    print('\nwrote %s  %d features  %.1f KB' % (HEAD, len(order), os.path.getsize(HEAD) / 1024))


if __name__ == '__main__':
    main()
