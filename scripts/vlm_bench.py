#!/usr/bin/env python3
"""Benchmark vision-language models on vehicle crops.

Compares the app's current request ("prod") with a tighter prompt ("tight").

  python scripts/vlm_bench.py [--models a,b,c] [--variants prod,tight]
                              [--ids 001,002] [--limit N] [--score-only]

Reads target/vlm/crops.json, target/vlm/gold.json (optional),
target/vlm/entry_list_bathurst_2026.json. Writes only under target/vlm/.
"""
import argparse
import base64
import copy
import difflib
import io
import json
import math
import os
import re
import statistics
import time
import urllib.error
import urllib.request
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
VLM = ROOT / "target" / "vlm"
RESULTS = VLM / "results"
OPENCODE_CFG = Path(r"C:\Users\kapsikkum\.config\opencode\opencode.json")
OLLAMA_URL = "http://127.0.0.1:11434/api/generate"
STRATA_HOST = "192.168.1.100:8088"
STRATA_URL = f"http://{STRATA_HOST}/v1/chat/completions"
STRATA_MODEL = "qwen3.8-flash-next-coder-iq1_m"
DEFAULT_MODELS = "qwen2.5vl:7b,mimo-iq4:latest,strata"
DEFAULT_VARIANTS = "prod,tight"
TIMEOUT = 180
LONG_EDGE = 1568
SCORE_ROWS = [("number", "number", None), ("plate", "plate", None), ("make", "make", None),
              ("model_exact", "model", None), ("model_family", "model", "family"), ("team", "team", None)]
GENERIC_MODEL_TOKENS = {"gr", "zl1", "s650", "gt", "coupe", "sedan", "hatch", "the"}

# prod: crates/conrod-io/src/prompts/car.txt:1 (verbatim).
CAR_PROMPT = (
    "Inspect only the vehicle. Return raw JSON matching the schema, without commentary. "
    "Record every clearly visible identity, colour, body, competition, team, sponsor, livery, "
    "driver, and country field. A flag beside a legible driver name may identify country. "
    "Use null or empty arrays when unreadable; never infer. Ignore plates and unrelated text. "
    "race_number is visible competition digits only (1 to 3 digits). Never use registration "
    "plates, year dates, engine badges, or model codes as race_number."
)
# prod: crates/conrod-io/src/vlm.rs:1031-1046 (vehicle_prompt), with settings.rs:152,168
# defaults read_plates=true, read_numbers=true.
PROD_TARGET = (
    "A separate reader handles registration plates. Read a competition number only from the "
    "subject and never copy plate characters."
)
# prod: crates/conrod-core/src/profile.rs:150-151 (Motorsport) and :162-163 (MotorsportMeet).
PROD_CTX = {
    "motorsport": "Motorsport: capture vehicle identity, competition details, livery, and action.",
    "carmeet": "Car meets: capture vehicle identity, modifications, finish, and visible details.",
}
# prod: crates/conrod-io/src/vlm.rs:1048-1070 (schema), 12 fields, all required.
PROD_SCHEMA = {
    "type": "object",
    "properties": {
        "make": {"type": ["string", "null"]},
        "model": {"type": ["string", "null"]},
        "colour": {"type": ["string", "null"]},
        "body_type": {"type": ["string", "null"]},
        "race_number": {"type": ["string", "null"]},
        "team": {"type": ["string", "null"]},
        "driver": {"type": ["string", "null"]},
        "country": {"type": ["string", "null"]},
        "sponsors": {"type": "array", "items": {"type": "string"}},
        "livery_text": {"type": "array", "items": {"type": "string"}},
        "is_competition": {"type": "boolean"},
        "confidence": {"type": "number"},
    },
    "required": [
        "make", "model", "colour", "body_type", "race_number",
        "team", "driver", "country", "sponsors", "livery_text",
        "is_competition", "confidence",
    ],
}

