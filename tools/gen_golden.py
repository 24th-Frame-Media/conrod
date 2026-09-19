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

from conrod import bursts, framing, keywords, marques, sharp_model, taste  # noqa: E402
from conrod.analyze import VehicleAnalysis  # noqa: E402
from conrod.mapping import NumberMap  # noqa: E402

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


def bursts_cases() -> dict:
    tag_sets = [
        {},
        {"Model": "Canon EOS R7", "SerialNumber": "358034000852"},
        {"Model": "Canon EOS 80D", "SerialNumber": 185023000306},       # numeric
        {"Model": "Canon EOS R7", "InternalSerialNumber": "AB1234"},
        {"Model": "Canon EOS R7", "SerialNumber": "-", "LensModel": "RF100-500mm"},
        {"Model": "Canon EOS R7", "LensID": "Canon RF 70-200mm"},
        {"Model": "", "SerialNumber": "", "LensModel": "x"},
        {"SerialNumber": "9999"},
        {"DateTimeOriginal": "2026:09:13 10:15:30"},
        {"DateTimeOriginal": "2026:09:13 10:15:30", "SubSecTimeOriginal": "45"},
        {"DateTimeOriginal": "2026:09:13 10:15:30", "SubSecTimeOriginal": 7},
        {"DateTimeOriginal": "2026:09:13 10:15:30", "SubSecTimeOriginal": "045 "},
        {"DateTimeOriginal": "2026:09:13 10:15:30", "SubSecTimeOriginal": "x"},
        {"SubSecDateTimeOriginal": "2026:09:13 10:15:30.12+10:00",
         "SubSecTimeOriginal": "99"},
        {"SubSecDateTimeOriginal": "", "DateTimeOriginal": "2026:09:13 10:15:30",
         "SubSecTimeOriginal": "5"},                   # key present: no sub-second
        {"DateTimeOriginal": "2026:09:13T10:15:30-05:00"},
        {"DateTimeOriginal": "2026/09/13 10:15:30"},
        {"DateTimeOriginal": "0000:00:00 00:00:00"},
        {"DateTimeOriginal": "2026:02:31 23:59:59.5"},  # rolls into March
        {"DateTimeOriginal": "2026:13:01 00:00:00"},
        {"DateTimeOriginal": "1969:12:31 23:59:59"},
        {"DateTimeOriginal": "2026:09:13"},
        {"DateTimeOriginal": "2026:09:13 10:15"},
        {"DateTimeOriginal": "garbage here"},
        {"DateTimeOriginal": " 2026:09:13 25:61:61 "},
        {"DateTimeOriginal": "2026-09-13 10:15:30"},
    ]
    singles = [{"tags": t, "camera": bursts.camera_of(t, "fallback cam"),
                "taken": bursts.taken_at(t)} for t in tag_sets]

    def row(path, cam, stamp, sub=None):
        r = {"SourceFile": path, "Model": cam[0], "SerialNumber": cam[1]}
        if stamp:
            r["DateTimeOriginal"] = stamp
        if sub is not None:
            r["SubSecTimeOriginal"] = sub
        return r

    r7, d80 = ("Canon EOS R7", "1"), ("Canon EOS 80D", "2")
    rows = [
        row("b/IMG_0003.CR3", r7, "2026:09:13 10:00:00", "50"),
        row("a/IMG_0001.CR3", r7, "2026:09:13 10:00:00", "00"),
        row("a/IMG_0002.CR3", r7, "2026:09:13 10:00:00", "50"),   # same instant: path breaks the tie
        row("a/IMG_0004.CR3", r7, "2026:09:13 10:00:04", "50"),   # exactly the gap: same burst
        row("a/IMG_0005.CR3", r7, "2026:09:13 10:00:08", "51"),   # just over: new burst
        row("c/IMG_9000.CR2", d80, "2026:09:13 10:00:01"),
        row("c/IMG_9001.CR2", d80, None),
        row("c/IMG_9002.CR2", d80, None),
        {"SourceFile": "d/phone.jpg"},
    ]
    frames = bursts.describe(rows, fallback="Job folder")
    described = [{"path": f.path, "camera": f.camera, "taken": f.taken, "burst": f.burst}
                 for f in frames]
    collected = [{"key": b.key, "camera": b.camera, "frames": b.frames,
                  "started": b.started, "ended": b.ended} for b in bursts.collect(frames)]
    return {"cases": singles, "rows": rows, "fallback": "Job folder",
            "frames": described, "bursts": collected}


