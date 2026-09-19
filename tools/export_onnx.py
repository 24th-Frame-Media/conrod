"""Export the vehicle and person detector to ONNX, and prove it matches.

    python tools/export_onnx.py [--frames 50] [--job 38]

Needs ultralytics, onnx and onnxruntime; run it from a venv that has them
(the app's own runtime does not need onnx). Writes
~/.conrod/models/yolo11s-960.onnx: dynamic 1x3xHxW input, raw [1, 84, N]
output, NMS left out on purpose.

Dynamic because the app never ran the detector on a square: YOLO.predict()
quietly sets rect=True, so a 3:2 frame goes in as 960x640 -- the long edge
scaled to 960, the short edge padded only up to the next multiple of 32. A
square export changes the answers (a 0.14 confidence swing on real frames)
and costs half as much again in compute for the grey bars.

The numpy decoder below is the specification the Rust port follows --
letterbox, decode, class filter, class-wise NMS, rescale -- written against
what ultralytics actually does rather than what its docs say. It is checked
against ultralytics' own predictions on real frames from the job database, so
the Rust port has something proven to copy and a fixture to be compared with.
"""

from __future__ import annotations

import argparse
import json
import os
import random
import sqlite3
import sys
import time
from pathlib import Path

import cv2
import numpy as np

ROOT = Path(__file__).resolve().parent.parent
HOME = Path(os.environ.get("CONROD_HOME") or Path.home() / ".conrod")
MODELS = HOME / "models"
IMGSZ = 960
CONF = 0.25
IOU = 0.7
MAX_DET = 300
# Classes the new scan types ask for: person, then the vehicles the app has
# always kept. Filtered after the argmax over all 80, as ultralytics does.
CLASSES = (0, 2, 3, 5, 7)
PAD = 114
STRIDE = 32


def letterbox(bgr: np.ndarray):
    """ultralytics' LetterBox(auto=True): long edge to IMGSZ, each side padded
    only to the next multiple of STRIDE, centred, with its -0.1/+0.1
    rounding of the padding."""
    h, w = bgr.shape[:2]
    r = min(IMGSZ / h, IMGSZ / w)
    new_w, new_h = int(round(w * r)), int(round(h * r))
    dw, dh = ((IMGSZ - new_w) % STRIDE) / 2, ((IMGSZ - new_h) % STRIDE) / 2
    if (w, h) != (new_w, new_h):
        bgr = cv2.resize(bgr, (new_w, new_h), interpolation=cv2.INTER_LINEAR)
    top, bottom = int(round(dh - 0.1)), int(round(dh + 0.1))
    left, right = int(round(dw - 0.1)), int(round(dw + 0.1))
    out = cv2.copyMakeBorder(bgr, top, bottom, left, right, cv2.BORDER_CONSTANT,
                             value=(PAD, PAD, PAD))
    tensor = out[:, :, ::-1].transpose(2, 0, 1)[None].astype(np.float32) / 255.0
    # Rescaling back uses ultralytics' scale_boxes, which recomputes gain and
    # padding from the padded input's shape rather than reusing the above.
    in_h, in_w = out.shape[:2]
    gain = min(in_h / h, in_w / w)
    pad = (round((in_w - round(w * gain)) / 2 - 0.1),
           round((in_h - round(h * gain)) / 2 - 0.1))
    return np.ascontiguousarray(tensor), gain, pad


def nms(boxes: np.ndarray, scores: np.ndarray, iou: float) -> list[int]:
    order = scores.argsort()[::-1]
    keep: list[int] = []
    area = (boxes[:, 2] - boxes[:, 0]) * (boxes[:, 3] - boxes[:, 1])
    while order.size:
        i = order[0]
        keep.append(int(i))
        xx1 = np.maximum(boxes[i, 0], boxes[order[1:], 0])
        yy1 = np.maximum(boxes[i, 1], boxes[order[1:], 1])
        xx2 = np.minimum(boxes[i, 2], boxes[order[1:], 2])
        yy2 = np.minimum(boxes[i, 3], boxes[order[1:], 3])
        inter = np.clip(xx2 - xx1, 0, None) * np.clip(yy2 - yy1, 0, None)
        overlap = inter / (area[i] + area[order[1:]] - inter)
        order = order[1:][overlap <= iou]
    return keep


