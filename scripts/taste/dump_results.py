import glob, json, os, sys
sys.stdout.reconfigure(encoding='utf-8', errors='replace')
TASTE = r'E:\code\photo\conrod\target\taste'
for p in sorted(glob.glob(os.path.join(TASTE, 'results_*.json'))):
    print('=' * 70)
    print(os.path.basename(p))
    d = json.load(open(p))
    if 'alone' in d:
        for k, v in d['alone'].items():
            print('  alone %-14s top1 %.3f [%.3f,%.3f] top3 %.3f mrr %.3f n %d'
                  % (k, v['top1'], v['ci'][0], v['ci'][1], v['top3'], v['mrr'], v['n']))
        for k, v in d['extra'].items():
            print('  extra %-20s top1 %.3f [%.3f,%.3f] top3 %.3f mrr %.3f lam %.3g'
                  % (k, v['top1'], v['ci'][0], v['ci'][1], v['top3'], v['mrr'],
                     v.get('l2', 0)))
            for g, r in v.get('per_genre', {}).items():
                print('      %-18s %.3f n %d (random %.3f)' % (g, r[0], r[2], r[3]))
    elif 'aes_laion' in d:
        for k in ('aes_laion', 'sharpness', 'random'):
            v = d[k]
            ci = v.get('ci', ['-', '-'])
            print('  %-12s top1 %.3f [%s,%s] top3 %.3f' % (k, v['top1'], ci[0], ci[1],
                                                           v['top3']))
        print('  meta', d['meta'])
    else:
        print('  meta', {k: d['meta'][k] for k in d['meta'] if k != 'l2'})
        print('  heads:')
        for k, v in sorted(d['heads'].items(), key=lambda kv: -kv[1]['top1']):
            print('    %-32s top1 %.3f [%.3f,%.3f] top3 %.3f mrr %.3f'
                  % (k, v['top1'], v['ci'][0], v['ci'][1], v['top3'], v['mrr']))
        print('  curve', {k: (round(v['top1'], 3), round(v['top3'], 3))
                          for k, v in d['curve'].items()})
