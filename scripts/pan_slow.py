"""Select slow-shutter frames (likely pans) from RAW folders and extract 1600px viewing copies.

Reads (read-only): target/pan/exif_{sun,sat,wetpan,roll}.csv (exiftool -csv output, UTF-16 or UTF-8),
RAW folders on D: (read-only). Writes only under target/pan/: view_slow/S###.jpg, frames_slow.json.
"""
import csv
import io
import json
import os
from PIL import Image

OUT = r"E:\code\photo\conrod\target\pan"
VIEW = os.path.join(OUT, "view_slow")
LONG = 1600
TOTAL = 60
SPACING = 5
MIN_EXP = 0.004
BAND = (1 / 160 - 1e-9, 1 / 30 + 1e-9)

FOLDERS = [
    ("wetpan", r"D:\24th Frame Media\02_Projects\2026-10-30_Sydney-Weekend\01_RAW\2026-10-31_Wetpan"),
    ("roll", r"D:\24th Frame Media\02_Projects\2026-10-30_Sydney-Weekend\01_RAW\2026-10-30_Roll-Racing"),
    ("sat", r"D:\24th Frame Media\02_Projects\2026-10-06_Bathurst-1000\01_RAW\2026-10-10_Saturday"),
    ("sun", r"D:\24th Frame Media\02_Projects\2026-10-06_Bathurst-1000\01_RAW\2026-10-11_Sunday"),
]


def load_csv(path):
    b = open(path, "rb").read()
    text = b.decode("utf-16") if b[:2] in (b"\xff\xfe", b"\xfe\xff") else b.decode("utf-8-sig")
    return list(csv.DictReader(io.StringIO(text)))


def secs(s):
    s = s.strip()
    try:
        if "/" in s:
            a, b = s.split("/")
            return float(a) / float(b)
        return float(s)
    except ValueError:
        return None


def candidates(key, folder):
    names = sorted(f for f in os.listdir(folder) if f.upper().endswith(".CR3"))
    rank = {n: i for i, n in enumerate(names)}
    rows = load_csv(os.path.join(OUT, f"exif_{key}.csv"))
    out = []
    for r in rows:
        name = r["FileName"]
        if name not in rank:
            continue
        e = secs(r["ExposureTime"])
        if e is None or e < MIN_EXP or not r["FocalLength"].strip():
            continue
        out.append({"name": name, "rank": rank[name], "exp": e, "exp_s": r["ExposureTime"].strip(),
                    "focal": r["FocalLength"].strip(), "folder": folder, "key": key})
    out.sort(key=lambda c: c["rank"])
    return out


def spaced(cands):
    kept, last = [], -10**9
    for c in cands:
        if c["rank"] - last >= SPACING:
            kept.append(c)
            last = c["rank"]
    return kept


def pick(cands_by_key):
    tierA = {k: spaced([c for c in v if BAND[0] <= c["exp"] <= BAND[1]]) for k, v in cands_by_key.items()}
    tierB = {k: spaced([c for c in v if not (BAND[0] <= c["exp"] <= BAND[1])]) for k, v in cands_by_key.items()}
    total_a = sum(len(v) for v in tierA.values())
    chosen = []
    for k in cands_by_key:
        a = tierA[k]
        quota = min(len(a), round(TOTAL * len(a) / max(total_a, 1)))
        if quota:
            step = len(a) / quota
            chosen += [a[int(i * step)] for i in range(quota)]
    if len(chosen) < TOTAL:
        extra = [c for k in tierB for c in tierB[k]]
        extra.sort(key=lambda c: abs(c["exp"] - 1 / 100))
        for c in extra:
            if len(chosen) >= TOTAL:
                break
            if all(not (x["key"] == c["key"] and abs(x["rank"] - c["rank"]) < SPACING) for x in chosen):
                chosen.append(c)
    chosen.sort(key=lambda c: (c["key"], c["rank"]))
    return chosen


def extract(path):
    d = open(path, "rb").read()
    best = None
    i = 0
    while True:
        i = d.find(b"\xff\xd8\xff", i)
        if i < 0:
            break
        try:
            im = Image.open(io.BytesIO(d[i:i + 30000000]))
            im.load()
            if best is None or im.width * im.height > best.width * best.height:
                best = im
        except Exception:
            pass
        i += 3
    return best


def main():
    os.makedirs(VIEW, exist_ok=True)
    cands = {}
    for key, folder in FOLDERS:
        cands[key] = candidates(key, folder)
        print(key, len(cands[key]))
    chosen = pick(cands)
    manifest = []
    for n, c in enumerate(chosen, 1):
        sid = f"S{n:03d}"
        path = os.path.join(c["folder"], c["name"])
        im = extract(path)
        pw, ph = im.size
        v = im.convert("RGB")
        s = LONG / max(v.size)
        v = v.resize((round(v.width * s), round(v.height * s)), Image.LANCZOS)
        v.save(os.path.join(VIEW, sid + ".jpg"), quality=90)
        manifest.append({"id": sid, "path": path, "exposure": c["exp_s"], "focal": c["focal"],
                         "preview_w": pw, "preview_h": ph, "shoot": os.path.basename(c["folder"])})
        print(sid, c["name"], c["exp_s"], c["focal"], pw, ph)
    with open(os.path.join(OUT, "frames_slow.json"), "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=1)
    print("total", len(manifest))


if __name__ == "__main__":
    main()
