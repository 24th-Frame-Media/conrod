# ONNX models for photo "keeper" scoring: deploy research

Scope: extra models for Conrod (offline Windows photo culling, Rust, ONNX Runtime). Target: DirectML on an RTX 3070 Ti laptop (8 GB), with CPU-only fallback.

Method: web search and page fetches, about 27 tool calls. Sizes and opsets are from HF file listings where stated. Anything marked **est.** or **unverified** was not checked against a primary source. No model was downloaded or run.

## 1. Headline findings

- **Ready ONNX exists for most of the list.** The exceptions are LAION aesthetic (MLP only, no ONNX file), NIMA (a third-party ONNX file with no clear licence), and gaze (no clean commercial-safe weights).
- **Cheapest real value:** the CLIP ViT-B/32 image tower (Qdrant, ~352 MB, MIT) as the embedding for the personal taste head. Add the 3.5 MB LAION aesthetic MLP on top of CLIP ViT-L/14 if the large tower is affordable.
- **Licence is the main blocker, not export.** The 86Cao IQA bundle (TOPIQ, MANIQA, MUSIQ, CLIP-IQA+, LIQE) is ready to use, but the pyiqa toolbox is PolyForm-Noncommercial and the weights are trained on KonIQ and similar sets. Treat these as non-commercial until checked.
- **DirectML is a maintenance-mode EP.** Microsoft's own doc now says DirectML is in "sustained engineering" and new work has moved to WinML. It also lists DeformConv and GridSample 5-D as unsupported, and maximum opset 20. Models with DeformConv (BiRefNet) will partly fall back to CPU on DML.
- **Tokenizer:** only the CLIP/SigLIP **text** tower needs a tokenizer. Aesthetic, IQA embedding and taste-head use need the **image tower only**. Text is only needed for zero-shot prompts (CLIP-IQA+ style prompts, text-to-image adherence).

## 2. Per-model table

Sizes are the ONNX file on HF (fp32 unless marked). Opset and input are as found; "?" means not verified.

