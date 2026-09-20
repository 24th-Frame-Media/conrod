"""Run the Rust album operations on a private COPY of the real library and compare with Python.

    python tools/parity_ops.py pick 38          # Rust pick_keepers vs the picks Python stored
    python tools/parity_ops.py pick-py 95       # Rust vs Python's own pick_of_pass, both run on copies
    python tools/parity_ops.py group 39         # Rust group vs the groups Python stored
    python tools/parity_ops.py rescore 95       # Rust rescore vs Python's stored ratings (a job with stored features)

The real ~/.conrod/conrod.db is only ever opened read-only (sqlite backup into %TEMP%); every run starts
from a fresh copy. Needs `cargo build --release -p conrod-cli` first. Results on 20 Sept 2026, see rust/HANDOFF.md.
"""
import json, os, sqlite3, subprocess, sys, tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CLI = ROOT / "rust/target/release/conrod-cli.exe"
REAL = "file:" + os.path.expanduser("~/.conrod/conrod.db").replace("\\", "/") + "?mode=ro"


def fresh(name: str) -> Path:
    home = Path(tempfile.gettempdir()) / name
    home.mkdir(exist_ok=True)
    for f in ("conrod.db", "conrod.db-wal", "conrod.db-shm"):
        (home / f).unlink(missing_ok=True)
    src = sqlite3.connect(REAL, uri=True)
    dst = sqlite3.connect(home / "conrod.db")
    src.backup(dst)
    dst.close()
    return home


def query(home, sql, *args):
    c = sqlite3.connect(home / "conrod.db")
    try:
        return c.execute(sql, args).fetchall()
    finally:
        c.close()


def rust(home, action, job):
    p = subprocess.run([str(CLI), "command", action, json.dumps({"jobId": job})], capture_output=True, text=True,
                       env=dict(os.environ, CONROD_HOME=str(home)), timeout=1800)
    return p.returncode, p.stdout.strip()[-200:], p.stderr.strip()[-200:]


PICKS = ("select d.id from detections d join images i on i.id=d.image_id "
         "where i.job_id=? and d.burst_pick=1")
GROUPS = ("select d.id, d.group_key from detections d join images i on i.id=d.image_id "
          "where i.job_id=? and d.group_key is not null")
SCORES = ("select d.id, d.rating, d.stars, d.cull_reason, d.sharpness from detections d "
          "join images i on i.id=d.image_id where i.job_id=? order by d.id")


def partitions(rows):
    out = {}
    for did, key in rows:
        out.setdefault(key, set()).add(did)
    return {frozenset(v) for v in out.values()}


mode, job = sys.argv[1], int(sys.argv[2])
home = fresh("conrod-parity")
before = {"pick": PICKS, "pick-py": PICKS, "group": GROUPS, "rescore": SCORES}[mode]
python_side = query(home, before, job)
if mode == "pick-py":  # Python's own pick on a second copy
    py_home = fresh("conrod-parity-py")
    code = f"from conrod import pipeline; from conrod.config import Settings; print(pipeline.pick_of_pass({job}, Settings()))"
    subprocess.run([str(ROOT / ".venv/Scripts/python.exe"), "-c", code], cwd=ROOT, env=dict(os.environ, CONROD_HOME=str(py_home)))
    python_side = query(py_home, PICKS, job)
if mode == "rescore":  # make sure the operation visibly does something: tamper first
    c = sqlite3.connect(home / "conrod.db")
    c.execute("update detections set rating=0.123, sharpness=0.5, cull_reason='tampered' where id in "
              "(select d.id from detections d join images i on i.id=d.image_id where i.job_id=? order by d.id limit 200)", (job,))
    c.commit(); c.close()
result = rust(home, {"pick": "pick_keepers", "pick-py": "pick_keepers", "group": "group", "rescore": "rescore"}[mode], job)
after = query(home, before, job)
if mode in ("pick", "pick-py"):
    a, b = {r[0] for r in python_side}, {r[0] for r in after}
    print(f"python {len(a)} keepers, rust {len(b)}; both {len(a & b)}, only python {len(a - b)}, only rust {len(b - a)}")
elif mode == "group":
    a, b = partitions(python_side), partitions(after)
    print(f"python {len(a)} groups, rust {len(b)}; identical {len(a & b)}")
else:
    diffs = sum(1 for x, y in zip(python_side, after) if x != y)
    print(f"{len(python_side)} detections; differing after the tamper and rescore: {diffs}")
print(result)
