"""K2: LAION improved-aesthetic-predictor on a genre-stratified burst subsample.

CLIP ViT-L/14 is far too slow on this CPU (measured ~1.1 s/frame) to embed all 6.7k frames,
so the aesthetic metric is evaluated on a stratified sample of whole bursts.
Writes aes_laion.csv and results_k2_aes.json
"""
import csv
import json
import os
import sys
import time
from collections import defaultdict

import numpy as np
import onnxruntime as ort
from safetensors.numpy import load_file

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.stdout.reconfigure(encoding='utf-8', errors='replace')
import embed as EM
import eval_taste as ET

TASTE = r'E:\code\photo\conrod\target\taste'
N_BURSTS = 120


def pick_bursts(rows, n_target=N_BURSTS, seed=11):
    byb = defaultdict(list)
    for r in rows:
        byb[int(r['burst_id'])].append(r)
    byg = defaultdict(list)
    for b, fr in byb.items():
        byg[fr[0]['genre']].append(b)
    rng = np.random.default_rng(seed)
    frac = min(1.0, n_target / len(byb))
    pick = []
    for g, bs in sorted(byg.items()):
        bs = sorted(bs)
        rng.shuffle(bs)
        k = max(2, int(round(len(bs) * frac)))
        pick.extend(bs[:k])
    return sorted(pick), byb


def main():
    rows = list(csv.DictReader(open(os.path.join(TASTE, 'frames.csv'), encoding='utf-8')))
    picks, byb = pick_bursts(rows)
    frames = [r for b in picks for r in sorted(byb[b], key=lambda r: int(r['pos']))]
    print('%d bursts, %d frames' % (len(picks), len(frames)))
    sp = EM.SPECS['vitl14']
    sess = EM.load(EM.model_path(sp['repo'], sp['nn']))
    iname = sess.get_inputs()[0].name
    E = np.zeros((len(frames), 768), dtype=np.float32)
    t0 = time.time()
    for i, r in enumerate(frames):
        p = os.path.join(TASTE, 'thumbs', '%05d.jpg' % int(r['frame_id']))
        x = EM.prep(p, sp['size'], sp['mean'], sp['std'])[None]
        E[i] = sess.run([sp['out']], {iname: x})[0][0]
        if (i + 1) % 100 == 0:
            el = time.time() - t0
            print('   %d/%d  %.0f ms/frame' % (i + 1, len(frames), 1000 * el / (i + 1)),
                  flush=True)
    ms = 1000 * (time.time() - t0) / len(frames)
    E /= np.maximum(np.linalg.norm(E, axis=1, keepdims=True), 1e-9)
    ids = np.array([int(r['frame_id']) for r in frames])
    np.save(os.path.join(TASTE, 'emb_vitl14_sub.npy'), E)
    np.save(os.path.join(TASTE, 'emb_vitl14_sub_ids.npy'), ids)
    json.dump({'ms_per_frame': ms, 'n': len(frames), 'bursts': len(picks), 'threads': 10},
              open(os.path.join(TASTE, 'emb_vitl14_sub.json'), 'w'), indent=1)

    MP = os.path.join(TASTE, 'models', '_hf',
                      'models--vivym--improved-aesthetic-predictor', 'snapshots')
    W = load_file(os.path.join(MP, os.listdir(MP)[0], 'mlp.safetensors'))
    h = E.astype(np.float64)
    for i in range(5):
        h = h @ W['%d.weight' % i].T + W['%d.bias' % i]
    score = h[:, 0]
    print('aesthetic score: mean %.2f sd %.2f range [%.2f, %.2f]  (%.0f ms/frame)'
          % (score.mean(), score.std(), score.min(), score.max(), ms))
    with open(os.path.join(TASTE, 'aes_laion.csv'), 'w', newline='', encoding='utf-8') as f:
        w = csv.writer(f)
        w.writerow(['frame_id', 'aes_laion'])
        for i, fid in enumerate(ids):
            w.writerow([int(fid), '%.5f' % score[i]])

    # evaluate on this subset, aligned to the burst structure
    d = ET.load(['clipb32'])
    fid = np.array([int(r['frame_id']) for r in d['frames']])
    sub = np.zeros(d['n'])
    have = np.isin(fid, ids)
    pos = {int(x): i for i, x in enumerate(ids)}
    for i, f in enumerate(fid):
        if int(f) in pos:
            sub[i] = score[pos[int(f)]]
    segs, by = d['segs'], d['by']
    which = np.array([k for k in range(d['nb'])
                      if have[segs[k][0]:segs[k][1] + 1].all()])
    print('evaluable bursts: %d' % len(which))
    sharp = d['H'][:, 0]
    out = {}
    for name, sc in [('aes_laion', sub), ('sharpness', np.log1p(np.maximum(sharp, 0)))]:
        m = ET.eval_scores(sc, segs, by, which)
        lo, hi = ET.boot_ci(ET.hits_from(sc, segs, by, which))
        out[name] = dict(m, ci=[lo, hi])
        print('  %-12s top1 %.3f [%.3f,%.3f] top3 %.3f' % (name, m['top1'], lo, hi, m['top3']))
    rnd = float(np.mean([1.0 / (segs[k][1] - segs[k][0] + 1) for k in which]))
    rnd3 = float(np.mean([min(3, segs[k][1] - segs[k][0] + 1) /
                          (segs[k][1] - segs[k][0] + 1) for k in which]))
    out['random'] = dict(top1=rnd, top3=rnd3)
    print('  %-12s top1 %.3f          top3 %.3f' % ('random', rnd, rnd3))
    out['meta'] = dict(bursts=len(which), frames=int(have.sum()), ms_per_frame=ms,
                       predictive=bool(out['aes_laion']['ci'][0] > rnd))
    json.dump(out, open(os.path.join(TASTE, 'results_k2_aes.json'), 'w'), indent=1)


if __name__ == '__main__':
    main()