| # | Model | ONNX source found | Size | Opset / input | Licence (code / weights) | CPU / GPU latency per image | Verdict |
|---|---|---|---|---|---|---|---|
| A | **LAION aesthetic v2 MLP** (`sac+logos+ava1-l14-linearMSE.pth`) | No ONNX file. Original `.pth` in GitHub repo | 3.54 MB `.pth` | Input 768-d L2-normalised CLIP ViT-L/14 image embedding; linear layers 768→1024→128→64→16→1, no activations | Repo Apache-2.0 (GitHub page). Weights trained on SAC, LOGOS and AVA data (AVA is a research dataset; flag) | MLP negligible (<1 ms). Cost is the CLIP tower below | Trivial to port: export to ONNX or reimplement in Rust with `ndarray` (five matmuls). Strong value for "is it aesthetically good" |
| B | **CLIP ViT-L/14 image tower** (for A) | `immich-app/ViT-L-14__laion400m_e32` (`visual/model.onnx`), also `immich-app/ViT-L-14__laion400m_e31` (1.22 GB) and `immich-app/ViT-L-14-336__openai` | ~1.22 GB (laion e31); ViT-L-14-336 pointer shows ~351 MB, **likely fp16 or mismatched, verify** | 224×224 for L/14 (the aesthetic MLP was trained on 224 input; the 336 variant is not compatible per Geonmo discussion) | OpenAI CLIP MIT; OpenCLIP LAION weights licence **unverified** (believed MIT) | GPU est. 15–30 ms fp16 on 3070 Ti; CPU est. 300–700 ms | Works. Large. The biggest practical cost in this group. Notes: "Large CLIP graphs were unreliable on DirectML" (Synthet doc), so benchmark before shipping |
| C | **CLIP ViT-B/32 image tower** | `Qdrant/clip-ViT-B-32-vision` `model.onnx` (LFS size 351,686,194 B; fastembed lists it as `size_in_GB 0.34`). Text: `Qdrant/clip-ViT-B-32-text` (`tokenizer.json` 2.22 MB, `vocab.json`, `merges.txt`, `model.onnx`) | 352 MB (fp32), 512-d output | 224×224; output 512-d | MIT (fastembed metadata) | CPU est. 15–30 ms; GPU est. 3–6 ms | **Easiest real-value embedding.** Use for the personalised taste head. Note the pair in the same FastEmbed family is MIT |
| D | CLIP ViT-B/32 (alt, Immich) | `immich-app/ViT-B-32__laion2b-s34b-b79k` (in Immich collection) | ? | ? | ? | ? | Pick Qdrant unless a LAION-trained variant is wanted |
| E | **SigLIP base** | `Xenova/siglip-base-patch16-224` `onnx/vision_model.onnx`: fp32 372 MB, fp16 186 MB, int8 94 MB, q4 63 MB. Text: `text_model.onnx` 441 MB. Also `-256`, `-384`, `-512` variants | 372 MB fp32 | 224 (also 256/384/512) | Google SigLIP Apache-2.0 (believed; **verify**); Xenova export MIT-compatible | CPU est. 40–80 ms; GPU est. 5–10 ms (fp16) | Good image-embedding alternative to CLIP. Needs a SentencePiece-style text tokenizer for text prompts |
| F | **SigLIP2 base** | `immich-app/ViT-B-16-SigLIP2__webli` (ONNX, ~4k downloads). Also `immich-app/ViT-SO400M-16-SigLIP2-384__webli` (6.4k downloads), `ViT-L-16-SigLIP2-256__webli` | ? (not fetched; HF listing had no file sizes) | ? | Google SigLIP2 Apache-2.0 (believed; **verify**) | ? | Ready ONNX. Most mature-looking SigLIP2 export in the wild. Check the immich model card licence before shipping |
| G | **DINOv2 small** | `onnx-community/dinov2-small-ONNX`: `onnx/model.onnx` (fp32) and `model_uint8.onnx`. Converted by transformers.js tooling | fp32 ~88 MB (est. from 22M params); uint8 ~25 MB (est.) | 224 default; patch 14; output CLS + patch tokens | DINOv2 code Apache-2.0; weights Apache-2.0 (believed; **verify**) | CPU est. 10–20 ms; GPU est. 3–5 ms | Ready. Good for composition/near-duplicate similarity (not aesthetics). Use the CLS token |
| H | DINOv2 base | Not fetched. `dinov2_reg_base_image` is already referenced by Synthet pipeline (timm, custom wrapper) | ~350 MB (est.) | 224 | as G | est. 30–60 ms CPU | Export via Optimum; no ready ONNX found |
| I | **MobileCLIP2** (S0–S4) | `RuteNL/MobileCLIP2-S3-OpenCLIP-ONNX` (open_clip Rust crate usable). Apple original via `open_clip` | S3: image 125 M params, text 124 M params (est. file ~250 MB + ~250 MB; **verify**) | 256 px for S3 (**verify**) | Apple MobileCLIP licence is the Apple sample-code style licence (**unverified; check before shipping**) | Apple iPhone 12 Pro Max, image encoder: S0 1.5 ms, S2 3.6 ms, S3 8.0 ms, S4 19.6 ms (from card, **not desktop**) | Good speed-to-accuracy. Licence needs a check before shipping |
| J | **IS-Net (DIS) general use**, int8 | `imgly/isnet-general-onnx` (MIT export). Int8 weight-only: `xrds/isnet-general-onnx-int8` (`onnx/model_quantized.onnx`). Lightframes ships the int8 at 45,314,968 B | ~45 MB int8 | Input `input_image` 1×3×1024×1024, output `output_image` | Weights Apache-2.0 (DIS, Xuebin Qin); ONNX export MIT (imgly) | CPU est. 0.5–1.5 s (1024, int8 est.); GPU est. 60–120 ms | Good salient-subject mask (subject area, background clutter). Smaller and simpler than BiRefNet |
| K | **BiRefNet-lite** | `onnx-community/BiRefNet_lite-ONNX` (1024, fp32/fp16). 512-px variants `studioludens/birefnet-lite-512`, `shozebDev/birefnet-lite-512-onnx`. DeformConv variant `AssetFurnace/birefnet-lite-onnx` (180,666,557 B, opset 19 native DeformConv) | ~180 MB (fp16 est.) | 1024 or 512 | MIT (weights and code; ZhengPeng7) | Apple CPU: 9.5 GB peak, ~5 s (common export); 4.0 GB, 1.8 s (DeformConv export). GPU: unknown | Best quality matte, but heaviest. **DeformConv not supported on DML**, so expect CPU fallback for those nodes. Use IS-Net first |
| L | **U²-Netp** (salient) | rembg release `u2netp.onnx`, mirrored at `edgetools/u2netp`, `skillsafe-ai/u2netp`, `nfa04/u2netp-onnx` | 4.6 MB | 320×320 (**verify**) | Apache-2.0 per mirrors; **rembg issue #837 disputes licence/provenance of the v0.0.0 asset**. Flag | <100 ms CPU (est.) | Tiny. Lower quality than IS-Net; use only if IS-Net is too big |
| M | **MediaPipe face mesh / blendshapes** | `Heliosoph/mediapipe-face-onnx` (Apache-2.0, includes Face Landmarker port and licence chain). `yakhyo/mediapipe-face-mesh-onnx` (PyTorch + ONNX; 468 landmarks + 10 iris, BlazeFace bundled). `py-feat/mp_blendshapes` is PyTorch (not ONNX) | Heliosoph file sizes not fetched; face mesh ~2–4 MB (est.), BlazeFace <1 MB | Face crop 192 or 256 px | Apache-2.0 (Google MediaPipe; mirrors keep the chain) | CPU est. 3–8 ms for mesh and blendshapes; GPU not needed | **High value for portraits**: eye-open (EAR), smile, head pose, gaze proxy. Check that the blendshape head is included in the chosen port |
| N | **FER+ emotion** `emotion-ferplus-8.onnx` | `onnx/models` (`validated/vision/body_analysis/emotion_ferplus`), `onnxmodelzoo/emotion-ferplus-8` (HF) | 33–35 MB (fp32) | opset 8, 64×64 grayscale, 8 emotion classes | Repo MIT. **FER+ dataset licence restricts use (FER2013-derived research set); flag** | CPU est. 2–5 ms | Easy to load, low value for keeper scoring (smile is better from blendshapes) |
| O | **NIMA (MobileNet)** | `cromsc/nima-mobilenet-aesthetic` (ONNX, licence not shown in snippet). TFLite only: `litert-community/NIMA-LiteRT` (Apache-2.0) | ? | 224 (NIMA standard) | Upstream idealo NIMA Apache-2.0 (believed); cromsc licence **unknown** | CPU est. 5–10 ms | Cheap aesthetic/technical scorer. Verify licence of the ONNX file, or convert from the LiteRT model (Apache-2.0) |
| P | **TOPIQ-NR, MANIQA, MUSIQ, CLIP-IQA+, LIQE, TReS, DBCNN, HyperIQA** | `86Cao/IQA-ONNX-Models`: maniqa 437 MB, musiq 104 MB, clipiqa 146 MB, liqe ~675 MB (two-stage with CLIP B/32), tres 575 MB, dbcnn 59 MB, hyperiqa 105 MB. Most have external data (`.onnx.data`) that must sit beside the `.onnx`. Opset 14–18 | see left | 224×224 fixed (single-scale MUSIQ is a simplification) | **pyiqa v0.1.16 is PolyForm-Noncommercial-1.0.0** (PyPI). Pretrained weights come from original authors, mostly research-only. **Treat as non-commercial** | est. CPU 0.3–1.5 s for MANIQA/LIQE (ViT); est. GPU 10–40 ms | Ready ONNX, but licence is a red flag for shipping. Also MUSIQ and MANIQA are fixed-224, which changes scores vs original multi-scale (Synthet doc warns about drift). Use only for personal builds, or find Apache-2.0 alternatives (Synthet notes ARNIQA and MUSIQ are Apache-2.0; not verified) |
| Q | **Gaze, L2CS-Net** | ONNX runs (libreyolo, Luxonis), but no clean repo | ~95 MB (ResNet50, est.) | 448 face crop (est.) | Code MIT (Ahmednull). **Weights trained on Gaze360 / MPIIGaze: research and non-commercial only** (libreyolo doc). Luxonis lists MIT, conflicting | CPU est. 80–150 ms; GPU est. 5–10 ms | Blocked for commercial use. Alternative: `yakhyo/gaze-estimation` (MobileNet/MobileOne, ONNX inference, pretrained weights; licence **unverified**) |

