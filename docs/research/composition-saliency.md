# Composition scoring and saliency fallback: research notes for Conrod

Scope: evidence on which composition rules track human judgement, usable learned models with licences, saliency models for locating the subject, concrete heuristic formulas, and a validation plan against the ~2,300 delivered-vs-not labels. Web research only; nothing was run locally. Items marked **[unverified]** are my own choices, not sourced values.

## 1. What the evidence says about the rules

**Summary:** rule-of-thirds (RoT) adherence is a weak, inconsistent predictor of aesthetic ratings. Horizon tilt and the golden-ratio *area split* have better support in the classic feature studies. Centred subjects are often preferred. Treat every rule as a bounded feature, not a score to maximise.

- **Rule of thirds is weak in large photo sets.** Denzler & Redies (2014) compared saliency-based RoT measures with 30 observers. Aesthetic ratings correlated only weakly with subjective RoT scores and not at all with calculated RoT values. Highly rated photos and a large painting set had calculated RoT values about as low as non-RoT photos. Their conclusion: RoT "seems to play only a minor, if any, role" in high-quality sets. https://library.imaging.org/ei/articles/36/11/HVEI-229 ; https://brill.com/view/journals/artp/2/1-2/article-p163_11.xml
- **Centred single subjects are preferred.** Hoh, Zhang & Dodgson (SIGGRAPH Asia 2023 poster; 117 participants, 2,808 judgements) chose the centred version 64.1% of the time (CI 56–71%) over RoT placement, at every object size tested (0.1–0.8 of frame). Authors call it "salient centeredness". https://fanglue.github.io/papers/AsiaPoster.pdf ; https://dl.acm.org/doi/10.1145/3610542.3626121
- **Rule breaks have patterns.** A statistical analysis of rule-breaking photos found centred subjects are justified by symmetry, circular shapes, reflections, framing and leading lines; a centred horizon is justified by reflections and busy foregrounds. https://pmc.ncbi.nlm.nih.gov/articles/PMC9249203
- **Golden ratio and tilted horizon beat RoT in feature studies.** Synthetic-photo study (AAAI workshop 2013) found tilted horizon and object count among the most weighted features in the individual models. Golden ratio alone was the single most accurate predictor (70%). RoT was "never the most weighted feature". https://cdn.aaai.org/ocs/ws/ws1060/7133-30745-1-PB.pdf . Bhattacharya et al. (2010, Flickr) reported 86% rating accuracy with three features: RoT, golden ratio and tilted horizon (cited via the same paper; original not fetched).
- **Expertise matters.** Eye-tracking (30 participants, 15 experts): experts selected RoT images more often; novices did not distinguish them. https://link.springer.com/chapter/10.1007/978-3-030-80091-8_89 ; https://soad.kmutt.ac.th/wp-content/uploads/2021/06/2021INTC3.pdf
- **Sophisticated observers do weight RoT.** A 2024 HVEI study found the RoT metric correlated more strongly with an arts-panel ranking than an information-theoretic aesthetic measure. https://library.imaging.org/ei/articles/36/11/HVEI-229
- **Top-down small feature sets can beat large ones.** Khan & Vogel (2012, portraits) used 7 composition-driven features and were up to 6% more accurate than a 66-feature bottom-up set. https://individual.utoronto.ca/shehroz/files/cae12-ssk-dv.pdf (URL as found: http://individual.utoronto.ca/shehroz/files/cae12-ssk-dv.pdf)
- **Composition is partly theme-dependent.** TAD66K/TANet work shows ratings depend on theme criteria, so a single global composition threshold will fail across genres. https://ijcai.org/proceedings/2022/132
- **Cropping can break composition.** Mai et al. (CVPR 2016) show that cropping a well-composed RoT image damages it; scaling distorts subjects. Relevant to any cut-off penalty. https://openaccess.thecvf.com/content_cvpr_2016/papers/Mai_Composition-Preserving_Deep_Photo_CVPR_2016_paper.pdf
- **Oil-painting statistics:** RoT correlated weakly and negatively with aesthetic and beauty ratings (r ≈ −0.04 to −0.13). Weak evidence, but it supports "RoT is not a universal good". https://pmc.ncbi.nlm.nih.gov/articles/PMC5496686/

