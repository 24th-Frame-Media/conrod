"""Visual QA: contact sheet of a few bursts (delivered frame marked) from the cached thumbs."""
import csv, os, sys
from collections import defaultdict
from PIL import Image, ImageDraw

sys.stdout.reconfigure(encoding='utf-8', errors='replace')
TASTE = r'E:\code\photo\conrod\target\taste'
rows = list(csv.DictReader(open(os.path.join(TASTE, 'frames.csv'), encoding='utf-8')))
byburst = defaultdict(list)
for r in rows:
    byburst[int(r['burst_id'])].append(r)
gen = defaultdict(list)
for b, fr in byburst.items():
    gen[fr[0]['genre']].append(b)
picks = []
for g in ['portrait', 'motorsport', 'car_meet', 'wildlife_personal']:
    bs = sorted(gen.get(g, []))[:1]
    picks.extend(bs)
TW, TH = 200, 200
tiles = []
for b in picks:
    fr = sorted(byburst[b], key=lambda r: int(r['pos']))[:8]
    sheet = Image.new('RGB', (TW * len(fr), TH), 'black')
    for i, r in enumerate(fr):
        im = Image.open(os.path.join(TASTE, 'thumbs', '%05d.jpg' % int(r['frame_id'])))
        im.thumbnail((TW, TH))
        sheet.paste(im, (i * TW + (TW - im.size[0]) // 2, (TH - im.size[1]) // 2))
        if r['delivered'] == '1':
            d = ImageDraw.Draw(sheet)
            d.rectangle([i * TW + 1, 1, (i + 1) * TW - 2, TH - 2], outline='lime', width=4)
            d.text((i * TW + 6, 6), 'KEPT', fill='lime')
    tiles.append(sheet)
W = max(t.size[0] for t in tiles)
H = sum(t.size[1] for t in tiles)
out = Image.new('RGB', (W, H), 'black')
y = 0
for t in tiles:
    out.paste(t, (0, y))
    y += t.size[1]
out.save(os.path.join(TASTE, 'qa_bursts.jpg'), quality=88)
print('wrote qa_bursts.jpg', out.size, [p for p in picks])
