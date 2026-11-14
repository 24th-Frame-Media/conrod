# Scan → cull redesign

Inputs: `docs/plans/scan-cull-audit.md` (bugs and friction, file:line), `docs/research/culling-flows.md`
(Aftershoot, Capture One, FilterPixel, Lightroom flows). User feedback: every step is clunky, burst
grouping in Review is "fighting with it… buggy", Aftershoot's flow is the model, and Aftershoot shows
no images until all AI analysis is done. The scan must be transparent so the user isn't just sitting there.

## Target flow (6 steps)
1. **Import**: drag a folder or click Import. One sheet: folder, label (prefilled from the folder name),
   **shoot type** (one control, defaults to the last used), **strictness** (one slider: strict / moderate /
   lenient). An "Advanced" fold holds detection toggles. One button: **Scan**.
2. **Scan screen** (Review is locked until it finishes): live thumbnails as frames are read, the current
   stage, what it is finding, rate, honest ETA. Scan = read → detect → sharpness → (identify, if the preset
   reads numbers/plates) → group bursts → pick keepers. No separate "Suggest culls" step.
3. **Summary**: "3,061 frames · 412 bursts · 286 suggested keepers · 640 soft · 12 closed eyes" → **Review**.
4. **Review**: lands on bursts as stacks, keeper on top, count badge. Filters (Keepers / All / Rejected /
   Undecided) apply to the frames inside stacks too.
5. **Cull by keyboard** (below). Every change saves instantly, with undo.
6. **Export**: one button with a summary and destinations: Write XMP for Lightroom (default), Copy keepers
   to a folder (copy, never move), plus a dry run.

## Naming
Import, Scan, Review, Export. "Identify" only as an optional re-run. Re-run actions live in one menu with
plain effects: "Re-group bursts (instant)", "Re-pick keepers (instant)", "Re-score sharpness", "Rescan
everything". Settings that only need regroup/repick apply instantly on change; anything that needs a
rescan says so next to the control.

## Keyboard (grid, loupe and compare alike)
- ←/→ within a burst, ↑/↓ between bursts (always; flat view: ←/→ frame, ↑/↓ row).
- Enter expands a stack in place / opens loupe; Esc collapses.
- P keeper (flag), X reject, U clear, 0-5 stars. Auto-advance after P/X (setting, default on).
- Tab: done with this burst (go to next burst). N: next undecided frame/burst.
- C: compare/survey the burst side by side. Ctrl+Z / Ctrl+Shift+Z: undo/redo.
- Visible focus ring on the active stack and frame at all times.

## Phases

### F1: Review correctness (bugs first)
Frontend (Sonnet):
- One filtered model: stacks are built from the filtered frame list; counts, filmstrip and keeper honour
  filters (`Review.tsx:191/198/209`).
- Selection never jumps: clicking a stack selects its first visible frame; if a filter hides the selection,
  move to the nearest visible neighbour, not `frames[0]`, and never scroll to top (`useReview.ts:36-57`,
  `PhotoGrid.tsx:82-94`).
- Keyboard model above (`Review.tsx:279-280`, `useHotkeys.ts`), never stepping into hidden frames.
- One keeper concept: the stack's keeper = the manually picked frame(s) if any, else the engine's
  `burst_pick`; P/X change it immediately and the "Keepers" filter follows (`lib/review.ts:76`).
- Grouping/sort/filter persisted per album (survives Library↔Review; `App.tsx:116`, `Review.tsx:225`).
- Expansion keyed by a stable id that survives refresh/regroup (`Review.tsx:35/178`).
- Virtualise stacks (3,000-frame shoots).
- Album toggles write album settings only, never global settings (`Review.tsx:447/462/572-578`,
  `ShootSettingsModal.tsx`).
- Drop the legacy `view === 'stacks'` id.
- Pure logic (stack building, keeper choice, navigation) in a testable `.mjs` with node tests.

Engine (DeepSeek, worktree `conrod-flow`):
- `burst_gap` per album (job settings), remove the 4.0→1.5 sentinel (`desktop.rs:33`, `settings.rs:193`,
  `bursts.rs:14/19`).
- `regroup` applies in place, fast, no rescan, preserving manual picked/rejected/stars; returns counts.
  Called automatically when the album's burst gap or preset changes.

### F2: Import sheet
Frontend: one shoot-type component shared by the import sheet and the album settings modal (replaces the
three preset controls); strictness slider; Advanced fold; label prefilled. Engine: a single `scan` that
runs every stage to the end including grouping and keeper picks (and identify when the preset reads
numbers/plates).

### F3: Transparent scan screen
Engine: progress events `{job_id, stage, stage_index, stage_count, done, total, rate_per_s, eta_s,
recent: [{image_id, thumb_url, subject, verdict: sharp|soft|panning, label?}], found: {subjects, soft,
closed_eyes, numbers, plates, bursts, keepers}}`, throttled to ~4/s; ETA from a moving average.
Frontend: Scan screen shown for an album until the scan finishes (other albums stay usable); live
thumbnail strip/grid with small verdict marks, stage list with ticks, counts, ETA, cancel; summary card
with one button into Review.

**Pipeline view (user request):** show which AI component is active and what each worker is doing, as a
workflow flowchart. The engine already runs worker pools (`settings.rs:117-120`: preview_workers 4,
detect_workers 2, analysis_workers 3, workers; `lib.rs:739-746`; `operations.rs:130` identify workers)
but the UI shows none of it, so the scan looks single-threaded.
- Engine: a `pipeline` snapshot in the progress event: per stage node `{id, label, model (e.g. "YOLO
  v8 · DirectML", "edge blur · CPU", "face detector", "qwen2.5vl:7b via Ollama"), device, workers_total,
  workers_busy, queue_len, done, rate_per_s}` and per worker `{stage, worker_index, state: idle|busy|
  waiting(gpu/lock/ollama), file, step, ms_in_step}`. Instrument the existing pools; no new threads needed.
- Frontend: horizontal flowchart of stage nodes connected by arrows (Read → Detect → Sharpness/blur →
  Faces → Identify → Group → Pick), each node showing model, device, busy/total workers, queue depth and
  rate; the bottleneck node highlighted. Expand a node to see its worker lanes (one row per worker:
  current file thumbnail, step, elapsed).
- Worker counts per stage adjustable (Settings > Performance, and live +/- on a node during a scan
  if the pool supports resizing; otherwise applies to the next scan and says so). Sensible caps: GPU
  stages limited by VRAM, Ollama by its parallel setting, CPU stages by cores minus headroom for the UI.

### F4: Culling speed
Auto-advance, Tab, N, undo/redo log, survey/compare of a whole burst with synced zoom, face/eye zoom panel.

### F5: Export
Export sheet: summary counts, destination (XMP sidecars [default] / copy keepers to a folder), dry run
first, progress, result. No silent "Accept all"; identifications get a preview before bulk accept.

## Order and owners
1. F1 frontend (Sonnet) ∥ F1 engine + F3 events + one-shot scan (DeepSeek worktree).
2. F2 + F3 frontend (Sonnet) once the engine contract lands.
3. F4, then F5.
Each phase: `npx tsc --noEmit`, `npm test`, cargo tests, a mock walk with screenshots, then commit.