TIGHT_PROMPT_DEFAULT = (
    "You are reading one vehicle in a photo. Report only what you can actually see. Never guess from memory.\n"
    "- number: the race number painted on the car (digits only). null if there is none or it is not clearly legible.\n"
    "- plate: the registration plate characters exactly as shown. null if none or not clearly legible.\n"
    "- visible_text: every legible word or logo text on the vehicle, exactly as written, most prominent first. [] if none.\n"
    "- make, model: the manufacturer and model. Give the model only if a badge, model script, or unmistakable design identifies it; otherwise null.\n"
    "- evidence: which visible clue identified make/model: \"badge\", \"script\", \"design\" or \"none\".\n"
    "{CANDIDATES}\n"
)
TIGHT_SCHEMA_DEFAULT = {
    "type": "object",
    "properties": {
        "number": {"type": ["string", "null"]},
        "plate": {"type": ["string", "null"]},
        "visible_text": {"type": "array", "items": {"type": "string"}},
        "make": {"type": ["string", "null"]},
        "model": {"type": ["string", "null"]},
        "evidence": {"type": "string", "enum": ["badge", "script", "design", "none"]},
    },
    "required": ["number", "plate", "visible_text", "make", "model", "evidence"],
}
TIGHT_CANDIDATES = {
    "motorsport": "This is a Supercars race car. make/model must be one of: Ford Mustang, Chevrolet Camaro, Toyota Supra.",
    "carmeet": "",
}
# tight2: prompt_tight2.txt / schema_tight2.json on disk. Motorsport drops the plate line and field.
TIGHT2_CANDIDATES = {
    "motorsport": "This is a Supercars race car. make/model must be one of: Ford Mustang S650, Chevrolet Camaro ZL1, Toyota GR Supra.",
    "carmeet": "",
}
AU_PLATE_FORMS = [r"[A-Z]{3}\d{2}[A-Z]", r"[A-Z]{2}\d{2}[A-Z]{2}", r"\d{3}[A-Z]{3}",
                  r"[A-Z]{3}\d{3}", r"\d{2}[A-Z]{4}", r"[A-Z]{6}"]


def ensure_tight_files():
    VLM.mkdir(parents=True, exist_ok=True)
    RESULTS.mkdir(parents=True, exist_ok=True)
    p = VLM / "prompt_tight.txt"
    if not p.exists():
        p.write_text(TIGHT_PROMPT_DEFAULT, encoding="utf-8")
    s = VLM / "schema_tight.json"
    if not s.exists():
        s.write_text(json.dumps(TIGHT_SCHEMA_DEFAULT, indent=2), encoding="utf-8")


def build_request(variant, group):
    if variant == "prod":
        ctx = PROD_CTX.get(group, PROD_CTX["motorsport"])
        prompt = f"{CAR_PROMPT}\n\nShoot context: {ctx} Context guides attention only; never infer unseen details. {PROD_TARGET}"
        return prompt, PROD_SCHEMA
    if variant == "tight2":
        text = (VLM / "prompt_tight2.txt").read_text(encoding="utf-8")
        if group == "motorsport":
            text = re.sub(r"^- plate:.*\n", "", text, flags=re.M)
        prompt = text.replace("{CANDIDATES}", TIGHT2_CANDIDATES.get(group, "")).strip()
        schema = json.loads((VLM / "schema_tight2.json").read_text(encoding="utf-8"))
        if group == "motorsport":
            schema["properties"].pop("plate", None)
            schema["required"] = [k for k in schema["required"] if k != "plate"]
        return prompt, schema
    text = (VLM / "prompt_tight.txt").read_text(encoding="utf-8")
    prompt = text.replace("{CANDIDATES}", TIGHT_CANDIDATES.get(group, "")).strip()
    schema = json.loads((VLM / "schema_tight.json").read_text(encoding="utf-8"))
    return prompt, schema


def load_image_b64(rel):
    path = Path(rel)
    if not path.is_absolute():
        path = ROOT / path
    im = Image.open(path).convert("RGB")
    im.thumbnail((LONG_EDGE, LONG_EDGE), Image.LANCZOS)
    buf = io.BytesIO()
    im.save(buf, "JPEG", quality=92)
    return base64.b64encode(buf.getvalue()).decode("ascii")