def marques_cases() -> dict:
    pairs = [(None, None), ("Yamaha", "Ninja H2"), ("Kawasaki", "Ninja H2"),
             ("kawasaki ", "ninja"), (None, "Falcon XR8"), ("Holden", "Falcon"),
             ("Ford", "Focus RS"), ("Ford", "GT"), ("Mazda", "MX-5 Miata"),
             ("", "RX-7"), ("Toyota", "Supra, Commodore"), ("Subaru", "WRX STI"),
             ("Honda", "CBR1000RR Fireblade"), ("Suzuki", "GSX-R1000"),
             ("BMW", "M3"), ("Holden", "VL Commodore SS"), ("X", "-ninja")]
    return {"cases": [{"make": m, "model": n, "out": marques.correct_make(m, n)}
                      for m, n in pairs]}


ENTRY_CSV = (
    "﻿No., Driver ,Team,Class,Sponsor,Empty\r\n"
    "88,Broc Feeney,Triple Eight Race Engineering,Supercars,Red Bull;Ampol,\r\n"
    "#07 ,Someone Else,\"Team, With Comma\",Supercars,,\r\n"
    "\r\n"
    "0,Zero Driver,Zeros,Club,\"A, B ; C\",\r\n"
    "abc,No Number,Nobody,,,\r\n"
    "17,Short Row\r\n"
    "5,Extra,Fields,X,Y,Z,surplus,more\r\n"
    "88,Duplicate Wins,T8,Supercars,,\r\n"
)


def mapping_cases() -> dict:
    import tempfile

    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "entries.csv"
        path.write_bytes(ENTRY_CSV.encode("utf-8"))
        nm = NumberMap.load(path)
        bad = Path(tmp) / "bad.csv"
        bad.write_text("driver,team\nA,B\n", encoding="utf-8")
        try:
            NumberMap.load(bad)
            bad_error = False
        except ValueError:
            bad_error = True
        empty = Path(tmp) / "empty.csv"
        empty.write_text("", encoding="utf-8")
        empty_rows = NumberMap.load(empty).rows
    lookups = []
    for number in ("88", "#88", "088", "7", "07", "0", "00", "5", "17", "99", "", "abc"):
        for prefix in ("", "Race|"):
            lookups.append({"number": number, "prefix": prefix,
                            "keywords": nm.keywords_for(number, prefix),
                            "describe": nm.describe(number)})
    return {"csv": ENTRY_CSV, "rows": nm.rows, "lookups": lookups,
            "bad_has_error": bad_error, "empty_rows": empty_rows}


def keywords_cases() -> dict:
    from types import SimpleNamespace

    analyses = [
        {},
        {"race_number": "88", "make": "Ford", "model": "Mustang GT", "colour": "red",
         "body_type": "coupe", "plate": "ABC123", "plate_state": "NSW",
         "team": "Triple Eight", "team_corroborated": True,
         "sponsors": ["Red Bull", "red bull", "Ampol"], "is_competition": True},
        {"make": "Holden", "model": "Holden Commodore", "colour": "blue"},
        {"model": "Commodore", "team": "Guessed Team", "number_source": "vlm"},
        {"race_number": "7", "team": "Manual Team", "number_source": "manual",
         "kind": "motorcycle", "is_bike": True, "make": "Kawasaki"},
        {"plate": "XYZ", "kind": "truck"},
        {"kind": "car"},
        {"colour": "dark metallic blue", "make": "  ", "model": " Golf "},
        {"race_number": "5", "make": "BMW", "model": "M3", "colour": "ßtraße"},
        {"make": "Ford", "model": "Falcon", "extra_unknown": 1, "sponsors": ["Castrol"]},
    ]
    with_map = mapping_number_map()
    out = []
    for raw in analyses:
        a = VehicleAnalysis.from_json(json.dumps(raw))
        for prefix, plate in (("", True), ("Motorsport|", False)):
            settings = SimpleNamespace(keyword_prefix=prefix, write_plate_keyword=plate)
            out.append({
                "analysis": raw, "prefix": prefix, "write_plate": plate,
                "keywords": keywords.for_vehicle(a, settings),
                "keywords_with_map": keywords.for_vehicle(a, settings, with_map),
                "title": a.title,
            })
    frame = [VehicleAnalysis.from_json(json.dumps(r)) for r in analyses[:5]]
    settings = SimpleNamespace(keyword_prefix="", write_plate_keyword=True)
    return {"cases": out, "csv": ENTRY_CSV,
            "frame": {"analyses": analyses[:5],
                      "keywords": keywords.for_frame(frame, settings, with_map),
                      "caption": keywords.caption_for(frame)}}


def mapping_number_map():
    import tempfile

    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "entries.csv"
        path.write_bytes(ENTRY_CSV.encode("utf-8"))
        return NumberMap.load(path)


def main() -> None:
    write("framing", framing_cases())
    write("ridge", ridge_cases(), compact=True)
    write("bursts", bursts_cases())
    write("marques", marques_cases())
    write("mapping", mapping_cases())
    write("keywords", keywords_cases())


if __name__ == "__main__":
    main()
