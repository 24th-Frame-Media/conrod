#!/usr/bin/env python
"""Build a ground-truth manifest of a photographer's keeper photos.

Walks the RAW directories (read-only), matches exported JPEGs to their source
RAW files by stem, and extracts Lightroom/ACR flags from the ``.xmp`` sidecars
that sit next to many RAWs.

Outputs (all under ``target/groundtruth``):
  * ``labels.json``           - per-shoot counts plus the labelled frames
  * ``unmatched_exports.txt`` - exports that resolved to no RAW

Standard library only.  Never reads image pixel data.
"""

import argparse
import json
import os
import re
import sys
from pathlib import Path

RAW_EXTS = {".cr3", ".dng"}
EXPORT_EXTS = {".jpg", ".jpeg"}
MIN_PREFIX = 6  # shortest allowed stem prefix when a RAW stem is a prefix of an export stem

REPO_ROOT = Path(__file__).resolve().parent.parent
OUT_DIR = REPO_ROOT / "target" / "groundtruth"

# (raw root, export root or None = auto-discover 01_RAW/02_Exports pairs)
DEFAULT_PAIRS = [
    (r"D:\Personal Work\2026", r"D:\Personal Work\Exported"),
    (r"D:\24th Frame Media\01_Clients", None),
    (r"D:\24th Frame Media\02_Projects", None),
    (r"D:\Client Work\Client A\2026", r"D:\Client Work\Client A\Exported"),
]

RE_RATING_ATTR = re.compile(r"""xmp:Rating\s*=\s*["']\s*(-?\d+)\s*["']""")
RE_RATING_ELEM = re.compile(r"<xmp:Rating>\s*(-?\d+)\s*</xmp:Rating>")
RE_LABEL_ATTR = re.compile(r"""xmp:Label\s*=\s*["']([^"']*)["']""")
RE_LABEL_ELEM = re.compile(r"<xmp:Label>\s*([^<]*?)\s*</xmp:Label>")
RE_HASSETTINGS = re.compile(r"""crs:HasSettings\s*=\s*["']True["']""", re.IGNORECASE)


# --------------------------------------------------------------------------- #
# directory discovery
# --------------------------------------------------------------------------- #
def iter_dirs(top):
    """Yield (dirpath, filenames) recursively.  scandir only, no stat."""
    stack = [top]
    while stack:
        cur = stack.pop()
        try:
            with os.scandir(cur) as it:
                files = []
                for entry in it:
                    try:
                        if entry.is_dir(follow_symlinks=False):
                            stack.append(entry.path)
                            continue
                        if entry.is_file(follow_symlinks=False):
                            files.append(entry.name)
                    except OSError:
                        continue
        except OSError:
            continue
        yield cur, files


def discover_pairs(root):
    """Find <x>/01_RAW -> <x>/02_Exports pairs beneath an umbrella directory."""
    pairs = []
    for dirpath, files in iter_dirs(root):
        names = []
        try:
            with os.scandir(dirpath) as it:
                names = [e.name for e in it if e.is_dir(follow_symlinks=False)]
        except OSError:
            continue
        raw_dir = next((n for n in names if n.lower() == "01_raw"), None)
        if not raw_dir:
            continue
        exp_dir = next((n for n in names if n.lower() == "02_exports"), None)
        pair = (os.path.join(dirpath, raw_dir), os.path.join(dirpath, exp_dir) if exp_dir else None)
        pairs.append(pair)
        # do not descend into the pair; it is handled as its own scope
        for skip in (raw_dir, exp_dir):
            if skip:
                try:
                    names.remove(skip)
                except ValueError:
                    pass
    return pairs


