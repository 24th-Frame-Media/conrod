"""Pass A: capture times -> bursts -> burst table.

Read-only over D:/P:. Only reads tiny XMP sidecars (or a 6 MB head of the RAW when
no sidecar exists) -- never the whole file.

Outputs: target/taste/bursts.csv, target/taste/frames.csv
"""
import csv
import hashlib
import json
import os
import re
import struct
import sys
from collections import defaultdict
from concurrent.futures import ThreadPoolExecutor

sys.stdout.reconfigure(encoding='utf-8', errors='replace')

ROOT = r'E:\code\photo\conrod'
TASTE = os.path.join(ROOT, 'target', 'taste')
LABELS = os.path.join(ROOT, 'target', 'groundtruth', 'labels.json')
TIMEDIR = os.path.join(TASTE, 'times')
os.makedirs(TIMEDIR, exist_ok=True)
os.makedirs(os.path.join(TASTE, 'thumbs'), exist_ok=True)

RAW_EXT = ('.cr3', '.cr2', '.dng')
GAP = 1.5
MINB, MAXB = 2, 30
WINDOW = 34

GENRE = {
    'Bathurst 1000': 'motorsport',
    '2026-10-06_Bathurst-1000': 'motorsport',
    'BLCC Conrod Flyer': 'motorsport',
    'Race Week': 'motorsport',
    'Bathurst Burn': 'motorsport',
    'Ford Falcon Tribute Cruise': 'car_meet',
    'SAU Cruise Mount Panorama': 'car_meet',
    'ROADSTER BROS & Client E2 + Fathers day laps Motorsport Museum': 'car_meet',
    'Bathurst Historic Car Club Swap Meet': 'car_meet',
    'Client G Car Cruises': 'car_meet',
    'Client H': 'car_meet',
    'AMW Wheels Meet': 'car_meet',
    'RPM Meets': 'car_meet',
    'Client F': 'car_meet',
    'Client A': 'property',
    'Variety Backyard Bash': 'event',
    'Jasmeen Kaur': 'portrait',
    'Natasha William': 'portrait',
    'Mum': 'portrait',
    'Aaron Boss': 'portrait',
    'Mandy Godwin': 'portrait',
    'Victor Jorgensen': 'portrait',
    'Client B': 'portrait',
    'Byron Shaw': 'portrait',
    'Client D': 'portrait',
    'Personal Work': 'wildlife_personal',
    '2026-10-30_Sydney-Weekend': 'wildlife_personal',
}

# chosen shoots: xmp coverage >= 85% (times are cheap) and a usable keep-ratio
SHOOTS = [
    r'P:\Client Work\Ford Falcon Tribute Cruise\2026\08\22',
    r'P:\Client Work\BLCC Conrod Flyer\2026\09\05',
    r'P:\Client Work\BLCC Conrod Flyer\2026\09\06',
    r'P:\Client Work\SAU Cruise Mount Panorama\2026\08\29',
    r'P:\Client Work\ROADSTER BROS & Client E2 + Fathers day laps Motorsport Museum\2026\09\13',
    r'P:\Client Work\Variety Backyard Bash\2026\08\14',
    r'P:\Client Work\Natasha William\2026\08\14',
    r'P:\Client Work\Jasmeen Kaur\2026\09\24',
    r'P:\Client Work\Client F\2026\08\15',
    r'P:\Client Work\Mum\2026\08\06',
    r'P:\Client Work\AMW Wheels Meet\2026\08\09',
    r'P:\Client Work\Bathurst 1000\2026\10\08',
    r'D:\24th Frame Media\02_Projects\2026-10-06_Bathurst-1000\01_RAW\2026-10-08_Thursday',
    r'D:\Personal Work\2026\08\02',
    r'D:\Personal Work\2026\08\11',
    r'D:\Personal Work\2026\08\01',
    r'D:\Personal Work\2026\08\28',
    r'D:\Personal Work\2026\08\19',
    r'D:\Personal Work\2026\08\13',
    r'D:\Personal Work\2026\08\31',
    r'D:\Personal Work\2026\08\17',
    r'D:\24th Frame Media\01_Clients\Client A\2026-10-12_24-Sample-Ave-Sampletown-Condition-Report\01_RAW',
]

import rawlib