def decode(raw: np.ndarray, gain: float, pad, shape) -> list[dict]:
    """[1, 84, N] -> boxes in frame pixels."""
    pred = raw[0].T                                    # N x 84
    scores = pred[:, 4:]
    cls = scores.argmax(1)
    conf = scores[np.arange(len(cls)), cls]
    keep = (conf > CONF) & np.isin(cls, CLASSES)
    pred, cls, conf = pred[keep], cls[keep], conf[keep]
    cx, cy, bw, bh = pred[:, 0], pred[:, 1], pred[:, 2], pred[:, 3]
    boxes = np.stack([cx - bw / 2, cy - bh / 2, cx + bw / 2, cy + bh / 2], 1)
    # Class-wise NMS: offset each class so boxes of different classes never
    # suppress each other.
    offset = boxes + (cls * 7680.0)[:, None]
    kept = nms(offset, conf, IOU)[:MAX_DET]
    h, w = shape
    out = []
    for i in kept:
        x1, y1, x2, y2 = boxes[i]
        x1, x2 = (x1 - pad[0]) / gain, (x2 - pad[0]) / gain
        y1, y2 = (y1 - pad[1]) / gain, (y2 - pad[1]) / gain
        out.append({"cls": int(cls[i]), "conf": float(conf[i]),
                    "box": [float(np.clip(x1, 0, w)), float(np.clip(y1, 0, h)),
                            float(np.clip(x2, 0, w)), float(np.clip(y2, 0, h))]})
    return out


def iou_of(a, b) -> float:
    ix = max(0.0, min(a[2], b[2]) - max(a[0], b[0]))
    iy = max(0.0, min(a[3], b[3]) - max(a[1], b[1]))
    inter = ix * iy
    union = (a[2] - a[0]) * (a[3] - a[1]) + (b[2] - b[0]) * (b[3] - b[1]) - inter
    return inter / union if union > 0 else 0.0


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--frames", type=int, default=50)
    ap.add_argument("--job", type=int, default=38)
    ap.add_argument("--force", action="store_true", help="re-export even if present")
    args = ap.parse_args()

    from ultralytics import YOLO
    import onnxruntime as ort

    weights = MODELS / "yolo11s.pt"
    target = MODELS / f"yolo11s-{IMGSZ}.onnx"
    model = YOLO(str(weights))
    if args.force and target.exists():
        target.unlink()
    if not target.exists():
        made = Path(model.export(format="onnx", imgsz=IMGSZ, dynamic=True,
                                 simplify=True, opset=17))
        made.replace(target)
    print(f"{target}  {target.stat().st_size / 1e6:.1f} MB")

    session = ort.InferenceSession(str(target), providers=["CPUExecutionProvider"])
    print("input", [(i.name, i.shape) for i in session.get_inputs()],
          "output", [(o.name, o.shape) for o in session.get_outputs()])

    db = sqlite3.connect(f"file:{HOME / 'conrod.db'}?mode=ro", uri=True)
    frames = [p for (p,) in db.execute(
        "SELECT preview_path FROM images WHERE job_id=? AND preview_path IS NOT NULL",
        (args.job,)) if Path(p).exists()]
    frames = random.Random(0).sample(frames, min(args.frames, len(frames)))

    matched = total_ref = total_ours = 0
    worst = 1.0
    conf_gap = 0.0
    timings = []
    golden = []
    for path in frames:
        bgr = cv2.imread(path)
        ref = model.predict(source=bgr, imgsz=IMGSZ, conf=CONF, iou=IOU,
                            classes=list(CLASSES), verbose=False, device="cpu")[0]
        ref_boxes = [{"cls": int(b.cls[0]), "conf": float(b.conf[0]),
                      "box": [float(v) for v in b.xyxy[0].tolist()]} for b in ref.boxes]

        tensor, gain, pad = letterbox(bgr)
        start = time.perf_counter()
        raw = session.run(None, {session.get_inputs()[0].name: tensor})[0]
        timings.append(time.perf_counter() - start)
        ours = decode(raw, gain, pad, bgr.shape[:2])

        total_ref += len(ref_boxes)
        total_ours += len(ours)
        used = set()
        for r in ref_boxes:
            best, best_j = 0.0, None
            for j, o in enumerate(ours):
                if j in used or o["cls"] != r["cls"]:
                    continue
                v = iou_of(r["box"], o["box"])
                if v > best:
                    best, best_j = v, j
            if best_j is not None and best >= 0.9:
                used.add(best_j)
                matched += 1
                worst = min(worst, best)
                conf_gap = max(conf_gap, abs(r["conf"] - ours[best_j]["conf"]))
        golden.append({"frame": path, "shape": list(bgr.shape[:2]), "boxes": ref_boxes})

    print(f"frames {len(frames)}: ultralytics {total_ref} boxes, ours {total_ours}, "
          f"matched {matched} (IoU>=0.9, same class), worst IoU {worst:.4f}, "
          f"max conf gap {conf_gap:.4f}")
    print(f"onnxruntime CPU {1000 * np.median(timings):.0f} ms/frame (median)")

    out = ROOT / "rust" / "fixtures" / "detector_local.json"
    out.write_text(json.dumps({"imgsz": IMGSZ, "conf": CONF, "iou": IOU,
                               "classes": list(CLASSES), "frames": golden}),
                   encoding="utf-8")
    print(f"wrote {out.relative_to(ROOT)} (local-only: refers to your frames)")
    if matched != total_ref or total_ours != total_ref:
        sys.exit("decoder does not reproduce ultralytics")


if __name__ == "__main__":
    main()
