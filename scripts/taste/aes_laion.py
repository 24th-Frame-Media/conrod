"""K2: LAION improved-aesthetic-predictor MLP (linear 768-1024-128-64-16-1, no dropout
at inference) applied to the cached CLIP ViT-L/14 image embeddings.

The original LAION model.py has its ReLUs commented out, so this is an affine chain;
we verify that empirically (score range 1-10) and keep the linear form.
Writes target/taste/aes_laion.csv
"""
import csv
import os
import sys

import numpy as np

sys.stdout.reconfigure(encoding='utf-8', errors='replace')
TASTE = r'E:\code\photo\conrod\target\taste'
from safetensors.numpy import load_file

MP = os.path.join(TASTE, 'models', '_hf',
                  'models--vivym--improved-aesthetic-predictor', 'snapshots')
snap = os.listdir(MP)[0]
W = load_file(os.path.join(MP, snap, 'mlp.safetensors'))


def mlp(X, relu=False):
    h = X
    for i in range(5):
        h = h @ W['%d.weight' % i].T + W['%d.bias' % i]
        if relu and i < 4:
            h = np.maximum(h, 0)
    return h[:, 0]


def main():
    E = np.load(os.path.join(TASTE, 'emb_vitl14.npy')).astype(np.float64)
    ids = np.load(os.path.join(TASTE, 'emb_vitl14_ids.npy'))
    E /= np.maximum(np.linalg.norm(E, axis=1, keepdims=True), 1e-9)
    lin = mlp(E, relu=False)
    rel = mlp(E, relu=True)
    print('linear-form: mean %.2f sd %.2f  p1 %.2f p99 %.2f'
          % (lin.mean(), lin.std(), np.percentile(lin, 1), np.percentile(lin, 99)))
    print('relu-form  : mean %.2f sd %.2f  p1 %.2f p99 %.2f'
          % (rel.mean(), rel.std(), np.percentile(rel, 1), np.percentile(rel, 99)))
    # the trained model predicts MOS on a 1-10 scale: pick the variant that lands there
    variants = {'aes_laion': lin, 'aes_laion_relu': rel}
    for name, v in variants.items():
        ok = (2.0 < v.mean() < 8.0) and (np.percentile(v, 99) < 15)
        print('  %-16s plausible=%s' % (name, ok))
    pick = lin if (2.0 < lin.mean() < 8.0 and np.percentile(lin, 99) < 15) else rel
    print('using', 'linear' if pick is lin else 'relu')
    with open(os.path.join(TASTE, 'aes_laion.csv'), 'w', newline='', encoding='utf-8') as f:
        w = csv.writer(f)
        w.writerow(['frame_id', 'aes_laion'])
        for i, fid in enumerate(ids):
            w.writerow([int(fid), '%.5f' % pick[i]])
    print('wrote aes_laion.csv n=%d' % len(ids))


if __name__ == '__main__':
    main()
