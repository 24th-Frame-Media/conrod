# Keeper models and motorsport attributes from a single photo (Nov 2026)

Scope: (1) can any 2025-26 model beat the current 0.249 within-burst top-1; (2) how to say
"airborne", "leaning hard right", "front wheel lifting" about one frame. Companion to
`aesthetic-models.md` and `models-onnx-deploy.md`; deeper on series ranking and vehicle geometry.

**[U]** = unverified/estimated (mostly my estimate for an RTX 3070 Ti Laptop, 8 GB). **NC** =
non-commercial. "Pair" = pairwise accuracy, "s-Acc" = pick-the-best-of-series accuracy.

## 0. Bottom line

1. **The 0.249 is probably chance.** With ~4 frames per burst, guessing scores 0.25; sharpness at
   0.235 is also chance. Compute the permutation baseline first. The honest status is "the taste
   head does not rank within a burst", and any model reaching 0.35-0.45 top-1 is a large gain.
2. **The ceiling is low but not zero.** Best burst-aware model on the standard Photo Triage
   benchmark: 0.706 pairwise (BuIQA, Nov 2025) vs 0.631 for a plain VGG. Human pairwise agreement on
   that data is itself only in the low 0.7s [U]. Fine-grained series ranking (FGAesQ, CVPR 2026)
   reaches 0.779 pair / 0.753 series-best on natural bursts with an **86 M** model that beats 7-8 B
   MLLMs (Q-Align 0.711/0.676, UNIAA 0.709/0.656). Small, series-trained models win at exactly our
   task.
3. **Nothing off the shelf is a keeper detector for this photographer.** The deliverable is a better
   *within-burst listwise ranker* trained on the 2,300 delivered frames with same-burst negatives,
   plus an honest "suggested keeper" UI. Expected top-1: **0.38-0.50** [U].
4. **Motorsport attributes have no public model.** No downloadable wheel-lift or brake-dive detector
   exists. Path: keypoints + PnP (SPARK recipe, Apache-2.0), or first pure geometry. Reliability
   varies hugely per attribute and must be measured on ~100 labelled frames.

## 1. How to read the current 0.249

- Mean burst length in the eval set **is** the chance line (1/L). Print it beside every top-1.
- Group-split by shoot, not by frame; one burst must never straddle train/test.
- Bootstrap over bursts (1,000 reps, cluster by burst). A 0.014 gap is noise.
- Report both within-burst top-1 and is-delivered AUC ("delivered" allows several per burst).

The bake-off target then becomes "beat 0.35 with a CI excluding 0.30".

## 2. Keeper candidates (2025-26)

