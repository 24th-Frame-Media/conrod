import csv
import io
import json
import math
import re
from collections import defaultdict
from pathlib import Path

from PIL import Image, ImageDraw, ImageOps

ROOT = Path(r"E:\code\photo\conrod")
EVAL = ROOT / "target" / "eval"
OUT = ROOT / "target" / "vlm"
CROPS = OUT / "crops"
KEYS = ["thursday", "ford-falcon", "sau-cruise", "client-work", "24th-frame"]
MOTOR_TOKENS = ("thursday", "bathurst")
VEH = {"car", "truck", "bus", "motorcycle"}
QUOTA = 32
CONF_MIN = 0.5
BF_MIN = 0.03
PAD = 0.15
SHEET_TILE = 450
CAP_H = 24


def is_motor(name):
    return any(t in name for t in MOTOR_TOKENS)


def frame_num(path):
    m = re.findall(r"(?<!\d)\d{3,6}(?!\d)", Path(path).stem)
    return int(m[-1]) if m else None


def find_preview(path):
    with open(path, "rb") as f:
        data = f.read()
    best, best_area = None, 0
    off = data.find(b"\xff\xd8\xff")
    while off != -1:
        try:
            im = Image.open(io.BytesIO(data[off:]))
            if im.format == "JPEG" and im.size[0] * im.size[1] > best_area:
                im.load()
                best, best_area = im, im.size[0] * im.size[1]
        except Exception:
            pass
        off = data.find(b"\xff\xd8\xff", off + 2)
    del data
    if best is None:
        raise RuntimeError("no JPEG preview found")
    return ImageOps.exif_transpose(best)


def load_rows():
    rows = []
    for csvp in sorted(EVAL.rglob("subjects.csv")):
        folder = csvp.parent.name
        if not any(k in folder for k in KEYS):
            continue
        motor = is_motor(folder)
        with open(csvp, newline="", encoding="utf-8-sig") as f:
            for r in csv.DictReader(f):
                try:
                    conf = float(r["conf"])
                    bf = float(r["box_fraction"])
                except (ValueError, KeyError, TypeError):
                    continue
                cls = (r.get("class") or "").strip().lower()
                region = (r.get("region") or "").strip().lower()
                if not (cls in VEH or region == "vehicle"):
                    continue
                if conf < CONF_MIN or bf < BF_MIN:
                    continue
                r["_folder"] = folder
                r["_motor"] = motor
                r["_exp"] = int(float(r.get("exported") or 0))
                r["_conf"] = conf
                rows.append(r)
    return rows


def select(cands, quota):
    by_folder = defaultdict(list)
    for r in sorted(cands, key=lambda r: (-r["_exp"], -r["_conf"], r["file"])):
        by_folder[r["_folder"]].append(r)
    folders = sorted(by_folder)
    idx = {f: 0 for f in folders}
    taken = defaultdict(list)
    seen_files = set()
    chosen = []
    progress = True
    while len(chosen) < quota and progress:
        progress = False
        for f in folders:
            if len(chosen) >= quota:
                break
            lst = by_folder[f]
            while idx[f] < len(lst):
                r = lst[idx[f]]
                idx[f] += 1
                n = frame_num(r["file"])
                if n is not None and sum(1 for t in taken[f] if abs(t - n) <= 3) >= 2:
                    continue
                chosen.append(r)
                if n is not None:
                    taken[f].append(n)
                progress = True
                break
    return chosen


def expand_box(box, pw, ph):
    x1, y1, x2, y2 = box
    w, h = x2 - x1, y2 - y1
    l = max(0, math.floor(x1 - PAD * w))
    t = max(0, math.floor(y1 - PAD * h))
    r = min(pw, math.ceil(x2 + PAD * w))
    b = min(ph, math.ceil(y2 + PAD * h))
    return l, t, r, b