NUM = re.compile(r'(\D*)(\d+)$')
XMP_DT = re.compile(rb'exif:DateTimeOriginal="([^"]+)"')
ISO = re.compile(rb'(\d{4})-(\d{2})-(\d{2})T(\d{2}):(\d{2}):(\d{2})(?:\.(\d+))?')


def num_key(stem):
    m = NUM.search(stem.lower())
    return (m.group(1), int(m.group(2))) if m else None


def parse_iso(b):
    m = ISO.search(b)
    if not m:
        return None
    y, mo, d, h, mi, s, frac = m.groups()
    base = (((int(y) * 372 + int(mo) * 31 + int(d)) * 24 + int(h)) * 60 + int(mi)) * 60 + int(s)
    f = float('0.' + frac.decode()) if frac else 0.0
    return base + f


def tiff_datetime(buf, base):
    """Minimal TIFF/EXIF parse: return (DateTimeOriginal, SubSecTimeOriginal) or None."""
    if len(buf) < 8:
        return None
    bo = {b'II': '<', b'MM': '>'}.get(buf[:2])
    if not bo:
        return None
    try:
        off = struct.unpack(bo + 'I', buf[4:8])[0]
        if off + 2 > len(buf):
            return None
        n = struct.unpack(bo + 'H', buf[off:off + 2])[0]
        dt = sub = None
        for i in range(n):
            e = off + 2 + 12 * i
            if e + 12 > len(buf):
                break
            tag, typ, cnt = struct.unpack(bo + 'HHI', buf[e:e + 8])
            if tag not in (0x9003, 0x9291):
                continue
            if cnt <= 4:
                val = buf[e + 8:e + 8 + cnt]
            else:
                vo = struct.unpack(bo + 'I', buf[e + 8:e + 12])[0]
                val = buf[base + vo: base + vo + cnt]
            val = val.split(b'\x00')[0]
            if tag == 0x9003:
                dt = val
            else:
                sub = val
        return dt, sub
    except Exception:
        return None


def raw_head_time(path, head_bytes=6 << 20):
    try:
        with open(path, 'rb') as f:
            buf = f.read(head_bytes)
    except Exception:
        return None
    i = 0
    while True:
        i = buf.find(b'Exif\x00\x00', i)
        if i < 0:
            return None
        r = tiff_datetime(buf[i + 6:], i + 6)
        if r and r[0]:
            dt, sub = r
            dt = dt.decode('ascii', 'replace')
            m = re.match(r'(\d{4}):(\d{2}):(\d{2}) (\d{2}):(\d{2}):(\d{2})', dt)
            if m:
                y, mo, d, h, mi, s = (int(x) for x in m.groups())
                base = (((y * 372 + mo * 31 + d) * 24 + h) * 60 + mi) * 60 + s
                f = 0.0
                if sub:
                    sg = ''.join(ch for ch in sub.decode('ascii', 'replace') if ch.isdigit())
                    if sg:
                        f = float('0.' + sg)
                return base + f
        i += 6


def get_time(shoot, name, has_xmp):
    """Prefer a full XMP sidecar (gives sub-seconds), else parse the RAW head."""
    stem = os.path.splitext(name)[0]
    if has_xmp:
        p = os.path.join(shoot, stem + '.xmp')
        try:
            with open(p, 'rb') as f:
                b = f.read(65536)
            t = parse_iso(b)
            if t is not None:
                return t
        except Exception:
            pass
    return rawlib.head_time(os.path.join(shoot, name))


def shoot_tag(shoot):
    return hashlib.md5(shoot.lower().encode()).hexdigest()[:10]