| Model (year) | Predicts | Training data | Reported accuracy | Size / cost | Weights, ONNX | Licence | Verdict |
|---|---|---|---|---|---|---|---|
| **FGAesQ** (CVPR 2026 oral) | Aesthetic score + **series ranking** | FGAesthetics: 32,217 imgs / 10,028 series (SPS bursts, LSVQ frames, AIGC, crops), 10 pairwise annotators; AVA for coarse | Natural: pair 0.779, s-Acc 0.753, s-SRCC 0.729; AVA SRCC 0.770 | ViT-B/16 86.3 M, ~5-8 ms [U] | PyTorch + HF weights, no ONNX seen | Repo has **no licence file** (Nov 2026) | **Copy the design**, do not ship the weights |
| Charm (CVPR 2025) | Aesthetics (ViT recipe) | AVA | AVA SRCC 0.777; FGAesthetics 0.672 / 0.602 | 85.7 M | code + weights [U] | check [U] | Weaker on series |
| Q-Align / OneAlign (ICML 2024) | Quality + aesthetics as text levels | KonIQ, SPAQ, KADID, AVA, LSVQ | AVA PLCC/SRCC 0.819/0.822; 0.711/0.676 on FGAesthetics | 7.8 B; 4-bit ~5-6 GB, 1-2 s/img [U] | HF, no ONNX | S-Lab + LLaMA-2 lineage -> **NC/unclear** | No: too slow per cull, worse fine-grained |
| Q-Insight / UNIAA (2024-25) | Quality + degradation via RL / aesthetics from levels | mixed IQA, AVA | UNIAA 0.709 / 0.656; Q-Insight beats prior SOTA on score + degradation (paper) | 7-8 B | code | research/NC | No |
| UniPercept (ICML 2026) | Unified aesthetics/quality/structure-texture profile, with text | UniPercept-Bench (2025-26) | Claims SOTA perceptual MLLM; 16xA100 training | 7-8 B [U] | Code + HF ckpt, **no licence file** | unclear/NC [U] | Useful as an offline *labeller*, not runtime |
| Aes-R1 (2025), VisualQuality-R1 (NeurIPS 2025) | Aesthetic/quality via RL-to-rank | 15 K from AVA+TAD66K+Flickr-AES | Aes-R1 avg PLCC 0.634 / SRCC 0.619 over 5 sets, **below** Q-Align on AVA | 7-8 B | Code | research | RL on small sets does not fix fine-grained ranking |
| BuIQA (Nov 2025) | **Per-frame quality inside a burst**, task-conditioned | 7,346 bursts / 45,827 imgs / 191 K scores (Photo Triage, SPAQ, HDR+, BurstSR) | Photo Triage pair **0.706 ± 0.121** (VGG 0.631, best prior IQA 0.680) | Swin + adapters ~100 M [U], ~10 ms | Code [U] | research [U] | **Architecture to copy** (margin loss + grouping ranks) |
| Co-Instruct (ECCV 2024 oral) | Open-ended 2-image comparison with reasons | Co-Instruct-562K (LLM-merged + GPT-4V teacher) | +30% avg over open LMMs, beats GPT-4V on MICBench | 7 B class | HF weights, NOASSERTION | unclear | Pairwise judge on top 2-3 of a burst only |
| Photo Triage / SPS (Chang 2016) | Pairwise "which is better in this series" | SPS bursts, pairwise | The reference task + Bradley-Terry ranking | tiny Siamese | data partly public | academic | Baseline + sanity-check dataset |
| TOPIQ / MUSIQ / LIQE / CLIP-IQA+ / NIMA | Technical + aesthetic scores | KonIQ/SPAQ/AVA | MUSIQ AVA SRCC 0.726; NIMA 0.633 | 50-80 M | via `pyiqa` | **pyiqa PolyForm NC**, weights CC BY-NC-SA | No licence-free win; MUSIQ is least-bad absolute scorer |
| LAION / improved-aesthetic (CLIP-L/14 + linear) | Absolute "how much people like it" | SAC, LAION-Logos, AVA | AVA ~0.666 | 300 M + 0.9 M head, 10-20 ms [U] | ONNX exists, head trivial | MIT/Apache-2.0; AVA provenance caveat | Already a feature in the app. Keep it as a feature |

### 2.1 Bake-off candidates and expected gain

| Candidate | What it adds | Expected top-1 (bursts >= 3) | Effort | Could fail because |
|---|---|---|---|---|
| **A. Listwise head v2**: frozen DINOv2-S + scalars (sharpness, faces, subject placement), ListMLE over each burst, 5-fold by shoot | A within-burst loss instead of an absolute score | 0.38-0.48 [U] | 2-3 days | Delivered labels may be near-random within burst (e.g. "all sharp ones delivered") -> ceiling ~0.35 |
| **B. FGAesQ-style series encoder** (CLIP ViT-B/16 + DiffToken + rank loss), pretrained on public series data then on ours | Learned fine-grained discrimination, not just a head | +0.03-0.08 over A [U] | 1-2 weeks + GPU | Licence of released weights; we have ~600-900 bursts [U] |
| **C. BuIQA-style margin loss + grouping ranks** | Tolerates ties, directly targets "which is better" | +0.02-0.05 over plain listwise [U] | 3-4 days | Small data; ties dominate |
| **D. Local VLM pairwise judge** on top 3 only (qwen3-vl:8b installed) | Reasoning + text ("eyes closed", "car cut off") | +0.02-0.05 [U], 1-2 s per comparison | 3 days | Already failed its gate; 8 B on near-identical pairs is weak |

Do not retrain an absolute aesthetic scorer on 2,300 frames. Absolute scores give near-identical
values to near-duplicates by construction - the point FGAesQ and BuIQA both make. The signal is in
the comparison, not the score.

### 2.2 Presentation

