#!/usr/bin/env python3
"""Conrod "is it ready?" scorecard.

Runs the export_recall eval per suite case (or scores existing CSVs with
--no-run), prints PASS/WARN/FAIL per case and for the whole suite, and writes
the same report to target/eval/readiness/report.md.

Usage:
  python scripts/readiness.py [--quick] [--only NAME] [--max N] [--no-run]
"""

import argparse
import csv
import os
import re
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EVAL = ROOT / "target" / "eval"
OUT = EVAL / "readiness"
CMD = ["cargo", "test", "-p", "conrod-engine", "--release", "export_recall",
       "--", "--ignored", "--nocapture"]
MARKER = "sources untouched:"

SUITE = [
    {"name": "Motorsport / panning", "shoot": "2026-10-08_Thursday", "max": 22, "neighbours": 6},
    {"name": "Birds / wildlife", "shoot": "Personal Work\\2026\\08\\02", "max": 24, "neighbours": 6},
    {"name": "Car meet + detail shots", "shoot": "Ford Falcon Tribute", "max": 18, "neighbours": 6},
    {"name": "Cruise", "shoot": "SAU Cruise", "max": 12, "neighbours": 6},
    {"name": "Interiors / property", "shoot": "Keightley-Ave-Windradyne-Condition-Report", "max": 20, "neighbours": 2},
    {"name": "Portrait", "shoot": "Jasmeen Kaur", "max": 15, "neighbours": 4, "profile": "portrait"},
]


def slugify(text):
    return re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")


def basename(path):
    return re.split(r"[\\/]", path)[-1]


def newest(paths):
    return max(paths, key=lambda p: p.stat().st_mtime) if paths else None


def find_by_slug(shoot):
    slug = slugify(shoot)
    if not EVAL.is_dir():
        return None
    return newest([d / "frames.csv" for d in EVAL.iterdir()
                   if d.is_dir() and d.name != "readiness" and slug in d.name
                   and (d / "frames.csv").is_file()])


def find_since(since):
    if not EVAL.is_dir():
        return None
    found = []
    for d in EVAL.iterdir():
        f = d / "frames.csv"
        if d.is_dir() and d.name != "readiness" and f.is_file() and f.stat().st_mtime >= since:
            found.append(f)
    return newest(found)


def num(s, cast=float):
    try:
        return cast(s)
    except (TypeError, ValueError):
        return None


def load(csv_path):
    rows = []
    with open(csv_path, newline="", encoding="utf-8", errors="replace") as fh:
        for r in csv.DictReader(fh):
            rows.append({
                "file": r.get("file", ""),
                "exported": r.get("exported") == "1",
                "rating": num(r.get("rating")) or 0.0,
                "verdict": r.get("verdict", ""),
                "cull": r.get("cull_reason", "") or "",
                "burst_key": r.get("burst_key", "") or "",
                "burst_size": num(r.get("burst_size"), int) or 0,
                "burst_pick": r.get("burst_pick") == "1",
                "seconds": num(r.get("seconds")),
            })
    return rows


def bsize(group):
    return max(r["burst_size"] for r in group)


def bursts(rows):
    groups = {}
    for r in rows:
        if r["burst_key"]:
            groups.setdefault(r["burst_key"], []).append(r)
    return [g for g in groups.values() if bsize(g) >= 2]


def sources_status(log):
    text = log.read_text(encoding="utf-8", errors="replace") if log.exists() else ""
    if MARKER in text:
        return "ok", "PASS"
    return "missing", "FAIL"


def rule(v, good, ok, higher=True):
    if v is None:
        return "n/a"
    if higher:
        return "PASS" if v >= good else "WARN" if v >= ok else "FAIL"
    return "PASS" if v <= good else "WARN" if v <= ok else "FAIL"


def pct(v):
    return "-" if v is None else f"{v * 100:.1f}%"


def share(items, pred):
    return sum(1 for x in items if pred(x)) / len(items) if items else None