def main():
    lab = json.load(open(LABELS))
    exp = defaultdict(set)
    client = {}
    for f in lab['frames']:
        client[f['shoot']] = f['client']
        if f['exported']:
            exp[f['shoot']].add(f['path'].lower())

    bursts = []
    stats = []
    for shoot in SHOOTS:
        if not os.path.isdir(shoot):
            print('MISSING', shoot)
            continue
        cl = client.get(shoot, '?')
        genre = GENRE.get(cl, 'other')
        exports = exp.get(shoot, set())
        if not exports:
            print('no exports', shoot)
            continue
        entries = [e for e in os.scandir(shoot) if e.is_file()]
        raws = [e.name for e in entries if e.name.lower().endswith(RAW_EXT)]
        xmp = {os.path.splitext(e.name)[0].lower() for e in entries if e.name.lower().endswith('.xmp')}
        if not raws:
            continue
        keys = {num_key(os.path.splitext(os.path.basename(p))[0]) for p in exports}
        keys.discard(None)
        wset = set()
        for pfx, n in keys:
            for j in range(n - WINDOW, n + WINDOW + 1):
                wset.add((pfx, j))
        cand = [r for r in raws if num_key(os.path.splitext(r)[0]) in wset]
        tag = shoot_tag(shoot)
        cache_p = os.path.join(TIMEDIR, tag + '.json')
        if os.path.exists(cache_p):
            tm = json.load(open(cache_p))
        else:
            def work(n):
                hx = os.path.splitext(n)[0].lower() in xmp
                return n, get_time(shoot, n, hx)
            tm = {}
            with ThreadPoolExecutor(max_workers=24) as ex:
                for i, (n, t) in enumerate(ex.map(work, cand)):
                    if t is not None:
                        tm[n] = t
                    if (i + 1) % 500 == 0:
                        print('   ..%d/%d' % (i + 1, len(cand)), flush=True)
            json.dump(tm, open(cache_p, 'w'))
        tl = sorted((t, n) for n, t in tm.items())
        groups, cur = [], []
        for t, n in tl:
            if cur and t - cur[-1][0] > GAP:
                groups.append(cur)
                cur = []
            cur.append((t, n))
        if cur:
            groups.append(cur)
        nb = 0
        for g in groups:
            if not (MINB <= len(g) <= MAXB):
                continue
            dels = [i for i, (_, n) in enumerate(g)
                    if os.path.join(shoot, n).lower() in exports]
            if len(dels) != 1:
                continue
            bursts.append({
                'shoot': shoot, 'client': cl, 'genre': genre,
                'files': [os.path.join(shoot, n) for _, n in g],
                'delivered': dels[0], 't0': g[0][0],
            })
            nb += 1
        stats.append((len(raws), len(cand), len(tm), nb, genre, cl))
        print('%6d raw %6d cand %6d timed  %4d bursts  %-16s %-24s %s'
              % (len(raws), len(cand), len(tm), nb, genre, cl[:24], shoot[:56]), flush=True)

    # cap per genre, and per shoot, to bound preview extraction
    by_genre = defaultdict(list)
    for b in bursts:
        by_genre[b['genre']].append(b)
    cap = {'motorsport': 520, 'car_meet': 520, 'wildlife_personal': 320,
           'portrait': 200, 'event': 160, 'property': 120, 'other': 100}
    sel = []
    import random
    rng = random.Random(7)
    for g, bs in by_genre.items():
        rng.shuffle(bs)
        # cap per shoot inside a genre
        pershoot = defaultdict(int)
        keep = []
        for b in bs:
            if pershoot[b['shoot']] >= 180:
                continue
            pershoot[b['shoot']] += 1
            keep.append(b)
            if len(keep) >= cap.get(g, 150):
                break
        sel.extend(keep)
    sel.sort(key=lambda b: (b['genre'], b['shoot'], b['t0']))

    with open(os.path.join(TASTE, 'bursts.csv'), 'w', newline='', encoding='utf-8') as f:
        w = csv.writer(f)
        w.writerow(['burst_id', 'shoot', 'client', 'genre', 'size', 'delivered',
                    'files'])
        for i, b in enumerate(sel):
            w.writerow([i, b['shoot'], b['client'], b['genre'], len(b['files']),
                        b['delivered'], json.dumps(b['files'])])
    with open(os.path.join(TASTE, 'frames.csv'), 'w', newline='', encoding='utf-8') as f:
        w = csv.writer(f)
        w.writerow(['frame_id', 'burst_id', 'path', 'delivered', 'genre', 'client', 'pos'])
        k = 0
        for i, b in enumerate(sel):
            for j, p in enumerate(b['files']):
                w.writerow([k, i, p, int(j == b['delivered']), b['genre'], b['client'], j])
                k += 1
    sizes = [len(b['files']) for b in sel]
    print('\nSELECTED bursts=%d frames=%d mean_size=%.2f' %
          (len(sel), sum(sizes), sum(sizes) / max(1, len(sizes))))
    agg = defaultdict(lambda: [0, 0])
    for b in sel:
        agg[b['genre']][0] += 1
        agg[b['genre']][1] += len(b['files'])
    for g, v in sorted(agg.items()):
        print('  %-18s bursts=%4d frames=%5d' % (g, v[0], v[1]))


if __name__ == '__main__':
    main()
