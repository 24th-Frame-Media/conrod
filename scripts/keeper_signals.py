#!/usr/bin/env python
"""Which per-frame signal predicts the frame of a burst the photographer kept?

Reads every ``target/eval/<slug>/frames.csv`` and ``subjects.csv`` written by
the ``export_recall`` harness and, for each burst of size >= 2 that contains
exactly one exported frame, asks each candidate key whether it ranks that
exported frame first (ties counted fractionally) and what percentile rank it
gives it. Random play scores ``1/size`` and a 0.5 mean percentile.

Keys come from the frame's ``set_frame_rating`` subject; where a frame has
subjects but none sets the rating the highest-rated subject stands in, and a
frame with no subjects falls back to its own row (its subject keys are then
missing and the burst is skipped for that key).

If numpy is present, a leave-one-shoot-out pairwise logistic model over the
standardised keys is fitted and its held-out top-1 reported.

    .venv\\Scripts\\python.exe scripts\\keeper_signals.py
"""

import collections
import csv
import math
import os
import sys

ROOT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "target", "eval")

# key -> 1 when the larger value should win
KEYS = [
    ("burst_pick", 1),
    ("frame_rating", 1),
    ("subject_rating", 1),
    ("tile_score", 1),
    ("neg_blur_worst", 1),
    ("neg_blur_pooled", 1),
    ("box_fraction", 1),
    ("conf", 1),
    ("neg_centre_dist", 1),
    ("framing_factor", 1),
    ("background", 1),
    ("panning", 1),
    ("subject_count", 1),
    ("pos_first", 1),
    ("pos_middle", 1),
    ("pos_last", 1),
    ("whole_score", 1),
]


def num(text, default=float("nan")):
    try:
        v = float(text)
    except (TypeError, ValueError):
        return default
    return v


def load(shoot_dir):
    with open(os.path.join(shoot_dir, "frames.csv"), newline="", encoding="utf-8") as f:
        frames = list(csv.DictReader(f))
    subj_path = os.path.join(shoot_dir, "subjects.csv")
    subs = collections.defaultdict(list)
    if os.path.exists(subj_path):
        with open(subj_path, newline="", encoding="utf-8") as f:
            for r in csv.DictReader(f):
                subs[r["file"].lower()].append(r)
    return frames, subs


def pick_subject(rows):
    if not rows:
        return None
    setters = [r for r in rows if r["set_frame_rating"] == "1"]
    if setters:
        return setters[0]
    return max(rows, key=lambda r: num(r["rating"], -1.0))


def features(row, subj):
    s = subj or {}
    w, h = num(s.get("frame_w")), num(s.get("frame_h"))
    cx, cy = num(s.get("cx")), num(s.get("cy"))
    centre = math.hypot(cx - 0.5, cy - 0.5) if w == w and h == h else float("nan")
    return {
        "burst_pick": num(row["burst_pick"]),
        "frame_rating": num(row["rating"]),
        "subject_rating": num(s.get("rating")),
        "tile_score": num(s.get("tile_score")),
        "neg_blur_worst": -num(s.get("blur_worst_ref"))
        if s.get("blur_worst_ref", "NaN") not in ("NaN", "")
        else float("nan"),
        "neg_blur_pooled": -num(s.get("blur_pooled_ref"))
        if s.get("blur_pooled_ref", "NaN") not in ("NaN", "")
        else float("nan"),
        "box_fraction": num(s.get("box_fraction"), num(row["score"]) * 0.0),
        "conf": num(s.get("conf")),
        "neg_centre_dist": -centre,
        "framing_factor": num(s.get("framing_factor")),
        "background": num(s.get("background")),
        "panning": num(s.get("panning"), num(row["panning"])),
        "subject_count": num(row["subjects"]),
        "whole_score": num(row["score"]),
    }


def bursts_of(frames, subs):
    """(size, [features dicts], exported index) for bursts of size >= 2 with
    exactly one exported frame."""
    groups = collections.defaultdict(list)
    for r in frames:
        if r["burst_key"] in ("", "-1"):
            continue
        groups[r["burst_key"]].append(r)
    out = []
    for _, rows in groups.items():
        if len(rows) < 2:
            continue
        if sum(1 for r in rows if r["exported"] == "1") != 1:
            continue
        rows.sort(key=lambda r: r["file"].lower())
        feats = []
        for r in rows:
            subj = pick_subject(subs.get(r["file"].lower(), []))
            f = features(r, subj)
            f["pos_first"] = 1.0 if r is rows[0] else 0.0
            f["pos_middle"] = 1.0 if 0 < rows.index(r) < len(rows) - 1 else 0.0
            f["pos_last"] = 1.0 if r is rows[-1] else 0.0
            feats.append(f)
        exp = next(i for i, r in enumerate(rows) if r["exported"] == "1")
        out.append((len(rows), feats, exp))
    return out