def build_scopes(specs):
    """Turn CLI/default root specs into (raw_dir, export_dir|None) pairs."""
    scopes, seen = [], set()
    for spec in specs:
        raw, exp = spec.strip(), ""
        if "=" in spec:
            raw, exp = spec.split("=", 1)
            raw, exp = raw.strip(), exp.strip()

        if exp:  # explicit RAW=EXPORT pair
            cands = [(raw, exp)]
        else:  # plain directory: use the built-in mapping, else auto-discover
            known = next(
                ((r, e) for r, e in DEFAULT_PAIRS if os.path.normcase(os.path.normpath(r)) == os.path.normcase(os.path.normpath(raw))),
                None,
            )
            if known and known[1]:
                cands = [known]
            else:
                cands = discover_pairs(raw)
                if not cands:
                    parent, name = os.path.split(os.path.normpath(raw))
                    sib = os.path.join(parent, "02_Exports" if name.lower() == "01_raw" else "Exported")
                    cands = [(raw, sib if os.path.isdir(sib) else None)]
        for pair in cands:
            key = os.path.normcase(pair[0])
            if key in seen:
                continue
            seen.add(key)
            scopes.append(pair)
    return scopes


# --------------------------------------------------------------------------- #
# metadata
# --------------------------------------------------------------------------- #
def parse_xmp(path):
    """Return (rating:int|None, label:str|None, edited:bool, found:bool)."""
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as fh:
            text = fh.read()
    except OSError:
        return None, None, False, False

    rating = None
    m = RE_RATING_ATTR.search(text) or RE_RATING_ELEM.search(text)
    if m:
        try:
            rating = int(m.group(1))
        except ValueError:
            rating = None

    label = None
    m = RE_LABEL_ATTR.search(text) or RE_LABEL_ELEM.search(text)
    if m:
        label = m.group(1).strip() or None

    return rating, label, bool(RE_HASSETTINGS.search(text)), True


def collect(scope_raw, scope_exp):
    """Scan one RAW tree (plus its export tree) and return frame records."""
    raw_by_dir = {}          # shoot dir -> [record]
    xmp_by_dir = {}          # shoot dir -> {lower stem: path}
    export_index = {}        # lower stem -> [export paths]
    exports_flat = []

    for dirpath, files in iter_dirs(scope_raw):
        for name in files:
            stem, ext = os.path.splitext(name)
            ext = ext.lower()
            if ext == ".xmp":
                xmp_by_dir.setdefault(dirpath, {})[stem.lower()] = os.path.join(dirpath, name)
            elif ext in RAW_EXTS:
                raw_by_dir.setdefault(dirpath, []).append(
                    {
                        "path": os.path.join(dirpath, name),
                        "stem": stem,
                        "shoot": dirpath,
                        "rating": None,
                        "label": None,
                        "edited": False,
                        "exported": False,
                        "export_paths": [],
                        "_has_xmp": False,
                    }
                )

    if scope_exp and os.path.isdir(scope_exp):
        for dirpath, files in iter_dirs(scope_exp):
            for name in files:
                stem, ext = os.path.splitext(name)
                if ext.lower() not in EXPORT_EXTS:
                    continue
                p = os.path.join(dirpath, name)
                exports_flat.append(p)
                export_index.setdefault(stem.lower(), []).append(p)

    # attach sidecar metadata
    for shoot_dir, recs in raw_by_dir.items():
        sidecars = xmp_by_dir.get(shoot_dir, {})
        for rec in recs:
            xp = sidecars.get(rec["stem"].lower())
            if xp is None:
                continue
            rating, label, edited, found = parse_xmp(xp)
            if not found:
                continue
            rec["_has_xmp"] = True
            rec["rating"] = rating
            rec["label"] = label
            rec["edited"] = edited

    # match exports -> raws
    all_recs = [r for recs in raw_by_dir.values() for r in recs]
    raw_stems = {r["stem"].lower(): r for r in all_recs}
    unmatched = []

    for exp_stem, paths in export_index.items():
        target = raw_stems.get(exp_stem)
        if target is None:
            for length in range(len(exp_stem) - 1, MIN_PREFIX - 1, -1):
                target = raw_stems.get(exp_stem[:length])
                if target is not None:
                    break
        if target is None:
            unmatched.extend(paths)
            continue
        target["exported"] = True
        target["export_paths"].extend(paths)

    return raw_by_dir, all_recs, unmatched