def post_json(url, body, headers=None):
    data = json.dumps(body).encode("utf-8")
    hdr = {"Content-Type": "application/json"}
    hdr.update(headers or {})
    req = urllib.request.Request(url, data=data, headers=hdr, method="POST")
    with urllib.request.urlopen(req, timeout=TIMEOUT) as r:
        return json.loads(r.read().decode("utf-8"))


def first_json(text):
    dec = json.JSONDecoder()
    for i, ch in enumerate(text or ""):
        if ch == "{":
            try:
                v, _ = dec.raw_decode(text, i)
            except ValueError:
                continue
            if isinstance(v, dict):
                return v
    return None


def parse_obj(text, extract):
    if not text:
        return None
    text = re.sub(r"^\s*```(?:json)?\s*|\s*```\s*$", "", text.strip())
    try:
        v = json.loads(text)
    except ValueError:
        if not extract:
            return None
        v = first_json(text)
        if v is None:
            return None
    return v if isinstance(v, dict) else None


def call_ollama(model, prompt, b64, schema):
    base = {"model": model, "images": [b64], "stream": False,
            "options": {"temperature": 0, "num_predict": 600}}
    if model.startswith("mimo"):
        base["think"] = False
    t0 = time.time()
    out = post_json(OLLAMA_URL, {**base, "prompt": prompt, "format": schema})
    text = out.get("response", "") or ""
    obj = parse_obj(text, False)
    attempts = 1
    if obj is None:
        attempts = 2
        out = post_json(OLLAMA_URL, {**base, "prompt": prompt + "\nReply with one JSON object only."})
        text = out.get("response", "") or ""
        obj = parse_obj(text, True)
    return {"text": text, "obj": obj, "attempts": attempts,
            "latency": time.time() - t0, "note": ""}


def strict_schema(schema):
    s = copy.deepcopy(schema)
    s.setdefault("additionalProperties", False)
    return s


def strata_key():
    raw = OPENCODE_CFG.read_text(encoding="utf-8")
    raw = "\n".join(line for line in raw.splitlines() if not line.lstrip().startswith("//"))
    cfg = json.loads(raw)
    for prov in (cfg.get("providers") or {}).values():
        s = prov.get("settings") or {}
        if STRATA_HOST in str(s.get("baseURL", "")):
            key = str(s.get("apiKey") or "")
            m = re.fullmatch(r"\{env:(\w+)\}", key)
            if m:
                key = os.environ.get(m.group(1), "")
            if key:
                return key
    raise RuntimeError(f"no apiKey for provider at {STRATA_HOST} in opencode.json")


def call_strata(prompt, b64, schema, key):
    content = [{"type": "text", "text": prompt},
               {"type": "image_url", "image_url": {"url": "data:image/jpeg;base64," + b64}}]
    base = {"model": STRATA_MODEL, "messages": [{"role": "user", "content": content}],
            "temperature": 0, "max_tokens": 2000,
            "chat_template_kwargs": {"enable_thinking": False}, "reasoning_effort": "none"}
    headers = {"Authorization": "Bearer " + key}
    rf = {"type": "json_schema", "json_schema": {"name": "vehicle", "schema": strict_schema(schema), "strict": True}}
    t0 = time.time()
    note = ""
    try:
        out = post_json(STRATA_URL, {**base, "response_format": rf}, headers)
    except urllib.error.HTTPError as e:
        body = e.read()[:300].decode("utf-8", "replace").replace(key, "<redacted>")
        note = f"response_format rejected HTTP {e.code}: {body}"
        out = post_json(STRATA_URL, base, headers)
    msg = out["choices"][0]["message"]
    text = msg.get("content") or ""
    if not isinstance(text, str):
        text = json.dumps(text)
    obj = parse_obj(text, True)
    if obj is None:
        reasoning = msg.get("reasoning_content") or ""
        obj = first_json(reasoning)
        if obj is not None:
            note = (note + " " if note else "") + "obj taken from reasoning_content"
        text = text or reasoning
    return {"text": text, "obj": obj, "attempts": 1,
            "latency": time.time() - t0, "note": note}


