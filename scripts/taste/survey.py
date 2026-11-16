import json, os, sys
from collections import Counter, defaultdict
sys.stdout.reconfigure(encoding='utf-8', errors='replace')

ROOT = r'E:\code\photo\conrod'
lab = json.load(open(os.path.join(ROOT, 'target', 'groundtruth', 'labels.json')))
sh = defaultdict(lambda: {'client': '', 'exp': 0})
for f in lab['frames']:
    sh[f['shoot']]['client'] = f['client']
    if f['exported']:
        sh[f['shoot']]['exp'] += 1
RAW = ('.cr3', '.cr2', '.dng')
rows = []
for d, v in sh.items():
    if v['exp'] < 5 or not os.path.isdir(d):
        continue
    c = Counter()
    for e in os.scandir(d):
        if e.is_file():
            c[os.path.splitext(e.name)[1].lower()] += 1
    raw = sum(c[x] for x in RAW)
    rows.append((v['exp'], raw, c.get('.xmp', 0), v['client'], c.get('.cr3', 0), c.get('.cr2', 0), c.get('.dng', 0), d))
rows.sort(key=lambda r: -r[0])
print('%5s %6s %6s  %-28s %5s %5s %5s' % ('exp', 'raw', 'xmp', 'client', 'cr3', 'cr2', 'dng'))
for r in rows:
    print('%5d %6d %6d  %-28s %5d %5d %5d  %s' % (r[0], r[1], r[2], r[3][:28], r[4], r[5], r[6], r[7][:80]))