## 3. Export feasibility (for models without a ready ONNX)

- **LAION aesthetic MLP:** trivial. Five `Linear` layers with no activations. Export with `torch.onnx.export` or reimplement in Rust. Keep the L2 normalisation outside the graph.
- **TOPIQ / ARNIQA:** tensor-only wrappers; export with the PyTorch Dynamo exporter, fixed shape first (Synthet doc, "amber-green").
- **MUSIQ (TensorFlow):** hard. The graph decodes bytes and builds a variable multi-scale patch pyramid. Export the numeric core only, or replace it (Synthet doc, "red").
- **BiRefNet:** ready ONNX exists. DeformConv is the risk on DirectML (see K).
- **LIQE:** needs a two-stage export (CLIP B/32 plus scoring head) plus precomputed text features. Ready in 86Cao, licence-blocked.

## 4. Rust side

- **ort crate:** crates.io shows `ort` up to **2.0.0-rc.15** (published 2026-10-19), with the older `rc.3` pulling ORT 1.28. Check the latest `rc` and pin it. Feature flags to check on that version: `directml`, `cuda`, `load-dynamic` (for shipping a DLL next to the exe). The `docs.rs` pages for 1.x reference ORT 1.13–1.17; ignore them.
- **DirectML:** the EP is "sustained engineering" in ORT docs. Session needs `memory_pattern = false` and sequential execution. One session cannot be `Run` from multiple threads on DML. Expect DML to lose ops (DeformConv, GridSample 5-D) and fall back to CPU silently; verify with a provider log.
- **Preferred plan on the RTX 3070 Ti:** CUDA EP (`onnxruntime-gpu`) plus CPU fallback, and benchmark WinML as the vendor-neutral option. The MEMORY/research note in the Synthet doc recommends the same.
- **Tokenizers:** for CLIP text use the HF `tokenizers` crate with `tokenizer.json` (Qdrant ships one, 2.22 MB). The image-only path needs no tokenizer.
- **Preprocessing:** do PIL-style resize, normalisation and centre crop in Rust, or externalise it. Score drift from different resize/interpolation is the main correctness risk (Synthet doc).

