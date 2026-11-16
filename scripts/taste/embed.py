"""Embed the 512 px thumbnails with an ONNX image tower -> float32 .npy (L2-normalised).

  python embed.py clipb32|dinov2|vitl14
"""
import glob
import json
import os
import sys
import time

import numpy as np
import onnxruntime as ort
from PIL import Image

sys.stdout.reconfigure(encoding='utf-8', errors='replace')

TASTE = r'E:\code\photo\conrod\target\taste'
THUMBS = os.path.join(TASTE, 'thumbs')
MROOT = os.path.join(TASTE, 'models', '_hf')
BATCH = 8

SPECS = {
    'clipb32': dict(repo='Qdrant--clip-ViT-B-32-vision', nn='model.onnx',
                    size=224, out='image_embeds',
                    mean=(0.48145466, 0.4578275, 0.40821073),
                    std=(0.26862954, 0.26130258, 0.27577711)),
    'dinov2': dict(repo='onnx-community--dinov2-small', nn='onnx/model.onnx',
                   size=224, out='last_hidden_state', cls=True,
                   mean=(0.485, 0.456, 0.406), std=(0.229, 0.224, 0.225)),
    'vitl14': dict(repo='immich-app--ViT-L-14__laion400m_e32', nn='visual/model.onnx',
                   size=224, out='embedding',
                   mean=(0.48145466, 0.4578275, 0.40821073),
                   std=(0.26862954, 0.26130258, 0.27577711)),
}


def model_path(repo, nn):
    hits = glob.glob(os.path.join(MROOT, 'models--' + repo, 'snapshots', '*',
                                 nn.replace('/', os.sep)))
    if not hits:
        raise SystemExit('missing model %s/%s' % (repo, nn))
    return hits[0]


def load(path):
    so = ort.SessionOptions()
    so.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL
    so.intra_op_num_threads = int(os.environ.get(
        'ORT_THREADS', max(2, (os.cpu_count() or 4) // 2)))
    return ort.InferenceSession(path, sess_options=so, providers=['CPUExecutionProvider'])


def prep(path, size, mean, std):
    im = Image.open(path).convert('RGB').resize((size, size), Image.BICUBIC)
    a = np.asarray(im, dtype=np.float32) / 255.0
    a = (a - np.array(mean, dtype=np.float32)) / np.array(std, dtype=np.float32)
    return np.transpose(a, (2, 0, 1))


def main(tag):
    sp = SPECS[tag]
    sess = load(model_path(sp['repo'], sp['nn']))
    iname = sess.get_inputs()[0].name
    files = sorted(glob.glob(os.path.join(THUMBS, '*.jpg')))
    ids = np.array([int(os.path.basename(f)[:5]) for f in files], dtype=np.int64)
    dim = 512 if tag == 'clipb32' else (384 if tag == 'dinov2' else 768)
    out = np.zeros((len(files), dim), dtype=np.float32)
    bs = 1 if tag == 'vitl14' else BATCH
    t0 = time.time()
    for i in range(0, len(files), bs):
        chunk = files[i:i + bs]
        x = np.stack([prep(f, sp['size'], sp['mean'], sp['std']) for f in chunk])
        y = sess.run([sp['out']], {iname: x})[0]
        if sp.get('cls'):
            y = y[:, 0, :]
        out[i:i + len(chunk)] = y
        if (i // bs) % 40 == 0:
            el = time.time() - t0
            done = min(len(files), i + bs)
            print('  %5d/%d  %.1fs  %.1f ms/frame' % (done, len(files), el,
                  1000 * el / max(1, done)), flush=True)
    el = time.time() - t0
    out /= np.maximum(np.linalg.norm(out, axis=1, keepdims=True), 1e-8)
    np.save(os.path.join(TASTE, 'emb_%s.npy' % tag), out)
    np.save(os.path.join(TASTE, 'emb_%s_ids.npy' % tag), ids)
    meta = dict(model=tag, dim=dim, n=len(files), seconds=el,
                ms_per_frame=1000 * el / max(1, len(files)), preprocess='squash-224')
    json.dump(meta, open(os.path.join(TASTE, 'emb_%s.json' % tag), 'w'), indent=1)
    print('saved emb_%s.npy %s  %.1f ms/frame' % (tag, out.shape, meta['ms_per_frame']))


if __name__ == '__main__':
    main(sys.argv[1])