def make_sheet(items, path):
    cols = 4
    rows_n = 4
    W = cols * SHEET_TILE
    cell_h = SHEET_TILE + CAP_H
    sheet = Image.new("RGB", (W, rows_n * cell_h), (0, 0, 0))
    draw = ImageDraw.Draw(sheet)
    for i, (cid, img) in enumerate(items):
        cx = (i % cols) * SHEET_TILE
        cy = (i // cols) * cell_h
        thumb = img.copy()
        thumb.thumbnail((SHEET_TILE, SHEET_TILE), Image.LANCZOS)
        ox = cx + (SHEET_TILE - thumb.size[0]) // 2
        oy = cy + CAP_H + (SHEET_TILE - thumb.size[1]) // 2
        sheet.paste(thumb, (ox, oy))
        draw.rectangle([cx, cy, cx + SHEET_TILE - 1, cy + CAP_H - 1], fill=(30, 30, 30))
        draw.text((cx + 6, cy + 6), cid, fill=(255, 255, 0))
    sheet.save(path, "JPEG", quality=85)


def main():
    CROPS.mkdir(parents=True, exist_ok=True)
    for old in CROPS.glob("*.jpg"):
        old.unlink()

    rows = load_rows()
    motor = [r for r in rows if r["_motor"]]
    carmeet = [r for r in rows if not r["_motor"]]
    sel_m = select(motor, QUOTA)
    sel_c = select(carmeet, QUOTA)
    chosen = [("motorsport", r) for r in sel_m] + [("carmeet", r) for r in sel_c]

    jobs = []
    for i, (grp, r) in enumerate(chosen, start=1):
        jobs.append({"id": f"{i:03d}", "group": grp, "row": r})

    by_file = defaultdict(list)
    for j in jobs:
        by_file[j["row"]["file"]].append(j)

    manifest = []
    mismatches = []
    crops_img = {}
    for raw, js in by_file.items():
        try:
            prev = find_preview(raw)
        except Exception as e:
            print("PREVIEW FAIL", raw, e)
            continue
        pw, ph = prev.size
        for j in js:
            r = j["row"]
            fw, fh = int(float(r["frame_w"])), int(float(r["frame_h"]))
            box = [float(r["x1"]), float(r["y1"]), float(r["x2"]), float(r["y2"])]
            if (pw, ph) == (fh, fw) and fw != fh:
                print("SKIP transposed preview/frame", j["id"], raw)
                mismatches.append({"id": j["id"], "skipped": True, "preview": [pw, ph], "frame": [fw, fh]})
                continue
            scaled = (pw, ph) != (fw, fh)
            if scaled:
                sx, sy = pw / fw, ph / fh
                box = [box[0] * sx, box[1] * sy, box[2] * sx, box[3] * sy]
                mismatches.append({"id": j["id"], "preview": [pw, ph], "frame": [fw, fh]})
            l, t, rr, b = expand_box(box, pw, ph)
            crop = prev.crop((l, t, rr, b)).convert("RGB")
            stem = Path(raw).stem
            name = f"{j['id']}_{stem}.jpg"
            crop.save(CROPS / name, "JPEG", quality=92)
            crops_img[j["id"]] = crop.copy()
            manifest.append({
                "id": j["id"],
                "crop": f"target/vlm/crops/{name}",
                "source": raw,
                "folder": r["_folder"],
                "box": [round(v, 2) for v in box],
                "csv_box": [float(r["x1"]), float(r["y1"]), float(r["x2"]), float(r["y2"])],
                "box_scaled_to_preview": scaled,
                "preview_w": pw,
                "preview_h": ph,
                "frame_w": fw,
                "frame_h": fh,
                "crop_w": crop.size[0],
                "crop_h": crop.size[1],
                "group": j["group"],
            })
        del prev

    manifest.sort(key=lambda m: m["id"])
    (OUT / "crops.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")

    ids = [m["id"] for m in manifest]
    for k, start in enumerate(range(0, len(ids), 16), start=1):
        group_ids = ids[start:start + 16]
        make_sheet([(cid, crops_img[cid]) for cid in group_ids], OUT / f"sheet_{k}.jpg")

    counts = defaultdict(int)
    for m in manifest:
        counts[m["group"]] += 1
    sizes = [(m["crop_w"], m["crop_h"]) for m in manifest]
    print("counts", dict(counts), "total", len(manifest))
    if sizes:
        ws = [s[0] for s in sizes]
        hs = [s[1] for s in sizes]
        print("crop w range", min(ws), max(ws), "h range", min(hs), max(hs))
    print("sheets", len(ids) and math.ceil(len(ids) / 16))
    print("preview/frame mismatches", len(mismatches), mismatches[:5])
    print("folders", sorted({m["folder"] for m in manifest}))


if __name__ == "__main__":
    main()
