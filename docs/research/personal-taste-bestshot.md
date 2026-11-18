# Personal taste for best-shot selection in bursts

Research note for Conrod. Scope: picking the frame a specific photographer delivers from a burst or near-duplicate series, using ~2,300 labelled delivered frames plus their non-delivered burst-mates.

Confidence markers: **[verified]** = read in a source this session. **[uncertain]** = from memory or inferred; check before relying on it.

## Summary

- Within-series selection is a **relative** problem. The best published approach (Chang et al. 2016) scores frames pairwise inside a series, not on absolute quality. That matches the observed failure of simple signals.
- Published burst-selection systems report roughly 64% top-1 and 86% top-3 against user choices (Light-Head Adversarial Network, 11 frames per burst on average), and 73% pairwise accuracy for Chang et al. Neither is a within-photographer personal-taste number.
- Personalised aesthetics work (PARA, LAPIS, and a 2026 VLM-probing study) shows that **a linear ridge/logistic head on frozen pretrained features, trained on one user's data, is a strong baseline**. It beats fine-tuning and text prompting in those papers.
- Recipe: frozen image embedding + **within-burst softmax (listwise) linear head** on embeddings centred per burst, plus a few hand-computed subject features. Validate with grouped CV by shoot and by top-1 within burst against the 1/n random baseline. Train in Rust or a bundled Python step; both are cheap.

## 1. Best-shot selection in bursts and series

### Chang et al., "Automatic Triage for a Photo Series" (SIGGRAPH 2016)
- Problem framing: relative quality within a series of photos of one scene. Explicitly the closest published match to Conrod's task. **[verified]** (abstract, https://dl.acm.org/doi/10.1145/2897824.2925908 and preprint https://pixl.cs.princeton.edu/pubs/Chang_2016_ATF/chang2016-triage-preprint.pdf)
- Method: a Siamese network (VGG-based prototype) learns a pairwise preference between frames in a series. Scores are then ranked within the series. **[verified]**
- Reported result: best model predicts human preference **73% of the time** (pairwise accuracy). Random pairwise is 50%. The paper also compares to several earlier baselines and reports mAP gains over them; I did not read the exact per-baseline numbers. **[verified for 73%; baseline table uncertain]**
- Dataset: photo series from personal albums, released publicly. This is the best-known public series dataset. Size and category mix **[uncertain]**; check the paper's dataset section.
- Lesson for Conrod: the pairwise *within-series* formulation is the right one. A pairwise accuracy of ~73% is the reference bar for a generic model, not a personal one.

### Follow-ups on the same dataset
- "Learning Multi-Scale Attentive Features for Series Photo Selection" (IEEE, 2020) and "Series Photo Selection via Multi-view Graph Learning" (arXiv 2203.09736) continue the series-selection benchmark. The multi-view graph paper reports around **72–74% accuracy** on the same kind of task **[verified from a table snippet; metric definition uncertain]**. Gains over Chang et al. are a few points. The graph/attention machinery is not worth the complexity for a single user with 2,300 labels.
- "Recursive Multi-Relational Graph Convolutional Network for Automatic Photo Selection" (IEEE, 2022) reports pairwise ranking with inference-conflict issues. **[verified title and framing only]**

### Real-time burst selection (Light-Head Adversarial Network, arXiv 1803.07212)
- Real-time "goodness" score per frame during capture. Trained on users' choices from bursts. **[verified]**
- Reported: best predicted frame matches the user's top-1 choice **64.1%** of the time, and a top-3 choice **86.2%**, with an average of **11 frames per burst**. **[verified]**
- Random top-1 with 11 frames is 1/11 ≈ 9.1%. The gain over random is therefore about 7x, but that is a population model, not a per-photographer one. **[my calculation]**
- Useful for Conrod: the top-3 metric is a sensible second number, and it is the one a culling UI actually uses.

### Commercial systems
- **Google Pixel Top Shot / Auto Best Take**: analyses burst frames for sharpness, lighting, closed eyes, smiles; eyes-closed and smile are explicit objective attributes. Google's own description says it "trained its best-moments algorithm" on such attributes; it is a generic, not personal, model. **[verified from 9to5google and Google support pages; training data not disclosed]**
- **Apple Live Photo key photo**: user-chosen; Apple does not publish an automatic selector. Not a model to copy. **[verified from Apple support]**
- Microsoft burst selection: no usable public paper found in this session. **[uncertain/not found]**

### Burst quality datasets
- HDR+ burst dataset (3,640 bursts, 28,461 images) is a capture-quality dataset, not a preference dataset. Not useful as labels. **[verified]**
- BuIQA (arXiv 2511.07958; 7,346 burst sequences) is a burst image-quality benchmark with subjective labels for multiple tasks. Likely about quality, not one photographer's keeps. **[verified existence only]**