def load_entries():
    data = json.loads((VLM / "entry_list_bathurst_2026.json").read_text(encoding="utf-8"))
    by_num = {str(e["number"]): e for e in data["entries"]}
    sponsors = set()
    for e in data["entries"]:
        if e.get("primary_sponsor"):
            sponsors.add(e["primary_sponsor"])
        sponsors.update(e.get("livery_sponsors") or [])
    return by_num, sponsors


def load_gold(path):
    if not path.exists():
        return None
    return {g["id"]: g for g in json.loads(path.read_text(encoding="utf-8"))}


def load_jsonl(path):
    recs = {}
    if path.exists():
        for line in path.read_text(encoding="utf-8").splitlines():
            if line.strip():
                r = json.loads(line)
                recs[r["id"]] = r
    return recs


def results_path(model, variant):
    return RESULTS / f"{model.replace(':', '_')}__{variant}.jsonl"


def clean(x):
    if isinstance(x, str) and x.strip():
        return x.strip()
    return None


def digits(x):
    s = clean(x)
    if s is None:
        return None
    return re.sub(r"\D", "", s) or None


def plate_ok(p, flags):
    if p is None:
        return None
    s = re.sub(r"[\s.\-]", "", p).upper()
    generic = 2 <= len(s) <= 7 and s.isalnum() and any(c.isdigit() for c in s) and any(c.isalpha() for c in s)
    if generic or any(re.fullmatch(f, s) for f in AU_PLATE_FORMS):
        return p
    flags.append("plate_rejected")
    return None


def dedupe(items):
    out = []
    for s in items:
        s = s.strip()
        if s and not any(fuzzy(s, t) for t in out):
            out.append(s)
    return out


def match_count(preds, golds):
    pairs = sorted(((ratio(p, g), i, j) for i, p in enumerate(preds)
                    for j, g in enumerate(golds) if fuzzy(p, g)), reverse=True)
    used_p, used_g, m = set(), set(), 0
    for _, i, j in pairs:
        if i in used_p or j in used_g:
            continue
        used_p.add(i)
        used_g.add(j)
        m += 1
    return m


def ratio(a, b):
    return difflib.SequenceMatcher(None, a.lower(), b.lower()).ratio()


def fuzzy(a, b, th=0.85):
    a = a.lower().strip()
    b = b.lower().strip()
    if not a or not b:
        return False
    if a in b or b in a:
        return True
    return ratio(a, b) >= th


def normalise(variant, group, obj, by_num, entry_sponsors, gold_sponsors):
    """Map a raw model reply to the common output keys (number, plate, make, model, sponsors, team, drivers)."""
    flags = []
    out = {"number": None, "plate": None, "make": None, "model": None,
           "sponsors": [], "team": None, "drivers": [], "visible_text": []}
    if not isinstance(obj, dict):
        flags.append("no_json")
        return out, flags
    if variant == "prod":
        out["number"] = clean(obj.get("race_number"))
        out["make"] = clean(obj.get("make"))
        out["model"] = clean(obj.get("model"))
        out["team"] = clean(obj.get("team"))
        d = clean(obj.get("driver"))
        out["drivers"] = [d] if d else []
        out["sponsors"] = [s.strip() for s in (obj.get("sponsors") or []) if clean(s)]
        out["visible_text"] = [s.strip() for s in (obj.get("livery_text") or []) if clean(s)]
        return out, flags

    number = digits(obj.get("number"))
    if group == "motorsport" and number and number not in by_num:
        flags.append("number_rejected")
        number = None
    out["number"] = number
    if variant == "tight2":
        if number and clean(obj.get("number_location")) in ("windscreen", "nose"):
            flags.append("number_weak")
        plate = None if group == "motorsport" else plate_ok(clean(obj.get("plate")), flags)
    else:
        plate = clean(obj.get("plate"))
    out["plate"] = plate
    vt = [s.strip() for s in (obj.get("visible_text") or []) if clean(s)]
    out["visible_text"] = vt
    model = clean(obj.get("model"))
    if clean(obj.get("evidence")) == "none":
        model = None
    out["make"] = clean(obj.get("make"))
    out["model"] = model
    if group == "motorsport" and number in by_num:
        e = by_num[number]
        out["team"] = e.get("team")
        out["drivers"] = list(e.get("drivers") or [])
    known = set(entry_sponsors) | set(gold_sponsors)
    out["sponsors"] = [t for t in vt if any(fuzzy(t, s) for s in known)]
    return out, flags


