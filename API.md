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
| health | none | Model rows plus VLM availability; Ollama model lookup has a four-second timeout |
| status | none | revision, tasks, log, activeJob, operations |
| jobs | none | Albums, counts, folder-name fallback for unnamed albums |
| review | jobId | frames and detections |
| scan | root?, jobId?, label?, profile?, recursive=true, stage="cull" | New/resumed jobId; stage index, cull, all, or identify (existing job required) |
| pause / resume_scan / stop | none | Control current cull; return status |
| identify | jobId | operation; readers, grouping consensus and registry seeding |
| write | jobId, dryRun=false, embedInRaw=false | operation; dry run logs proposed keywords without changing files or written timestamps |
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
| pick_keepers | jobId | Best subject per car per pass |
| group / regroup | jobId | operation; embeddings, groups and consensus |
| rename_job | jobId, label? | Trim label; blank/null restores folder name |
| summary | jobId | job, counts, images, numbers (with entry-list descriptions), plates, map_size, by_region |
| cover | jobId | {path, source} or null; existing crop or best thumbnail |
| known | none | Known-vehicle records |
| save_known | plate, make?, model?, colour?, team?, race_number? | Save a vehicle |
| delete_known | plate | Delete registry entry |
| export_known | none | CSV text |
| import_known | csv | written, skipped; supplied nonblank fields win |
| seed_known | jobId? | looked_at, cars, written, known; grouped consensus fills blanks and preserves aliases |
| import_entries | csv | path, count; validated CSV is saved under entries/ and selected for subsequent scans |
| training_status | none | Label counts and active models by region |
| train_label | detectionId, stars, pan=false | Save rating; Train UI advances its subject queue |
| undo_label | none | Restore previous training label |
| train_model | region="vehicle" | Fit/validate sharpness model; region vehicle, person, face, eye |
| forget_model | region="vehicle" | Remove learned model, preserve labels |
| train_taste | none | Learn visual preferences from manual ratings |
| cache_info | none | path and file/byte totals for thumbs, previews, crops, orphaned, total |
| cache_clear | orphaned=false, previews=false, jobId? | Clear only selected native cache files |
| reset_identifications | jobId? | Remove identification results; preserve detections/manual work |
| reset_detections | jobId? | Remove detections/crops, return images to pending; keep photographs |
| reset_all | none | Empty albums and native caches; keep registry, training, settings and originals |
| watch_status | none | active, folder, jobId, recursive, interval, added, checked, message |
| set_watch | active, jobId?, path?, recursive?, interval? | Persistent watch; newly settled files resume the album's selected processing stage |
| check_update | none | ok, current, latest, newer, tag, size, notes, installable (or error) |
| install_update | none | Verified installer download and restart; installed copies only, requires idle work |

## Detection edits

Text fields: make, model, colour, team, raceNumber, plate, plateState, bodyType.
`number` and `race_number` alias raceNumber; plate_state and body_type aliases support
existing UI records. `sponsors` is an array of strings. Null clears a field; omission
leaves it alone. rejected/bystander are booleans; reviewed defaults to true.
Edit stars 1–5 sets a hand rating; 0/null restores automatic rating. `mark` deliberately
allows manual zero. Numbers retain digits, plates are normalized, sponsor lists are deduplicated.

## Events and previews

Tauri emits `status` when the TaskHub revision changes. The frontend refreshes albums and
review on changes, throttles scan refreshes to four seconds and has a five-second fallback.
Preview/crop paths are exposed through Tauri's scoped asset protocol. A native folder picker
replaces the Python web folder-browsing routes. Index-only albums extract thumbnails when
opened without running detection. Full previews are generated when viewed.

## Isolation and releases

Set CONROD_HOME to an isolated directory for tests. Never exercise writing on originals.
Watch uses the saved native scan stage; older albums default to cull. Index-only watches
stay index-only. Disabling watch removes its saved configuration. Update installation is
explicit and requires a matching SHA-256 release checksum; portable builds do not self-install.
