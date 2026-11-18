# Aesthetic / keeper models for Conrod: research notes (Nov 2026)

Scope: learned IQA/IAA models, personalised aesthetics, composition, burst/moment/expression models, and local VLM judging. Built from about 20 web searches and fetches, with sources at the end.

Conventions: **[U]** = unverified, estimated, or taken from memory. Speeds are estimates for an RTX 3070 Ti Laptop (8 GB) unless a source says otherwise. "NC" = non-commercial.

## 0. Bottom line

- **The licences matter more than the leaderboard.** The popular toolbox `pyiqa` (chaofengc/IQA-PyTorch) moved to PolyForm Noncommercial 1.0.0 in 0.1.16, and its weights are CC BY-NC-SA 4.0. Every pyiqa-backed model (NIMA, TOPIQ-IAA, MUSIQ, CLIP-IQA+, LIQE, Q-Align wrapper) is therefore off-limits if Conrod is ever sold or distributed commercially. Permissive options exist for everything below.
- **The best personal-taste signal is a head trained on the photographer's own labels**, not a generic aesthetic score. Public models are trained on AVA, KonIQ, SPAQ or Flickr raters. Literature on personalised aesthetics (PARA, Flickr-AES) says that a small head on frozen features, trained per user, is the practical route.
- **Within a burst, the question is relative.** Absolute scores tend to give near-identical numbers to near-duplicate frames. Use listwise or pairwise losses trained within burst groups.
- **Shortlist:** (1) frozen DINOv2 or CLIP embedding plus a personal head trained on the 2,300 labels; (2) continuous MediaPipe face blendshapes; (3) a generic LAION aesthetic head on CLIP ViT-L/14 as a feature; (4) a local VLM pairwise judge on each burst's top 3 only; (5) saliency and thirds-grid scalars as features. Details in section 6.

## 1. Learned quality and aesthetic models

| Model | Predicts | Training data | Reported accuracy (source) | Size / input | Speed (est.) | ONNX | Licence / commercial | In pyiqa? |
|---|---|---|---|---|---|---|---|---|
| NIMA (Inception-ResNet-v2; VGG16) | Aesthetic score distribution (1-10) | AVA (pyiqa retrains KonIQ/SPAQ variants) | AVA PLCC/SRCC 0.717/0.713 (inception), 0.662/0.657 (VGG16) [pyiqa benchmark] | ~50M [U]; 224 to 299 px | ~5 ms [U] | Easy (CNN); no official ONNX seen | Paper code academic; pyiqa weights CC BY-NC-SA. **NC** | Yes (`nima`, `nima-vgg16-ava`) |
| LAION Aesthetic Predictor V1/V2 (linear or MLP head on CLIP ViT-L/14) | Aesthetic 1-10 ("how much do you like this image") | SAC, LAION-Logos, AVA (V2); SAC (V1) | AVA PLCC/SRCC 0.666/0.665 [pyiqa benchmark, `laion_aes`] | Head is 768→1024→128→64→16→1 (~0.9M params [U]); backbone ViT-L/14 224 px (~300M params) | Head: negligible. Backbone: ~10–25 ms/img fp16 batched [U] | Head trivial. CLIP ViT-L/14 ONNX exports exist | Repo MIT (LAION). Improved predictor repo Apache-2.0. OpenAI CLIP MIT | Appears in pyiqa benchmark table |
| improved-aesthetic-predictor (sac+logos+ava1-l14-linearMSE.pth) | Same as above | Same as above | Same as above | Same as above | Same as above | Same as above | **Apache-2.0** for code; weights ride the repo licence. AVA images are research-only [U on weight provenance] | No |
| MUSIQ (musiq-ava, musiq-koniq) | Multi-scale IQA; the AVA variant does aesthetics | KonIQ, SPAQ, AVA variants | Not in pyiqa's IAA table (no split info) | Transformer, multi-scale input up to ~512 px [U] | Fast [U] | PyTorch only found | Upstream permissive; pyiqa weights NC | Yes |
| TOPIQ-IAA (topiq_iaa, topiq_iaa_res50) | Aesthetics | AVA | PLCC/SRCC 0.790/0.791 (Swin/ResNet-50 variant) and 0.737/0.736 (res50) [pyiqa benchmark] | ResNet-50 class [U] | ~10 ms [U] | Possible but no export found | pyiqa NC | Yes |
| Q-Align / OneAlign (mPLUG-Owl2, 7.8B LLM) | Quality and aesthetics, as text levels converted to a score | KonIQ, SPAQ, KADID, AVA, LSVQ | AVA PLCC/SRCC 0.819/0.822 [pyiqa benchmark]; claims beat LAION by ~10% on IAA [Q-Align paper] | 448×448 with 64 visual tokens; 7.8B LLM [paper] | Too heavy for 8 GB at fp16 [U]; 4-bit ~5–6 GB and ~0.3–1 s/img [U] | No (HF transformers, `trust_remote_code`) | Q-Align repo is S-Lab licence plus LICENSE; model card and LLaMA-2 lineage mean **commercial use unclear; treat as NC** | Yes (`qalign`) |
| Q-ReAlign (Qwen3.5-VL 0.8B / 4B / 9B) | Same as Q-Align, newer backbone | Same | Not checked | 0.8B and 4B are feasible on 8 GB [U] | Likely fast [U] | No | Backbone licence (Qwen) plus project terms: **unverified** | Yes (`qrealign`) |
| ArtiMuse (InternVL-based MLLM) | Score plus 8 expert attributes, with text | ArtiMuse-10K (research only), AVA, FLICKR-AES, PARA, TAD66K variants | AVA SRCC 0.827 / PLCC 0.826 [CVPR 2026 paper, vendor-reported] | ~8B [U] | Heavy: 1–2 s/img at 4-bit [U] | No | HF card says checkpoint Apache-2.0, but dataset is non-commercial and the PARA variant inherits PARA's academic-only terms | No |
| CLIP-IQA+ / LIQE / MANIQA / QualiCLIP / ARNIQA / TOPIQ-NR | Technical quality (noise, blur, exposure) | KonIQ, SPAQ, KADID | Dataset-specific, see pyiqa benchmark | Mostly ResNet50 / ViT-B [U] | 5–20 ms [U] | Possible per model | pyiqa NC | Yes |