## 2. Personalised image aesthetics (PIAA)

### PARA (Yang et al., CVPR 2022; arXiv 2203.16754)
- 31,220 images annotated by 438 subjects, with 9 image-level attributes and user attributes. **[verified]**
- Personal annotations are sparse per user. Evaluation typically uses per-user splits. Not a burst dataset; it is single-image ratings. Useful as a source of ideas, not a training set.

### LAPIS (Maerten et al., CVPR 2025)
- Personalised aesthetics dataset, mostly artworks. Cited in the 2026 VLM study below. **[verified as cited]**

### "What Do Vision–Language Models Encode for Personalized Image Aesthetics Assessment?" (Ryu & Yanaka, arXiv 2604.11374, April 2026)
This is the most directly useful recent result. **[verified from full text]**
- Method ("Linear-Hidden"): frozen VLM hidden states (one layer, average pooled) → **ridge regression per user**. No fine-tuning.
- Setup: 200 random users per dataset; **100-shot** support set per user (also 10-shot), 50 test images per user.
- PARA, Qwen3-VL 4B, layer LT15: user-averaged Spearman ρ **0.611** (R² 0.362). Raw text prompting ρ ≈ 0.50–0.57. LoRA fine-tuning ρ ≈ 0.49–0.58. Few-shot 10-shot prompting ρ ≈ 0.2–0.4.
- LAPIS, same setup: Linear-Hidden ρ ≈ 0.57; text baselines ≈ 0.1–0.26; LoRA worst of all.
- Domain-specific PIAA-ICI (trained on 12k PARA images) reaches ρ 0.620 on PARA but drops to 0.206 on LAPIS when transferred. The frozen-feature linear head transfers better (0.568). **[verified]**
- Key caution from the paper: they found the per-user *numeric calibration* inflates correlation unless evaluated per user. Evaluate per burst (ranking), not with pooled correlation.
- Another caution: on "hard" users (low agreement with generic aesthetics) the gain shrinks, and personalisation needs attributes beyond the generic probe set. **[verified]**
- Implication: 100 labelled examples per user is enough to get a usable linear head on frozen features. Conrod has ~2,300 frames, so the data is ample for one photographer, but ~9 broad genres with differing taste may need a per-genre term.

