# Changelog

What changed in Conrod, release by release, in plain language. Generated from the git history by
`node scripts/release-notes.mjs --all --write`; wording is steered by `scripts/release-notes.overrides.json`.

Installers and checksums are on the [releases page](https://github.com/24th-Frame-Media/conrod/releases).

## Unreleased

Culling works with the camera's own focus point, Conrod talks to Lightroom, ingest is one live screen, a crash no longer loses your place, and model downloads are real: verified, resumable, and honest about what is on your PC.

### New

- Read Canon AF areas natively from MakerNotes.
- One live ingest screen with scan and vision workers.
- The camera's focus area decides the subject when the detector misses it.
- Join look-alike bursts a few seconds apart.
- Conrod Bridge plugin (flags out, verdicts in).
- Send verdicts to Lightroom and import its flags.
- Destructive reset back to the imported state.
- The new mark - conrod with the low kink on slate.
- Read the focus grid when the detector only found something small.
- Export verdicts and import flags through the bridge.
- Live per-worker frames and honest stage counters.
- Forged con-rod mark v2 - filleted eyes, split big end, clearer kink; header uses the real mark.
- In-app What's new sheet, opens once per new version.
- Per-type policy table, Lanes, per-album detector/reader overrides.
- Wire lanes end to end; set_feature, preset_policy, per-album overrides.
- 'what Conrod will do' list with per-type switches in import and album settings.
- Per-type moment vocabulary, judge backends, burst peak, rank lift with guards.
- Moment tags, maybes, lift note and facet in review; moment switches and threshold in the shoot-type list and settings.
- Judge moments with the on-device decision model, gated against the scan's GPU stages.
- Faces are your choice at motorsport, jersey numbers at field and court sport, your defaults per shoot type.
- First-run setup that tunes Conrod to what you shoot.
- The engine reports its start-up steps and answers startup_status.
- Show what the engine is doing while it starts.
- Add albums while a scan runs; they wait in line.
- OpenRouter is available as a vision provider, so a cloud model can read your cars, and its key has a test button that does not use up a request.
- Settings has one AI models page that lists every model in plain words, says where each one runs and whether it is ready, with a switch for each. The decision model can run on your graphics card or through OpenRouter, with a key test, and OpenRouter can also read cars for you.
- The decision model works from the start of identify.
- Conrod installs and updates the Lightroom Classic plug-in for you from Settings, with its own status line.
- Review says which car a frame is about and why it was kept or set aside, queued albums show in the scan screen and activity popover, and the export sheet explains what happens to undecided frames.
- Name why a scan stopped in plain words, not a raw error.
- One readiness model for the badge, its popover and Setup.
- Keep a failed scan in view, open Review mid-scan, show one health status.
- A compact live view, a whole-album time left, and plain model names.
- File-based metadata is the main path.
- Read Nikon, Sony, Olympus and Panasonic AF areas.
- Judge moments with OpenCode's decision model JEV.
- Rebuild the Library as a grouped cover wall.
- New scans group photos shot together into bursts before any photo is read, so a stopped album still shows sensible stacks.
- Bursts appear as finished units rather than the whole album flipping at once, and the time-left estimate now covers every remaining step.
- Rerun one pipeline stage on demand.
- Run again controls for each stage.
- Recommend qwen3-vl 8B as the local vision default.
- Add moments services, strategy and fallback.
- Fail over across every enabled moment service.
- Yolo26m is the default car and person detector.
- Enumerate decision models by service metadata.
- Model thinking is off by default for car reading.
- The AI services page is rebuilt: add the services you use, pick each model, and see what each photo costs.
- Each AI service in Settings has its own thinking switch, so one service can reason before it answers while another replies straight away.
- Open any album and see what Conrod is doing with it — the work screen is always there: scanning, waiting its turn, stopped, or finished with its summary.
- Show the bursts pass as its own phase on the identify screen.
- Dev mode is unmistakable: an amber hairline, a solid DEV chip and a live-reload light in the title bar, a badged app and tray icon, and a clear isolated data home on the splash, popover and Settings. A normal launch stays exactly as it was.
- The built-in vision model (Qwen3-VL 8B, the winner of our November benchmark) downloads from Settings with real gigabyte progress, and Download fetches just the model you clicked instead of everything.
- An optional decision model, switched on in Settings, answers yes/no questions about your frames on your own graphics card. It is a large one-time download and stays off until you choose it; with it off, or on a card without enough video memory, nothing changes.

### Improved

- Select the percentile instead of sorting the plane.
- Bigger page cache, in-memory temp store and mmap on every connection.
- Pool face and expression sessions across worker lanes.
- Read each RAW's AF header in parallel.
- Group on the reader connection, lock the writer only to store.
- Free the encoded preview once it is decoded.
- Confine DescribeImage and map_path, Gemini key in header, ProviderModels host/timeouts, CSP, drop dead update.rs.
- A cloud moments service now lists its decision models to pick from instead of asking you to type one, and every cloud model shows its price so you can estimate what a shoot costs.
- Each service gets its own card with a key or host field, a Test button and a cost estimate per photo. Vision and Decisions dropdowns are filled from that service's models, and a per-service Thinking switch keeps reads fast by default. One control shares work across every enabled service; switching a card off leaves the pool. Photo helpers are compact rows showing the model in use, with a small popup when several models serve one, and the built-in models download with one click.

### Fixed

- Harden CR3 parsing, Retry-After, poisoned locks and swallowed DB writes.
- Single-frame stacks render and open as plain frames.
- Forward the join look-alike bursts setting.
- Use the camera's in-focus point when a grid record names one.
- The reset album is indexed, not scanning.
- The focus area rescues a frame and never demotes one.
- Reconcile crashed scans, report frame errors, resume at the stored stage, guard re-imports.
- Place the AF label where no detection label sits.
- Worker frames fill their card instead of hugging the left.
- Stable list keys and a readable new-imports badge.
- Call interrupted scans unfinished, not crash leftovers.
- Scroll the ingest worker list and the review detections list in place.
- Albums follow the Settings vision switch, and an off run says why.
- A sharp frame the detector found no car in is kept and marked as kept for its sharp focus, instead of being shown as culled.
- On motorsport shoots a car the detector labels a truck or bus is still treated as a car and never dropped.
- The decision model sees the whole frame, then the car crop.
- A car close enough to fill the frame is found even when the detector is unsure, and a sharp car that touches the edge is kept instead of being culled.
- Trust the vision model number and let OCR fill gaps only.
- A disabled service no longer shows up as qwen2.5vl.
- Only the car the camera focused on counts now. Other cars in the picture are shown dimmed as background cars and never change the rating, the pick or what gets identified.
- An identified album stays identified; new cars are read on their own, or skipped.
- Conrod now applies its suggested stars and picks to your photos and to Lightroom automatically when an album finishes.
- A car detail you corrected by hand is no longer overwritten when Conrod tidies the album.
- Zoom a tall photo inside its own frame.
- An identified album is not asked to identify again.
- Bill OpenCode services to pay-as-you-go credits, never the Go subscription.
- An album sitting alone in a group no longer stretches into one giant card.
- Lay out the identified-album banner as one row.
- Clean a damaged character in the jobs list.
- Use real example photos instead of drawn placeholder covers.
- Fixed-height service cards clipped the model, effort and variant option rows.
- Anchored menus stay on screen.
- Activity popover normalises health rows and stays on screen.
- Action menus flip and clamp to the viewport.
- Car reading no longer trips over the moments job's own entry: a settings file that picked the built-in reader as a vision service is cleaned on load, and the two AI service lists can no longer mix.
- The moments job says who is actually doing the work: when a cloud service like OpenCode JEV judges your photos, the AI services card and the live DECIDER step name that service and its model, instead of claiming the local decision model is not installed.
- Conrod refuses to send your key to anything but a secure or local address. A service on your own machine or network keeps working; a plain-http address on the internet is turned away with a note saying which setting to change, and the Ollama model list shows that note instead of an answer.
- The Test button tells the truth: it checks the key or host with the real service, and when the service refuses it says plainly why it is not ready instead of claiming it is ready.
- After the last car is read, Conrod says what it is still doing — comparing your bursts, with a count — instead of sitting silently at a full bar.
- Editing a car by hand no longer splits it off from its frames: regrouping still groups it with the right car.
- A car you fill in by hand is treated as read: Identify no longer offers it again or counts it among the new cars.
- Settings are saved safely, and an unreadable settings file is kept aside and reported instead of being replaced with the defaults.
- Download now actually installs the missing model and waits until that model appears before offering its switch, instead of pretending a short timer finished the job.
- The built-in model rows read the real list of models on your PC and the decision-model setting, so a model that is installed and active shows its switch on instead of a Download button.
- Choosing None - skip for a model now leaves it out of the pool and its estimated cost, instead of still counting and charging for it.
- Your own picks can't be overwritten any more. If you pick, star or reject a frame in Review while Conrod is still writing metadata for that album, your choice stays and that frame is left out of that write instead of being marked as a reject.
- Model downloads check free disk space before they start and say plainly when there is not enough. A download that fails or is interrupted keeps its reason and its partial file so the next try resumes, and a model with no published source yet reads not available yet instead of offering a Download that cannot work.
- The AI settings card tells the truth about what is on your PC the moment you open it: an installed decision model shows its switch even while it is turned off, the built-in vision model only reads as installed after its files download and pass their checksum, and a failed install keeps its plain reason with a Retry.

## 1.0.0-beta.8 - 2026-11-19

A much smarter cull: sharpness measured on the subject itself, better pans and bursts, top-three picks, undo, a light theme and live identification.

### New

- Culling roadmap (pick, compare, filmstrip, dupes, eyes, people, NIMA).
- Gradient-ratio edge blur estimator and labelled experiment.
- Subject sharpness from native-resolution edge blur, mapped onto the star scale.
- Split bursts into moments by subject framing; keeper prefers the prominent uncut subject among equally sharp frames.
- Per-album blur strictness (strict/moderate/lenient), moderate by default.
- Per-album race entry list (CSV/JSON) with lookup, candidates and settings field.
- Tight vehicle-reading prompt (visible text only, racing hints, plate validation) and design-only models kept as suggestions.
- Race entry list fills team, drivers, car and sponsor spellings from the read number.
- Panning = sharp subject over a coherently streaked background (85% of real pans, 5.6% false on static).
- Review marks each burst's top three candidates and can filter to them.
- Unslop polish (graphite chrome, amber china-pencil keeper mark, line icons, sentence case) and per-shoot facet tabs.
- Framing features from detection boxes (K4).
- Exposure measure for blown highlights and crushed shadows.
- Sport subjects -- the ball in play, and the rider on the animal.
- Per-album burst gap with one default, one-time migration, worker caps and the scan-progress payload.
- Instant regroup with counts, one-shot scan and the pipeline snapshot.
- Emit one scan-progress event from the status thread.
- Expression reads eye open, smile and head pose from MediaPipe landmarks.
- One-sheet import and transparent scan screen with pipeline flowchart, worker lanes, live thumbnails and summary.
- Album-scoped sheet choices, real stage queues and a weighted overall.
- Wire the keeper signals into the scan, the sets and the rank.
- Export sheet with album counts, dry run and Lightroom instructions; Review shows a loading state instead of an empty album.
- Ctrl+Z / Ctrl+Shift+Z undo and redo the last culling decision, moving back to the frame it changed.
- Settings overhaul - culling first, per-album chips, search, dirty footer, known vehicles as a catalog.
- Learned taste sort from DINOv2 embeddings, never a keeper pick.
- A deliberate light theme with a header toggle.
- Identifications sheet (what was read, accept one by one), sheets render through a portal and only close on a backdrop press, pan is a flag beside sharp/soft.
- OpenCode (Console/Go) provider with model list; research note on keeper and motorsport models.
- Pans are rated on the picture - capped by the sharp share of the frame, so most pans land at three stars.
- Suggested keepers are green and one click to accept.
- A burst with nothing above two stars has no keeper, and the grid says so.
- Hide duplicate copies of a shot; a rated subject beats a frame the detector missed.
- Critical strictness for track work; a pan's frame stars read the picture; a missed frame is rated on the whole frame.
- Group a burst by its pictures, and rate a missed frame where the camera focused.
- Composition labels - placement, balance, symmetry, leading lines and background clutter.
- Tunable stacking, the camera's focus area in the payload, and a blurred subject the background cannot vouch for.
- Keep/cull pills, stack decisions, composition labels, the AF box and cross-stack arrows.
- Measure a subject inside the camera's focus box on a re-score.
- An album can cull, identify, or both.
- Choose cull, identify or both, and follow it in the pipeline.
- Store lens, ISO and file size and add shoot_stats.
- A shoot statistics card and shot details in the viewer.
- Identify the effective keeper by default and open shoot statistics in their own dialog.
- Add library-wide statistics and fix the keeper-index freeze.
- Add the live identification flow and OpenCode Go support.
- One live page per worker, action reads and a richer viewer.
- A connecting-rod C for the mark and app icon.

### Improved

- Time-windowed near-duplicate search, parallel hashing, one burst gap default.
- Compact the review payload and make it testable.

### Fixed

- Keep opt-in NIMA out of install-everything.
- Detect birds and other animals as subjects in every preset.
- Rate subject-less frames on their in-focus area so detail shots are not blurred.
- Continuous top of the blur-to-rating map; sharper subject breaks rating ties.
- A frame whose framing changed stands alone instead of merging into the previous moment.
- App vehicle reading matches the benchmark: no repeat penalty on first ask, entry-list numbers trusted, 8k Ollama context.
- Top-3 ranks skip rejected frames and NaN ratings, shared rank badge on grid and filmstrip, keyboard blur-strictness control.
- Portrait picks use the softer eye and never a shut face, subject-less frames take directional edge blur, profile-aware readiness.
- Filters apply inside burst stacks, selection never jumps, burst keyboard model, P sets the keeper, per-album view prefs, virtualised stacks, album-only settings.
- Post-merge.
- Rank framing after size and give exposure and smiles margins.
- Let the blendshape mesh only confirm a closed eye, never invent one.
- Preset blur default reaches the app, deliver-most culls real blur, dark_ok in the preset policy; add wiring review.
- Bottleneck becomes an hourglass in the card corner, steadier time-left estimate (median agreement, smoothed readout), blind-run pick export.
- Name COCO animals (cow, horse, bird, ...) instead of calling every one a vehicle.
- Lead room from the pan axis, and a total keeper order.
- Keep six-letter personalised plates the bench already reads.
- Rows with a number or plate come first, and a livery with no number shows its words.
- A pan is one moment however far the car travels, and a pan that only keeps part of the car sharp is capped by its weakest quarter.
- A subject-less frame with pan streaks is judged like a pan, with a diagnostic example.
- Re-read pixels and rate subject-less frames, so a re-score actually replaces a stale pan rating.
- A pan is rated by its cap, not by its sharpest detail; the scan and a re-score agree.
- A pan stays one moment through the frames the detector missed.
- A pan no longer swallows a static moment framed elsewhere.
- The Close button sits at the top right of the side panel.
- Read focus points only for the frames the detector missed.
- The crop's tile score must not overrule the edge census.
- The camera's focus area may only lower a subject's score.
- A frame whose own crop reads soft does not keep the burst.
- One toolbar row with controls the same height.
- A featureless patch is unmeasured, not perfectly sharp.
- Keep the model's number confidence and read the official window number.
- Let a driver name on the car correct a misread number.

## 1.0.0-beta.7 - 2026-09-25

Use several vision servers at once, compare vision models on your own photos, and cull shoots with people in them.

### New

- VLM servers, benchmark, people split, updater fixes.

## 1.0.0-beta.6 - 2026-09-25

Safer update and metadata handling, and one version number across the app.

### Fixed

- Audit fixes, single-source version, release script.

## 1.0.0-beta.5 - 2026-09-24

Lightroom-style zoom and pan, and a redesigned Settings screen with update status.

### New

- Decode full-resolution preview and zoom directly to subject.
- Overhaul zoom and pan with Lightroom photographic inspection controls.
- Resilient update check with atom fallback, settings makeover, and ollama model discovery.
- Add right-hand stats, version, and updater sidebar dashboard.

### Fixed

- Add category tabs to settings and fix dropdown text clipping.

## 1.0.0-beta.4 - 2026-09-24

A main button that always offers the next step, shoot settings, and a rebuilt Review table.

### New

- Add ML-eligible filtering, EXIF subsecond extraction, and bulk edit support.
- Overhaul Review and Known Vehicles UI with DetectionsTable and vehicle merge.
- Split primary action button, shoot settings, resizable inspector, and release v1.0.0-beta.4.

### Fixed

- Fix panned shutter burst grouping across angle changes and unculled album threshold.
- Eliminate hallucinated race numbers from plates, calendar years, and VLM guesses.

## 1.0.0-beta.3 - 2026-09-21

A shorter path from import to a finished cull.

## 1.0.0-beta.2 - 2026-09-20

A clearer view of a scan while it runs.

### New

- Improve scan monitoring and review.

## 1.0.0-beta.1 - 2026-09-20

Conrod is now a native Windows app. The Python version is retired; your library and settings carry over.

### New

- A native app with an installer and a portable zip: no Python to install, and it starts much faster.
- Everything is rebuilt natively: reading RAW files, finding cars, rating sharpness, grouping bursts, reading plates and race numbers, and writing XMP files for Lightroom.
- Updates arrive through the app itself.

## 0.8.0 - 2026-09-06

Faster capture-time reading, and vision work can spread over several machines.

## 0.7.0 - 2026-09-05

Big shoots open in seconds; thumbnails and details fill in behind.

## 0.6.4 - 2026-09-05

Updates install even when last time's folder will not go away.

## 0.6.3 - 2026-09-05

A watched folder no longer keeps scanning after you delete its album.

## 0.6.2 - 2026-09-05

Frames with no vehicle in them are measured again too.

## 0.6.1 - 2026-09-05

Each car is learned once, not once per frame.

## 0.6.0 - 2026-09-05

Conrod remembers the plates it has already read.