def run_pass(model, variant, crops, key, by_num, entry_sponsors, gold_sponsors):
    path = results_path(model, variant)
    recs = load_jsonl(path)
    for c in crops:
        cid, group = c["id"], c["group"]
        t0 = time.time()
        rec = {"id": cid, "group": group, "model": model, "variant": variant}
        try:
            prompt, schema = build_request(variant, group)
            img = load_image_b64(c["crop"])
            if model == "strata":
                r = call_strata(prompt, img, schema, key)
            else:
                r = call_ollama(model, prompt, img, schema)
            rec.update(obj=r["obj"], json_ok=r["obj"] is not None, text=r["text"][:2000],
                       latency=round(r["latency"], 3), attempts=r["attempts"],
                       note=r["note"], error=None)
        except Exception as e:
            rec.update(obj=None, json_ok=False, text="", latency=round(time.time() - t0, 3),
                       attempts=0, note="", error=f"{type(e).__name__}: {e}")
        recs[cid] = rec
        print(f"{model} {variant} {cid} {group} {rec['latency']:.1f}s json={rec['json_ok']} "
              f"{rec['error'] or rec['note'] or ''}", flush=True)
    with path.open("w", encoding="utf-8") as f:
        for cid in sorted(recs):
            f.write(json.dumps(recs[cid], ensure_ascii=False) + "\n")


def model_tokens(s):
    return {t for t in re.findall(r"[a-z0-9]+", s.lower()) if t not in GENERIC_MODEL_TOKENS}


def eq_model_family(pred, gold):
    pt, gt = model_tokens(pred), model_tokens(gold)
    return bool(pt and gt) and (pt <= gt or gt <= pt) and bool(pt & gt)


def eq_field(field, pred, gold):
    if field in ("number", "plate"):
        return re.sub(r"[\s\-]", "", pred).upper() == re.sub(r"[\s\-]", "", gold).upper()
    if field == "make":
        return pred.strip().lower() == gold.strip().lower()
    if field == "model":
        gt = set(re.findall(r"[a-z0-9]+", gold.lower()))
        pt = set(re.findall(r"[a-z0-9]+", pred.lower()))
        return gt <= pt
    if field == "team":
        return ratio(pred, gold) >= 0.8
    return False


def pct(n, d):
    return f"{100 * n / d:.0f}%" if d else "-"


def pct_stats(vals):
    if not vals:
        return "-", "-"
    s = sorted(vals)
    p90 = s[max(0, math.ceil(0.9 * len(s)) - 1)]
    return f"{statistics.median(s):.1f}s", f"{p90:.1f}s"