def outcome(values, exp):
    pairs = [(v, i) for i, v in enumerate(values) if v == v]
    if not pairs or exp not in [i for _, i in pairs]:
        return None
    vals = [v for v, _ in pairs]
    v = values[exp]
    n = len(vals)
    top = max(vals)
    hit = 1.0 / sum(1 for x in vals if x == top) if v == top else 0.0
    rank = 1.0 + sum(1 for x in vals if x > v) + (sum(1 for x in vals if x == v) - 1) / 2.0
    pct = (rank - 1.0) / (n - 1) if n > 1 else 0.5
    return hit, pct, n


def score_key(bursts, key):
    hits, pcts, bases = [], [], []
    for size, feats, exp in bursts:
        r = outcome([f[key] for f in feats], exp)
        if r is None:
            continue
        hit, pct, n = r
        hits.append(hit)
        pcts.append(pct)
        bases.append(1.0 / n)
    if not hits:
        return None
    n = len(hits)
    lift = [h - b for h, b in zip(hits, bases)]
    mean_lift = sum(lift) / n
    if n > 1:
        var = sum((x - mean_lift) ** 2 for x in lift) / (n - 1)
        se = math.sqrt(var / n)
    else:
        se = float("nan")
    z = mean_lift / se if se and se == se and se > 0 else 0.0
    return (n, sum(hits) / n, sum(bases) / n, sum(pcts) / n, z)


def table(all_shoots):
    print("\nPer shoot: bursts used / top-1 hit rate (random baseline) / mean percentile rank")
    names = [n for n, _ in all_shoots]
    per = {}
    for name, bursts in all_shoots:
        per[name] = {k: score_key(bursts, k) for k, _ in KEYS}
    print(f"\n{'key':18s} {'n':>4s} {'hit':>6s} {'rand':>6s} {'pct':>6s} {'z':>6s}  " + "  ".join(f"{n[:16]:>16s}" for n in names) + "   (hit % vs random)")
    for key, _ in KEYS:
        pooled_bursts = [b for _, bs in all_shoots for b in bs]
        p = score_key(pooled_bursts, key)
        if p is None:
            print(f"{key:18s} no data")
            continue
        n, hit, base, pct, z = p
        cells = []
        for name in names:
            q = per[name][key]
            cells.append(f"{100*q[1]:5.1f} vs {100*q[2]:4.1f}" if q else "     -       ")
        print(f"{key:18s} {n:4d} {100*hit:5.1f}% {100*base:5.1f}% {100*pct:5.1f}% {z:+6.2f}  " + "  ".join(f"{c:>16s}" for c in cells))


# --- model -----------------------------------------------------------------

