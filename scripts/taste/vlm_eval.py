"""Score the K5 VLM tiebreak against the head on the same bursts."""
import json
import os
import sys
from collections import defaultdict

import numpy as np

sys.stdout.reconfigure(encoding='utf-8', errors='replace')
TASTE = r'E:\code\photo\conrod\target\taste'


def boot_paired(a, b, B=4000, seed=1):
    rng = np.random.default_rng(seed)
    n = len(a)
    idx = rng.integers(0, n, size=(B, n))
    d = np.asarray(a)[idx].mean(1) - np.asarray(b)[idx].mean(1)
    return float(np.mean(d)), float(np.percentile(d, 2.5)), float(np.percentile(d, 97.5))


def boot_ci(x, B=4000, seed=2):
    rng = np.random.default_rng(seed)
    n = len(x)
    idx = rng.integers(0, n, size=(B, n))
    m = np.asarray(x)[idx].mean(1)
    return float(np.percentile(m, 2.5)), float(np.percentile(m, 97.5))


def main():
    recs = [json.loads(l) for l in open(os.path.join(TASTE, 'vlm_judge.jsonl'),
                                        encoding='utf-8') if l.strip()]
    print('bursts judged:', len(recs))
    head_hit, vlm_hit, cons_hit, fin_hit, cons = [], [], [], [], []
    agree = rescue = break_ = 0
    secs = []
    byg = defaultdict(lambda: dict(n=0, head=0, vlm=0, fin=0, cons=[]))
    for r in recs:
        d = r['delivered']
        hp = r['head_pick']
        vp = r['vlm_pick']
        fp = r['final_pick']
        h = int(hp == d)
        v = int(vp == d) if vp is not None else 0
        head_hit.append(h)
        vlm_hit.append(v)
        cons.append(bool(r['consistent']))
        fin_hit.append(int(fp == d))
        if vp is not None and r['consistent']:
            cons_hit.append(v)
        if vp is not None:
            agree += int(vp == hp)
            rescue += int(vp == d and hp != d)
            break_ += int(vp != d and hp == d)
        secs.append(r['sec1'] + r['sec2'])
        g = byg[r['genre']]
        g['n'] += 1
        g['head'] += h
        g['vlm'] += v
        g['fin'] += int(fp == d)
        g['cons'].append(int(r['consistent']))

    nn = len(recs)
    hh = float(np.mean(head_hit))
    vv = float(np.mean(vlm_hit))
    cc = float(np.mean(cons))
    ff = float(np.mean(fin_hit))
    print('consistent answers: %.3f (%d/%d)' % (cc, int(cc * nn), nn))
    print('head top-1 (same bursts):      %.3f  [%.3f,%.3f]'
          % (hh, *boot_ci(head_hit)))
    print('VLM top-1 (all, miss if bad):  %.3f  [%.3f,%.3f]' % (vv, *boot_ci(vlm_hit)))
    if cons_hit:
        print('VLM top-1 (consistent only):   %.3f  (n=%d)'
              % (float(np.mean(cons_hit)), len(cons_hit)))
    print('final (VLM, fallback to head): %.3f  [%.3f,%.3f]' % (ff, *boot_ci(fin_hit)))
    rand = float(np.mean([1.0 / min(3, r['size']) for r in recs]))
    print('random among candidates:       %.3f' % rand)
    d, lo, hi = boot_paired(fin_hit, head_hit)
    print('paired delta final-head: %+.3f [%+.3f, %+.3f]  -> gate (>=+0.05 & CI>0): %s'
          % (d, lo, hi, 'PASS' if (d >= 0.05 and lo > 0) else 'FAIL'))
    dv, lo2, hi2 = boot_paired(vlm_hit, head_hit)
    print('paired delta VLM-head:   %+.3f [%+.3f, %+.3f]' % (dv, lo2, hi2))
    print('VLM agrees with head %.1f%%; rescues %d head misses; breaks %d head hits'
          % (100 * agree / nn, rescue, break_))
    s = np.array(secs)
    print('time per burst: mean %.1fs (median %.1fs, p90 %.1fs)'
          % (s.mean(), np.median(s), np.percentile(s, 90)))
    print('\nper genre (top-1):')
    for g, v in sorted(byg.items()):
        print('  %-18s n=%3d head %.3f  vlm %.3f  final %.3f  consistent %.2f'
              % (g, v['n'], v['head'] / v['n'], v['vlm'] / v['n'],
                 v['fin'] / v['n'], np.mean(v['cons'])))
    json.dump(dict(n=nn, head=hh, vlm=vv, final=ff, consistency=cc, random=rand,
                   delta=d, delta_ci=[lo, hi], agree=agree, rescue=rescue,
                   broken=break_, sec_per_burst=float(s.mean()),
                   per_genre={g: dict(n=v['n'], head=v['head'] / v['n'],
                                      vlm=v['vlm'] / v['n'], final=v['fin'] / v['n'],
                                      consistency=float(np.mean(v['cons'])))
                              for g, v in byg.items()}),
              open(os.path.join(TASTE, 'vlm_eval.json'), 'w'), indent=1)


if __name__ == '__main__':
    main()
