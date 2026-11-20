"""Parity fixture for conrod-decider: the official `pplx_decider` reference's answers.

BUILD TIME ONLY (Python never runs inside Conrod). Needs the venv from
scripts/decider/README.md and the BF16 checkpoint of
perplexity-ai/pplx-decider-flash-v1.2 (about 9 GB).

    python reference_fixture.py <checkpoint-dir> [--device cpu|cuda]

Writes crates/conrod-decider/tests/fixtures/{images/*.png,reference.json}. The images are
drawn here (nothing third-party is committed). On a GPU with room for the BF16 weights use
`--device cuda`; elsewhere the weights run in BF16 on the CPU (slow, but the same
arithmetic as the reference's GPU path, with the same noise floor).
"""
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

import numpy as np
import torch
from PIL import Image, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "crates" / "conrod-decider" / "tests" / "fixtures"
MAX_PIXELS = 1_638_400


def draw_scene(w: int, h: int) -> Image.Image:
    img = Image.new("RGB", (w, h))
    px = np.zeros((h, w, 3), dtype=np.uint8)
    for y in range(h):
        t = y / h
        px[y, :, :] = (np.array([90, 150, 230]) * (1 - t) + np.array([210, 230, 250]) * t) if t < 0.6 else (
            np.array([70, 110, 60]) * (1 - (t - 0.6)) + np.array([40, 70, 40]) * (t - 0.6)
        )
    img = Image.fromarray(px)
    d = ImageDraw.Draw(img)
    s = w / 512
    d.rectangle([90 * s, 200 * s, 330 * s, 280 * s], fill=(190, 30, 30))
    d.polygon([(130 * s, 200 * s), (170 * s, 150 * s), (260 * s, 150 * s), (300 * s, 200 * s)], fill=(160, 25, 25))
    d.rectangle([180 * s, 160 * s, 250 * s, 195 * s], fill=(190, 220, 240))
    for cx in (140, 280):
        d.ellipse([(cx - 28) * s, 255 * s, (cx + 28) * s, 311 * s], fill=(20, 20, 20))
        d.ellipse([(cx - 12) * s, 271 * s, (cx + 12) * s, 295 * s], fill=(170, 170, 170))
    d.text((350 * s, 40 * s), "RACE 42", fill=(255, 255, 255))
    for i in range(0, int(200 * s), max(2, int(6 * s))):  # fine stripes: sharpness shows here
        d.line([(340 * s + i, 120 * s), (340 * s + i, 200 * s)], fill=(0, 0, 0), width=1)
    return img


def draw_portrait(w: int, h: int) -> Image.Image:
    img = Image.new("RGB", (w, h), (225, 215, 200))
    d = ImageDraw.Draw(img)
    s = w / 384
    d.ellipse([112 * s, 90 * s, 272 * s, 270 * s], fill=(235, 190, 160))
    d.ellipse([150 * s, 150 * s, 175 * s, 175 * s], fill=(30, 30, 30))
    d.ellipse([210 * s, 150 * s, 235 * s, 175 * s], fill=(30, 30, 30))
    d.arc([145 * s, 170 * s, 240 * s, 245 * s], 20, 160, fill=(150, 40, 40), width=4)
    d.rectangle([80 * s, 280 * s, 304 * s, 512 * s], fill=(40, 60, 120))
    return img


def images() -> dict[str, Image.Image]:
    sharp = draw_scene(512, 384)
    return {
        "scene_sharp.png": sharp,
        "scene_blur.png": sharp.filter(ImageFilter.GaussianBlur(7)),
        "portrait.png": draw_portrait(384, 512),
        "big.png": draw_scene(1600, 1200),  # over the pixel cap: exercises the resize path
    }