**Practical reading:** RoT and golden ratio are cheap, interpretable features; horizon level and balance have better support; centring must be an allowed mode, not a penalty.

## 2. Learned composition models (code, weights, licences)

| Model | What it outputs | Code / weights | Licence | Notes for Conrod |
|---|---|---|---|---|
| **SAMP-Net + CADB** (Zhang, Niu, Zhang, BMVC 2021) | Composition score distribution (1–5) plus 8 composition-pattern weights (dominant pattern = explanation) | https://github.com/bcmi/Image-Composition-Assessment-Dataset-CADB ; weights ~180 MB via Dropbox/Baidu | Repo MIT. Dataset built on AADB (research-use origin); dataset terms **[unverified]** | Best fit for a *score*. ResNet18, 224×224, PyTorch; ONNX export feasible. 9,497 images, 5 raters each. Failure: distracting backgrounds, complex patterns (paper §5.6). |
| **CACNet** (Hong et al., CVPR 2021) | Best crop box plus composition class | Unofficial MIT PyTorch: https://github.com/bo-zhang-cs/CACNet-Pytorch (~75 MB weights, FLMS-trained) | MIT (unofficial port); official licence not checked | VGG16. Crop-proposal, not a score. Useful as "how far is the current frame from the model's preferred crop" = a composition-distance feature. |
| **GAIC / GAIC-Pytorch** (Zeng et al., CVPR 2019) | Crop proposals, aspect-ratio-specific | https://github.com/bo-zhang-cs/GAIC-Pytorch (~98 MB, MIT) ; original https://github.com/HuiZeng/Grid-Anchor-based-Image-Cropping | MIT (port) | 125 FPS reported; VGG16, ~40 Mbit. Same use as CACNet. Good for batch. |
| **S2CNet** (Su et al., AAAI 2024) | Crops with object-aware reasoning | https://github.com/suyukun666/S2CNet | Not shown in fetched content **[unverified]** | Needs custom CUDA ops (roi_align) built from source. Poor fit for ONNX Runtime. |
| **TANet** (He et al., IJCAI 2022) | Aesthetic score per theme (TAD66K) | https://github.com/mRobotit/TANet (Apache-2.0); official repo https://github.com/woshidandan/TANet-image-aesthetics-and-quality-assessment | Apache-2.0 (code); weights and TAD66K terms **[unverified]** | Aesthetic, not composition. Possible prior for "keeper-worthy" but needs Places-trained ResNet18 too. |
| **UNIAA** (Zhou et al., 2024) | MLLM aesthetic description and scores | https://github.com/KwaiVGI/Uniaa | Not checked | Built on LLaVA-1.5-7B. Too heavy for a local culling app. Skip. |

**Recommendation:** start with CADB/SAMP-Net as the single learned composition score (ONNX, MIT code). Use the GAIC crop distance as a second feature only if throughput allows. Do not use S2CNet or UNIAA. Note that CADB and CACNet/GAIC were trained on stock or Flickr-style photos, not motorsport, so calibrate on Conrod labels before trusting them.

## 3. Saliency models for locating the subject

Use these only when the detector finds nothing or when the subject is ambiguous (for example a car meet with a crowd). The detector is primary.