| Element | Design | Reason |
|---|---|---|
| Green highlight | 2 px green border + check badge on the burst's top frame only | One mark per burst, not a field of green |
| Confidence | "Best of 6" plus a calibrated 0-100; green only above threshold | Relative is what the decision needs; absolute scores drift per shoot |
| Word | One reason: "sharpest + eyes open", "best of burst" | Explainable = trustable |
| Accept | Space or click badge = accept and advance; Shift+Space takes top 2 | One keystroke per burst; never auto-delete or silently reject |
| Uncertain | Amber ring on the runner-up when the gap is < 0.1 | Honest use of low confidence |
| Never | Green on an already-rejected frame; reordering the grid | Breaks muscle memory and hides mistakes |

## 3. Motorsport attributes from one frame

Available per frame: YOLO11s car box (COCO), sharpness/edge-blur + panning flag, faces, burst
neighbours, EXIF focal length/shutter. Enough for a good V1.

### 3.1 Attribute by attribute

| Attribute | V1 signal (no training) | V2 learned | Reliability | Cost |
|---|---|---|---|---|
| **Airborne** | Tyre bottom vs ground plane: fit the line from the two contact patches + Hough segments; fire when the lowest tyre pixel is > 0.35 x tyre radius above it on 3+ wheels, confirmed by no contact shadow | Tyre keypoints -> 3D ride height via PnP; airborne = ride height > tyre radius | V1 good on clean side/3-4 views; fails on crests, hidden ground, gravel [U] | V1 2-4 ms; V2 10-15 ms |
| **Wheel lift** | Same geometry, one corner; needs both visible contact points + a roll estimate | Per-wheel 3D height vs the car's own plane | Poor in 3/4 views (far wheel hidden), good side-on | same |
| **Body roll** | Angle between the wheel-centre line (front to rear axle) and the track horizon; sign = direction. Image tilt = roll only with a level camera, so correct with EXIF roll + horizon | 6-DoF pose; roll = car roll minus track plane roll | Medium: strong on side-on pans, poor head-on | cheap |
| **Brake dive** | Pitch from roofline vs axle line, nose down; dive peaks 1-3 frames before apex | PnP pitch rate | Medium; needs a roofline, rear 3/4 best | cheap |
| **Suspension compression** | Wheel-arch gap: tyre top to arch lip, normalised by tyre radius; compare across burst and corners | Segmented wheel + arch, or arch keypoints -> 3D gap | Medium-good with a segmenter, bad from a box | seg ~8 ms |
| **Contact / impact** | Sparks: small saturated, bright blobs within ~2 tyre radii of the ground plus radial blur, persisted 2 frames; contact = two boxes < 0.2 x width apart | Spark crop classifier | Sparks: good recall, moderate precision (sun glints, kerb paint) [U]. Contact: weak | ~1 ms |
| **Pan quality** | Existing panning flag + streak direction (Radon/spectrum of background) vs subject edge sharpness | none | Good; we lack a score, not a detector | ~1 ms |
| **Motion direction** | Streak angle sign + leading edge of the subject | RAFT flow between burst frames | Medium; wheel rotation at 1/1000 s is hard | 30-60 ms/pair [U] |
| **Driver visible** | MediaPipe face in the cockpit ROI; helmet = no skin tone + round blob | YOLO helmet class (none public for cars [U]) | Good for GT/road cars, useless for F1 helmets | ~2 ms |

### 3.2 Code and weights that exist

| Component | Gives | Licence | Notes |
|---|---|---|---|
| **SPARK** (TUM 2026, `TUMFTM/SPARK-camera-det`) | YOLO11-pose with 9 car keypoints (4 tyres, wing edges, roof camera) + SQPnP -> 6-DoF pose; 4.9 ms at v11m-960; AOE 0.012, better orientation than LiDAR | **Apache-2.0** | One Dallara EAV24 on one circuit: the **recipe** transfers, not the weights. Fine-tune needs ~300-600 of our frames (theirs: 976 train / 245 test) |
| OpenPifPaf ApolloCar3D plugin | 24/66 car keypoints, AP 76.1% (shufflenetv2k16) | AGPL-3.0 [U] | Road cars; wheels often occluded in racing shots. Copyleft is a problem for a bundled app |
| Ultralytics **carparts-seg** (23 classes incl. wheel) | Wheel vs body masks -> arch gap, wheel centre | AGPL-3.0 (Ultralytics) [U for dataset terms] | 3,156 road-car images; aero + slicks is out of domain, expect to fine-tune |
| Depth Anything V2 | Relative depth, ground-plane sanity | Small 24.8 M **Apache-2.0**; Base/Large **CC-BY-NC-4.0** | Metric outdoor ckpts capped at 80 m [U]; only Small is shippable |
| UniK3D (CVPR 2025) | Universal monocular 3D, handles long lenses/fisheye, ViT-S/B/L | check repo [U] | Better fit than DA-V2 for long-lens track geometry |
| RAFT / SEA-RAFT | Optical flow between burst frames | BSD-3 / MIT [U] | Only for motion direction and wheel-rotation checks |