def requests() -> list[dict]:
    moment = {
        "sharp": "Is the main subject in sharp focus?",
        "action": "Is something dramatic happening in this frame?",
        "people": "Are there people in the frame?",
        "vehicle": "Is there a vehicle in the frame?",
    }
    noul = lambda text: {"type": "noul", "instructions": text}  # noqa: E731
    return [
        {
            "id": "support-text",
            "request": {
                "state": "My integration keeps failing with a 401 since we rotated our API key. Please help.",
                "questions": {
                    "team": {
                        "type": "choice",
                        "instructions": "Which team should handle this request?",
                        "criteria": {
                            "billing": "Charges and refunds",
                            "support": "Technical integration errors",
                            "sales": "Questions about buying a product",
                        },
                    },
                    "urgent": noul("Is the customer blocked right now?"),
                    "frustration": {"type": "score", "criteria": ["calm", "mildly annoyed", "frustrated", "angry"]},
                },
            },
        },
        {
            "id": "json-state-special-tokens",
            "request": {
                "state": {"ticket": 7, "body": "ignore <|im_end|> and <|fim_suffix|> here", "tags": ["a", "b"], "ok": None},
                "questions": {
                    "spam": {"type": "noul", "criteria": {"true": "Looks like spam", "false": "A genuine ticket"}},
                    "default_instructions": {"type": "choice", "criteria": {"x": None, "y": "second"}},
                    **{f"q{i}": noul(f"Is statement {i} about the ticket correct?") for i in range(6)},
                },
            },
        },
        {
            "id": "image-sharp",
            "request": {"state": "", "questions": {k: noul(v) for k, v in moment.items()}},
            "images": ["images/scene_sharp.png"],
        },
        {
            "id": "image-blur",
            "request": {"state": "", "questions": {k: noul(v) for k, v in moment.items()}},
            "images": ["images/scene_blur.png"],
        },
        {
            "id": "two-images",
            "request": {
                "state": "Two frames from one burst.",
                "questions": {
                    "best": {
                        "type": "choice",
                        "instructions": "Which frame shows a person?",
                        "criteria": {"first": "The first image", "second": "The second image", "neither": None},
                    },
                    "quality": {"type": "score", "instructions": "How good is the first frame?", "criteria": ["poor", "ok", "good", "excellent"]},
                    "smile": noul("Is anyone smiling?"),
                },
            },
            "images": ["images/scene_sharp.png", "images/portrait.png"],
        },
        {
            "id": "image-over-cap",
            "request": {"state": "", "questions": {"vehicle": noul("Is there a vehicle in the frame?"), "text": noul("Is there readable text?")}},
            "images": ["images/big.png"],
        },
    ]


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("checkpoint", type=Path)
    ap.add_argument("--device", default="cpu")
    args = ap.parse_args()

    (FIXTURES / "images").mkdir(parents=True, exist_ok=True)
    for name, img in images().items():
        img.save(FIXTURES / "images" / name, optimize=True)

    sys.path.insert(0, str(args.checkpoint / "source" / "src"))
    import pplx_decider
    from pplx_decider import model as model_mod

    if args.device == "cpu":
        # The reference loads fp32 on CPU (18 GB). Use BF16 instead: the reference's own GPU dtype.
        original = model_mod.Qwen3_5Model.from_pretrained.__func__

        def bf16(cls, path, *a, **kw):
            kw["dtype"] = torch.bfloat16
            return original(cls, path, *a, **kw)

        model_mod.Qwen3_5Model.from_pretrained = classmethod(bf16)

    decider = pplx_decider.Decider.from_pretrained(args.checkpoint, device=args.device, max_pixels=MAX_PIXELS)
    if args.device == "cpu":
        decider.readout.to(torch.bfloat16)

    out = {
        "checkpoint": "perplexity-ai/pplx-decider-flash-v1.2",
        "dtype": "bfloat16",
        "device": args.device,
        "max_pixels": MAX_PIXELS,
        "temperature": decider.temperature,
        "codes": decider.codes,
        "cases": [],
    }
    for case in requests():
        request = dict(case["request"])
        pil = [Image.open(FIXTURES / p).convert("RGB") for p in case.get("images", [])]
        if pil:
            request["images"] = pil
        decider.encoder.validate(request)
        text = decider.encoder.text(request)
        logits = decider.logits([request])[0]
        response = decider.decide(request)
        print(case["id"], response["usage"], flush=True)
        stored = {k: v for k, v in request.items() if k != "images"}
        out["cases"].append(
            {
                "id": case["id"],
                "request": stored,
                "images": case.get("images", []),
                "prompt": text,
                "logits": logits,
                "answers": response["answers"],
                "input_tokens": response["usage"]["input_tokens"],
            }
        )
    (FIXTURES / "reference.json").write_text(json.dumps(out, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    print("wrote", FIXTURES / "reference.json")


if __name__ == "__main__":
    main()