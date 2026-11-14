"""Select panning-labelling frames, extract embedded JPEGs and draw subject boxes.

Reads (read-only): target/eval/*{thursday,ford-falcon,sau-cruise}*/subjects.csv,
target/groundtruth/labels.json (Wetpan / Roll-Racing frames, whole frame).
Writes only under target/pan/: view/NNN.jpg and frames.json.
"""
import csv
import glob
import io
import json
import os
import random
from PIL import Image, ImageDraw, ImageOps

ROOT = r"E:\code\photo\conrod"
EVAL = os.path.join(ROOT, "target", "eval")
GT = os.path.join(ROOT, "target", "groundtruth", "labels.json")
OUT = os.path.join(ROOT, "target", "pan")
VIEW = os.path.join(OUT, "view")
LONG = 1600
SPACING = 4
MOTOR_N = 50
CARMEET_N = 40
VEHICLE_CLASSES = {"car", "truck", "bus", "motorcycle"}
GROUPS = [
    ("thursday", "thursday", "motorsport"),
    ("ford-falcon", "ford-falcon", "car-meet"),
    ("sau-cruise", "sau-cruise", "car-meet"),
]


def pick_subjects(csv_path):
    rows = list(csv.DictReader(open(csv_path, newline="", encoding="utf-8")))
    by_file = {}
    for r in rows:
        by_file.setdefault(r["file"], []).append(r)
    files = sorted(by_file)
    cands = []
    for f in files:
        best = None
        for r in by_file[f]:
            if r["region"] != "vehicle" or r["class"] not in VEHICLE_CLASSES:
                continue
            conf = float(r["conf"])
            bf = float(r["box_fraction"])
            if conf < 0.5 or not (0.03 <= bf <= 0.6):
                continue
            if best is None or bf > float(best["box_fraction"]):
                best = r
        if best is not None:
            cands.append({
                "file": f,
                "exported": best["exported"] == "1",
                "class": best["class"],
                "conf": float(best["conf"]),
                "box": [float(best[k]) for k in ("x1", "y1", "x2", "y2")],
                "frame_w": int(float(best["frame_w"])),
                "frame_h": int(float(best["frame_h"])),
                "box_fraction": float(best["box_fraction"]),
            })
    return files, cands


def spaced(files, cands):
    """Greedy: keep a candidate only if it is >= SPACING positions after the last kept one."""
    pos = {f: i for i, f in enumerate(files)}
    out, last = [], -10**9
    for c in cands:
        p = pos[c["file"]]
        if p - last >= SPACING:
            out.append(c)
            last = p
    return out