### Linear probes on CLIP for aesthetics
- "CLIP knows image aesthetics" (Hentschel, Kobs & Hotho; Frontiers in AI 2022, doi 10.3389/frai.2022.976235) shows linear regression on frozen CLIP features works for generic aesthetics. **[verified]**
- The LAION aesthetic predictor is a single linear layer (768 → 1) on frozen CLIP ViT-L/14 embeddings, trained on AVA+ user ratings. **[verified from repo README snippet, https://raw.githubusercontent.com/LAION-AI/aesthetic-predictor/main/README.md]**
- Community "preference from star ratings" projects do exactly Conrod's pattern (CLIP embedding + Ridge head trained on personal ratings; e.g. github.com/qweliant/fineshyt). Anecdotal; no accuracy. **[verified existence; no metrics]**

### Meta-learning and transductive PIAA
- Zhu et al. (IEEE TCYB 2020, "Personalized Image Aesthetics Assessment via Meta-Learning With Bilevel Gradient Optimization") meta-learns across users so a new user adapts from few examples. Reported to beat average-aesthetics baselines on personal albums. **[verified title/claims; numbers uncertain]**
- Transductive Aesthetic Preference Propagation (ACM MM 2022, doi 10.1145/3503161.3548244). **[verified existence]**
- Verdict for Conrod: meta-learning needs many users to pretrain. Conrod has one photographer. Skip it; use plain per-user regularised heads and only consider meta-learning if you later have many photographers.

### Individual differences caveat
- Chen et al. (BMVC 2025, arXiv 2502.20518, "On the Role of Individual Differences...") finds that transferring from generic to personal aesthetics is extrapolation and the reverse is interpolation, which works better. Implication: train on the photographer's own data, use generic scores only as features. **[verified from abstract]**

## 3. Active learning and online updating

No paper in this session directly studied online updating for burst culling. What is supported:
- The linear head is cheap enough to **retrain from scratch on each album export** (seconds on CPU for ~20k pairs × 768-d). Retraining beats incremental SGD for stability with small, shifting data. **[uncertain; engineering judgement]**
- For drift (the photographer's taste shifts between motorsport and property), use **recency weighting** on new albums and a replay buffer of older ones, rather than discarding history. **[uncertain]**
- Active learning: uncertainty sampling picks bursts where the head's top-2 margin is small, so the photographer's next cull labels are most informative. Worth it only after the first model is in use. **[uncertain]**
- Shrink towards the previous weights (MAP prior with L2 towards w_prev) to avoid abrupt changes after one small album. **[uncertain]**

## 4. Features that explain choices within a series

Relative, within-burst features matter more than absolute ones. Compute each per frame, then z-score or centre within the burst.

| Feature | How | Expected value | Confidence |
|---|---|---|---|
| Eye openness (continuous) | Landmark eye aspect ratio, or a learned eye-openness regressor, on the detected face | High for portraits and car-meet drivers. Binary closed-eye is common in Pixel-style systems. | Good idea; magnitude unknown |
| Subject sharpness | Laplacian or high-frequency energy in the subject box, not whole frame | High for motorsport/wildlife where background is blurred | Good idea |
| Expression change | Embedding distance to the burst mean of face crops | Moderate | Uncertain |
| Peak action / motion | Subject displacement across neighbours (tracked box or optical flow); pick frames at velocity extrema or peak of trajectory | High for motorsport and wildlife | Uncertain |
| Facing direction | Pose keypoints (nose vs ear); head yaw | Moderate for portraits and wildlife | Uncertain |
| Occlusion | Fraction of subject box covered by other objects, detected by segmentation | Moderate | Uncertain |
| Background clutter | Edge density or CLIP "cluttered background" probe, outside subject box | Low–moderate | Uncertain |

Caveat: these features are what the photographer's eye probably uses, but how much each shifts the choice is unknown until Conrod measures it. Use the learned head to weight them; do not hand-tune weights.

## 5. Concrete recipe for Conrod

### Data construction
1. Group frames by burst (use capture time gap and the existing near-duplicate clustering). Keep bursts with ≥2 frames.
2. Label: delivered = positive; non-delivered burst-mates = negatives. Drop bursts with no delivered frame. For bursts with several delivered frames, treat them as multi-positive (see loss).
3. Store the **shoot ID** (the album/session) per burst. You need it for grouped validation.
4. Expect the label to be noisy: a delivered frame may be picked for framing or for a client-facing reason, not just quality. Note this in the validation discussion; do not clean it without evidence.

### Embedding
- Primary: **SigLIP-2 (ViT-B/16 or B/32)** image embedding, or **DINOv2 ViT-B/14** as a comparison. Both are strong frozen features for aesthetics-like tasks, but the claims that DINOv2 beats CLIP for aesthetics are **[uncertain]**; the 2026 VLM paper found DINOv3 weaker than VLM layers on attribute probing. Run a 2-way bake-off on your data rather than trusting either.
- Baseline: **CLIP ViT-L/14** (the LAION-predictor backbone, 768-d). **[verified backbone]**
- Store the embedding as f16 per frame: ~1.5 KB for 768-d. ~10k frames ≈ 15 MB.
- Compute: ViT-B ≈ one forward pass per frame; roughly 10–30 ms on a consumer GPU and a few hundred ms on CPU **[uncertain, estimate]**. Only embed once per frame and cache by content hash.

### Head
- Preferred: **listwise softmax over each burst** (conditional logit). For each burst with positives P and members B: score s_i = w·(z_i − mean_burst(z)); loss = −log Σ_{i∈P} softmax_B(s)_i. This is the pairwise ranking objective in its listwise form, with one linear weight vector. It matches "pick the best frame in this burst".
- Pointwise baseline: logistic regression on centred embeddings, delivered = 1. Compare; keep whichever wins on the top-1 metric.
- Pairwise (RankNet) alternative: logistic on differences s(delivered) − s(mate). Equivalent in spirit; the listwise form uses all burst members at once.
- Regularisation: L2 with C tuned by nested grouped CV; optionally PCA to 128–256 dims first. Expect a strong regulariser to be needed; with ~2,300 positives and 768-d features, overfitting is the main risk. **[uncertain]**
- Add the handcrafted features above as extra columns (also centred within burst), so the head can learn their weights.
- Start with a linear head. An MLP with one hidden layer of 64 units is worth one test only if the linear head plateaus.

### Validation
- **Grouped CV by shoot** (leave-one-shoot-out): hold out all bursts from one shoot (one motorsport meet, one wildlife trip, etc.), train on the rest, test on the held-out shoot. Report mean and spread across shoots. This is the honest estimate, because the test set is a new scene and new lighting.
- Metrics per held-out burst:
  - **Top-1 hit rate** (best prediction is a delivered frame), vs random baseline mean(|P|/|B|). Report the lift.
  - **Top-3 hit rate**, vs random.
  - **Pairwise accuracy** (delivered beats mate), comparable to Chang et al.'s 73%.
- Report per category (motorsport, car meets, wildlife, property, portraits) because the category mix dominates the pooled number.
- Baselines to beat: random; sharpness-only; eye-open-only; generic (non-personal) CLIP aesthetic predictor; the existing position/size heuristics.
- Target: lift well above the simple signals. The published bar is ~73% pairwise and ~64% top-1 (generic models, different data). Treat anything above the generic predictor's top-1 as the signal to ship. **[uncertain target]**

### Compute and shipping
- Training: logistic/listwise on ~2,300 bursts × 768-d is well under a second on CPU per fit; a full nested-CV sweep is seconds to minutes. Embedding extraction dominates.
- Inference: one embedding per frame + a dot product. Negligible.
- Shipping option A (preferred for a desktop app): **do everything in Rust**. Run the embedding model via ONNX Runtime (`ort` crate) with an exported SigLIP/CLIP/DINOv2 ONNX file; train the linear/listwise head with a small hand-written L-BFGS or Adam (about 100 lines; no heavy ML dependency). Save weights as JSON with model name and embedding hash, so stale weights are detected.
- Shipping option B: a bundled Python step (PyTorch or scikit-learn) run as a subprocess for training only. Simpler to iterate; adds a Python dependency to the installer.
- Recommended: prototype in Python to pick the embedding and head (quick bake-off), then port the chosen head to Rust. Keep the ONNX embedding in Rust for inference.

### Online updating
- On each album export: add its delivered and non-delivered bursts to the store; retrain from scratch on all data with recency weights (for example, half-life of ~1 year of shoots; tune); warm-start from previous weights with an L2 pull towards them.
- Gate display: show the model's top pick only after a minimum number of bursts (e.g. ≥200) have been trained; show the margin as confidence. **[uncertain thresholds]**
- Keep the previous model version; roll back if the next album's top-1 lift drops.

## 6. Open questions to measure on Conrod's own data
1. Random top-1 baseline across all bursts, and the sharpness-only and eye-only top-1. This sets the bar.
2. Whether DINOv2 or SigLIP-2 beats CLIP ViT-L/14 on this photographer's data.
3. Learning curve: top-1 vs number of training shoots, to find where it saturates.
4. Whether per-category heads beat one head with category features.
5. How often the delivered frame is not the best-looking one (framing or client reasons). If high, no model reaches high top-1.

## Sources
- Chang et al., Automatic Triage for a Photo Series, SIGGRAPH 2016 / ACM TOG: https://dl.acm.org/doi/10.1145/2897824.2925908 ; preprint https://pixl.cs.princeton.edu/pubs/Chang_2016_ATF/chang2016-triage-preprint.pdf ; project page https://www.juew.org/publication/sig16-triage.pdf
- Series Photo Selection via Multi-view Graph Learning: https://arxiv.org/html/2203.09736v1
- Learning Multi-Scale Attentive Features for Series Photo Selection: https://ieeexplore.ieee.org/document/9053731
- Recursive Multi-Relational GCN for Automatic Photo Selection: https://ieeexplore.ieee.org/document/9756859
- Real-time Burst Photo Selection Using a Light-Head Adversarial Network, arXiv 1803.07212: https://arxiv.org/html/1803.07212v1
- HDR+ burst dataset: https://hdrplusdata.org/dataset.html
- Burst Image Quality Assessment benchmark (BuIQA), arXiv 2511.07958: https://arxiv.org/html/2511.07958
- Google Pixel Top Shot: https://support.google.com/pixelcamera/answer/9937175?hl=en ; https://9to5google.com/2019/09/02/pixel-3-smart-burst
- Apple Live Photo key photo: https://support.apple.com/en-us/104966
- PARA, Yang et al., CVPR 2022, arXiv 2203.16754: https://arxiv.org/abs/2203.16754
- LAPIS, Maerten et al., CVPR 2025 (cited in 2604.11374; no separate fetch)
- Ryu & Yanaka, What Do VLMs Encode for Personalized IAA?, arXiv 2604.11374: https://arxiv.org/html/2604.11374
- Chen et al., On the Role of Individual Differences in Computational Image Aesthetics, arXiv 2502.20518 / BMVC 2025: https://arxiv.org/abs/2502.20518 ; code https://github.com/lwchen6309/aesthetics_transfer_learning
- Hentschel, Kobs & Hotho, CLIP knows image aesthetics, Frontiers in AI 2022: https://frontiersin.org/journals/artificial-intelligence/articles/10.3389/frai.2022.976235/full
- LAION aesthetic predictor (linear head on CLIP ViT-L/14): https://github.com/LAION-AI/aesthetic-predictor
- Zhu et al., PIAA via meta-learning with bilevel gradient optimization, IEEE TCYB 2020: https://ieeexplore.ieee.org/document/9115059
- Transductive Aesthetic Preference Propagation for PIAA, ACM MM 2022: https://doi.org/10.1145/3503161.3548244
- Community CLIP+ridge personal-rating projects (anecdotal, no metrics): https://github.com/qweliant/fineshyt

Not fetched this session (budget): the Chang et al. dataset size and baseline table, and the DINOv2/SigLIP-2 aesthetics comparisons. Both need a direct read before publishing numbers.
