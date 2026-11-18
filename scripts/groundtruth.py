#!/usr/bin/env python
"""Build a ground-truth manifest of a photographer's keeper photos.

Walks the RAW directories (read-only), matches exported JPEGs to their source
RAW files by stem, and extracts Lightroom/ACR flags from the ``.xmp`` sidecars
that sit next to many RAWs.

Both the working drive (D:) and the older archive (P:) are scanned with the
same layout rules.  A shoot that exists on both drives is the same shoot: frames
are de-duplicated on their path below the drive letter, the D: copy (the newer
one) is kept, and a frame counts as exported if either drive exported it.

Outputs (all under ``target/groundtruth``):
  * ``labels.json``           - per-shoot counts plus the labelled frames
  * ``unmatched_exports.txt`` - exports that resolved to no RAW

Standard library only.  Never reads image pixel data and never writes to the
source drives.
"""

import argparse
import json
import os
import re
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

RAW_EXTS = {".cr3", ".cr2", ".dng"}
EXPORT_EXTS = {".jpg", ".jpeg"}
MIN_PREFIX = 6  # shortest allowed stem prefix when a RAW stem is a prefix of an export stem

REPO_ROOT = Path(__file__).resolve().parent.parent
OUT_DIR = REPO_ROOT / "target" / "groundtruth"

# drives to scan, newest first (D: wins a de-duplication tie)
DRIVES = ("D", "P")
PERSONAL_YEARS = ("2026",)

# folder names that are never worth walking
SKIP_DIR_NAMES = {
    "temp", "tmp", "cache", "caches", ".cache", ".tmp", "trash", ".trash",
    "$recycle.bin", "@eadir", "lost+found", "previews", "catalog",
}

RE_RATING_ATTR = re.compile(r"""xmp:Rating\s*=\s*["']\s*(-?\d+)\s*["']""")
RE_RATING_ELEM = re.compile(r"<xmp:Rating>\s*(-?\d+)\s*</xmp:Rating>")
RE_LABEL_ATTR = re.compile(r"""xmp:Label\s*=\s*["']([^"']*)["']""")
RE_LABEL_ELEM = re.compile(r"<xmp:Label>\s*([^<]*?)\s*</xmp:Label>")
RE_HASSETTINGS = re.compile(r"""crs:HasSettings\s*=\s*["']True["']""", re.IGNORECASE)
RE_YEAR = re.compile(r"^\d{4}$")

# P: is slow on small random reads (roughly 60 ms per .xmp file serially), so
# the sidecars are read on a thread pool; the reads are pure I/O, no locks.
XMP_WORKERS = 32


# --------------------------------------------------------------------------- #
# directory discovery
# --------------------------------------------------------------------------- #
def skip_name(name):
    """AppleDouble droppings and temp/cache clutter."""
    n = name.lower().strip()
    if n.startswith("._"):
        return True
    return (
        n in SKIP_DIR_NAMES
        or n.startswith("temp")
        or n.endswith("_temp")
        or n.startswith("cache")
        or n.startswith("temp")
    )


def list_subdirs(path):
    """Immediate sub-directory names, scandir only, clutter skipped."""
    out = []
    try:
        with os.scandir(path) as it:
            for entry in it:
                try:
                    if entry.is_dir(follow_symlinks=False) and not skip_name(entry.name):
                        out.append(entry.name)
                except OSError:
                    continue
    except OSError:
        pass
    out.sort(key=str.lower)
    return out


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
                            if not skip_name(entry.name):
                                stack.append(entry.path)
                            continue
                        if entry.is_file(follow_symlinks=False):
                            if not skip_name(entry.name):
                                files.append(entry.name)
                    except OSError:
                        continue
        except OSError:
            continue
        yield cur, files


def discover_pairs(root):
    """Find <x>/01_RAW -> <x>/02_Exports pairs beneath an umbrella directory."""
    pairs = []
    stack = [root]
    while stack:
        cur = stack.pop()
        subs = list_subdirs(cur)
        raw_dir = next((n for n in subs if n.lower() == "01_raw"), None)
        if raw_dir:
            exp_dir = next((n for n in subs if n.lower() == "02_exports"), None)
            pairs.append(
                (os.path.join(cur, raw_dir), os.path.join(cur, exp_dir) if exp_dir else None)
            )
            # do not descend into the pair; it is handled as its own scope
            for name in subs:
                if name not in (raw_dir, exp_dir):
                    stack.append(os.path.join(cur, name))
        else:
            for name in subs:
                stack.append(os.path.join(cur, name))
    return pairs