Notes:
- The Q-Align AVA gain is about 0.82 versus 0.67 for LAION and 0.66 for NIMA VGG. Those are general-taste numbers. They do not show that a model picks the photographer's keeper within a burst. The 22% top-1 from sharpness already suggests that technical-quality models will miss most of the signal.
- Technical IQA (sharpness, noise) is already covered by Conrod's native-resolution edge-blur measure. Learned IQA adds little here.

## 2. Personalised aesthetics

| Approach | Labels needed | Evidence | Fit for Conrod |
|---|---|---|---|
| Linear personalisation of frozen VLM hidden states (ridge per user) | Per-user; 10-shot versus 100-shot tested | ACL Findings 2026: all Linear-Hidden variants improve with more shots. On PARA, 10-shot Linear-Hidden was below the raw-text baseline [paper] | Good design pattern. Needs the VLM hidden state; cheap per user |
| Residual offset on a generic model (Ren and Foran, 2017) | Small per-album sets (14 real albums in REAL-CUR) | Introduced FLICKR-AES (40k images, 210 raters) and REAL-CUR; residual adaptation with SVR [ICCV 2017 paper] | Historical baseline; supports the "generic plus small user offset" pattern |
| Attribute features plus linear head (Gilley, 2025 blog) | 5–10 VLM-scored attributes; hundreds of ratings per user | Spearman ρ 0.75 on one Flickr-AES rater, vs 0.668 SOTA cited [blog; single rater; not peer-reviewed] | Interpretable; useful as a feature set |
| MLP over base-model features (EyeEm, NVIDIA blog 2017) | "as few as 10 to 20" samples to adapt (vendor claim) | Vendor blog; qualitative [U] | Shows the approach works in production, but the number is a claim |
| PRAC (ACM MM 2026 oral; code and weights released Oct 2026) | PARA | Generic GIAA plus PreferSelect/PreferMerge [GitHub] | Worth reading. Licence not checked |
| Commercial culling (Aftershoot, Imagen) | Thousands of past edits or keepers | Vendor docs: Aftershoot recommends ≥5,000 edits for edit profiles (starts at 2,500); Imagen cites ~2,000 | Vendor marketing, but it confirms the scale. Aftershoot's AI-Automated mode learns from final keepers [support docs] |