def mix(cands, quota, rng):
    ex = [c for c in cands if c["exported"]]
    ne = [c for c in cands if not c["exported"]]
    n_ex = min(len(ex), quota // 2)
    n_ne = min(len(ne), quota - n_ex)
    n_ex = min(len(ex), quota - n_ne)

    def even(lst, k):
        if k <= 0:
            return []
        if k >= len(lst):
            return list(lst)
        idx = [round(i * (len(lst) - 1) / (k - 1)) if k > 1 else 0 for i in range(k)]
        return [lst[i] for i in sorted(set(idx))]

    return even(ex, n_ex) + even(ne, n_ne)


ORIENT_OPS = {
    2: [Image.FLIP_LEFT_RIGHT],
    3: [Image.ROTATE_180],
    4: [Image.FLIP_TOP_BOTTOM],
    5: [Image.TRANSPOSE],
    6: [Image.ROTATE_270],
    7: [Image.TRANSVERSE],
    8: [Image.ROTATE_90],
}


def raw_orientation(path):
    """RAW IFD0 Orientation tag; the embedded JPEG usually carries none."""
    try:
        with Image.open(path) as im:
            return int(im.getexif().get(274, 1))
    except Exception:
        return 1


def extract_jpeg(path):
    with open(path, "rb") as fh:
        data = fh.read()
    offs = []
    p = data.find(b"\xff\xd8\xff")
    while p != -1:
        offs.append(p)
        p = data.find(b"\xff\xd8\xff", p + 2)
    sized = []
    for o in offs:
        try:
            im = Image.open(io.BytesIO(data[o:]))
            sized.append((im.size[0] * im.size[1], o))
        except Exception:
            continue
    for _, o in sorted(sized, reverse=True):
        try:
            im = Image.open(io.BytesIO(data[o:]))
            im.load()
            im = im.convert("RGB")
            for op in ORIENT_OPS.get(raw_orientation(path), []):
                im = im.transpose(op)
            return im, o
        except Exception:
            continue
    raise RuntimeError(f"no decodable JPEG in {path}")


def main():
    rng = random.Random(7)
    os.makedirs(VIEW, exist_ok=True)
    gt = json.load(open(GT, encoding="utf-8"))

    picks = []  # dicts: source group, category, path, box info or None

    motor_sel = []
    for key, name, cat in GROUPS[:1]:
        for d in glob.glob(os.path.join(EVAL, f"*{key}*", "subjects.csv")):
            files, cands = pick_subjects(d)
            sp = spaced(files, cands)
            for c in sp:
                c["group"] = name
                c["category"] = cat
            motor_sel.extend(sp)
            print(name, "eligible", len(cands), "spaced", len(sp))

    wet = [f for f in gt["frames"] if "Wetpan" in f["shoot"] or "Roll-Racing" in f["shoot"]]
    wet.sort(key=lambda f: f["path"])
    wet_sel = []
    last = -10**9
    for i, f in enumerate(wet):
        if i - last >= SPACING:
            wet_sel.append({
                "file": f["path"], "exported": bool(f["exported"]), "class": None,
                "conf": None, "box": None, "frame_w": None, "frame_h": None,
                "box_fraction": None, "group": "wetpan" if "Wetpan" in f["shoot"] else "roll-racing",
                "category": "motorsport",
            })
            last = i
    print("wetpan frames", len(wet), "selected", len(wet_sel))

    motor_quota = MOTOR_N - len(wet_sel)
    motor = mix(motor_sel, motor_quota, rng)
    motor_all = wet_sel + motor
    print("motorsport total", len(motor_all), "exported", sum(c["exported"] for c in motor_all))

    cm_sel = []
    for key, name, cat in GROUPS[1:]:
        for d in glob.glob(os.path.join(EVAL, f"*{key}*", "subjects.csv")):
            files, cands = pick_subjects(d)
            sp = spaced(files, cands)
            for c in sp:
                c["group"] = name
                c["category"] = cat
            cm_sel.extend(sp)
            print(name, "eligible", len(cands), "spaced", len(sp))
    cm = mix(cm_sel, CARMEET_N, rng)
    print("car-meet total", len(cm), "exported", sum(c["exported"] for c in cm))

    allsel = motor_all + cm
    frames = []
    for n, c in enumerate(allsel, 1):
        fid = f"{n:03d}"
        img, off = extract_jpeg(c["file"])
        w, h = img.size
        scale = LONG / max(w, h)
        pw, ph = round(w * scale), round(h * scale)
        view = img.resize((pw, ph), Image.LANCZOS)
        box = None
        if c["box"] is not None:
            fw, fh = c["frame_w"], c["frame_h"]
            if (fw, fh) != (w, h):
                print("DIM MISMATCH", fid, c["file"], (fw, fh), (w, h))
            # CSV coordinates are in frame_w x frame_h space; map to preview by normalized coords
            x1, y1, x2, y2 = c["box"]
            box = [round(x1 / fw * pw, 1), round(y1 / fh * ph, 1),
                   round(x2 / fw * pw, 1), round(y2 / fh * ph, 1)]
            draw = ImageDraw.Draw(view)
            draw.rectangle(box, outline=(255, 255, 0), width=max(3, pw // 400))
        view.save(os.path.join(VIEW, f"{fid}.jpg"), quality=90)
        frames.append({
            "id": fid,
            "path": os.path.join(VIEW, f"{fid}.jpg"),
            "box": box,
            "preview_w": pw,
            "preview_h": ph,
            "shoot": c["group"],
            "category": c["category"],
            "source": c["file"],
            "exported": c["exported"],
            "class": c["class"],
            "conf": c["conf"],
            "box_fraction": c["box_fraction"],
        })
        print(fid, c["group"], c["exported"], os.path.basename(c["file"]), "jpeg@", off, (pw, ph))

    with open(os.path.join(OUT, "frames.json"), "w", encoding="utf-8") as fh:
        json.dump(frames, fh, indent=1)
    print("wrote", len(frames), "frames")


if __name__ == "__main__":
    main()