def drive_pairs(drive):
    """Every (raw_root, export_root|None) pair for one drive, by listing only."""
    root = "{}:\\".format(drive)
    pairs = []

    personal = os.path.join(root, "Personal Work")
    personal_exp = os.path.join(personal, "Exported")
    if os.path.isdir(personal):
        for name in list_subdirs(personal):
            if name in PERSONAL_YEARS:
                pairs.append(
                    (os.path.join(personal, name),
                     personal_exp if os.path.isdir(personal_exp) else None)
                )

    tfm = os.path.join(root, "24th Frame Media")
    for umbrella in ("01_Clients", "02_Projects"):
        if os.path.isdir(os.path.join(tfm, umbrella)):
            pairs.append((os.path.join(tfm, umbrella), None))

    client_work = os.path.join(root, "Client Work")
    for client in list_subdirs(client_work):
        cdir = os.path.join(client_work, client)
        exp = os.path.join(cdir, "Exported")
        exp = exp if os.path.isdir(exp) else None
        years = [n for n in list_subdirs(cdir) if RE_YEAR.match(n)]
        if not years:
            pairs.append((cdir, exp))  # no YYYY level: the client folder is the root
            continue
        for name in years:
            pairs.append((os.path.join(cdir, name), exp))
    return pairs


def default_pairs():
    pairs = []
    for drive in DRIVES:
        pairs.extend(drive_pairs(drive))
    return pairs


def build_scopes(specs, known=()):
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
            known_pair = next(
                (
                    (r, e)
                    for r, e in known
                    if os.path.normcase(os.path.normpath(r))
                    == os.path.normcase(os.path.normpath(raw))
                ),
                None,
            )
            if known_pair and known_pair[1]:
                cands = [known_pair]
            else:
                cands = discover_pairs(raw)
                if not cands:
                    parent, name = os.path.split(os.path.normpath(raw))
                    sib = os.path.join(
                        parent, "02_Exports" if name.lower() == "01_raw" else "Exported"
                    )
                    cands = [(raw, sib if os.path.isdir(sib) else None)]
        for pair in cands:
            key = os.path.normcase(pair[0])
            if key in seen:
                continue
            seen.add(key)
            scopes.append(pair)
    return scopes


# --------------------------------------------------------------------------- #
# shoot type / identity
# --------------------------------------------------------------------------- #
def drive_of(path):
    return os.path.splitdrive(os.path.normpath(path))[0][:1].upper()


def frame_key(path):
    """De-duplication key: the path below the drive letter, case-insensitive."""
    _drive, rest = os.path.splitdrive(os.path.normpath(path))
    return rest.replace("/", "\\").lower()