def metrics(rows, groups, sources, sources_value):
    exp = [r for r in rows if r["exported"]]
    non = [r for r in rows if not r["exported"]]
    one = [g for g in groups if sum(r["exported"] for r in g) == 1]
    keepers = [next(r for r in g if r["exported"]) for g in one]
    complete = [g for g in groups if len(g) == bsize(g)]
    base = sum(1 / bsize(g) for g in one) / len(one) if one else None
    pick = share(keepers, lambda r: r["burst_pick"])
    credits = []
    for g in one:
        e = next(r for r in g if r["exported"])
        greater = sum(1 for o in g if o is not e and o["rating"] > e["rating"])
        ties = sum(1 for o in g if o["rating"] == e["rating"])
        credits.append(max(0, min(3, greater + ties) - greater) / ties)
    top3 = sum(credits) / len(credits) if credits else None
    every = share(complete, lambda g: sum(r["burst_pick"] for r in g) == 1)
    secs = [r["seconds"] for r in rows if r["seconds"] is not None]
    speed = sum(secs) / len(secs) if secs else None
    kept = share(exp, lambda r: r["cull"] == "")
    sharp_k = share(exp, lambda r: r["verdict"] == "sharp")
    non_sharp = share(non, lambda r: r["verdict"] == "sharp")
    if pick is None or base is None:
        pick_status = "n/a"
    elif pick >= 2 * base:
        pick_status = "PASS"
    elif pick > base:
        pick_status = "WARN"
    else:
        pick_status = "FAIL"
    return [
        ("Keepers kept", pct(kept), ">=97% / >=90%", rule(kept, .97, .90)),
        ("Keepers rated sharp", pct(sharp_k), ">=85% / >=70%", rule(sharp_k, .85, .70)),
        ("Non-keepers rated sharp (info: unexported frames are often sharp too)", pct(non_sharp), "not scored", "info"),
        ("Keeper is burst pick", f"{pct(pick)} (rand {pct(base)})", ">=2x rand / >rand", pick_status),
        ("Keeper in top 3", pct(top3), ">=60% / >=45%", rule(top3, .60, .45)),
        ("Every burst has pick", pct(every), "100% / >=95%", rule(every, 1.0, .95)),
        ("Seconds per frame", "-" if speed is None else f"{speed:.2f}s", "<=1.5s / <=3s",
         rule(speed, 1.5, 3, False)),
        ("Originals untouched", sources_value, "marker in log", sources),
    ]


def verdict(statuses):
    if not any(s not in ("n/a", "info") for s in statuses):
        return "NO DATA"
    if "FAIL" in statuses:
        return "NOT READY"
    if statuses.count("WARN") <= 2:
        return "READY"
    return "NEARLY"


def worst(rows):
    bad = [r for r in rows if r["exported"] and (r["cull"] or r["verdict"] != "sharp")]
    bad.sort(key=lambda r: r["rating"])
    out = []
    for r in bad[:5]:
        reason = f"culled: {r['cull']}" if r["cull"] else "not culled"
        out.append(f"{basename(r['file'])}: {r['verdict'] or 'no verdict'}, {reason}")
    return out


def render(title, info, table, worst_lines, md):
    lines = []
    if md:
        lines += [f"### {title}", "", info, ""]
        if table:
            lines += ["| Metric | Value | Target | Status |", "|---|---|---|---|"]
            lines += [f"| {m} | {v} | {t} | {s} |" for m, v, t, s in table]
        if worst_lines:
            lines += ["", "Worst exported files:"] + [f"- {w}" for w in worst_lines]
    else:
        lines += [f"== {title} ==", f"  {info}"]
        lines += [f"  {m:<26}{v:<26}{t:<20}{s}" for m, v, t, s in table]
        if worst_lines:
            lines += ["  Worst exported files:"] + [f"    - {w}" for w in worst_lines]
    lines.append("")
    return lines


