"""OpenCV's own YuNet results on the photographer's frames, for conrod-vision's
face decode to be checked against. Local-only.

    <venv with opencv>/python tools/gen_faces_local.py

Scans preview frames until it has ~40 that contain a face, and records
FaceDetectorYN's boxes and eye landmarks at the same 640 letterbox the Rust
side uses.
"""
import json, os, random, sqlite3
from pathlib import Path

import cv2
import numpy as np

HOME = Path(os.environ.get("CONROD_HOME") or Path.home() / ".conrod")
MODEL = HOME / "models" / "face_detection_yunet_2023mar.onnx"
OUT = Path(__file__).resolve().parent.parent / "rust" / "fixtures" / "faces_local.json"

db = sqlite3.connect(f"file:{HOME / 'conrod.db'}?mode=ro", uri=True)
frames = [p for (p,) in db.execute("SELECT preview_path FROM images WHERE preview_path IS NOT NULL")
          if Path(p).exists()]
random.Random(5).shuffle(frames)
det = cv2.FaceDetectorYN.create(str(MODEL), "", (640, 640), 0.6, 0.3, 5000)
cases = []
for path in frames:
    bgr = cv2.imread(path)
    h, w = bgr.shape[:2]
    s = 640 / max(w, h)
    small = cv2.resize(bgr, (round(w * s), round(h * s)), interpolation=cv2.INTER_LINEAR)
    canvas = np.zeros((640, 640, 3), np.uint8)
    canvas[:small.shape[0], :small.shape[1]] = small
    _, faces = det.detect(canvas)
    if faces is None:
        continue
    cases.append({"frame": path, "faces": [
        {"score": float(f[14]), "bbox": [float(f[0] / s), float(f[1] / s),
                                         float((f[0] + f[2]) / s), float((f[1] + f[3]) / s)],
         "eyes": [[float(f[4] / s), float(f[5] / s)], [float(f[6] / s), float(f[7] / s)]]}
        for f in faces]})
    if len(cases) >= 40:
        break
OUT.write_text(json.dumps({"cases": cases}), encoding="utf-8")
print(f"{OUT.name}: {len(cases)} frames with {sum(len(c['faces']) for c in cases)} faces")