def client_of(directory):
    """Which client/project a frame directory belongs to."""
    parts = Path(directory).parts
    if len(parts) < 2:
        return ""
    rest = parts[1:]
    low = [p.lower() for p in rest]

    if low[0] == "client work" and len(rest) >= 2:
        return rest[1]
    if low[0] == "24th frame media" and len(rest) >= 2:
        if low[1] == "01_clients" and len(rest) >= 3:
            return rest[2]
        if low[1] == "02_projects" and len(rest) >= 3:
            for i in range(3, len(low)):
                if low[i] == "01_raw":
                    return rest[i - 1]
            return rest[2]
        return rest[1]
    if low[0] == "personal work":
        return "Personal Work"
    return rest[0]


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
    unmatched = []

    for dirpath, files in iter_dirs(scope_raw):
        client = None
        for name in files:
            stem, ext = os.path.splitext(name)
            ext = ext.lower()
            if ext == ".xmp":
                xmp_by_dir.setdefault(dirpath, {})[stem.lower()] = os.path.join(dirpath, name)
            elif ext in RAW_EXTS:
                path = os.path.join(dirpath, name)
                if client is None:
                    client = client_of(dirpath)
                raw_by_dir.setdefault(dirpath, []).append(
                    {
                        "path": path,
                        "stem": stem,
                        "shoot": dirpath,
                        "client": client,
                        "drive": drive_of(path),
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
                export_index.setdefault(stem.lower(), []).append(p)

    # attach sidecar metadata (thread pool: P: is slow per file)
    pending = []
    for shoot_dir, recs in raw_by_dir.items():
        sidecars = xmp_by_dir.get(shoot_dir, {})
        for rec in recs:
            xp = sidecars.get(rec["stem"].lower())
            if xp is not None:
                pending.append((rec, xp))
    if pending:
        paths = [xp for _rec, xp in pending]
        with ThreadPoolExecutor(max_workers=XMP_WORKERS) as pool:
            results = list(pool.map(parse_xmp, paths))
        for (rec, _xp), (rating, label, edited, found) in zip(pending, results):
            if not found:
                continue
            rec["_has_xmp"] = True
            rec["rating"] = rating
            rec["label"] = label
            rec["edited"] = edited

    # match exports -> raws
    all_recs = [r for recs in raw_by_dir.values() for r in recs]
    raw_stems = {r["stem"].lower(): r for r in all_recs}

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
# de-duplication across drives
# --------------------------------------------------------------------------- #
def dedupe(records):
    """Collapse frames that exist on both drives; prefer the D: copy."""
    groups = {}
    for rec in records:
        groups.setdefault(frame_key(rec["path"]), []).append(rec)

    out, merged = [], 0
    for recs in groups.values():
        if len(recs) == 1:
            out.append(recs[0])
            continue
        merged += len(recs) - 1
        recs.sort(
            key=lambda r: (
                r["drive"] != "D",          # D: first (newer copy)
                not os.path.exists(r["path"]),
                r["path"].lower(),
            )
        )
        keep = recs[0]
        exports = list(keep["export_paths"])
        for other in recs[1:]:
            if other["exported"]:
                keep["exported"] = True
            for p in other["export_paths"]:
                if p not in exports:
                    exports.append(p)
            if not keep["_has_xmp"] and other["_has_xmp"]:
                keep["_has_xmp"] = True
                keep["rating"] = other["rating"]
                keep["label"] = other["label"]
                keep["edited"] = other["edited"]
            elif keep["rating"] is None and other["rating"] is not None:
                keep["rating"] = other["rating"]
            if keep["label"] is None and other["label"] is not None:
                keep["label"] = other["label"]
        keep["export_paths"] = sorted(exports, key=str.lower)
        out.append(keep)
    return out, merged


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

    started = time.time()
    pairs = default_pairs()
    if args.root:
        scopes = build_scopes(args.root, pairs)
    else:
        scopes = build_scopes(
            ["{}={}".format(r, e or "") for r, e in pairs], pairs
        )
    if not scopes:
        print("no scopes resolved", file=sys.stderr)
        return 1

    all_recs, unmatched, per_scope = [], [], []
    for i, (raw_dir, exp_dir) in enumerate(scopes, 1):
        if not os.path.isdir(raw_dir):
            print("skip (missing): {}".format(raw_dir), file=sys.stderr)
            continue
        print(
            "scan {}/{}: {} -> {}".format(
                i, len(scopes), trunc(raw_dir, 60), exp_dir or "(no export root)"
            ),
            flush=True,
        )
        raw_by_dir, recs, unexp = collect(raw_dir, exp_dir)
        all_recs.extend(recs)
        unmatched.extend(unexp)
        print(
            "  {} raws, {} exports".format(
                len(recs), sum(len(r["export_paths"]) for r in recs) + len(unexp)
            ),
            flush=True,
        )
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

    raw_total = len(all_recs)
    print("de-duplicating {} frames across {} ...".format(raw_total, "/".join(DRIVES)), flush=True)
    all_recs, merged = dedupe(all_recs)

    # rebuild the per-shoot table from the de-duplicated frames
    by_shoot = {}
    for r in all_recs:
        by_shoot.setdefault(r["shoot"], []).append(r)
    per_scope = []
    for shoot_dir, srecs in by_shoot.items():
        hist = {}
        for r in srecs:
            hist[r["rating"]] = hist.get(r["rating"], 0) + 1
        per_scope.append(
            {
                "dir": shoot_dir,
                "client": srecs[0]["client"],
                "drive": srecs[0]["drive"],
                "raws": len(srecs),
                "exported": sum(1 for r in srecs if r["exported"]),
                "rated": sum(1 for r in srecs if r["rating"] is not None),
                "_xmp": sum(1 for r in srecs if r["_has_xmp"]),
                "_edited": sum(1 for r in srecs if r["edited"]),
                "_hist": hist,
            }
        )

    # the same export path must not be reported twice
    seen = set()
    deduped_unmatched = []
    for p in unmatched:
        k = os.path.normcase(p)
        if k in seen:
            continue
        seen.add(k)
        deduped_unmatched.append(p)
    unmatched = deduped_unmatched

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
    matched_exports = sum(len(f["export_paths"]) for f in frames)
    print("frames written: {}  ->  {}".format(len(frames), out / "labels.json"))
    print(
        "raws={} (deduped from {}) matched_exports={} unmatched_exports={} frames_merged={}".format(
            len(all_recs), raw_total, matched_exports, len(unmatched), merged
        )
    )
    print("elapsed: {:.1f}s".format(time.time() - started))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