## 5. Ranking: easiest to ship with real value

| Rank | Model | Why | Effort | Licence risk |
|---|---|---|---|---|
| 1 | CLIP ViT-B/32 image (Qdrant ONNX, 352 MB) | Ready ONNX, MIT, 512-d embedding for the taste head | Low | Low |
| 2 | LAION aesthetic MLP (3.5 MB) + CLIP ViT-L/14 image tower (Immich ONNX, ~1.2 GB) | Direct "good photo" score; MLP is trivial to port | Medium (large tower) | Medium (AVA data, OpenCLIP weights unverified) |
| 3 | MediaPipe blendshapes / face mesh ONNX (Apache-2.0) | Eye-open, smile, head pose on portraits: high culling value, ~few MB | Low–medium | Low |
| 4 | IS-Net general int8 (~45 MB, MIT export) | Subject mask for composition and keep/reject | Low | Low (Apache DIS weights) |
| 5 | DINOv2 small (onnx-community, ~88 MB fp32) | Good near-duplicate and composition embedding | Low | Low (verify weights licence) |
| 6 | SigLIP2 B/16 (immich ONNX) | Strong embedding alternative to CLIP | Low–medium | Medium (verify licence) |
| 7 | NIMA MobileNet (ONNX or LiteRT) | Cheap generic aesthetic/technical score | Medium (licence check) | Medium (cromsc licence unknown) |
| 8 | MobileCLIP2-S3 (RuteNL ONNX) | Fast embedding, reported good accuracy | Low–medium | Medium (Apple licence to check) |
| 9 | BiRefNet-lite (MIT) | Best matte, but heavy and DML-unfriendly | Medium–high | Low |
| 10 | FER+ emotion (ONNX zoo) | Expression signal; blendshapes cover most of this | Low | Medium (FER+ dataset) |
| 11 | TOPIQ / MANIQA / MUSIQ / CLIP-IQA+ / LIQE (86Cao bundle) | Ready technical-quality scores | Low | **High (pyiqa PolyForm-NC; KonIQ data)** |
| 12 | U²-Netp (4.6 MB) | Tiny subject mask | Low | Medium (rembg provenance issue) |
| 13 | L2CS gaze | Gaze direction | Medium | **High (Gaze360 research-only)** |

