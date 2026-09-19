"""Sharpness the way this photographer rates it, learned from frames they rated.

The hand-built measure in sharpness.py answers "how much focus energy is in
the car". What someone wants from a cull is "would I keep this", and the two
differ in ways no single formula captures: how much softness a fast pan
forgives, how hard a long lens is judged, what is acceptable on a distant
car. So the training screen asks for a one-to-five rating of the subject,
sharpness.py describes each crop as a short vector of numbers (see
``FEATURE_NAMES`` there), and this fits a ridge regression from one to the
other.

Deliberately small. A few hundred ratings cannot support anything bigger than
a linear model on a dozen-odd hand-built features, and a linear model is a
list of numbers -- it is stored as JSON, read by anything, and can be
reimplemented in any language in ten lines. The features are the part that
has to match, which is why they carry a version.

With too few ratings there is no model at all, and the measure carries on as
before: a confident fit to fifteen frames is worse than none.
"""

from __future__ import annotations

import json
import threading

import numpy as np

from .config import MODEL_DIR

MODEL_NAME = "sharpness.json"

# Bump when sharpness._features changes meaning or length. A model or a label
# stored under another version describes a different vector and is ignored.
FEATURE_VERSION = 1

# Below this the fit is noise. Fewer than the 200 taste.py wants, because
# this has eighteen inputs rather than 384.
MIN_LABELS = 60

# Ridge penalty on standardised features. Swept 0.1 to 30 on cross-validated
# error; the surface is flat between 1 and 5.
PENALTY = 2.0

LOWEST, HIGHEST = 1, 5

_lock = threading.Lock()
# -1 is a stamp no file has, so the first call, and any call after a save or
# a forget, always reads the disk -- None is taken, it means "no file".
_cache: dict = {"stamp": -1, "model": None}


def model_path():
    return MODEL_DIR / MODEL_NAME


def fit(vectors: list, stars: list) -> dict | None:
    """Ridge regression from feature vectors onto the ratings given by hand."""
    if len(vectors) < MIN_LABELS or len(vectors) != len(stars):
        return None
    features = np.asarray(vectors, dtype=np.float64)
    target = np.asarray(stars, dtype=np.float64)
    if features.ndim != 2 or len(set(target.tolist())) < 2:
        return None

    mean = features.mean(axis=0)
    spread = np.maximum(features.std(axis=0), 1e-6)
    design = np.hstack([(features - mean) / spread, np.ones((len(features), 1))])
    penalty = PENALTY * np.eye(design.shape[1])
    penalty[-1, -1] = 0.0               # the intercept is not shrunk
    try:
        weights = np.linalg.solve(design.T @ design + penalty, design.T @ target)
    except np.linalg.LinAlgError:
        return None
    return {"version": FEATURE_VERSION, "trained_on": int(len(target)),
            "mean": mean.tolist(), "spread": spread.tolist(),
            "weights": weights[:-1].tolist(), "intercept": float(weights[-1])}


def predict(model: dict | None, vector) -> float | None:
    """The rating this photographer would give, on the one-to-five scale but
    not rounded or clamped -- the caller decides what to do with 0.7."""
    if not model or model.get("version") != FEATURE_VERSION or not len(vector):
        return None
    mean = np.asarray(model["mean"])
    if mean.size != len(vector):
        return None
    z = (np.asarray(vector, dtype=np.float64) - mean) / np.asarray(model["spread"])
    return float(z @ np.asarray(model["weights"]) + model["intercept"])


def agreement(vectors: list, stars: list, baseline: list, folds: int = 5) -> dict | None:
    """Cross-validated agreement with the ratings, next to the measure it would
    replace.

    ``baseline`` is what the hand-built measure gives those same frames, in
    stars. Scoring the fit on frames it was fitted to would report something
    flattering and meaningless, so every prediction here is for a frame its
    model never saw.
    """
    if len(vectors) < MIN_LABELS:
        return None
    features = np.asarray(vectors, dtype=np.float64)
    target = np.asarray(stars, dtype=np.float64)
    base = np.asarray(baseline, dtype=np.float64)
    order = np.random.default_rng(0).permutation(len(target))
    features, target, base = features[order], target[order], base[order]

    predicted = np.zeros(len(target))
    index = np.arange(len(target))
    for fold in range(folds):
        train, test = index[index % folds != fold], index[index % folds == fold]
        model = fit(features[train].tolist(), target[train].tolist())
        if not model:
            return None
        for i in test:
            predicted[i] = predict(model, features[i])
    predicted = np.clip(np.rint(predicted), LOWEST, HIGHEST)

    def score(guess):
        return {"exact": float((guess == target).mean()),
                "within_one": float((np.abs(guess - target) <= 1).mean()),
                "mean_error": float(np.abs(guess - target).mean())}

    return {"n": int(len(target)), "model": score(predicted), "measure": score(base)}


def save(model: dict) -> None:
    with _lock:
        model_path().parent.mkdir(parents=True, exist_ok=True)
        model_path().write_text(json.dumps(model), encoding="utf-8")
        _cache["stamp"] = -1


def forget() -> None:
    """Back to the measure alone."""
    with _lock:
        model_path().unlink(missing_ok=True)
        _cache["stamp"] = -1


def current() -> dict | None:
    """The saved model, reread only when the file changes.

    Called for every crop, so a scan running while someone trains picks the
    new model up on its next frame without a restart -- and a stat is what it
    costs when nothing changed.
    """
    path = model_path()
    try:
        stamp = path.stat().st_mtime_ns
    except OSError:
        stamp = None
    with _lock:
        if stamp != _cache["stamp"]:
            model = None
            if stamp is not None:
                try:
                    loaded = json.loads(path.read_text(encoding="utf-8"))
                    if isinstance(loaded, dict) and loaded.get("weights"):
                        model = loaded
                except (OSError, ValueError):
                    model = None
            _cache["stamp"], _cache["model"] = stamp, model
        return _cache["model"]