# --------------------------------------------------------------------------- #
# reporting
# --------------------------------------------------------------------------- #
def trunc(path, width):
    if len(path) <= width:
        return path
    return "..." + path[-(width - 3):]


def print_table(shoots, unmatched_total):
    hdr = "{:<52} {:>7} {:>7} {:>7} {:>7}  {}".format(
        "shoot", "raws", "exp", "xmp", "edit", "ratings"
    )
    print(hdr)
    print("-" * len(hdr))
    for s in shoots:
        hist = s["_hist"]
        rs = " ".join("{}:{}".format(k, v) for k, v in sorted(hist.items(), key=lambda kv: (kv[0] is None, kv[0]), reverse=True))
        print(
            "{:<52} {:>7} {:>7} {:>7} {:>7}  {}".format(
                trunc(s["dir"], 52), s["raws"], s["exported"], s["_xmp"], s["_edited"], rs or "-"
            )
        )
    print("-" * len(hdr))
    tot = sum(s["raws"] for s in shoots)
    exp = sum(s["exported"] for s in shoots)
    xmp = sum(s["_xmp"] for s in shoots)
    edt = sum(s["_edited"] for s in shoots)
    rated = sum(s["rated"] for s in shoots)
    print(
        "{:<52} {:>7} {:>7} {:>7} {:>7}  rated={}".format("TOTAL", tot, exp, xmp, edt, rated)
    )
    print("unmatched exports: {}".format(unmatched_total))


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument(
        "--root",
        nargs="*",
        default=None,
        metavar="RAW[=EXPORT]",
        help="override roots; RAW=EXPORT pairs, or a directory (auto discovers 01_RAW/02_Exports)",
    )
    ap.add_argument("--out", default=str(OUT_DIR), help="output directory")
    args = ap.parse_args(argv)

    specs = args.root if args.root else ["{}={}".format(r, e or "") for r, e in DEFAULT_PAIRS]
    scopes = build_scopes(specs)
    if not scopes:
        print("no scopes resolved", file=sys.stderr)
        return 1

    all_recs, unmatched, per_scope = [], [], []
    for raw_dir, exp_dir in scopes:
        if not os.path.isdir(raw_dir):
            print("skip (missing): {}".format(raw_dir), file=sys.stderr)
            continue
        raw_by_dir, recs, unexp = collect(raw_dir, exp_dir)
        all_recs.extend(recs)
        unmatched.extend(unexp)
        for shoot_dir, srecs in raw_by_dir.items():
            hist = {}
            for r in srecs:
                hist[r["rating"]] = hist.get(r["rating"], 0) + 1
            per_scope.append(
                {
                    "dir": shoot_dir,
                    "raws": len(srecs),
                    "exported": sum(1 for r in srecs if r["exported"]),
                    "rated": sum(1 for r in srecs if r["rating"] is not None),
                    "_xmp": sum(1 for r in srecs if r["_has_xmp"]),
                    "_edited": sum(1 for r in srecs if r["edited"]),
                    "_hist": hist,
                }
            )

    per_scope.sort(key=lambda s: s["dir"].lower())
    unmatched.sort(key=str.lower)

    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)

    frames = [
        {k: v for k, v in r.items() if not k.startswith("_")}
        for r in sorted(all_recs, key=lambda r: r["path"].lower())
        if r["exported"] or r["rating"] is not None or r["edited"]
    ]
    payload = {
        "shoots": [{k: v for k, v in s.items() if not k.startswith("_")} for s in per_scope],
        "frames": frames,
    }
    (out / "labels.json").write_text(
        json.dumps(payload, indent=1, ensure_ascii=False), encoding="utf-8"
    )
    (out / "unmatched_exports.txt").write_text("\n".join(unmatched) + ("\n" if unmatched else ""), encoding="utf-8")

    print_table(per_scope, len(unmatched))
    print("frames written: {}  ->  {}".format(len(frames), out / "labels.json"))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