Skip Mono3DT/Keypoint3D/RTM3D: SPARK beats them on racing data and is simpler.

### 3.3 V1 (deterministic, ~1 day)

1. **Horizon**: RANSAC over long Hough segments in the lower half (track edge, kerb, armco); fall
   back to the two tyre contact points + EXIF roll. On failure, suppress all attitude words.
2. **Wheels**: circular Hough (dark, round, low-texture) in the lower third of the box, or
   carparts-seg `wheel` masks if we accept AGPL. Output 2-4 centres + radii.
3. **Attitude**: roll = angle(axle centre line, horizon); pitch = angle(roofline, axle line);
   ride height = tyre bottom - ground plane; arch gap = tyre top to the nearest bright body edge.
4. **Burst sanity**: attributes must be continuous at 20 fps. A roll sign flip between neighbours =
   suppress, do not guess. **Sparks**: HSV blob filter near the ground line + 2-frame persistence.

Abstain by default. "Airborne" needs geometry, shadow and burst to agree.

### 3.4 V2 (learned, 2-3 weeks)

1. Label 400-600 frames with 9-13 keypoints (4 tyre centres, 4 arch tops, wing/bumper corners,
   roof/cockpit); 1-2 frames per burst, all bursts.
2. Fine-tune YOLO11-pose (m, 960 px) - same architecture family we already ship.
3. SQPnP against a per-car-type rough 3D keypoint set from box dimensions; a handful of car types per
   event makes a small preset list enough.
4. Attributes become geometry: roll/pitch/yaw, per-corner ride height, arch gap in mm using box
   height as scale - this is what allows "front-left lifting 40 mm".
5. Keep V1 as fallback when PnP has < 4 visible keypoints.

### 3.5 Words to show

| Condition | Text |
|---|---|
| All wheels clear | "Airborne" (tooltip: all four wheels off the ground) |
| One corner clear | "Front-left lifting" / "Rear-right lifting" |
| Roll > 2 deg | "Leaning hard right" (or left); below 2 deg say nothing |
| Nose-down pitch in a braking zone | "Braking - nose down" |
| Arch gap < 60% of rest | "Front suspension compressed" |
| Sparks near a wheel | "Contact - sparks at right rear" |
| Pan shot, subject sharp, streak smooth | "Clean pan" (never invent a shutter speed) |
| Uncertain | say nothing. Never "possibly airborne" |

## 4. Ranked shortlist

| # | Item | Expected value | Effort | Runtime | Licence |
|---|---|---|---|---|---|
| 1 | **Listwise head v2** on existing DINOv2-S features + scalars, burst-grouped CV, calibrated | High: 0.249 chance -> 0.38-0.48, no new runtime model | 2-3 days | +1 ms | ours |
| 2 | **BuIQA/FGAesQ-style loss** (margin + grouping ranks, ties) on the same features | +0.02-0.05 over #1 | 3-5 days | none | ours |
| 3 | **Classical motorsport V1** (horizon, Hough wheels, attitude, arch gap, sparks) | High: real words, zero ML risk | 1-2 days | 3-5 ms | ours |
| 4 | **SPARK-style keypoints + PnP V2** | High: only route to "front wheel lifting" with numbers | 2-3 weeks + 500 labels | 10-15 ms | Apache-2.0 code, our weights |
| 5 | **carparts-seg wheel masks** to stabilise #3 | Medium | 1 day | 8 ms | AGPL-3.0 (check) |
| 6 | **FGAesQ encoder retrained on our bursts** | +0.03-0.08 if #1 plateaus | 1-2 weeks + GPU | 5-8 ms | ours |

