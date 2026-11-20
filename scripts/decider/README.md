# Decision model (pplx-decider-flash) build scripts

Python is for BUILD time only; nothing here runs inside Conrod.

| Script | Purpose |
|---|---|
| `reference_fixture.py` | Runs Perplexity's reference (`pplx_decider`, Transformers) and writes the parity fixture `crates/conrod-decider/tests/fixtures/reference.json` plus the test images. |

## Environment

```powershell
python -m venv C:\path\decider-venv
C:\path\decider-venv\Scripts\python -m pip install torch==2.14.0 torchvision==0.29.0 --index-url https://download.pytorch.org/whl/cpu
C:\path\decider-venv\Scripts\python -m pip install transformers==5.17.0 safetensors==0.8.0 pillow==12.3.0 huggingface-hub==1.31.0 numpy
```

The checkpoint is public (not gated): `perplexity-ai/pplx-decider-flash-v1.2`, files
`model-0000?-of-00002.safetensors`, `readout.safetensors`, `tokenizer.json`, the small
JSON/Jinja files and `source/`. Download with `hf download` or curl.

```powershell
python scripts\decider\reference_fixture.py <checkpoint-dir>            # BF16 on the CPU
python scripts\decider\reference_fixture.py <checkpoint-dir> --device cuda   # if the BF16 weights fit your GPU
```

## What ships

The runtime weights are the official GGUF conversion
(`perplexity-ai/pplx-decider-flash-gguf-v1.2`: `decider-flash-v1.2-q8_0.gguf`,
`mmproj-decider-flash-v1.2-f16.gguf`); the 4-bit file is produced on the user's machine by
`Decider::quantize_int4` (llama.cpp's own quantizer), so no extra file is hosted. See
`docs/research/decider-runtime.md`.