def score(models, variants, by_num, entry_sponsors, gold, gold_sponsors):
    md = ["# VLM benchmark report", ""]
    for model in models:
        for variant in variants:
            recs = load_jsonl(results_path(model, variant))
            if not recs:
                continue
            lat = [r["latency"] for r in recs.values() if r.get("latency") is not None]
            fails = sum(1 for r in recs.values() if not r.get("json_ok"))
            med, p90 = pct_stats(lat)
            md += [f"## {model} / {variant}", "",
                   f"Latency median {med}, p90 {p90}; JSON failures {fails}/{len(recs)}; "
                   f"crops {len(recs)}", ""]
            groups = sorted({r["group"] for r in recs.values()})
            for group in groups:
                rows = []
                for cid, r in sorted(recs.items()):
                    if r["group"] != group or cid not in gold:
                        continue
                    pred, _ = normalise(variant, group, r.get("obj"), by_num, entry_sponsors, gold_sponsors)
                    rows.append((pred, gold[cid]))
                if not rows:
                    continue
                md += [f"### {group} (n={len(rows)} scored)", "",
                       "| field | gold non-null | correct | wrong | abstain | hallucinations |",
                       "|---|---|---|---|---|---|"]
                for label, f, mode in SCORE_ROWS:
                    if f == "plate" and (variant == "prod" or (variant == "tight2" and group == "motorsport")):
                        md.append(f"| {label} | n/a | n/a | n/a | n/a | n/a |")
                        continue
                    G = C = W = A = H = 0
                    for pred, g in rows:
                        gv = g.get(f)
                        pv = pred.get(f)
                        if gv is None:
                            if pv is not None:
                                H += 1
                            continue
                        G += 1
                        if pv is None:
                            A += 1
                        elif (eq_model_family(pv, gv) if mode == "family" else eq_field(f, pv, gv)):
                            C += 1
                        else:
                            W += 1
                    md.append(f"| {label} | {G} | {pct(C, G)} | {pct(W, G)} | {pct(A, G)} | {H} |")
                P = Mp = Gs = Mg = 0
                for pred, g in rows:
                    ps = dedupe(pred["sponsors"])
                    gs = dedupe(g.get("sponsors") or [])
                    m = match_count(ps, gs)
                    P += len(ps)
                    Gs += len(gs)
                    Mp += m
                    Mg += m
                md += ["", f"Sponsors: precision {pct(Mp, P)} ({Mp}/{P}), recall {pct(Mg, Gs)} ({Mg}/{Gs})", ""]
    text = "\n".join(md) + "\n"
    print(text)
    (VLM / "report.md").write_text(text, encoding="utf-8")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--models", default=DEFAULT_MODELS)
    ap.add_argument("--variants", default=DEFAULT_VARIANTS)
    ap.add_argument("--ids", default="")
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--score-only", action="store_true")
    ap.add_argument("--gold", default="", help="gold JSON path (default target/vlm/gold_v2.json if present, else gold.json)")
    a = ap.parse_args()
    models = [m.strip() for m in a.models.split(",") if m.strip()]
    variants = [v.strip() for v in a.variants.split(",") if v.strip()]
    for v in variants:
        if v not in ("prod", "tight", "tight2"):
            ap.error(f"unknown variant {v}")

    ensure_tight_files()
    by_num, entry_sponsors = load_entries()
    if a.gold:
        gold_path = Path(a.gold)
    elif (VLM / "gold_v2.json").exists():
        gold_path = VLM / "gold_v2.json"
    else:
        gold_path = VLM / "gold.json"
    print(f"gold: {gold_path}")
    gold = load_gold(gold_path)
    gold_sponsors = set()
    if gold:
        for g in gold.values():
            gold_sponsors.update(g.get("sponsors") or [])

    if not a.score_only:
        crops = json.loads((VLM / "crops.json").read_text(encoding="utf-8"))
        if a.ids:
            keep = {x.strip() for x in a.ids.split(",") if x.strip()}
            crops = [c for c in crops if c["id"] in keep]
        if a.limit:
            crops = crops[: a.limit]
        key = strata_key() if "strata" in models else None
        for model in models:
            for variant in variants:
                print(f"== {model} / {variant} ({len(crops)} crops)", flush=True)
                run_pass(model, variant, crops, key, by_num, entry_sponsors, gold_sponsors)

    if gold is None:
        print("gold.json not found: scoring skipped (results saved under target/vlm/results/)")
        return
    score(models, variants, by_num, entry_sponsors, gold, gold_sponsors)


if __name__ == "__main__":
    main()
