import csv, os, sys
from collections import Counter
import numpy as np
sys.stdout.reconfigure(encoding='utf-8', errors='replace')
TASTE = r'E:\code\photo\conrod\target\taste'
rows = list(csv.DictReader(open(os.path.join(TASTE, 'bursts.csv'), encoding='utf-8')))
rel, sizes = [], []
for r in rows:
    sz = int(r['size'])
    d = int(r['delivered'])
    sizes.append(sz)
    rel.append(d / (sz - 1) if sz > 1 else 0)
rel = np.array(rel)
print('bursts %d  mean size %.2f  median %d' % (len(sizes), np.mean(sizes),
                                                int(np.median(sizes))))
print('relative position of delivered frame: mean %.3f median %.2f' % (rel.mean(), np.median(rel)))
print('  in first quartile %.1f%%  last quartile %.1f%%  exact last %.1f%%'
      % (100 * (rel <= 0.25).mean(), 100 * (rel >= 0.75).mean(), 100 * (rel == 1).mean()))
print('  exact first %.1f%%' % (100 * (rel == 0).mean()))
szs = Counter(sizes)
print('sizes:', dict(sorted(szs.items())))
