"""B) K5: VLM tiebreak among the best head's top-3 candidates.

Sends 3 candidate frames (A/B/C) to a local Ollama vision model, twice with rotated order,
keeps swap-consistent answers. Writes target/taste/vlm_judge.jsonl.

  python vlm_judge.py --probe            # one burst, prints prompt+answer+timing
  python vlm_judge.py --limit 120        # full run
"""
import argparse
import base64
import csv
import json
import os
import sys
import time
from collections import defaultdict

import numpy as np
import urllib.request

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
sys.stdout.reconfigure(encoding='utf-8', errors='replace')
import eval_taste as ET

TASTE = r'E:\code\photo\conrod\target\taste'
OLLAMA = 'http://127.0.0.1:11434'
MODEL = 'qwen3-vl:8b-instruct-q4_K_M'

PROMPT = (
    "You are judging one photo shoot burst. The three frames below (in order: A, B, C) were "
    "taken seconds apart by a professional photographer. Exactly one of them was delivered "
    "to the client.\n"
    "Decide which frame a professional photographer would deliver: the strongest frame in "
    "terms of moment, expression or action phase, sharpness/focus, framing and subject "
    "placement, background clutter, and crop.\n"
    "Answer with JSON only: "
    '{"pick": "A"|"B"|"C", "why": "<one short sentence>"}'
)


def img_b64(fid):
    p = os.path.join(TASTE, 'thumbs', '%05d.jpg' % fid)
    return base64.b64encode(open(p, 'rb').read()).decode()


def call(images, model=MODEL, num_ctx=6144, timeout=300):
    body = {
        'model': model,
        'messages': [{'role': 'user', 'content': PROMPT, 'images': images}],
        'stream': False,
        'format': 'json',
        'keep_alive': '10m',
        'options': {'num_ctx': num_ctx, 'temperature': 0, 'top_k': 1,
                    'num_predict': 80},
    }
    req = urllib.request.Request(OLLAMA + '/api/chat',
                                 data=json.dumps(body).encode(),
                                 headers={'Content-Type': 'application/json'})
    t = time.time()
    with urllib.request.urlopen(req, timeout=timeout) as r:
        d = json.loads(r.read().decode())
    return d.get('message', {}).get('content', ''), time.time() - t


def parse(txt):
    try:
        j = json.loads(txt)
        p = str(j.get('pick', '')).strip().upper()[:1]
        if p in 'ABC':
            return p, str(j.get('why', ''))[:200]
    except Exception:
        pass
    for ch in txt.upper():
        if ch in 'ABC':
            return ch, txt.strip()[:200]
    return None, txt.strip()[:200]


def load_bursts():
    d = ET.load(['dinov2'])
    preds = np.load(os.path.join(TASTE, 'preds_best.npy'))
    rows = list(csv.DictReader(open(os.path.join(TASTE, 'frames.csv'), encoding='utf-8')))
    byb = defaultdict(list)
    for r in rows:
        byb[int(r['burst_id'])].append(r)
    out = []
    for k, (a, b) in enumerate(d['segs']):
        fr = sorted(byb[k], key=lambda r: int(r['pos']))
        sc = preds[a:b + 1]
        order = list(np.argsort(-sc))
        top3 = [int(i) for i in order[:3]]
        out.append(dict(burst=k, genre=fr[0]['genre'], size=len(fr), frames=fr,
                        top3=top3, delivered=int(d['by'][k]),
                        head_pick=int(order[0]), head_in_top3=int(d['by'][k]) in top3))
    return d, out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--probe', action='store_true')
    ap.add_argument('--limit', type=int, default=120)
    ap.add_argument('--model', default=MODEL)
    args = ap.parse_args()
    d, bursts = load_bursts()
    elig = [b for b in bursts if b['head_in_top3'] and b['size'] >= 3]
    print('bursts %d, head top-3 contains delivered and size>=3: %d (%.1f%%)'
          % (len(bursts), len(elig), 100 * len(elig) / len(bursts)))

    if args.probe:
        b = elig[0]
        imgs = [img_b64(int(b['frames'][i]['frame_id'])) for i in b['top3']]
        print('burst %d genre %s size %d top3 %s delivered %d'
              % (b['burst'], b['genre'], b['size'], b['top3'], b['delivered']))
        txt, dt = call(imgs, args.model)
        print('%.1fs  raw=%r' % (dt, txt))
        print('parsed', parse(txt))
        return

    # stratified sample across genres (limit 0 = every eligible burst)
    if args.limit == 0:
        sel = sorted(elig, key=lambda b: b['burst'])
        print('selected ALL %d eligible bursts' % len(sel))
    else:
        rng = np.random.default_rng(17)
        byg = defaultdict(list)
        for b in elig:
            byg[b['genre']].append(b)
        per = max(10, args.limit // max(1, len(byg)))
        sel = []
        for g, bs in sorted(byg.items()):
            rng.shuffle(bs)
            sel.extend(bs[:per])
        sel.sort(key=lambda b: b['burst'])
        print('selected %d bursts: %s' % (len(sel), {g: min(per, len(v))
                                                     for g, v in byg.items()}))

    outp = os.path.join(TASTE, 'vlm_judge.jsonl')
    done = set()
    if os.path.exists(outp):
        for line in open(outp, encoding='utf-8'):
            try:
                done.add(json.loads(line)['burst'])
            except Exception:
                pass
    new = [b for b in sel if b['burst'] not in done]
    print('%d already done, %d to run' % (len(done), len(new)))
    f = open(outp, 'a', encoding='utf-8')
    t0 = time.time()
    for n, b in enumerate(new):
        order = b['top3']                      # A,B,C
        rot = b['top3'][1:] + b['top3'][:1]    # B,C,A
        rec = dict(burst=b['burst'], genre=b['genre'], size=b['size'],
                   top3=b['top3'], delivered=b['delivered'],
                   head_pick=b['head_pick'])
        try:
            t1, dt1 = call([img_b64(int(b['frames'][i]['frame_id'])) for i in order],
                           args.model)
            p1, w1 = parse(t1)
            t2, dt2 = call([img_b64(int(b['frames'][i]['frame_id'])) for i in rot],
                           args.model)
            p2, w2 = parse(t2)
        except Exception as e:
            print('  burst %d FAILED %s' % (b['burst'], str(e)[:120]))
            continue
        idx1 = 'ABC'.index(p1) if p1 else None
        idx2 = 'ABC'.index(p2) if p2 else None
        # map rotation answer back to candidate index 0..2
        pick_a = order[idx1] if idx1 is not None else None
        pick_b = rot[idx2] if idx2 is not None else None
        consistent = (pick_a is not None and pick_a == pick_b)
        final = pick_a if consistent else b['head_pick']
        rec.update(order1={'A': order[0], 'B': order[1], 'C': order[2]},
                   answer1=p1, why1=w1, answer2=p2, why2=w2,
                   consistent=bool(consistent), vlm_pick=pick_a,
                   final_pick=final, sec1=round(dt1, 2), sec2=round(dt2, 2))
        f.write(json.dumps(rec) + '\n')
        f.flush()
        if (n + 1) % 5 == 0:
            el = time.time() - t0
            print('  %d/%d  %.0fs  %.1fs per call  eta %.0f min'
                  % (n + 1, len(new), el, el / (2 * (n + 1)),
                     (len(new) - n - 1) * el / (n + 1) / 60), flush=True)
    f.close()
    print('wrote', outp)


if __name__ == '__main__':
    main()