Excluded: Q-Align/UNIAA/UniPercept/Q-Insight (NC, 1-2 s/frame, worse fine-grained); VLM judge
(already failed its gate, revisit only if #1 stalls); Depth Anything V2 Base/Large (NC).

## 5. Validation plan

**Keeper (2,300 delivered frames)**

1. Rebuild the split **by shoot/date**, freeze it; hold out whole shoots.
2. Four numbers per model: within-burst top-1, pairwise accuracy, is-delivered AUC, precision@1 of
   the green highlight. Publish the chance line and a **cluster bootstrap CI over bursts** (1,000
   reps); ship only if the lower bound clears chance + 0.08.
3. Learning curve at 100 / 300 / 1,000 / 2,300 positives. Ablations: DINOv2-S vs CLIP-B vs SigLIP-B;
   with/without sharpness, faces, motorsport attributes; listwise vs pairwise vs pointwise. Keep the
   smallest model that holds.
4. Leakage check: DINOv2 cosine between train and test bursts; drop test bursts with a > 0.95
   neighbour. Then two shoots in shadow mode before enabling by default.

**Motorsport (~100 frames)**

1. Four or more events, panning and head-on, three or more car types; ~12 frames per attribute with
   deliberately ambiguous negatives. Two annotators + adjudication; report Cohen's kappa per
   attribute (below ~0.7 it is an opinion, not a label).
2. Per attribute precision/recall at the shipped threshold plus AUC. **Require precision >= 0.85
   before a word is shown** - a false "airborne" costs more trust than ten misses.
3. Temporal consistency: fraction of bursts where the attribute sign flips between adjacent frames
   (should be ~0 for roll; nonzero only at the airborne transition).
4. Hold out one whole event; a > 0.1 AUC drop means it learned the track, not the car.
5. Only then wire the words into the grid beside the keeper badge.

## Sources

- BuIQA, *Burst Image Quality Assessment* (arXiv 2511.07958): https://arxiv.org/html/2511.07958v1
- FGAesQ / FGAesthetics (arXiv 2603.03907, CVPR 2026 oral): https://arxiv.org/html/2603.03907v1 · https://github.com/yzc-ippl/FG-IAA
- Chang et al., *Automatic Triage for a Photo Series*: https://pixl.cs.princeton.edu/pubs/Chang_2016_ATF/chang2016-triage-preprint.pdf
- Ren et al., *Best Frame Selection in a Short Video* (WACV 2020): https://openaccess.thecvf.com/content_WACV_2020/papers/Ren_Best_Frame_Selection_in_a_Short_Video_WACV_2020_paper.pdf
- Q-Align: https://q-align.github.io · Q-Insight: https://neurips.cc/virtual/2025/poster/119363 · UniPercept: https://github.com/thunderbolt215/UniPercept
- Aes-R1: https://arxiv.org/html/2509.21871v1 · Co-Instruct: https://arxiv.org/abs/2402.16641 · LAION aesthetics: https://laion.ai/blog/laion-aesthetics
- improved-aesthetic-predictor: https://huggingface.co/camenduru/improved-aesthetic-predictor · pyiqa licence: see `aesthetic-models.md` §0
- SPARK (arXiv 2606.17936): https://arxiv.org/html/2606.17936v1 · https://github.com/TUMFTM/SPARK-camera-det
- OpenPifPaf car keypoints: https://openpifpaf.github.io/plugins_apollocar3d.html · ApolloCar3D: https://openaccess.thecvf.com/content_CVPR_2019/papers/Song_ApolloCar3D_A_Large_3D_Car_Instance_Understanding_Benchmark_for_Autonomous_CVPR_2019_paper.pdf
- carparts-seg: https://docs.ultralytics.com/datasets/segment/carparts-seg · DSMLR: https://github.com/dsmlr/Car-Parts-Segmentation
- Depth Anything V2: https://github.com/DepthAnything/Depth-Anything-V2 · GGUF: https://huggingface.co/Acly/Depth-Anything-V2-GGUF · UniK3D: https://github.com/lpiccinelli-eth/UniK3D
- Wheel-rim double ellipses (Sensors 2026): https://www.mdpi.com/1424-8220/26/18/5880 · YAEN: https://pdfs.semanticscholar.org/ae25/3a8bc86def34d70085b15edb72a6d75146e2.pdf
- Sports frame-selection thesis: https://digitalcommons.lib.uconn.edu/srhonors_theses/1165 · action-spotting survey: https://arxiv.org/html/2505.03991v2
- Commercial culling context: https://imagic.ink/blog/ai-photo-culling-software-comparison-2026
