# VLM judge for burst culling: research notes

Question: can a local vision-language model (VLM) judge photo quality or aesthetics, or pick the best frame from 2-4 near-identical burst frames, well enough to help Conrod? Current baseline: `qwen2.5vl:7b` via Ollama, about 8 s per crop at `num_ctx` 8192, on an 8 GB RTX 3070 Ti laptop GPU.

Confidence markers: **[verified]** = read in a source; **[uncertain]** = my estimate or a number I could not confirm; **[check]** = conflicting or unclear sources.

## 1. Benchmarks: how well do VLMs agree with humans?

**Single-image quality and aesthetics are mostly solved by fine-tuned scorers, not by zero-shot chat models.**

- Q-Align (ICML 2024), built on mPLUG-Owl2, scores images by reading the logits of five level words ("excellent", "good", "fair", "poor", "bad"). It reaches AVA aesthetic SRCC 0.822 / PLCC 0.817, and on IQA it trained on KonIQ/SPAQ/KADID. [verified: https://arxiv.org/html/2312.17090v1]
- The same paper's cross-dataset table shows an IQA-trained model scoring only 0.208 SRCC on AVA. Technical quality and aesthetics are different signals. [verified: same URL]
- The untuned mPLUG-Owl2 base scores KonIQ SRCC 0.552 before Q-Align training. Zero-shot general VLMs are much weaker than the tuned scorers. [verified: same URL]
- OneAlign (`q-future/one-align`) is the unified IQA + IAA + VQA model, with KonIQ SRCC about 0.941 and AVA about 0.823 (reported in a secondary comparison table). [verified via https://huggingface.co/q-future/one-align and https://arxiv.org/pdf/2507.14997v1; numbers secondhand]
- Q-ReAlign (Q-Future, 2025) moves the same recipe onto newer VLMs (Qwen-family backbones) and claims the same performance with 50% fewer parameters. [verified: https://github.com/Q-Future/Q-Align (update note)]

**Zero-shot or instruction-tuned VLMs on aesthetic questions (not scoring):**

- AesBench (2024): on aesthetic-perception multiple-choice, Q-Instruct, GPT-4V and Gemini Pro Vision all score around 72%, with Qwen-VL around 63%. Overall, models answer attribute questions far better than they judge overall quality. [verified: https://arxiv.org/html/2401.08276v1]
- UNIAA (2024): its fine-tuned LLaVA reaches 78.5% on aesthetic-perception MCQ, above GPT-4V (74.7%) and mPLUG-Owl2 (67.2%). Composition is the weakest dimension for almost every model (most below 60%). [verified: https://arxiv.org/html/2404.09619]
- AesTest (Nov 2025) is the most relevant to photographers. On the Photography subtask, professionals reach 0.83 accuracy and the best model 0.46 accuracy (Qwen2.5-VL-32B, 0.30 for the 7B). Qwen2.5-VL-7B scores 0.52 on Perception and 0.58 on Appreciation. Scaling from 7B to 32B helps Perception/Appreciation but not Photography. [verified: https://arxiv.org/html/2511.06360; table values for Qwen2.5-VL-7B read from the extracted table]

Takeaway: a 7B VLM is near chance on the "what makes a photo a keeper" question, and reasonable on generic attribute questions. Do not expect a zero-shot chat prompt like "rate this photo 1-10" to match a photographer's keeper decision.

**Multi-image comparison (what Conrod actually needs):**

- Co-Instruct / MICBench (ECCV 2024): 996 test MCQs over groups of 3-4 images, comparing quality. Random guess is 31.5%. Non-expert humans score 78.4%, GPT-4V 78.8%, Qwen-VL-Max 70.6%, and Co-Instruct (a fine-tuned mPLUG-Owl2 variant) 83.3%. Most open-source models at the time stayed under 50%. [verified: https://arxiv.org/html/2402.16641v2, Table 6]
- The MICBench gap between 3-image and 4-image questions is large for models with extended context. More images hurt, and visual-token reduction helped. [verified: same]
- Caveat: MICBench measures agreement on factual quality questions ("which image is sharpest?"), not the keeper decision. It is still the closest published fit to "pick the best of 3-4 near-identical frames".
- Caveat: Co-Instruct weights are distributed on request only, and the MICBench data is distributed on request because of NSFW content. Check licence before any commercial use. [verified: https://github.com/Q-Future/Co-Instruct/]

**Burst-specific data:**

- Microsoft's burst "goodness" model (2018) hit the user's top-1 choice 64.1% of the time and top-3 86.2%, with about 11 frames per burst on average. This is a useful baseline: a tiny purpose-built ranker. [verified: https://arxiv.org/abs/1803.07212]
- Series Photo Selection (SPS, Chang et al.) is the burst dataset cited by the 2026 IAA paper, but only partly open. [verified: https://arxiv.org/html/2603.03907v1]

**Model-to-model numbers I could not verify:** I could not find a single table comparing InternVL 2.5/3, MiniCPM-V 2.6/4, LLaVA-OneVision and Gemma 3 on quality agreement. AesTest's table gives InternVL2-8B Photography 0.26 Acc and MiniCPM-V (8B) Photography 0.25 Acc, against Qwen2.5-VL-7B at 0.30 [uncertain: read from an extracted table]. No source I found covers Qwen3-VL on these tasks. **[uncertain]**

## 2. Small specialised models: sizes, licences, GGUF, VRAM

| Model | Base | Size | Output | GGUF / Ollama | Notes |
|---|---|---|---|---|---|
| Q-Align / OneAlign (`q-future/one-align`) | mPLUG-Owl2 | about 8B [uncertain; table says 8.2B] | scalar via logits | transformers only; no official GGUF | Strong IQA/IAA; single image |
| Q-ReAlign-Lite-4B | Qwen-VL family | 4B | scalar 0-1 via prefill + logits | community GGUF `VERIGEN/q-realign-GGUF`: Q8_0 5.2 GB + mmproj F16 0.67 GB | Maintainer-verified vs. transformers (Pearson 1.000 on 20 images) in llama.cpp; community conversion, so re-verify [verified: https://huggingface.co/VERIGEN/q-realign-GGUF] |
| Q-ReAlign-Pro-9B | Qwen-VL family | 9B | scalar 0-1 | transformers; GGUF status unknown | Heavier than fits comfortably in 8 GB with image tokens [uncertain] |
| Co-Instruct | mPLUG-Owl2 | about 7-8B | free text / MCQ | no GGUF found | Multi-image comparison; research licence, distribution by request |
| Qwen3-VL-8B-Instruct | Qwen | 8.8B | free text | official GGUF `Qwen/Qwen3-VL-8B-Instruct-GGUF` (Q4_K_M, Q8_0; mmproj FP16/Q8_0); Ollama `qwen3-vl:8b-instruct-q4_K_M` 6.2 GB incl. projector; Apache 2.0 | Use the explicit `-instruct` tag: plain `qwen3-vl:8b` pulls the Thinking variant. Needs Ollama >= 0.12.7 [verified: https://ollama.com/library/qwen3-vl:8b, https://huggingface.co/Qwen/Qwen3-VL-8B-Instruct-GGUF] |
| Qwen2.5-VL-7B (current) | Qwen | 7B | free text | `qwen2.5vl:7b` | Already in use; no multi-image comparison benchmark found for it [uncertain] |

**VRAM estimates (8 GB card):**

- Q4_K_M weights for an 8B model take about 5 GB, and the vision projector about 1.1-1.2 GB. [verified: https://jacar.es/en/local-multimodal-models-16gb-vram/, https://ollama.com/library/qwen3-vl:8b]
- KV cache for Qwen3-VL-8B is about 144 KiB per token (BF16 cache); at 8K context that is about 1.1 GiB. [verified: same jacar.es page; cross-checked with studiotvai.com table]
- Ollama's own runtime plus buffers add 1-3 GiB [verified: studiotvai.com]. Realistic total for Qwen3-VL-8B Q4 at 8K: about 7 GB, which is tight on 8 GB but workable; check `ollama ps` for CPU/GPU split. [uncertain on exact fit]
- Q-ReAlign-Lite-4B Q8 plus mmproj: about 6 GB at 8K context [uncertain, estimated from file sizes]. This fits 8 GB with headroom.

**Ollama logprobs (needed for the Q-Align-style score):**

- Ollama's native `/api/chat` and `/api/generate` accept `logprobs` and `top_logprobs` (per docs and the Go source for `ChatRequest` / `GenerateRequest`). [verified: https://docs.ollama.com/api/chat, https://github.com/ollama/ollama/blob/main/openai/openai.go]
- The OpenAI-compatible `/v1/chat/completions` layer lists "Logprobs" as supported in one doc version and unchecked in another, and an open issue (#16117, 2026) says the compat layer drops `logprobs`. [check: https://docs.ollama.com/api/openai-compatibility, https://github.com/ollama/ollama/issues/16117]
- Recommendation: use the native `/api/chat` endpoint for any logprob-based scoring. Verify on the installed Ollama version before relying on it.
- The Q-ReAlign reference code uses llama-server's OpenAI endpoint with `logprobs: true`, which works there. Ollama's compat layer may not.

## 3. Prompt patterns and failure modes

**Position bias is the main reliability problem for pairwise and listwise use.**

- Multi-image position bias is documented across LVLMs: reordering images changes answers, open-source models do best on late positions, and even GPT-4o is sensitive. [verified: https://arxiv.org/html/2503.13792v1]
- Pairwise judging with GPT-4-Turbo on shuffled answer pairs gave consistent verdicts 80.3% of the time (MLLM-Bench). [verified: https://aclanthology.org/2025.naacl-long.256.pdf]
- A 2026 de-biased VLM-judge protocol found that seven images in one prompt caused Qwen2.5-VL to answer by position (100% order flips). Their fix was two-image comparisons, with each pair judged in both orders and kept only when consistent. [verified: https://arxiv.org/html/2606.20364 (a 3D-generation paper; the failure mode transfers)]
- A 2025 study of LVLM judges for image editing found that order swaps reverse verdicts 60.9% of the time for qwen3.5, against 10.5% for identical re-queries. Swap-and-keep-consistent filters out order-sensitive pairs. [verified: https://arxiv.org/html/2610.01670v1; a 2026 Qwen generation, so re-measure with the Conrod prompt]

**Known failure modes relevant to culling:**

- Visual-bias fooling: brightness changes and text overlays systematically raise LVLM judge scores, and pairwise judging is equally vulnerable (FRAME benchmark, EMNLP 2025). A brighter, sharper-looking frame can win for the wrong reason. [verified: https://arxiv.org/html/2505.15249]
- Informativeness and length bias: judges prefer longer and more "informative" answers. For culling this could translate into preferring busier frames. [verified: https://aclanthology.org/2026.acl-long.703.pdf, MM-JudgeBias]
- Moment blindness: IQA and aesthetic models do not see expression, blink or gesture unless trained on it. The Metlivi and PhotoCore guides both stress expression and moment as the decisive factors, which a sharpness/aesthetic score misses. [verified as practitioner guidance: https://metlivi.com/blog/how-to-choose-which-burst-photo-to-keep-when-several-frames-look-nearly-identical]
- Hallucinated reasons: AesBench and AesTest both show models answer "why" questions with plausible but wrong attribute language. Treat free-text reasons as explanations, not evidence. [verified: AesBench Table 5 (interpretation scores ~1.0-1.3 for all models); AesTest]

**Prompt patterns that help (from the sources above):**

1. Use a single fixed rubric in the prompt: "Which image shows the subject sharpest, with eyes open and the best expression? Answer with one digit and nothing else."
2. For scoring, prefill the assistant answer (Q-ReAlign pattern) and read the logits of the level words. This avoids free-text parsing and is the most stable single-image signal. [verified: Q-ReAlign GGUF card]
3. For pairs or triples, query both orders and keep only consistent verdicts. Report the disagreement rate per burst.
4. Keep to 2 images per call where possible. Co-Instruct's MICBench shows accuracy falls as image count rises.
5. Ask for a short reason only after the verdict is recorded, and never use the reason as the decision input.

## 4. Cost: images per minute on 8 GB

I could not find measured throughput for Conrod's hardware. These are estimates to confirm with a benchmark: **[uncertain]**

- Image tokens: a 1080p image is about 2,025 visual tokens for Qwen3-VL (32x32 px per token); a 4K image is about 8,100 (jacar.es calculation from the processor config). Four 1080p images are about 8,100 tokens, which pushes a `num_ctx` of 8192 to its limit. [verified arithmetic: https://jacar.es/en/local-multimodal-models-16gb-vram/]
- Downsizing: a 768-1024 px long edge is probably enough for "which frame is sharpest" at subject level but not for eye-open checks on a small face. Downsizing cuts tokens roughly with the square of the scale factor. Crop to the subject (the existing crop step) before judging, then downsize to a fixed budget.
- Suggested budgets: 3 crops at about 768 px each, `num_ctx` 4096-6144. This should keep prefill under about 3,000 tokens. Measure `prompt_eval_duration` and `eval_duration` in the `/api/chat` response.
- Burst throughput: with 3 pairwise calls each in both orders (6 calls) at about 3-5 s each, a burst costs about 20-30 s, roughly 2-3 bursts per minute. A single listwise call with 3 crops in both orders (2 calls) is about 2x cheaper. [uncertain; measure]
- Q-ReAlign-Lite-4B scoring is one prefill and one generated token per image, so likely 1-2 s per crop. [uncertain]

## 5. Recommendation

**Do not use a VLM as the primary culling signal.** The evidence says zero-shot 7B VLMs are weak on photographer-level keeper judgements (AesTest Photography: best model 0.46 vs professionals 0.83), and they are sensitive to brightness, position and length. Keep the existing deterministic signals (subject-region sharpness, eye-open detection, exposure) as the primary ranker.

**Use a VLM only as a tiebreaker on the burst's top 3 candidates, after the deterministic ranker.**

- **Scoring model:** Q-ReAlign-Lite-4B (GGUF, Q8 about 5.2 GB + mmproj 0.67 GB) for an absolute aesthetic/quality scalar per crop. It is single-image, so position bias does not apply, and the reference GGUF was checked against the authors' code. Use llama.cpp's server or the native Ollama API with logprobs, after verifying Ollama's logprob support on the installed version. Caveat: the GGUF is a third-party conversion of a Qwen-family model and has no Ollama tag, so it adds a second runtime to Conrod unless the Ollama logprob path checks out.
- **Comparison model:** Qwen3-VL-8B-Instruct Q4_K_M (`qwen3-vl:8b-instruct-q4_K_M`, Ollama >= 0.12.7) for pairwise or triplet MCQs ("which crop has the subject sharpest / best expression?"). Keep it to 2-3 crops, both orders, and discard inconsistent pairs. Set `num_ctx` to 6144 or lower to stay in 8 GB.
- **Co-Instruct:** only worth trying if its licence allows Conrod's use, and no GGUF exists, so it would need transformers on the GPU. Not recommended as the first step.
- **Keep Qwen2.5-VL-7B out of the judge role:** it has no published multi-image comparison result, and the Qwen3-VL instruct model is the newer option with the same memory class.

**Prompt:** single rubric question, 2-3 crops, both orders, answer as a digit only, logprob or first-token read. Log the reason text separately and never use it for the decision.

## 6. Validation plan against delivered frames

Conrod has the photographer's delivered frames, which are the best available ground truth.

1. **Build the set:** for each burst, group frames by time and similarity. Mark the delivered frame (or frames) as the positive label. Keep only bursts with at least 3 frames and exactly one delivered frame. Aim for at least 100 bursts for a usable confidence interval; 50 is the minimum.
2. **Baselines:** compute three numbers on the same bursts:
   - random top-1 hit rate (1/n per burst, averaged; about 33% for 3-frame bursts, and the Co-Instruct random baseline of about 31% fits this range);
   - the deterministic ranker's top-1 and top-3 hit rates (the bar to beat);
   - the VLM ranker's top-1 and top-3 hit rates, applied only to the deterministic top 3.
3. **Metrics:** top-1 and top-3 hit rate, with 95% confidence intervals (bootstrap over bursts). Also report the order-consistency rate (fraction of pairs whose verdict survives the swap) and the verdict flip rate between the two orders.
4. **Decision rule:** keep the VLM tiebreaker only if it beats the deterministic top-1 on at least 5 percentage points more bursts than it loses, with the interval excluding zero. Otherwise drop it. Re-run after any prompt, crop-size or model change.
5. **Leakage check:** ensure the judge never sees filenames, EXIF, delivery order or the delivered-frame flag. Shuffle frame order per call.
6. **Cost check:** log wall time and `prompt_eval_count` per burst, so the throughput estimate in section 4 is replaced with a measured number.

## Sources

- Q-Align, arXiv:2312.17090 - https://arxiv.org/html/2312.17090v1
- OneAlign / Q-Align repo and model card - https://github.com/Q-Future/Q-Align , https://huggingface.co/q-future/one-align
- Q-ReAlign repo - https://github.com/Q-Future/Q-ReAlign
- Q-ReAlign GGUF (community conversion) - https://huggingface.co/VERIGEN/q-realign-GGUF
- Co-Instruct / MICBench, arXiv:2402.16641 - https://arxiv.org/html/2402.16641v2 ; repo https://github.com/Q-Future/Co-Instruct/
- AesBench, arXiv:2401.08276 - https://arxiv.org/html/2401.08276v1
- UNIAA, arXiv:2404.09619 - https://arxiv.org/html/2404.09619
- AesTest, arXiv:2511.06360 - https://arxiv.org/html/2511.06360
- AesBiasBench (EMNLP 2025) - https://aclanthology.org/2025.emnlp-main.386/
- Burst photo selection (Microsoft), arXiv:1803.07212 - https://arxiv.org/abs/1803.07212
- Fine-grained IAA with series photos, arXiv:2603.03907 - https://arxiv.org/html/2603.03907v1
- RvTC regression on AVA, arXiv:2507.14997 - https://arxiv.org/pdf/2507.14997v1
- Multi-image position bias, arXiv:2503.13792 - https://arxiv.org/html/2503.13792v1
- MLLM-Bench (NAACL 2025) - https://aclanthology.org/2025.naacl-long.256.pdf
- FRAME: Fooling the LVLM judges (EMNLP 2025), arXiv:2505.15249 - https://arxiv.org/html/2505.15249
- Informativeness bias (ACL 2026) - https://aclanthology.org/2026.acl-long.703.pdf
- MM-JudgeBias (ACL 2026) - https://aclanthology.org/2026.acl-long.1162.pdf
- Judge editing evaluation with order swaps, arXiv:2610.01670 - https://arxiv.org/html/2610.01670v1
- De-biased VLM judge protocol, arXiv:2606.20364 - https://arxiv.org/html/2606.20364
- Qwen3-VL-8B-Instruct GGUF - https://huggingface.co/Qwen/Qwen3-VL-8B-Instruct-GGUF
- Ollama qwen3-vl tags - https://ollama.com/library/qwen3-vl:8b
- Ollama API (chat, logprobs) - https://docs.ollama.com/api/chat ; OpenAI compat - https://docs.ollama.com/api/openai-compatibility ; issue #16117 - https://github.com/ollama/ollama/issues/16117
- VRAM and image-token arithmetic (secondary) - https://jacar.es/en/local-multimodal-models-16gb-vram/ ; https://studiotvai.com/vram-requirements/qwen3-vl-8b

Secondary sources (blog/vendor pages) used only for practitioner guidance and VRAM cross-checks: metlivi.com, photocore.pro, publicaihub.com, runlocal.laftel.io.
