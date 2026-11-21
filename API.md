# Native command API

Tauri: `invoke('command', {action, args})`. The CLI exposes the same API as
`conrod-cli command ACTION '{"jobId":1}'`. Action names are snake_case; argument names
are camelCase except settings and known-vehicle records, which retain Python field names.
Omitted/null `args` becomes `{}`. IDs must be positive. Errors reject with a readable string.
Commands returning `operation` run in the background; inspect `status.tasks` for completion
or failure, and cancel with `cancel_operation`. CLI commands wait for completion and fail
with a nonzero exit code if a task fails.

| Action | Arguments | Result / behavior |
|---|---|---|
| bootstrap | none | settings, jobs, models, status |
| startup_status | none | the start-up splash state: `step` and `label` (the running step, or `ready`), `detail`, `model`, `error`, `done` / `total`, `ready`, and `completed` (one record per finished step: `step`, `label`, `detail`, `model`, `error`, `ms`). Steps are library, unfinished albums, models, ExifTool, and graphics card |
| health | none | Model rows plus VLM availability; Ollama model lookup has a four-second timeout |
| status | none | revision, tasks, log, activeJob, operations, queue (albums waiting their turn, first to run first), scan (the running scan's live summary) |
| jobs | none | Albums with total/done frame counts and hidden `copies`, folder-name fallback for unnamed albums |
| review | jobId | frames and detections |
| scan | root?, jobId?, label?, profile?, recursive=true, stage="cull", readPlates?, readNumbers?, includePeople?, overrides?, scanProfile?, blurStrictness?, burstGap?, groupingSensitivity?, groupingPause?, groupingKeepPans?, groupingMerge?, groupingFocusAnchor?, groupingStacks?, workflowMode?, identifyScope? | New/resumed jobId; stage index, cull, all, or identify (existing job required). The sheet's choices are stored on the new album only; a grouping word is normalised to off/gentle/normal/eager and a pause to 0–10. `scanProfile` is the shoot type; `overrides` is a map of detector/reader switches (`profile::Feature` keys to booleans) and a missing key follows the type; `readPlates` / `readNumbers` / `includePeople` are the older single switches. `workflowMode` ("both", "cull" or "identify") is normalised and stored on the new album; "cull" runs no identification lane, "identify" clears the scan's automatic cull. `identifyScope` ("keepers" or "everything") is normalised and stored on the new album; the automatic identification at the end of a scan then works on that scope |
| pause / resume_scan / stop | none | Control current cull; return status |
| unqueue | jobId | Take a waiting album out of the line; its status goes back to `stopped` and it can be started by hand |
| identify | jobId, scope?, onlyNew=false | operation; readers, grouping consensus and registry seeding. `scope` ("keepers" or "everything") overrides the album's `identify_scope` for this run; omitted, the album's setting applies (default "keepers"). "keepers" identifies the effective keeper: a frame picked by hand (which suppresses the suggestion for its burst), or, with no hand pick in the burst, the engine's suggested pick (`d.burst_pick=1 OR i.burst_pick=1`); "everything" identifies every unreviewed non-bystander subject, culled or not. `onlyNew` is the incremental run the review screen offers after a redetect or re-import: subjects already read are left alone; false reads every unaccepted subject again |
| skip_identify | jobId | Accept the album's identifications as they stand: every unread vehicle/person subject is marked read without a model call, so Review opens. New subjects found later still count as unread. Returns `{ok, skipped}` |
| write | jobId, dryRun=false, embedInRaw=false | operation; dry run logs proposed keywords without changing files or written timestamps |
| lightroom_export | jobId | Write the album's verdicts to `%USERPROFILE%\.conrod\bridge\out\<album>-<unix>.json` for the Conrod Bridge plug-in: one entry per visible, non-copy, done frame with `rating` (conrod's 0–5 stars), `pick` (1 hand-picked, -1 rejected or auto-culled under `respect_culling`, else 0), `label` (green keeper, red rejected, yellow "needs look", else null), keywords (`<prefix> keeper`, `<prefix> cull`, `<prefix> burst <n>`), plus `stars`/`verdict`/`burst`/`keeper`. `collection` names the collection the plug-in creates. Returns `package` (name without `.json`), `photos` and the absolute `path`. The package is written `*.json.tmp` then renamed, so the plug-in never sees a partial file |
| lightroom_pending | dir="in" | `{packages: [{name, album, photos, age_s}]}` for the `*.json` packages in `in` or `out`, newest first. `*.tmp` is ignored. Reading never consumes a package |
| lightroom_import | jobId, catalog? | Apply what the photographer decided in Lightroom. Without `catalog`, consume every package in `bridge\in` (a package for the other direction, an unknown format or a newer version goes to `failed\` with a `.txt` reason; a bad photo entry is skipped and counted) and move it to `done\`. With `catalog`, read a Lightroom Classic `.lrcat` read-only/immutable and apply its flags instead. Matching is by normalised absolute path, then file name plus capture time within 2 s, only when exactly one album frame qualifies. Writes `images.catalog_rating`, `catalog_pick`, `catalog_label`, `catalog_at` and nothing else, only when a value changes. Returns `matched`, `unmatched` (package/catalog entries with no unique frame), `missing` (album frames with no catalog entry), `percent` (share of effective keepers carrying `pick=1`, 0–100 or null), `skipped`, `failed` and `left` (packages that matched nothing in this album, so they stay in `in\` for the album they belong to) |
| lightroom_plugin_status | none | Where the Conrod Bridge plug-in stands: `installed`, `version`, `bundled`, `bundledVersion`, `needsUpdate`, `modules` folder, `path`, `running` (is Lightroom Classic open), `enabled` (the bridge setting) |
| install_lightroom_plugin | none | Copy the bundled Conrod Bridge plug-in into Lightroom's Modules folder, replacing only our own folder and marker; a same-version build is left alone. Returns `installed`, `updated`, `version`, `path`, `running`. Restart Lightroom Classic after an install or update |
| remove_lightroom_plugin | none | Remove only our plug-in folder from Lightroom's Modules folder. Returns `{installed: false, removed, path}`; a second remove is a no-op |
| openrouter_check | key | Validate an OpenRouter key without spending a model call. Returns `{ok: true, label, limit, usage}` or `{ok: false, error}`; a blank key says to add one first |
| decider_status | none | The decision model card: install state, download progress, VRAM, precision/quantization, and what the model will do for this machine |
| install_decider | none | Download the missing decision-model files in the background (resumable, hash-checked), reporting task progress |
| cancel_operation | key | Signal background cancellation |
| install_models | none | Install missing pinned model/tool assets, reporting task progress |
| save_settings | settings fields | Updated settings |
| delete_job | jobId | Remove album records; original photographs stay intact |
| mark | imageId, stars?, rejected? | stars 0–5, null restores automatic; omitted fields unchanged |
| edit_detection | detectionId, fields below | Updated attributes; preserves unsent fields |
| bulk_edit | ids, number?, rejected?, bystander? | Apply to selected detection IDs |
| preview | imageId | Cached, oriented preview path |
| filling | jobId | Start missing-thumbnail extraction; also used when opening indexed albums |
| rescore | jobId | operation; apply learned/current scoring to stored features, preserving hand ratings |
| rescan | jobId | operation; re-read every photo and re-measure sharpness inside the stored boxes, preserving all hand edits |
| pick_keepers | jobId | Best subject per car per pass |
| focus_points | jobId | operation; read the camera's autofocus area from each RAW, rate frames with no subject inside it, then regroup |
| redetect | jobId | operation; run the detector at a one-off 0.25 confidence and 2% box over the frames with no subject, then regroup |
| composition | jobId | operation; measure placement, balance, symmetry, background clutter and leading lines for every frame |
| collapse_copies | jobId | operation; hide the extra imports of one shot (same time, size and sampled bytes) as `copy_of`; bursts, picks, counts and writes ignore them |
| restore_copies | jobId | operation; show every hidden copy again, then regroup |
| taste_rank | jobId, force? | operation; score each frame's cached preview with the learned taste head into `images.taste` (skips scored frames unless `force`); `head: false` when no head is installed |
| group / regroup | jobId | operation; embeddings, groups and consensus |
| rename_job | jobId, label? | Trim label; blank/null restores folder name |
| summary | jobId | job, counts, images, numbers (with entry-list descriptions), plates, map_size, by_region |
| shoot_stats | jobId | The shoot's statistics card: frames/copies, size, cameras, lenses, most-used camera/lens/aperture/shutter/ISO/focal length, per-value histograms, pans, bursts, blur verdicts, AF summary, keepers, rejects, stars, capture range. Backfills an old album's lens/ISO/file size first (`backfilled`: rows read) |
| global_stats | none | The same numbers for the whole library: albums/complete albums, frames/complete frames, copies hidden, size, effective keepers, rejects, pans, bursts, blur verdicts, AF summary, shooting time (sum/average/longest span and timed-album count). Gear and exposure lists (`cameras`, `lenses`, `most_used`, `f_numbers`, `shutters`, `isos`, `focal_lengths`) count **only complete albums** -- those whose every visible frame has a non-empty camera and lens and a non-null ISO, f-number, shutter and focal length. `af.soft` uses the app's global `settings.blurred_below` (per-album strictness is not stored per frame). Nothing is backfilled from the files |
| cover | jobId | {path, source} or null; existing crop or best thumbnail |
| known | none | Known-vehicle records |
| save_known | plate, make?, model?, colour?, team?, race_number? | Save a vehicle |
| delete_known | plate | Delete registry entry |
| export_known | none | CSV text |
| import_known | csv | written, skipped; supplied nonblank fields win |
| seed_known | jobId? | looked_at, cars, written, known; grouped consensus fills blanks and preserves aliases |
| import_entries | csv | path, count; validated CSV is saved under entries/ and selected for subsequent scans |
| cache_info | none | path and file/byte totals for thumbs, previews, crops, orphaned, total |
| cache_clear | orphaned=false, previews=false, jobId? | Clear only selected native cache files |
| reset_identifications | jobId? | Remove identification results; preserve detections/manual work |
| reset_detections | jobId? | Remove detections/crops, return images to pending; keep photographs |
| reset_album | jobId (required) | Return one album to just after import, in one transaction: delete every detection and clear every measurement, star, pick, reject and identification on its frames, delete its cached crops and thumbnails, and leave the album ready for a first scan ("indexed", 0 done). Keeps its settings, the global registry and training labels, and every photograph and XMP sidecar. Frames keep only id, album and path |
| reset_all | none | Empty albums and native caches; keep registry, training, settings and originals |
| watch_status | none | active, folder, jobId, recursive, interval, added, checked, message |
| set_watch | active, jobId?, path?, recursive?, interval? | Persistent watch; newly settled files resume the album's selected processing stage |
| check_update | none | ok, current, latest, newer, tag, size, notes, installable (or error) |
| install_update | none | Verified installer download and restart; installed copies only, requires idle work |

## Removed commands

`training_status`, `train_label`, `undo_label`, `train_model`, `forget_model` and
`train_taste` were removed with the Train tab (be1df63) and are no longer
accepted: they now fail to parse like any unknown action. The `sharpness_labels`,
`native_labels` and `native_label_undo` tables and the `detections.predicted_stars`
column are kept, along with any labels a person saved in them; nothing trains
from them or writes them any more.

## Detection edits

Text fields: make, model, colour, team, raceNumber, plate, plateState, bodyType.
`number` and `race_number` alias raceNumber; plate_state and body_type aliases support
existing UI records. `sponsors` is an array of strings. Null clears a field; omission
leaves it alone. rejected/bystander are booleans; reviewed defaults to true.
Edit stars 1–5 sets a hand rating; 0/null restores automatic rating. `mark` deliberately
allows manual zero. Numbers retain digits, plates are normalized, sponsor lists are deduplicated.

## Grouping settings

`update_job_settings` (`jobId`, `patch`) and `save_settings` take these album
fields, in snake_case like every settings key. Each defaults to the behaviour an
album had before it existed, so an older `settings_json` keeps grouping exactly
as it did. A change to any of them regroups the album in place, like a burst gap
-- `workflow_mode` and `identify_scope` are the exceptions: they change what the
next scan or identify run does and never regroup on their own.

| Key | Default | Meaning |
|---|---|---|
| burst_gap | 1.5 | Seconds between two frames of one camera that still belong to one burst |
| grouping_sensitivity | "normal" | How eagerly a time burst is split into moments: "off" is one stack per burst, "gentle" moves every cut threshold 1.5x looser, "normal" is the measured default, "eager" moves them 0.7x tighter (about 1/1.4). An unknown word reads as "normal" |
| grouping_pause | 0.8 | Seconds between two frames that count as a pause: the next frame starts a new moment if the picture also changed. Clamped to 0..10 when used; 0 disables the pause cut |
| grouping_keep_pans | true | `false` compares a panning frame's box like any other, instead of allowing it to travel further |
| grouping_merge | true | `false` keeps the time gap final; `true` joins neighbouring time bursts of one camera that look like the same subject a few seconds apart (same orientation, whole-frame look within 0.12, continuing subject boxes and vehicles' liveries within 0.50, chained no further than 30 s) |
| grouping_focus_anchor | true | `false` leaves a frame the detector missed with no subject, rather than anchoring it on the camera's focus area (`af_*`) |
| grouping_stacks | true | `false` gives every frame a stack of its own (dense keys, in moment order, and stable across regroups) and suggests no keeper: a "best of stack" pick would mark every frame, which says nothing |
| workflow_mode | "both" | What this album's scans do: "both" is scan, cull and identify; "cull" is scan and cull with no identification lane; "identify" clears the automatic cull after a scan. Trimmed and lowercased when read; an unknown or missing word is "both" |
| identify_scope | "keepers" | Which subjects this album's identify lane works on: "keepers" is the effective keeper the UI shows -- the frame the photographer picked by hand when its burst has one (which suppresses the suggestion), otherwise the engine's suggested pick (`d.burst_pick=1 OR i.burst_pick=1`); a hand star alone is not a keeper. "everything" is every unreviewed non-bystander subject, culled or not. Independent of `workflow_mode`, which decides whether the lane runs at all. Trimmed and lowercased when read; an unknown or missing word is "keepers" |

## Events and previews

Tauri emits `status` when the TaskHub revision changes. The frontend refreshes albums and
review on changes, throttles scan refreshes to four seconds and has a five-second fallback.
Preview/crop paths are exposed through Tauri's scoped asset protocol. A native folder picker
replaces the Python web folder-browsing routes. Index-only albums extract thumbnails when
opened without running detection. Full previews are generated when viewed.

`identify-progress` is one JSON snapshot of the live identification feed, emitted about four
times a second while an identify run is live and once more after it ends. It carries the run
(`job_id`, `state` = `running`/`done`/`stopped`/`error`, `done`/`total`, `started_at`,
`elapsed_ms`, coarse `stage`, the `model` the VISION node should name, `workers` (how
many run at once), and run-wide `errors`), the `current` car being read, up to
8 cars still in flight in `active` (one per identify worker, newest first, including
`current`; each carries its own `prompt` and its `worker` index), and up to 16 finished
items in `recent` (newest first, never including
`current`). `vision_off_reason` is null when vision runs, else `album` (off for this
album), `settings` (off globally and the album did not choose otherwise),
`no_service` (on, but no vision server is enabled), or `lanes` (this shoot type has
nothing to ask the model). `decider` is the decision model's live feed, present while
the cars are read because the model runs live during identify: `state`, `enabled`,
`message`, `model`, `device`, `quant`, `capacity`, `busy`, `queued`, `done`,
`errors`, `avg_ms`, `install`, `current` (the frame's `label`, `thumb`, `queued`,
`elapsed_ms`, and `asking` questions with their kind, text and options), and
`recent` (finished questions with their answers, probabilities and `bars`). An item is
`detection_id`, `image_id`, `path`, `thumb_path`, `preview_path`, `crop_path`, its own
`stage` (`read`/`crop`/`vision`/`merge`/`done`), `worker` (0-based lane), the vision
model's `prompt` (the current car and every active one) and `reply`, the visible
`action` tags, the reader `trace`
and `failures`, the final `number`, `number_source`,
`number_conf` (0..1; null until the car is read), `plate`, `make`, `model`, `team`,
`colour`, `ms` and `at`. Paths are
passed through the same `asset()` scope as previews. A run with the vision model off still
reports OCR and entry-list fields, with `prompt`/`reply` null. The feed stays available after
the run so the screen can show its summary; the next run replaces it.

## Isolation and releases

Set CONROD_HOME to an isolated directory for tests. Never exercise writing on originals.
Watch uses the saved native scan stage; older albums default to cull. Index-only watches
stay index-only. Disabling watch removes its saved configuration. Update installation is
explicit and requires a matching SHA-256 release checksum; portable builds do not self-install.
