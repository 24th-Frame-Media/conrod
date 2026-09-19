"""Freeze what the Python implementation does, as JSON the Rust port must match.

    python tools/gen_golden.py

Writes rust/fixtures/*.json. Each file is a list of cases, each case an input
and the output Python produced for it, so a port is checked against behaviour
rather than against a reading of the source. Regenerate (and read the diff)
whenever the Python changes on purpose; a Rust test failing after a
regeneration means the two have diverged.

Only deterministic, dependency-light modules belong here. Anything that needs
real photographs stays out of git and is covered by the parity harness.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(ROOT))

from conrod import framing, sharp_model, taste  # noqa: E402

OUT = ROOT / "rust" / "fixtures"


def write(name: str, payload, compact: bool = False) -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    path = OUT / f"{name}.json"
    text = (json.dumps(payload, sort_keys=True, separators=(",", ":")) if compact
            else json.dumps(payload, indent=1, sort_keys=True))
    path.write_text(text + "\n", encoding="utf-8")
    print(f"{path.relative_to(ROOT)}  {len(payload.get('cases', payload))} entries")


def framing_cases() -> dict:
    frames = [(4000, 3000), (6960, 4640), (1920, 1080), (5, 5), (0, 0), (-1, 100)]
    boxes = [
        None,
        (800, 600, 3200, 2400),          # air on every side
        (2000, 600, 4000, 2400),         # one edge
        (0, 0, 1200, 900),               # a corner
        (0, 0, 4000, 3000),              # the whole frame
        (3, 600, 3200, 2400),            # a few pixels short of the edge
        (16, 600, 3200, 2400),           # exactly on the tolerance (0.004 * 4000)
        (16.0001, 600, 3200, 2400),      # just past it
        (0, 12, 3984, 2988),             # three edges, y tolerance edge
        (-50, -50, 4050, 3050),          # box larger than the frame
        (10, 10, 10, 10),                # degenerate
    ]
    cases = []
    for width, height in frames:
        for box in boxes:
            out = framing.assess(box, width, height)
            cases.append({
                "box": list(box) if box else None,
                "width": width, "height": height,
                "sides": out.sides, "factor": out.factor,
                "cut_off": out.cut_off, "describe": framing.describe(out),
            })
    return {"cases": cases}


def _ridge_data(n: int, dims: int, seed: int, unit: bool):
    rng = np.random.default_rng(seed)
    x = rng.normal(size=(n, dims))
    if unit:
        x /= np.linalg.norm(x, axis=1, keepdims=True)
    truth = rng.normal(size=dims)
    y = np.clip(np.rint(3 + x @ truth * (2.0 if unit else 0.5)
                        + rng.normal(0, 0.3, n)), 1, 5)
    return x, y


def ridge_cases() -> dict:
    """sharp_model (standardised features) and taste (unit embeddings): the
    two ridge flavours in the app, each with held-out predictions."""
    cases = []
    for seed, n, dims in ((1, 80, 18), (2, 150, 18), (3, 250, 18)):
        x, y = _ridge_data(n, dims, seed, unit=False)
        model = sharp_model.fit(x.tolist(), y.tolist())
        probe = np.random.default_rng(seed + 100).normal(size=(12, dims))
        cases.append({
            "kind": "sharp_model", "vectors": x.tolist(), "stars": y.tolist(),
            "model": model, "probe": probe.tolist(),
            "predictions": [sharp_model.predict(model, v) for v in probe.tolist()],
        })
    for seed, n, dims in ((4, 210, 16), (5, 220, 24)):
        x, y = _ridge_data(n, dims, seed, unit=True)
        model = taste.fit(x.tolist(), y.tolist())
        probe = np.random.default_rng(seed + 100).normal(size=(12, dims))
        probe /= np.linalg.norm(probe, axis=1, keepdims=True)
        cases.append({
            "kind": "taste", "vectors": x.tolist(), "stars": y.tolist(),
            "model": model, "probe": probe.tolist(),
            "predictions": [taste.predict(model, v) for v in probe.tolist()],
        })
    # Refusals: too few ratings, and ratings that are all one value.
    x, y = _ridge_data(30, 18, 9, unit=False)
    cases.append({"kind": "sharp_model", "vectors": x.tolist(), "stars": y.tolist(),
                  "model": sharp_model.fit(x.tolist(), y.tolist()),
                  "probe": [], "predictions": []})
    x, _ = _ridge_data(100, 18, 9, unit=False)
    cases.append({"kind": "sharp_model", "vectors": x.tolist(), "stars": [3.0] * 100,
                  "model": sharp_model.fit(x.tolist(), [3.0] * 100),
                  "probe": [], "predictions": []})
    return {"cases": cases}


def main() -> None:
    write("framing", framing_cases())
    write("ridge", ridge_cases(), compact=True)


if __name__ == "__main__":
    main()