| Model | Size | Input | Licence (code / weights) | Speed notes | Verdict |
|---|---|---|---|---|---|
| **U²-Netp** | ~4.6 MB ONNX | 320×320 | Apache-2.0 (code); ONNX from rembg release v0.0.0 | Fast CPU | First choice for CPU fallback. https://github.com/patelnet/SnapBGR/blob/main/models/README.md |
| **U²-Net** | ~176 MB | 320×320 | Apache-2.0 | Moderate | Better edges than Netp. |
| **ISNet general-use** (DIS, ECCV 2022) | ~176 MB ONNX | 1024×1024 | Code Apache-2.0 (https://github.com/xuebinqin/DIS). Weights published for general use per the repo README; no separate licence sentence on the weights (DIS issue 150 open) | ~20 FPS in one comparison (GPU unspecified) | Good balance. Caveat: the DIS release was trained on DIS V1, which has few cars and animals (repo note). Test on motorsport frames first. |
| **BiRefNet** (MIT, CAAI AIR 2024) | General ~0.2B params; Swin-B ~844 MB | 1024×1024 | Code and weights MIT (https://github.com/ZhengPeng7/BiRefNet) | ~0.3 FPS on a comparison table; ONNX ~90% slower than PyTorch on A100 per authors | Offline/batch only. The "lite" tiny-backbone variant is the only realistic option for a per-frame culling pass. |
| **BRIA RMBG-1.4/2.0** | ~176 MB–1 GB | 1024×1024 | **Non-commercial** unless licensed (BRIA) | — | Avoid if Conrod is or may become commercial. |
| **TRACER, MSI-Net** | — | — | Not researched **[unverified]** | — | Not evaluated; prefer the above. |

Saliency caveats: these models find "salient objects", so in a car meet a person or a sign can win over the car. Use the detected box to gate: accept saliency only if its peak is inside or overlapping a detected vehicle box, otherwise mark composition as "no subject".

## 4. Heuristic formulas

Normalise coordinates to [0,1] with origin top-left. Subject centroid (cx, cy) comes from the detection box centre or the saliency centroid. Frame W×H.

**4.1 Thirds and golden-ratio distance (use the paper's normalisation)**

Mai et al. and Liu et al. use per-axis normalisation, not the diagonal: `d = min_i sqrt( ((cx−Px_i)/W)² + ((cy−Py_i)/H)² )` over the four points P_i. Their feature range is [0, 0.47], and they used 0.25 as the threshold for "following the rule" (average from good professional photos). Source: https://ijcai.org/proceedings/2011/0344.pdf (Liu/Puri et al. Netflix CVPRW 2019 uses the same style: https://openaccess.thecvf.com/content_CVPRW_2019/papers/FFSS-USAD/Liu_Modeling_Image_Composition_for_Visual_Aesthetic_Assessment_CVPRW_2019_paper.pdf).

Score: `s_thirds = clamp(1 − d / 0.47, 0, 1)`; treat `d < 0.25` as "on the grid".

If you want a diagonal-normalised version (the user request), use `d_diag = min_i sqrt(((cx−Px_i)² + (cy−Py_i)²)) / sqrt(W² + H²)`. This is **[unverified]** as a better measure; it is only a different scale.

**Golden-ratio / phi grid:** points at x, y ∈ {0.382, 0.618}. Same distance as above with the four phi points. Mai et al.'s "golden ratio" feature is a different quantity: horizon split ratio (below). Keep the two separate.

**4.2 Horizon level (golden-ratio split and tilt)**

- Tilt: detect the dominant near-horizontal long line (Hough / LSD); `θ` in degrees from horizontal. Bhattacharya-style: tilted horizon is among the strongest features. **Thresholds [unverified]:** `|θ| < 1°` good, `> 3°` penalise. Exclude frames without a long line; do not guess. Gate with "is this a horizon?" (sea, track straight, horizon with a visible sky/ground split).
- Split: horizon height `h` (fraction of H); ratio `r = max(h, 1−h) / min(h, 1−h)`; feature `|r − 1.618|` (Mai et al.: f_gr). Their threshold was 0.30.

**4.3 Visual balance (saliency mass)**

Saliency map S(x,y) ≥ 0, normalised to sum 1.
- Horizontal balance: `b_x = (Σ S·(x−0.5)) / Σ S` (range roughly ±0.5). Penalise `|b_x| > 0.25` unless the subject is centred.
- Vertical: same with y, with an inverted sign (top-heavy penalty `|b_y| > 0.25`).
- Diagonal balance (Bhattacharya-style line-of-sight): use `x−y` and `x+y` centroids. **[unverified]** threshold.

**4.4 Lead room (direction-aware)**

Define motion direction `d ∈ {−1, +1}` (+1 = moving or facing right).
- `ahead = (W − x1) / W` if d = +1, else `x0 / W` (box edges x0, x1 in [0,1]).
- `behind = x0 / W` if d = +1, else `(W − x1) / W`.
- Lead ratio `L = ahead / (ahead + behind + ε)`.
- Target: L ≈ 0.67 (about twice as much room ahead, a common guideline; source: https://photographyicon.com/rule-of-space/ and https://photographyicon.com/headroom). Score `s_lead = clamp(1 − |L − 0.67| / 0.33, 0, 1)`. Add a hard penalty `ahead < 0.08` (nose or wheel pinned to the leading edge). Faster subjects and head-on angles change the target; for head-on cars, lead room is not used (use centring).

Where does `d` come from (the hard part, **no source**):
1. Burst or sequence (preferred): sign of the subject centroid displacement across neighbouring frames of the same burst, after subtracting camera pan. For a pan shot, subject box stays near the centre while background flow streaks; use background flow sign opposite the subject's apparent direction.
2. Single frame: vehicle facing from the detector class or a small orientation classifier. **[unverified]** This is not available with the current detector, so do not promise it.
3. Fallback: score `s_lead` as unknown (weight 0), not as failing.

**4.5 Headroom (people and animals only)**

`head = y0 / H` (top of box, or top of the detected head or eyes). Target: eyes near 1/3 from top (UNC and video guides: https://guides.lib.unc.edu/media-design-center/video-concepts). Use only when a face or head keypoint exists; do not apply to cars.

**4.6 Cut-off at frame edges**

`clip = area(box ∩ frame) / area(box)` where the box is the unclipped detection. Penalty `1 − clip` for the subject class. Separately, `edge_gap = min(x0, 1−x1, y0, 1−y1)`; `edge_gap < 0.02` on a motorsport car nose or tail is a common reject **[unverified]**. Wheels cut at the bottom edge are common and acceptable on a panning shot, so treat wheels as optional.

**4.7 Subject size and prominence**

`A = (box area) / (frame area)`. Use as a band, not a maximum: below ~0.02 small subject, above ~0.6 fill-the-frame (a valid CADB class). Band targets are **[unverified]**; calibrate per genre (for example wildlife tolerates small; motorsport likes 0.1–0.4).

**4.8 Centring and symmetry**

`c = |cx − 0.5| / 0.5`. Do not penalise centring. For single subjects, Hoh et al. support centring as a valid mode. Use the best of `{centre, thirds, phi}` as the composition score, so that each frame is judged against the mode it fits best.

**4.9 Aggregate**

`S_comp = max(s_centre, s_thirds, s_phi) · w_mode + w_lead · s_lead + w_hz · s_horizon − penalties(cut-off, size band, balance)`

Start with equal weights; tune on labels (section 6). Keep the score as a tie-breaker inside a burst, gated by sharpness and eyes-open. Composition on its own is a weak signal (Denzler & Redies, Hoh et al.).

## 5. Known failure modes

- **Deliberate centring, symmetry, minimalism, negative space**: a centred car on an empty track or an architecture shot should score high. Thirds-only scoring will reject good frames.
- **Crowded or busy backgrounds**: saliency and CADB both fail (CADB paper §5.6; ISNet trained on few cars).
- **Motion blur**: subject box is unstable, direction is ambiguous. Gate on sharpness first.
- **Mirror, reflection, duplicate boxes**: detection can double-count a car in its reflection; de-duplicate boxes before any balance sum.
- **Cropping after the fact**: a frame that is perfect in composition may lose a wheel at export. Compute cut-off on the original frame and on the planned crop.
- **Genre mismatch**: CADB, CACNet and GAIC were trained on general or Flickr-style photos. Expect bias toward landscapes and portraits.
- **Lead room without direction**: scoring lead room with an unknown direction produces noise. Use weight zero when direction is unknown.

## 6. Recommendations

**Implement first (cheap, interpretable, low-risk):**
1. Cut-off penalty (4.6) for all genres. It is the easiest to verify and the most likely reject reason.
2. Subject size band (4.7) per genre.
3. Lead room (4.4) with direction from bursts, for motorsport and wildlife, weight zero when direction is unknown.
4. Horizon tilt (4.2) for property, car meets with horizon, and sea/track shots.

**Second wave:** thirds/phi distance (4.1) as a mode among the three; saliency balance (4.3) with CADB as a learned check; headroom (4.5) for portraits and wildlife with keypoints.

**By genre:**
- **Motorsport**: lead room (direction from panning/flow), cut-off nose/tail, size band; centring for head-on; horizon tilt only where the horizon is visible.
- **Car meets**: centring/symmetry for static cars, cut-off, size band, balance against crowd (saliency gated on vehicle boxes), horizon tilt.
- **Wildlife**: lead room from gaze/heading, headroom, size band (tolerant of small), thirds for the eye.
- **Portraits**: headroom with eyes on the upper third, nose room (lead room with gaze), thirds, face size band, cut-off at joints.
- **Property**: horizon/verticals tilt, golden split for horizon, balance; leading lines are not implemented (no reliable detector).

## 7. Validation against delivered-vs-not labels (~2,300)

1. **Frame in the burst, not the whole set.** Compare candidates within the same burst (same subject, seconds apart). Use conditional logistic regression or a within-burst pairwise AUC. This removes scene and photographer-taste confounds.
2. **Control for sharpness and exposure first.** Delivered frames are mostly sharp; composition effects are small by comparison (Denzler & Redies). Report composition effect after adjusting for sharpness, eye-open, detection confidence.
3. **Split by event or day**, not randomly, to avoid leakage between near-duplicate frames.
4. **Per heuristic:** AUC with bootstrap CI (1,000 resamples over bursts), plus permutation test. Report by genre.
5. **Check failure modes:** list the top 30 false positives and false negatives per genre and check whether they match section 5.
6. **Threshold selection:** pick thresholds on half the events, test on the other half. Do not reuse the thresholds quoted in this note without checking them on Conrod's labels.
7. **Report what is learnable:** if a heuristic's within-burst AUC is below ~0.55, drop it or keep it as a display hint only.

Expected outcome, based on the evidence above: cut-off and size band will show the clearest signal; RoT/phi distance the weakest; centring should not be penalised.

## Sources

- Denzler & Redies, Art & Perception 2014: https://brill.com/view/journals/artp/2/1-2/article-p163_11.xml ; HVEI 2024: https://library.imaging.org/ei/articles/36/11/HVEI-229
- Hoh, Zhang, Dodgson, SIGGRAPH Asia 2023 poster: https://dl.acm.org/doi/10.1145/3610542.3626121 ; PDF https://fanglue.github.io/papers/AsiaPoster.pdf
- Synthetic photographs for aesthetic preferences (AAAI 2013 workshop): https://cdn.aaai.org/ocs/ws/ws1060/7133-30745-1-PB.pdf
- Rule-breaking statistics: https://pmc.ncbi.nlm.nih.gov/articles/PMC9249203
- Expert vs novice eye-tracking: https://link.springer.com/chapter/10.1007/978-3-030-80091-8_89
- Oil-painting SIP correlations: https://pmc.ncbi.nlm.nih.gov/articles/PMC5496686/
- Khan & Vogel, portraits: http://individual.utoronto.ca/shehroz/files/cae12-ssk-dv.pdf
- Mai et al., CVPR 2016: https://openaccess.thecvf.com/content_cvpr_2016/papers/Mai_Composition-Preserving_Deep_Photo_CVPR_2016_paper.pdf
- Liu, Puri et al. (Netflix), CVPRW 2019: https://openaccess.thecvf.com/content_CVPRW_2019/papers/FFSS-USAD/Liu_Modeling_Image_Composition_for_Visual_Aesthetic_Assessment_CVPRW_2019_paper.pdf ; formula in https://ijcai.org/proceedings/2011/0344.pdf
- SAMP-Net / CADB: https://github.com/bcmi/Image-Composition-Assessment-Dataset-CADB ; https://arxiv.org/abs/2104.03133
- CACNet unofficial: https://github.com/bo-zhang-cs/CACNet-Pytorch
- GAIC: https://openaccess.thecvf.com/content_CVPR_2019/papers/Zeng_Reliable_and_Efficient_Image_Cropping_A_Grid_Anchor_Based_Approach_CVPR_2019_paper.pdf ; https://github.com/bo-zhang-cs/GAIC-Pytorch
- S2CNet: https://github.com/suyukun666/S2CNet
- TANet: https://ijcai.org/proceedings/2022/132 ; https://github.com/mRobotit/TANet
- UNIAA: https://github.com/KwaiVGI/Uniaa
- DIS / ISNet: https://github.com/xuebinqin/DIS
- BiRefNet: https://github.com/ZhengPeng7/BiRefNet ; https://huggingface.co/ZhengPeng7/BiRefNet
- U²-Netp ONNX and licence table: https://github.com/patelnet/SnapBGR/blob/main/models/README.md
- Lead room / headroom guides: https://photographyicon.com/rule-of-space/ ; https://photographyicon.com/headroom ; https://guides.lib.unc.edu/media-design-center/video-concepts
