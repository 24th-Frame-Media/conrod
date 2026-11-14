# Keeper signals plan

Goal: pick the frame the photographer would deliver from each burst/moment, and score
"good" photos overall. Baselines on delivered-frame labels: random ~12% top-1 within a
burst, sharpness 22%. Research: `docs/research/{aesthetic-models,models-onnx-deploy,
composition-saliency,personal-taste-bestshot,vlm-judge,competitor-scoring}.md`.

Conrod is free and open source (non-commercial), so research-licensed weights
(pyiqa: TOPIQ, MUSIQ, CLIP-IQA+, LIQE, NIMA; L2CS gaze) are allowed.

## Principles
- Every signal is a *feature* first. It only changes picks after it beats the current
  ranker on held-out shoots (leave-one-shoot-out, bootstrap 95% CI).
- Cheap signals run on every frame; expensive ones only on a moment's top 3.
- One evaluation set for everything: `target/taste/bursts.csv` (bursts with exactly one
  delivered frame, built from `target/groundtruth/labels.json`).
- Report top-1 / top-3 within burst, pooled and per genre, vs random and sharpness.

## Work items

| # | Item | Where | Runs on | Gate | Owner |
|---|---|---|---|---|---|
| K1 | Taste experiment: frozen embedding (CLIP B/32, DINOv2-S, CLIP L/14) + listwise head | `scripts/taste/`, `target/taste/` | every frame (~5-25 ms) | >= +10 pts top-1 over sharpness | DeepSeek (running) |
| K2 | IQA/aesthetic bake-off as features: LAION aesthetic (CLIP L/14 + MLP), TOPIQ, MUSIQ, CLIP-IQA+, LIQE, NIMA | `scripts/taste/` | every frame | adds >= +2 pts to K1 head, or beats sharpness alone | DeepSeek (after K1) |
| K3 | Expression: MediaPipe face landmarker blendshapes (eye blink L/R, smile, jaw open, brow), head pose; optional L2CS gaze | `conrod-vision/src/expression.rs` | frames with faces (~5-10 ms/face) | continuous eye-open beats binary closed-eyes on portrait/event bursts | DeepSeek (worktree) |
| K4 | Framing: edge cut-off per side, subject size band, lead room vs motion direction, horizon/vertical tilt (reuse `level.rs`), thirds/phi distance, balance | `conrod-vision/src/framing.rs` | every frame (<1 ms, from boxes) | feature only; validated through K1 head | DeepSeek (worktree) |
| K5 | VLM tiebreak: `qwen3-vl:8b-instruct-q4_K_M`, top 3 of a moment, both orders, keep swap-consistent verdicts | `scripts/taste/vlm_judge.py`, later `conrod-io/src/vlm.rs` | top 3 per moment (~5-15 s) | >= +5 pts over the deterministic ranker, CI excludes 0 | DeepSeek (after K1/K2, one GPU job at a time) |
| K6 | Productionise the winning head in Rust: ONNX embedding in `conrod-vision/src/embed.rs`, head weights in the model dir, retrain step from the user's own exports (bundled Python or Rust), score into `Candidate::beats` | engine | every frame | K1 gate met | DeepSeek, after K1 |
| K7 | UI: show keeper reasons (taste score, eyes, smile, framing flags, aesthetic) in Inspector; "hero" tier above keepers | frontend | - | after K6 | Sonnet |

## Order
1. K1 (running) -> K2 on the same burst set and cached previews.
2. K3 and K4 in parallel worktrees (pure Rust modules with unit tests; no wiring yet).
3. K5 once K1/K2 free the GPU.
4. K6 wiring: K1 head + K2/K3/K4 features feed one head; `cull_frame` / `pick_of_pass`
   get the head score as a new rank key above rating band.
5. K7 UI.

## Wiring (K6)
- New per-frame columns: `taste`, `aesthetic`, `eye_open_min`, `smile_max`,
  `edge_cut_sides`, `subject_frac`, `lead_room`, `tilt_deg`.
- Rank order becomes: stars > not culled > taste band > fewer cut-off sides > eyes open >
  larger box > rating > quality > sharpness > earlier frame (exact order decided by the
  K1/K2 ablation, not by hand).
- Presets decide which features apply (no smile for motorsport; no lead room for portraits).