def main():
    ap = argparse.ArgumentParser(description="Conrod readiness scorecard.")
    ap.add_argument("--quick", action="store_true", help="max halved, neighbours 3")
    ap.add_argument("--only", metavar="NAME", help="only cases whose name contains NAME")
    ap.add_argument("--max", type=int, metavar="N", help="override exported frame count")
    ap.add_argument("--no-run", action="store_true", help="score existing CSVs only")
    args = ap.parse_args()

    cases = [c for c in SUITE if not args.only or args.only.lower() in c["name"].lower()]
    if not cases:
        raise SystemExit(f"no case matches --only {args.only!r}")
    OUT.mkdir(parents=True, exist_ok=True)

    results = []
    for i, case in enumerate(cases, 1):
        res = {"case": case, "rows": [], "groups": [], "run": "", "note": "",
               "sources": "n/a", "sources_value": "n/a", "folder": ""}
        log = OUT / f"{slugify(case['name'])}.log"
        if args.no_run:
            found = find_by_slug(case["shoot"])
            if not found:
                res["note"] = "no CSV matching shoot under target\\eval"
        else:
            mx = args.max or (max(1, case["max"] // 2) if args.quick else case["max"])
            nb = 3 if args.quick else case["neighbours"]
            print(f"[{i}/{len(cases)}] {case['name']}: running (max {mx}, neighbours {nb})...", flush=True)
            env = {
                k: v
                for k, v in os.environ.items()
                if k not in ("CONROD_EVAL_ONLY", "CONROD_EVAL_PROFILE")
            }
            env.update(CONROD_EVAL_SHOOT=case["shoot"], CONROD_EVAL_MAX=str(mx),
                       CONROD_EVAL_NEIGHBOURS=str(nb))
            if case.get("profile"):
                env["CONROD_EVAL_PROFILE"] = case["profile"]
            since = time.time() - 1
            t0 = time.time()
            code = None
            with open(log, "w", encoding="utf-8", errors="replace") as fh:
                try:
                    code = subprocess.run(CMD, cwd=ROOT, env=env, stdout=fh,
                                          stderr=subprocess.STDOUT).returncode
                except OSError as e:
                    fh.write(f"failed to start: {e}\n")
            secs = time.time() - t0
            res["run"] = f"exit {code}, {secs / 60:.1f} min"
            print(f"    {res['run']}, log {log}", flush=True)
            found = find_since(since)
            res["sources_value"], res["sources"] = sources_status(log)
            if not found:
                res["note"] = f"no output after run ({res['run']}), see log"
        if found:
            res["rows"] = load(found)
            res["groups"] = bursts(res["rows"])
            res["folder"] = found.parent.name
        results.append(res)

    mode = "no-run" if args.no_run else "run"
    md = []
    txt = [f"Conrod readiness ({mode}{', quick' if args.quick else ''})", ""]
    md += ["# Conrod readiness", "", f"Mode: {mode}{', quick' if args.quick else ''}", ""]

    checks = []
    all_rows = [r for res in results for r in res["rows"]]
    all_groups = [g for res in results for g in res["groups"]]
    src_vals = [res["sources"] for res in results]
    if "FAIL" in src_vals:
        suite_src = "FAIL"
    elif "PASS" in src_vals:
        suite_src = "PASS"
    else:
        suite_src = "n/a"
    suite_table = metrics(all_rows, all_groups, suite_src, suite_src if suite_src != "n/a" else "n/a")

    for res in results:
        case = res["case"]
        if res["rows"]:
            rows, groups = res["rows"], res["groups"]
            n_exp = sum(1 for r in rows if r["exported"])
            info = (f"{res['folder']}: {len(rows)} frames, {n_exp} exported, "
                    f"{len(groups)} bursts")
            if res["run"]:
                info += f" | {res['run']}"
            table = metrics(rows, groups, res["sources"], res["sources_value"])
            statuses = [t[3] for t in table]
            info += f" -> {verdict(statuses)}"
            wl = worst(rows)
        else:
            info = f"no data ({res['note'] or 'no CSV'})"
            table, wl = [], []
        txt += render(case["name"], info, table, wl, False)
        md += render(case["name"], info, table, wl, True)
        if res["rows"]:
            checks += [(case["name"], m, s) for m, _, _, s in table]
        else:
            checks.append((case["name"], "no data", "WARN"))

    ndata = sum(1 for res in results if res["rows"])
    suite_info = f"{ndata} of {len(results)} cases with data, {len(all_rows)} frames"
    txt += render("SUITE (pooled)", suite_info, suite_table, [], False)
    md += render("SUITE (pooled)", suite_info, suite_table, [], True)
    checks += [("SUITE", m, s) for m, _, _, s in suite_table]
    statuses = [s for _, _, s in checks if s not in ("n/a", "info")]
    fails = statuses.count("FAIL")
    warns = statuses.count("WARN")
    failed = [f"{c} / {m}" for c, m, s in checks if s == "FAIL"]
    overall = verdict(statuses)
    line = f"OVERALL: {overall} ({fails} FAIL, {warns} WARN)"
    fail_line = "FAILS: " + ("; ".join(failed) if failed else "none")
    nodata_line = f"cases without data: {len(results) - ndata} of {len(SUITE)}"
    txt += [line, fail_line, nodata_line]
    md += ["", f"**{line}**", "", fail_line, "", nodata_line, ""]

    report = OUT / "report.md"
    report.write_text("\n".join(md), encoding="utf-8")
    print("\n".join(txt))
    print(f"report: {report}")


if __name__ == "__main__":
    main()