## 6. Licence red flags

1. **pyiqa / IQA-PyTorch (PolyForm-Noncommercial-1.0.0):** the code and the derived ONNX bundles (86Cao) inherit non-commercial restrictions. Do not ship without a legal check.
2. **Gaze360 / MPIIGaze (L2CS-Net):** research and non-commercial only, even though the code is MIT.
3. **AVA dataset (LAION aesthetic MLP training):** the dataset is for research use. Weights trained on it may inherit that restriction. Flag.
4. **FER / FER+ dataset:** research-restricted. The ONNX model is MIT but the training data is not.
5. **rembg U²-Net asset (issue #837):** provenance of the v0.0.0 `u2net.onnx` / `u2netp.onnx` is disputed. Prefer the original Apache-2.0 DIS / U²-Net weights.
6. **Apple MobileCLIP:** check the Apple licence before bundling weights.
7. **Unknown licences to verify before use:** `cromsc/nima-mobilenet-aesthetic`, `yakhyo/gaze-estimation` weights, OpenCLIP LAION weights (Immich), SigLIP/SigLIP2 weights (Apache believed).

Conrod is a personal, offline app today. If it will ever be sold or redistributed, rules 1–3 are blockers.

## 7. Sources

- LAION aesthetic predictor, Apache-2.0 repo and `.pth` (3.54 MB): https://github.com/christophschuhmann/improved-aesthetic-predictor
- Aesthetic predictor v2 architecture (768→1024→128→64→16→1): https://github.com/christophschuhmann/improved-aesthetic-predictor and https://huggingface.co/spaces/Geonmo/laion-aesthetic-predictor
- Immich CLIP collections: https://huggingface.co/collections/immich-app/clip and https://huggingface.co/collections/immich-app/multilingual-clip
- Immich ViT-B/16 SigLIP2 ONNX: https://huggingface.co/immich-app/ViT-B-16-SigLIP2__webli
- Immich ViT-L-14 laion400m_e31 / e32 visual ONNX: https://huggingface.co/immich-app/ViT-L-14__laion400m_e31 and https://huggingface.co/immich-app/ViT-L-14__laion400m_e32
- Immich ViT-L-14-336 openai: https://huggingface.co/immich-app/ViT-L-14-336__openai
- Immich ONNX export pipeline (opset 19 default): https://deepwiki.com/immich-app/ml-models/4.2-onnx-export
- Immich PR #4700: https://github.com/immich-app/immich/pull/4700
- Qdrant CLIP ViT-B/32 vision: https://huggingface.co/Qdrant/clip-ViT-B-32-vision (352 MB LFS pointer)
- Qdrant CLIP ViT-B/32 text + tokenizer: https://huggingface.co/Qdrant/clip-ViT-B-32-text
- FastEmbed model list (licence, sizes): https://github.com/qdrant/fastembed/blob/main/fastembed/image/onnx_embedding.py and https://qdrant.tech/documentation/embeddings/fastembed
- Xenova SigLIP base (fp32 372 MB vision; fp16/int8/q4 variants): https://huggingface.co/Xenova/siglip-base-patch16-224/tree/main/onnx
- Xenova SigLIP base-384: https://huggingface.co/Xenova/siglip-base-patch16-384
- DINOv2 small ONNX: https://huggingface.co/onnx-community/dinov2-small-ONNX
- MobileCLIP2 ONNX (RuteNL): https://huggingface.co/RuteNL/MobileCLIP2-S3-OpenCLIP-ONNX
- IQA ONNX bundle (86Cao): https://huggingface.co/86Cao/IQA-ONNX-Models
- IQA-PyTorch model cards: https://iqa-pytorch.readthedocs.io/en/stable/ModelCard.html
- pyiqa licence (PolyForm-Noncommercial-1.0.0): https://pypi.org/project/pyiqa/
- Synthet ONNX feasibility doc (per-model export verdicts, DML lessons): https://github.com/synthet/image-scoring-pipeline/blob/master/docs/planning/models/ONNX_CONVERSION_FEASIBILITY.md
- IS-Net int8 and licence record (MIT export, Apache DIS): https://traderings.com/vendor/isnet/LICENSE.txt and https://huggingface.co/imgly/isnet-general-onnx
- Lightframes IS-Net int8 (45,314,968 B), Real-ESRGAN, MI-GAN: https://lightframes.si (blog post on in-browser ONNX)
- BiRefNet_lite ONNX: https://huggingface.co/onnx-community/BiRefNet_lite-ONNX ; 512 variants https://huggingface.co/studioludens/birefnet-lite-512 and https://huggingface.co/shozebDev/birefnet-lite-512-onnx ; DeformConv variant https://huggingface.co/AssetFurnace/birefnet-lite-onnx ; licence record https://dica.com.br/js/vendor/birefnet-LICENSE.txt
- U²-Net / u2netp mirrors: https://huggingface.co/edgetools/u2netp , https://huggingface.co/skillsafe-ai/u2netp ; provenance issue https://github.com/danielgatis/rembg/issues/837
- MediaPipe ONNX ports: https://huggingface.co/Heliosoph/mediapipe-face-onnx , https://github.com/yakhyo/mediapipe-face-mesh-onnx , https://huggingface.co/py-feat/mp_blendshapes
- FER+ ONNX: https://github.com/onnx/models/tree/main/validated/vision/body_analysis/emotion_ferplus and https://huggingface.co/onnxmodelzoo/emotion-ferplus-8
- NIMA: https://huggingface.co/cromsc/nima-mobilenet-aesthetic , https://huggingface.co/litert-community/NIMA-LiteRT , https://github.com/idealo/image-quality-assessment
- L2CS-Net licence: https://github.com/Ahmednull/L2CS-Net/blob/main/LICENSE and https://libreyolo.com/docs/models/l2cs.md ; gaze alternative https://github.com/yakhyo/gaze-estimation
- ORT DirectML EP (sustained engineering, opset 20, DeformConv and GridSample 5-D unsupported): https://onnxruntime.ai/docs/execution-providers/DirectML-ExecutionProvider.html
- ORT WinML preference issue: https://github.com/microsoft/onnxruntime/issues/25899
- ort crate versions: https://crates.io/api/v1/crates/ort/versions
