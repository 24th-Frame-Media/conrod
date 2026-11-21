# Running Perplexity's decision model in-process: runtime choice

Model: `perplexity-ai/pplx-decider-flash-v1.2` (Apache-2.0, **public, not gated** -- the card says
"private", the Hub API says `private:false, gated:false`; every file downloaded without a token).
Qwen3.5-4B backbone, 4.54 B parameters in BF16 (9.1 GB) plus a vision tower. v1.3 is public too
(`pplx-decider-flash-v1.3`, protocol v4: one question per sequence, `<|fim_middle|>` separator, plus a
`locate-span` adapter); this work pins **v1.2** because it is the protocol `joint_config.json` and
the brief describe. Moving to v1.3 later is a new GGUF plus a prompt change in `protocol.rs`.

What a runtime must do (from the card and `protocol.py`):

1. Qwen3.5 hybrid stack: 24 Gated-DeltaNet (linear attention, causal) + 8 full-attention layers
   (`full_attention_interval 4`, head_dim 256, output gate, partial RoPE with interleaved M-RoPE).
2. Full-attention layers **noncausal** over the whole request; linear layers stay causal.
3. A `[255, 2560]` readout read at each `<|fim_suffix|>` marker (not an LM head).
4. Vision tower (24-layer ViT, patch 16, merge 2) feeding image positions before the text.
5. Softmax over the valid options at temperature 1.5539.

## Options

