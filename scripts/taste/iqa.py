"""K2: per-frame no-reference quality / aesthetic scores on the cached 512 px previews.

One model in memory at a time, batch 8. Writes target/taste/iqa_scores.csv + iqa_timing.json
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
M = os.path.join(TASTE, 'models', '_hf')

CLIP_M = (0.48145466, 0.4578275, 0.40821073)
CLIP_S = (0.26862954, 0.26130258, 0.27577711)
IN_M = (0.485, 0.456, 0.406)
IN_S = (0.229, 0.224, 0.225)

SPECS = {
    'iqa_musiq': dict(repo='86Cao--IQA-ONNX-Models', fn='musiq_model.onnx', size=224,
                      mean=(0.5, 0.5, 0.5), std=(0.5, 0.5, 0.5), out='score', bs=1),
    'iqa_clipiqa': dict(repo='86Cao--IQA-ONNX-Models', fn='clipiqa_model.onnx', size=224,
                        mean=CLIP_M, std=CLIP_S, out='score', bs=8),
    'iqa_topiq': dict(repo='nocturna--topiq-nr-koniq-onnx', fn='model.onnx', size=384,
                      mean=IN_M, std=IN_S, out='score', bs=1),
}


def path_of(repo, fn):
    hits = glob.glob(os.path.join(M, 'models--' + repo, 'snapshots', '*', fn))
    if not hits:
        raise SystemExit('missing ' + repo + '/' + fn)
    return hits[0]


def prep(p, size, mean, std):
    im = Image.open(p).convert('RGB').resize((size, size), Image.BICUBIC)
    a = np.asarray(im, dtype=np.float32) / 255.0
    a = (a - np.array(mean, np.float32)) / np.array(std, np.float32)
    return np.transpose(a, (2, 0, 1))


def main():
    files = sorted(glob.glob(os.path.join(THUMBS, '*.jpg')))
    ids = np.array([int(os.path.basename(f)[:5]) for f in files], dtype=np.int64)
    print('%d thumbnails' % len(files))
    scores, timing = {}, {}
    for name, sp in SPECS.items():
        try:
            p = path_of(sp['repo'], sp['fn'])
        except SystemExit as e:
            print('SKIP', name, e)
            continue
        so = ort.SessionOptions()
        so.intra_op_num_threads = max(2, (os.cpu_count() or 4) // 2)
        t0 = time.time()
        sess = ort.InferenceSession(p, sess_options=so, providers=['CPUExecutionProvider'])
        iname = sess.get_inputs()[0].name
        bs = sp.get('bs', 8)
        out = np.zeros(len(files), dtype=np.float64)
        for i in range(0, len(files), bs):
            x = np.stack([prep(f, sp['size'], sp['mean'], sp['std'])
                          for f in files[i:i + bs]])
            y = sess.run([sp['out']], {iname: x})[0]
            if len(x) > 1 and np.asarray(y).size != len(x):
                raise SystemExit('%s batch>1 mismatch (%s)' % (name, np.asarray(y).shape))
            out[i:i + len(x)] = np.asarray(y, dtype=np.float64).reshape(-1)[:len(x)]
            if i and i % 800 == 0:
                print('   %s %d/%d' % (name, i, len(files)), flush=True)
        el = time.time() - t0
        timing[name] = 1000 * el / len(files)
        scores[name] = out
        print('%-16s %.1f ms/frame  mean %.3f sd %.3f  range [%.2f, %.2f]'
              % (name, timing[name], out.mean(), out.std(), out.min(), out.max()),
              flush=True)
        del sess

    import csv
    with open(os.path.join(TASTE, 'iqa_scores.csv'), 'w', newline='', encoding='utf-8') as f:
        w = csv.writer(f)
        names = list(scores)
        w.writerow(['frame_id'] + names)
        for i, fid in enumerate(ids):
            w.writerow([int(fid)] + ['%.6f' % scores[n][i] for n in names])
    json.dump(timing, open(os.path.join(TASTE, 'iqa_timing.json'), 'w'), indent=1)
    print('wrote iqa_scores.csv', list(scores))


if __name__ == '__main__':
    main()