def model(all_shoots):
    try:
        import numpy as np
    except ImportError:
        print("\nnumpy not installed: skipping the leave-one-shoot-out model")
        return
    keys = [k for k, _ in KEYS]
    k = len(keys)

    def matrix(bursts):
        rows, labels, owners = [], [], []
        for bi, (_, feats, exp) in enumerate(bursts):
            for i, f in enumerate(feats):
                rows.append([f[key] for key in keys])
                labels.append(1.0 if i == exp else 0.0)
                owners.append(bi)
        return np.array(rows, dtype=float), np.array(labels), np.array(owners)

    def pairs(Z, y, own, bursts):
        diffs, tags = [], []
        for bi in range(len(bursts)):
            idx = np.where(own == bi)[0]
            yb = y[idx]
            for a in range(len(idx)):
                for b in range(a + 1, len(idx)):
                    diffs.append(Z[idx[a]] - Z[idx[b]])
                    tags.append(1.0 if yb[a] > yb[b] else 0.0)
        return np.array(diffs), np.array(tags)

    def fit(D, Y, lam=1e-3, iters=6000, lr=0.1):
        w = np.zeros(k)
        for _ in range(iters):
            p = 1.0 / (1.0 + np.exp(-D @ w))
            g = D.T @ (p - Y) / len(D) + lam * w
            w -= lr * g
            if np.max(np.abs(g)) < 1e-6:
                break
        return w

    def hits_of(scores, y, own):
        hits, bases = [], []
        for bi in np.unique(own):
            idx = np.where(own == bi)[0]
            vals = scores[idx]
            exp = int(np.argmax(y[idx]))
            top = vals.max()
            hits.append(1.0 / sum(1 for v in vals if v == top) if vals[exp] == top else 0.0)
            bases.append(1.0 / len(vals))
        return hits, bases

    def top1(scores, y, own, bursts):
        hits, bases = hits_of(scores, y, own)
        return sum(hits) / len(hits), sum(bases) / len(bases)

    data = {}
    start = 0
    for name, bursts in all_shoots:
        X, y, own = matrix(bursts)
        data[name] = (X, y, own + start, bursts)
        start += len(bursts)

    print("\nLeave-one-shoot-out pairwise logistic over all keys")
    pooled_hits = []
    for hold in [n for n, _ in all_shoots]:
        Xtr = np.vstack([data[n][0] for n in data if n != hold])
        Xte, yte, own, bursts = data[hold]
        med = np.nanmedian(Xtr, axis=0)
        med = np.where(np.isnan(med), 0.0, med)
        Xtr = np.where(np.isnan(Xtr), med, Xtr)
        Xte = np.where(np.isnan(Xte), med, Xte)
        mu, sd = Xtr.mean(axis=0), Xtr.std(axis=0)
        sd = np.where(sd < 1e-12, 1.0, sd)
        Ztr, Zte = (Xtr - mu) / sd, (Xte - mu) / sd
        D, Y = pairs(Ztr, np.concatenate([data[n][1] for n in data if n != hold]),
                     np.concatenate([data[n][2] for n in data if n != hold]),
                     [b for n in data if n != hold for b in data[n][3]])
        w = fit(D, Y)
        train_acc = (((1 / (1 + np.exp(-D @ w))) > 0.5) == (Y > 0.5)).mean()
        scores = Zte @ w
        h, b = top1(scores, yte, own, bursts)
        # keep the per-burst hits so the pooled figure weights bursts equally
        pooled_hits.extend(hits_of(scores, yte, own)[0])
        print(
            f"  hold out {hold[:44]:44s} bursts {len(bursts):3d}  hit {100*h:5.1f}%"
            f"  rand {100*b:5.1f}%  train pair acc {100*train_acc:4.1f}%"
        )
    print(f"  pooled held-out top-1: {100*sum(pooled_hits)/len(pooled_hits):.1f}%")

    # coefficients from the full data, for reading the signs only
    Xall = np.vstack([data[n][0] for n in data])
    yall = np.concatenate([data[n][1] for n in data])
    oall = np.concatenate([data[n][2] for n in data])
    ball = [b for n in data for b in data[n][3]]
    med = np.nanmedian(Xall, axis=0)
    med = np.where(np.isnan(med), 0.0, med)
    Xall = np.where(np.isnan(Xall), med, Xall)
    mu, sd = Xall.mean(axis=0), Xall.std(axis=0)
    sd = np.where(sd < 1e-12, 1.0, sd)
    Zall = (Xall - mu) / sd
    D, Y = pairs(Zall, yall, oall, ball)
    w = fit(D, Y)
    order = sorted(range(k), key=lambda i: -w[i])
    print("  full-fit standardised coefficients (pairwise, + favours the kept frame):")
    for i in order:
        print(f"    {keys[i]:18s} {w[i]:+.3f}")
    h, b = top1(Zall @ w, yall, oall, ball)
    print(f"  in-sample top-1 of the full fit: {100*h:.1f}% (random {100*b:.1f}%), "
          f"pair acc {100*((((1/(1+np.exp(-D@w)))>0.5)==(Y>0.5)).mean()):.1f}%")
    print("  note: frame_rating and subject_rating are collinear "
          f"(r={np.corrcoef(Zall[:, 1], Zall[:, 2])[0,1]:.3f}), so their "
          "coefficients split between them.")


def main():
    all_shoots = []
    if not os.path.isdir(ROOT):
        sys.exit(f"no {ROOT}: run the export_recall harness first")
    for name in sorted(os.listdir(ROOT)):
        d = os.path.join(ROOT, name)
        if not os.path.exists(os.path.join(d, "frames.csv")):
            continue
        frames, subs = load(d)
        all_shoots.append((name, bursts_of(frames, subs)))
        print(f"{name}: {len(frames)} frames, {len(all_shoots[-1][1])} bursts used")
    table(all_shoots)
    model(all_shoots)
    print("\nbursts used = size >= 2 and exactly one exported frame; hit rate is the")
    print("fractional chance the exported frame ranks first, ties split.")


if __name__ == "__main__":
    main()