(a) and (b) were judged from op/model-support inspection (ONNX op set, DirectML notes in this repo,
candle's source tree), **not built**. (c) was built and measured.

### (a) ONNX export + `ort` + DirectML -- rejected

* Gated DeltaNet has no ONNX op. Export means tracing the chunked recurrence into thousands of
  scatter/matmul nodes (or a hand-rolled scan), data-dependent in sequence length; `torch.onnx`
  cannot trace Transformers' fallback `chunk_gated_delta_rule` into a usable static graph, and the
  fused kernels (`flash-linear-attention`) are CUDA/Triton only.
* DirectML (the repo's own `docs/research/models-onnx-deploy.md`) has no
  `GroupQueryAttention`/`Attention` with a per-layer noncausal mask at these sizes, no int4
  `MatMulNBits` on all adapters, and a 2 GB-per-tensor class of limits that a 248 k x 2560 embedding
  (1.27 GB fp16) sits next to. Even the plain Qwen3 text path is a research project on DML.
* A from-scratch export would also need re-validating parity layer by layer. Not attempted (estimate: weeks), and
  the result would run through an op-by-op fallback path nobody else exercises.

### (b) Rust-native `candle` -- rejected

* candle `main` ships `qwen3`, `qwen3_moe`, `qwen3_vl` but **no Qwen3.5 / Gated-DeltaNet**
  (`candle-transformers` 0.11.1 `models/mod.rs`: no `qwen3_5`, no delta-rule kernel). It would have to
  be written: the chunked delta rule, the short causal conv, the gated RMSNorm, interleaved M-RoPE and
  the noncausal mask, all with CUDA kernels.
* candle's CUDA backend needs an NVIDIA GPU, the CUDA toolkit at build time and CUDA DLLs
  (hundreds of MB) at run time; Conrod ships DirectML so that any DX12 GPU works. Fine for the
  author's RTX 3070 Ti, wrong for everyone else.
* int8/int4 on candle is GGUF-only (`quantized_*`), again with no Qwen3.5 variant.

### (c) llama.cpp (ggml), loaded as DLLs and driven through its C API -- **chosen**

The brief rules llama.cpp out "unless it can do noncausal attention + per-marker readout". It can:

* Upstream llama.cpp (tag **b12359**, in the `qwen35` arch) already runs Qwen3.5's Gated DeltaNet and
  M-RoPE, and `llama-mtmd` runs the Qwen3-VL vision tower. Perplexity's own GGUF
  (`perplexity-ai/pplx-decider-flash-gguf-v1.2`, Q8_0 + F16 projector) stores the readout as
  `output.weight` (zero outside the 255 code tokens), the temperature and codes as
  `pplx_decider.*` keys, and `qwen35.attention.causal = false`. The runtime flag is already a normal
  llama.cpp hparam; **no patch of llama.cpp is needed for v1.2**
  (the patch that exists is for v1.3's protocol v4 and for MACaQ 2/4-bit).
* Conrod does not use `llama-server` (its `/v1/systemone` speaks protocol v1). It binds
  `llama.dll` / `mtmd.dll` / `ggml.dll` at run time (`libloading`, ~60 functions, no link-time
  dependency, nothing to load unless the user turns the feature on) and does what the card says a
  serving system must: builds the v3 joint prompt itself, tokenizes with mtmd, evaluates text
  tokens and image embeddings in **one** `llama_process` batch with explicit positions (M-RoPE),
  requests logits only at the `<|fim_suffix|>` markers, reads the code-token columns and applies
  temperature + softmax. (`crates/conrod-decider/src/lib.rs`, `Inner::run`.)
* GPU backend: **Vulkan** build (any vendor, no CUDA install; the same "nothing to install" promise
  as DirectML). It runs on the laptop's RTX 3070 Ti via the NVIDIA driver's Vulkan ICD. A CUDA build
  would be ~1.5x faster on NVIDIA but adds a >500 MB redistributable; left as a later optional asset.
* Quantization: the official **Q8_0** GGUF is the shipped asset (5.2 GB). **4-bit is produced on the
  user's machine** by llama.cpp's own quantizer from the Q8_0 (`Decider::quantize_int4`, Q4_K_M,
  `--allow-requantize`, ~60 s, 2.86 GB), so no second multi-GB file has to be hosted or pinned.
* No Python at run time. Python is used at build time only to make the reference fixture.

Rejected variants of (c): `llama-server` over HTTP (protocol mismatch, an extra process and port,
and it re-encodes the image per question); MACaQ 2/4-bit files (need the patched fork and a
custom runtime for ~0.4 GB less than Q4_K_M).

## Measured on this laptop (RTX 3070 Ti Laptop, 8 GB, Vulkan, llama.cpp b12359)

| | int8 (Q8_0) | int4 (Q4_K_M from Q8_0) |
|---|---|---|
| weights file | 4.80 GiB (5.16 GB) + 0.63 GiB projector | 2.86 GiB + 0.63 GiB projector |
| VRAM after load, 589 k-pixel cap (of 7.07 GiB free) | **5.9 GiB** used, 1.2 GiB left | **4.2 GiB** used, 2.9 GiB left |
| VRAM at the 1.6 MP cap (1648 tokens) | 0.00 GiB left: spills to shared memory, **10 s encode + 10.6 s decode** (unusable) | 2.8 GiB left, no spill |
| text-only, 181 tokens, 3 questions | ~90-105 ms | ~110 ms |
| 1 image at 0.59 MP (598 tokens) + 2 questions | 120 ms encode + 215 ms decode = **~0.34 s** | 111 + 222 = **~0.33 s** |
| 1 image at 1.6 MP (1648 tokens) + 2 questions | 20 s (spill, see above) | 380 + 545 = **~0.93 s** |
| load incl. warm-up (warm file cache) | 6-8 s at the 589 k cap (20-33 s at the 1.6 MP cap) | 6 s (33 s at 1.6 MP) |
| first request after load | `Decider::load` runs a throw-away image request itself: without it the first real frame took 17 s (Vulkan compiles kernels on first use) | same |

Target was < 1 s/frame for ~10 `noul` questions + 1 image: met at the default 589 k-pixel cap on both
(questions cost almost nothing: ten extra question blocks add ~150 tokens; the time is the
image; 3 questions vs 2 was not distinguishable in the timings). At the 1.6 MP cap only the int4 file runs on 8 GB.

Free-VRAM thresholds used by the gate (`gating.rs`): int8 needs 6.5 GiB free, int4 4.5 GiB, else the
model is "not available". (This 8 GB card has 7.07 GiB free with the desktop running: int8 fits at the
default 589 k-pixel cap, measured; 6 GB cards get int4; 4 GB cards get nothing. Those two thresholds
are the measured usage above plus margin, not measured on other cards.)

## Parity against the reference

`scripts/decider/reference_fixture.py` runs the official `pplx_decider` (Transformers 5.17,
BF16 on CPU -- same arithmetic as its GPU path) on 6 requests / 24 questions (support-ticket text with
choice+noul+score, JSON state with special tokens in the content and default instructions, single
images sharp/blurred, two images with choice+score, an image over the pixel cap that forces the
resize path) and stores prompts, readout logits and answers
(`crates/conrod-decider/tests/fixtures/reference.json`). `cargo test -p conrod-decider --test parity`:

* **prompt text byte-identical on all 6 cases** (always run, no weights needed);
* **input token count identical on all 6 cases, images included** (181 / 341 / 369 / 369 / 545 / 1677),
  so the image resize reproduces the processor's grid;
* int8: mean |dp| **0.0070**, worst **0.0616** (one `noul` question, an unanswerable junk prompt near
  p=0.5), **0 argmax flips**; the answers on realistic questions are within 0.005;
* int4: mean 0.0165, worst 0.076, 1 argmax flip (a near-tie).

This agrees with the third-party validation of the same GGUF
(Q8_0 vs reference: mean gap 0.008, largest 0.044; same pick 99.7 %).

## Known limits

* Requests are serialized (one context); `max_tokens` defaults to 4096. More than 64 questions per
  request is refused (the output buffer is sized for it).
* The image encode runs before the decode; there is no batching across requests.
* v1.2 recommends one question per request for accuracy; the joint format is what the GGUF
  was converted for. The brief's per-frame use (10 `noul` questions + 1 image) is joint; callers
  that care should send the questions they need as separate requests (the image is re-encoded each
  time).