### Recommended approach

1. **Frozen embedding.** Use DINOv2 ViT-S/14 (21M params) or ViT-B/14 (86M). Both are Apache-2.0, and DINOv2 is strong on near-duplicate and fine-grained similarity. Fallbacks: CLIP ViT-B/32 (MIT; ONNX vision and text towers are published, e.g. Xenova and LibrePhotos' split vision/text ONNX) or SigLIP-B/16 (Apache-2.0; FP16 IR about 390 MB for the vision+text bundle [OpenVINO doc]). Pick by validation, not by reputation.
2. **Head.** Logistic regression or a 1–2 layer MLP (≤100k params) on the embedding plus scalar features (sharpness, blendshapes, subject-to-grid distance). Train with a within-burst softmax (listwise) loss, where the positive is the delivered frame and the negatives are the other frames in the same burst.
3. **Data.** 2,300 delivered frames. Negatives are the non-delivered frames from the same bursts (confirm this). Plot a learning curve at 100 / 300 / 1,000 / 2,300 positives. The public evidence suggests gains level off quickly for a linear head, but this is an estimate for this photographer [U].
4. **Cost.** Embedding ~3–8 ms/frame on GPU in fp16 [U]; head training seconds on CPU.

## 3. Composition

| Method | Type | Evidence | Practical? |
|---|---|---|---|
| SAMP-Net on CADB (BMVC 2021) | Learned composition score | CADB: 9,497 images, 5 raters, scored 1-5; ResNet-18 backbone; EMD loss [GitHub, paper] | Research dataset from AADB; **commercial and weight licences not verified**. CADB-trained models were weak on the composition-rule tasks in CVPR 2025 benchmark (avg mAP and CDA results were low) |
| Composition-aware GNN / graph (2019–2020 papers) | Learned | Reported gains on AVA and AADB [papers] | Research code, not packaged |
| Rule of thirds classifier (Göring and Raake, MMSP 2021) | Learned binary rule detector | ResNet152 accuracy 0.841 on Flickr-derived labels; drops to 0.672 on a 5-annotator set (AVT) vs annotator mean [slides] | Shows rule labels are noisy; treat as weak |
| Subject centroid vs thirds and phi-grid points (classical) | Heuristic | Standard practice; no benchmark found | Cheap. Use YOLO boxes or saliency centroid; fine as a head feature |
| Saliency: U2-Net (Apache-2.0; 176 MB full, 4.7 MB "p") | Saliency map | Published on six SOD datasets; 30 FPS at 320 px on a 1080 Ti [paper] | Good. u2netp is small enough to run on every frame [U on speed] |
| Saliency: BASNet (MIT), TRACER (Apache-2.0) | Saliency | Published, boundary-aware | Alternatives to U2-Net |
| Horizon / balance / leading lines | Classical (Hough, gradients) | Not searched | Low value for culling; skip unless the photographer's keepers show a clear horizon signal |

Verdict: learned composition models are research-grade, licence-unclear, and noisy. Use heuristics (centroid, thirds distance, saliency balance) as features in the personal head, and let the head weigh them.

## 4. Burst, moment, and expression models

| Source | What it does | Evidence | Usable? |
|---|---|---|---|
| Wang et al., "Real-time burst photo selection" (arXiv 1803.07212; IEEE TIP 2019) | Light-head adversarial ranker; scores each frame; pairwise labels from AMT (838k pairs) | Top-1 hit 64.1% of bursts (about 11 frames each) and top-3 86.2% on consensus labels; 0.47 MB model; 13 ms on iPhone 7 [paper] | Architecture good. **Code not confirmed.** Labels are generic consensus, not personal |
| Chang et al., "Automatic Triage for a Photo Series" (SIGGRAPH 2016 / Princeton) | Relative ranking within a photo series | Dataset: 15,545 photos in 5,953 series with human preferences; argues absolute scores fail within series [paper] | **Closest published task to Conrod.** Use as a benchmark reference; dataset licence unchecked |
| Burst Image Quality Assessment (BuIQA, arXiv 2511.07958) | Task-driven quality per burst frame | 7,346 bursts, 45,827 frames; frame importance varies by downstream task [paper] | Relevant framing; code and licence unchecked |
| Google Best Take / Auto Best Take (Pixel 10+) | Blends face expressions from up to 150 burst frames | Product docs; no quantitative eval [Google blog, store page] | Not a keeper picker. Blending is out of scope |
| Apple Photos burst "suggested" keeps | Marks suggested frames | Product docs only [MacRumors] | No method published |
| MediaPipe Face Landmarker (blendshapes) | 52 continuous blendshapes per face: eyeBlink L/R, mouthSmile L/R, jawOpen, browDown, eyeLook* (gaze proxy) | Blendshape head is Apache-2.0; training on lab captures; degrades beyond ~80° yaw or under 50% occlusion [model card] | **Yes**: continuous eye openness and smile per face, run on every frame. Replaces a binary closed-eye check. Note that *Left and *Right refer to the subject, and mirroring inverts them |

## 5. VLM as judge

| Model | Local fit | Evidence | Notes |
|---|---|---|---|
| Qwen2.5-VL-7B (already in Conrod) | Ollama `qwen2.5vl:7b`, 6.0 GB | VQualA 2025 MICBench (2-, 3-, 4-image comparison): overall 0.734; 0.812 on triplets, 0.825 on quadruplets [challenge paper]. Qwen2.5-VL-72B: 0.755 | Good for distortions. Not tested for aesthetic keepers |
| Qwen3-VL-8B | Ollama `qwen3-vl:8b`, 6.2 GB + 1.2 GB projector; 256K context | Apache-2.0 per model card; MMMU 69.6, DocVQA 96.1 [TinyWeights; vendor-reported]. Ollama page lists text and image input | Probably the best local option. Multi-image: "up to 8" per promptquorum (secondary) |
| Qwen2.5-VL-32B in 2AFC | Too big for 8 GB | 2AFC accuracy: BAPPS 0.638, KADID 0.794, PIPAL 0.732 [2AFC benchmark paper] | Reference point for pairwise accuracy |
| MiniCPM-V 4.5 (8B, Qwen3-8B + SigLIP2-400M) | Ollama `minicpm-v`; multi-image | OpenCompass 77.0 (vendor) [model card, CVPR 2026] | Licence not checked [U] |
| GPT-4V / Gemini (cloud) | Not allowed (offline) | 78% human agreement on pairwise tasks [MLLM-as-a-Judge] | Reference only |

### Known failure modes

- **Position bias is large.** In one VLM-judge study (Qwen2.5-VL-7B as judge, mesh task), about 26% of raw verdicts reversed when the two items were swapped. The same paper reports that uncorrected agreement was 0.333 versus 0.714 after keeping only swap-consistent verdicts. Transfer to photos is unverified [U]. Judging the Judges (IJCNLP 2025) reports strong position bias across many LLM judges.
- **Pairwise is more reliable than scoring.** Q-Align and the 2AFC work both show that comparisons are more stable than absolute ratings.

### Prompt pattern

Show 2–4 frames (ideally 2 at a time) with a fixed prompt: "Which frame best shows the moment and expression the photographer would keep? Answer A or B, then one reason." Run both orders. Keep only swap-consistent verdicts. Use the output as a feature for the personal head, not as the final decision.

### Cost (estimates) [U]

- About 5–15 s per burst comparison on the 3070 Ti at ~720 px per frame, two orders included.
- VRAM: ~6–7 GB for qwen3-vl:8b at Q4 including the projector. This is tight with other models loaded. Use qwen3-vl:4b (3.3 GB) if VRAM is shared.

## 6. Ranked shortlist for Conrod

| Rank | Component | Where it runs | Expected value | Effort | Runtime per frame (est.) | Licence | Verdict |
|---|---|---|---|---|---|---|---|
| 1 | Personal head on frozen DINOv2 (S or B) embedding, trained listwise per burst | Every frame (embedding); head on every frame | **High.** Directly targets the photographer's keeper signal, which current features miss | 3–5 days including label extraction and grouping | 3–8 ms GPU (B/14, fp16) | Apache-2.0 | Build first |
| 2 | MediaPipe blendshape features (eyeBlink, smile, gaze, brow) per face, max and mean over faces | Every frame with faces | Medium–high for group shots: closed eyes and smiles are classic reasons to keep or reject | 1–2 days | 5–10 ms CPU [U] | Apache-2.0 | Build second; replaces the binary closed-eye flag |
| 3 | LAION aesthetic head on CLIP ViT-L/14 (or the DINO/CLIP embedding if it fits) | Every frame | Medium. Generic taste, so it is a feature, not a judge | 0.5–1 day | 15–25 ms GPU [U] | MIT (LAION), OpenAI CLIP MIT | Add as a feature. Choose CLIP-L only if the embedding can be shared |
| 4 | VLM pairwise judge (Qwen3-VL-8B or existing qwen2.5vl:7b) on each burst's top 3 | Top-3 of each burst only | Medium, uncertain. Only component that sees expression and moment semantically | 2–4 days (prompt, cache, double order, parsing) | 5–15 s per burst [U] | Qwen3-VL Apache-2.0 | Add behind a flag; validate before enabling by default |
| 5 | Saliency and thirds-grid scalars (U2-Net-p, centroid distance, balance) | Every frame | Low–medium | 1 day | 3–10 ms [U] | Apache-2.0 | Add as features; let the head decide their weight |

### Not recommended

- pyiqa-backed models (NIMA, TOPIQ-IAA, MUSIQ-AVA, CLIP-IQA+, LIQE, Q-Align wrapper, Q-ReAlign wrapper): PolyForm NC code, CC BY-NC-SA weights.
- Q-Align / OneAlign: 7.8B LLM, too heavy for 8 GB; licence unclear.
- ArtiMuse: 8B MLLM; dataset and PARA variant restrictions.
- SAMP-Net and other composition CNNs: research dataset, weak evidence, licence unclear.
- Google Best Take, Apple burst suggestions: no reusable method.

### Integration plan

**Every frame (ingest):**
1. Decode and preview, as now.
2. Compute the DINOv2 embedding (ONNX via ONNX Runtime + DirectML). Export with `optimum-cli export onnx --model facebook/dinov2-small` [Emgu doc]. Cache the vector in the DB alongside the frame.
3. Compute MediaPipe blendshapes per face (CPU, or GPU delegate). Store per-face values.
4. Compute saliency (U2-Net-p) and centroid-to-grid features.
5. Keep the existing sharpness and COCO outputs.

**Burst stage:**
1. Group frames (existing burst grouping).
2. Score each frame with the personal head (listwise model). Rank.
3. For the top 3 only, if the VLM toggle is on: run the pairwise judge in both orders. Keep swap-consistent verdicts. Combine with the head score (e.g. final = head + λ·VLM preference). Show the reason text.
4. Surface the top-1 as "suggested keeper", with the reasons.

**Training and validation:**
1. Labels: delivered (positive) versus not delivered, grouped by burst. Use only bursts with at least one delivered frame.
2. Split by shoot or date, never by random frame. Adjacent frames are near-duplicates, so a random split will leak.
3. Metrics against the scorecard:
   - Top-1 burst hit rate. Baselines: random 12%, sharpness 22% (measured).
   - Top-3 hit rate.
   - Within-burst pairwise accuracy (ties excluded).
   - Global AUC, reported for context only.
   - Per-shoot spread, to catch shoots where the model fails.
4. Ablate: embedding only, scalars only, embedding plus scalars, with and without the VLM term.
5. Learning curve at 100 / 300 / 1,000 / 2,300 positives.
6. Suggested ship gate (my proposal): beat sharpness top-1 by ≥10 percentage points on held-out shoots, with no shoot worse than sharpness.

## 7. Caveats

- Public numbers are on AVA, KonIQ, SPAQ, KADID and Flickr. None measure burst keeper selection for wedding, event, or sports work. Use them only to order candidates.
- The 12% random baseline implies about 8 frames per burst on average. Check that the 2,300 labels are per frame and that negatives are included.
- Vendor claims (EyeEm, Aftershoot, Imagen, Google, Apple) are marketing unless stated otherwise.
- Licence notes here are not legal advice. Re-check model cards before distributing any weights.

## Sources

Model and toolbox
- pyiqa model cards: https://iqa-pytorch.readthedocs.io/en/latest/ModelCard.html
- pyiqa benchmark (IAA table): https://iqa-pytorch.readthedocs.io/en/latest/benchmark.html
- IQA-PyTorch repo, licence (PolyForm NC 1.0.0; weights CC BY-NC-SA): https://github.com/chaofengc/IQA-PyTorch
- Licence issue #294: https://github.com/chaofengc/IQA-PyTorch/issues/294
- IQA-PyTorch-Weights (CC BY-NC-SA 4.0): https://huggingface.co/chaofengc/IQA-PyTorch-Weights
- Facet third-party licence table (secondary): https://github.com/ncoevoet/facet/blob/master/docs/THIRD_PARTY_MODELS.md

Q-Align, ArtiMuse, LAION
- Q-Align paper (ICML 2024): https://proceedings.mlr.press/v235/wu24ah.html ; arXiv 2312.17090: https://arxiv.org/abs/2312.17090
- Q-Align repo and model zoo: https://github.com/Q-Future/Q-Align ; https://q-future.github.io/Q-Align/model_zoo
- ArtiMuse (CVPR 2026): https://openaccess.thecvf.com/content/CVPR2026/html/Cao_ArtiMuse_Fine-Grained_Image_Aesthetics_Assessment_with_Joint_Scoring_and_Expert-Level_CVPR_2026_paper.html ; arXiv 2507.14533: https://arxiv.org/abs/2507.14533
- ArtiMuse HF card (Apache-2.0 checkpoints; dataset non-commercial): https://huggingface.co/Thunderbolt215215/ArtiMuse
- LAION-Aesthetics: https://laion.ai/blog/laion-aesthetics
- LAION aesthetic predictor (MIT): https://github.com/LAION-AI/aesthetic-predictor
- improved-aesthetic-predictor (Apache-2.0): https://github.com/christophschuhmann/improved-aesthetic-predictor

Personalised aesthetics
- PARA (CVPR 2022): https://arxiv.org/abs/2203.16754 ; dataset page (academic use): https://yuzheyang.github.io/blog/Dataset/PARA.html
- PRAC (ACM MM 2026): https://github.com/yzc-ippl/PRAC
- Personalised AIA with VLM representations (ACL Findings 2026): https://aclanthology.org/2026.findings-acl.1706.pdf
- Ren and Foran, Personalized Image Aesthetics (ICCV 2017): https://openaccess.thecvf.com/content_ICCV_2017/papers/Ren_Personalized_Image_Aesthetics_ICCV_2017_paper.pdf
- Gilley, linear personalised aesthetics (blog): https://jagilley.github.io/linear-aesthetics.html
- EyeEm personalised aesthetics (NVIDIA blog): https://developer.nvidia.com/blog/personalized-aesthetics-machine-learning
- LAPIS dataset (CVPRW 2025): https://openaccess.thecvf.com/content/CVPR2025W/CVEU/papers/Maerten_LAPIS_A_novel_dataset_for_personalized_image_aesthetic_assessment_CVPRW_2025_paper.pdf
- Aftershoot support docs: https://support.aftershoot.com/en/articles/10601968-technical-answers-about-aftershoot ; https://support.aftershoot.com/en/articles/9190187-the-difference-between-ai-assisted-culling-ai-automated-culling
- Imagen AI comparisons (vendor-adjacent): https://imagen-ai.com/valuable-tips/aftershoot-vs-imagen-guide

Foundation embeddings
- DINOv2 (Apache-2.0): https://github.com/facebookresearch/dinov2 ; model card: https://github.com/facebookresearch/dinov2/blob/main/MODEL_CARD.md
- DINOv2 ONNX export note (Emgu doc): https://www.emgu.com/wiki/files/5.0.0/document/html/T_Emgu_CV_Models_Dinov2.htm
- OpenAI CLIP ViT-B/32 ONNX: https://huggingface.co/openai/clip-vit-base-patch32/blob/12b36594d53414ecfba93c7200dbb7c7db3c900a/onnx/model.onnx
- LibrePhotos CLIP ONNX usage: https://github.com/LibrePhotos/librephotos/blob/dev/apps/backend/service/clip_embeddings/clip_onnx.py
- open_clip (model list): https://github.com/mlfoundations/open_clip
- SigLIP ONNX/OpenVINO sizes: https://docs.openvino.ai/2024/notebooks/siglip-zero-shot-image-classification-with-output.html

Composition and saliency
- CADB and SAMP-Net: https://github.com/bcmi/Image-Composition-Assessment-Dataset-CADB ; https://arxiv.org/html/2104.03133v2
- CVPR 2025 composition benchmark: https://openaccess.thecvf.com/content/CVPR2025/papers/Zhao_Can_Machines_Understand_Composition_Dataset_and_Benchmark_for_Photographic_Image_CVPR_2025_paper.pdf
- Göring and Raake, rule of thirds (MMSP 2021 slides): https://stg7.github.io/pdfs/2021/rules_presentation.pdf
- TANet / TAD66K (IJCAI 2022): https://ijcai.org/proceedings/2022/0132.pdf
- U2-Net (Apache-2.0): https://github.com/xuebinqin/U-2-Net
- BASNet (MIT): https://github.com/xuebinqin/BASNet
- TRACER (Apache-2.0): https://github.com/Karel911/TRACER

Burst, expression, and MediaPipe
- Wang et al., real-time burst photo selection: https://arxiv.org/abs/1803.07212 ; IEEE TIP: https://ieeexplore.ieee.org/document/8920214
- Chang et al., automatic triage for a photo series: http://gfx.cs.princeton.edu/pubs/Chang_2016_ATF/chang2016-triage-preprint.pdf
- BuIQA burst benchmark: https://arxiv.org/html/2511.07958v1
- Google Best Take: https://blog.google/products/photos/how-google-photos-best-take-works/ ; Auto Best Take: https://store.google.com/magazine/pixel-best-take?hl=en-US
- Apple burst suggestions: https://www.macrumors.com/how-to/take-burst-photos-iphone
- MediaPipe Face Landmarker: https://developers.google.com/edge/mediapipe/solutions/vision/face_landmarker
- MediaPipe Blendshape V2 model card (Apache-2.0): https://storage.googleapis.com/mediapipe-assets/Model%20Card%20Blendshape%20V2.pdf

VLM judges
- Qwen3-VL on Ollama: https://ollama.com/library/qwen3-vl ; 8B tag: https://ollama.com/library/qwen3-vl:8b
- Qwen3-VL-8B licence and benchmarks (secondary): https://tinyweights.dev/posts/best-local-vision-language-models-2026
- Local VLM comparison for Ollama, including multi-image (secondary): https://www.promptquorum.com/power-local-llm/local-vision-models-llava-ollama-2026
- MiniCPM-V 4.5 (CVPR 2026): https://openaccess.thecvf.com/content/CVPR2026/papers/Yu_MiniCPM-V_4.5_Cooking_Efficient_MLLMs_via_Architecture_Data_and_Training_CVPR_2026_paper.pdf
- VQualA 2025 MICBench: https://arxiv.org/html/2509.09190
- 2AFC-LMM prompting: https://arxiv.org/html/2402.01162v1
- Compare2Score (NeurIPS 2024): https://proceedings.neurips.cc/paper_files/paper/2024/file/393367b4ccf1d56e51276243d5cb85c4-Paper-Conference.pdf
- Position bias in LLM judges (IJCNLP 2025): https://aclanthology.org/2025.ijcnlp-long.18.pdf
- Swap-consistency for VLM judges (mesh task, arXiv 2606.18451): https://arxiv.org/html/2606.18451
- MLLM-as-a-Judge: https://mllm-judge.github.io/
