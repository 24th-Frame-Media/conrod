# Rust port handoff, 20 September 2026 (state after Codex's pass and a verification pass)

Branch `rust-port` (HEAD `6bca7cf`). **Everything is uncommitted** (about 80 paths); the owner has not asked for a
commit, so ask before committing or pushing. Original plan: `C:/Users/kapsikkum/.claude/plans/cryptic-humming-thacker.md`
(its egui decision is superseded: the app is **Tauri v2 + React**). Wire format of every command: [API.md](API.md).
Build, dev and CLI: [README.md](README.md). Release steps: [RELEASING.md](RELEASING.md).
Owner goal: every feature works and is ported properly; the Rust is idiomatic, not a Python imitation; a release can
be built; the UI resembles the Python web UI with a native touch; the app installs missing models itself.

## 0. Next steps, in order

1. Ask the owner to authorize committing this work (about 80 uncommitted paths; nothing is safe from a stray checkout).
2. Owner decisions, made 20 Sept 2026: **no code signing** (releases stay unsigned; do not add signing steps),
   **keep Rust's rule that the keeper is of the subject kind the profile rates** (do not "fix" it to Python's), and
   Tauri **may download NSIS** to build the installer. Also: **the native app replaces the Python app**, and `gh` may
   be used to publish `assets-v1` (done, section 1). Still the owner's: the go-ahead to commit and push, and to cut the
   first tag. The version is now `1.0.0-beta.1` (my choice: the native line continues above Python's 0.8.x; change it
   in the three places `check-version.mjs` lists if the owner prefers another number).
3. Close the remaining parity checks in section 2 (item 2) with `tools/parity_ops.py` style comparisons.
4. Tagged pre-release `rust-v1.0.0-beta.1` through `release-rust.yml` (needs the commit and push; the local
   installer test and the assets are done, section 1).
5. UI and idiom items in section 2 (items 3 to 6).
6. Cutover and deleting the Python app (section 5). The owner has said the native app will replace it; deleting
   the Python code waits for their go-ahead once the beta is accepted.

## 1. State

Green at the end of this pass: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo test --workspace` (engine 40 tests, io 18, core parity 22, store 10, vision 15+ and fixtures),
`npm run build` and `npm test` in `rust/frontend`.

Every Python route now has a Rust command (see the table in section 3). Codex added: album operations (`rescore`,
`pick_keepers`, `group`/`regroup`, `bulk_edit`, `rename_job`, `summary`, `cover`, `filling`), cache and the three
resets, known-vehicle CSV import/export/seed, entry-list import, `health`, folder watch, in-app update check/install,
scan `stage` (index | cull | all | identify) and `recursive`, tray icon with close-to-tray, and CLI commands
(`jobs`, `scan`, `identify`, `write`, `command <action> <json>`).

### Verified live (release exe, isolated `CONROD_HOME`, 14 real JPEGs copied from job 15)

Scan with `stage: all` (cull, then identify with the local Ollama `qwen2.5vl:7b`), then: summary, cover, review,
`pick_keepers` (33 keepers of 50), `rescore`, `group`, `rename_job` (trim, blank restores the folder name), single and
bulk edit, `cache_info`/`cache_clear`, `seed_known`/`export_known`/`import_known`, `import_entries` (empty CSV rejected),
`health` (models plus the vision model), dry-run `write`, `scan` stage `index` then `cull` on the indexed album,
`reset_identifications` (kept OCR numbers and reviewed detections), `reset_detections`, `install_update` refusing a
non-installed copy, and **folder watch**: a JPEG dropped into the watched folder was picked up after settling and the
album resumed through the saved stage; deleting a watched album stopped the watch and removed it from settings.

**Real NSIS installer** (built 20 Sept with `node ../../frontend/node_modules/@tauri-apps/cli/tauri.js build` from
`rust/crates/conrod-app`, as CI does; Tauri downloaded NSIS 3.11 and the WebView2 bootstrapper): `Conrod_0.1.0_x64-setup.exe`,
93.7 MB. Silent `/S` install took 11 s, per-user with no admin prompt, into `%LOCALAPPDATA%\Conrod` (`Conrod.exe`,
`uninstall.exe`, `resources` with the 7 model files and ExifTool), a Start Menu shortcut and an Add/Remove Programs entry
(Conrod 0.1.0, kapsikkum). `Conrod.exe --selftest` passes from the installed folder and the installed app reports no
missing models. Silent uninstall removed the folder, shortcut and Add/Remove entry.

**In-app update with the real installer**: a local fake release server (`rust-v9.9.9`, `SHA256SUMS.txt`) served the real
installer with `CONROD_UPDATE_API`; the installed app saw it as installable, downloaded and verified it, quit, NSIS
reinstalled silently **in place** (same folder, nothing written to the default location), and the app relaunched. A wrong
checksum failed the task with the app still running and nothing kept.

Testing notes: pass **long paths** to `/D=` (an 8.3 path such as `KAPSIK~1` makes the uninstaller's shortcut comparison
fail and leaves a dead Start Menu shortcut); the uninstaller keeps `HKCU\Software\kapsikkum\Conrod` (the remembered
install folder) unless the "delete app data" box is ticked, so a later install reuses the old folder; remove that key
after a test install into a temp folder.

Screens checked in the real window: Library, Review (with Album tools and bulk bar), Scan (entry-list picker), Vehicles
(CSV buttons), Settings (Maintenance card), activity popover.

### Fixed in this pass (on top of Codex's work)

Library cover text shows only when there is no cover (as the Python home screen); inspector preview no longer squashed by
the flex column; styled `FilePick` button instead of the browser's file control (Scan, Vehicles); **Write XMP now asks
for confirmation** (what will be written and whether ratings/labels are overwritten) and a **Dry run** button exists
(Python had both); "0 of 0" hidden in the activity list; `identify`/`write` on a missing album say "No such album";
closing the window reads one setting (`Desktop::close_to_tray`) instead of building a bootstrap; tray Quit and window
close stop the scan through the typed `Command::Stop`.

Two real bugs found by comparing with Python on private copies of the real library (`tools/parity_ops.py`):
**grouping used a same-car threshold of 0.8 where Python uses 0.90** (`grouping::SAME_CAR`), so albums merged cars that
Python keeps apart; fixed in `conrod-engine/src/passes.rs`, and job 39 now reproduces Python's 36 groups exactly (was 23).
**The update launcher could never relaunch the app**: it started PowerShell with `DETACHED_PROCESS`, which leaves it
without a console, and `Start-Process -Wait` needs one. It now uses a hidden console, always relaunches the app in a
`finally`, and logs a failure next to the installer (`update.rs`).

### Parity measured on real data (private copies of `~/.conrod/conrod.db`; `python tools/parity_ops.py <mode> <job>`)

| Operation | Job | Result |
| --- | --- | --- |
| `pick_keepers` | 38 (11,330 detections) | 207 of 207 keepers identical to Python's |
| `pick_keepers` vs Python's own `pick_of_pass` | 95 (mixed shoot) | 22 of 24 units identical. In the other two Python keeps a *person* crop (0.63, 0.99) over the car (0.61, 0.98). Rust ranks only the subject kind the profile rates the frame by (`passes.rs`), a deliberate difference; **decided 20 Sept: keep Rust's rule**. The 32 keepers stored in job 95 came from the cull, not `pick_of_pass`. |
| `group` | 39 (186 detections) | 36 of 36 groups identical after the threshold fix |
| `rescore` | 95 (only job with stored features) | 0 of 728 differ after tampering 200 rows first; jobs 15, 38 and 39 have no stored features, so old Python albums cannot be re-measured (Python cannot either) |

**Assets published**: `assets-v1` is live at `github.com/kapsikkum/conrod/releases/tag/assets-v1` (8 files, created as a
**pre-release with `--latest=false`**: the Python updater reads GitHub's latest release, still `v0.8.0`, and would
otherwise have offered the assets release to every Python install). Each file was re-downloaded from its public URL and
matched the manifest size and SHA-256; a first run on an empty library installed the missing models from it in 17 s.

**Updater and version**: the app is now `1.0.0-beta.1`. The updater accepts `rust-v*` and plain `v*` tags but only
releases that carry a `-win64-setup.exe`, so the Python zips and the asset files are never offered (test:
`plain_v_tags_are_updates_once_they_carry_an_installer`). A beta therefore can move to the final `v1.0.0`.

## 2. Not yet verified or not yet done

1. **Installer**: built and tested locally (section 1). Still to do: a tagged pre-release through `release-rust.yml`
   (needs the commit and push) and one install from the GitHub release on another machine.
   Tauri names the file `Conrod_<v>_x64-setup.exe`; the workflow renames it to `Conrod-<v>-win64-setup.exe`, which is what
   the updater looks for. An installed copy running while the installer runs is not tested (the updater quits first).
2. **Parity still unchecked**: `summary`, `seed_known` (Python's consensus rules), the known-vehicle CSV column
   set and the entry-list format, `bulk_edit` semantics, and `identify` with `stage: all` on a whole album. Method in
   section 6 (private copy of the DB, never the real one). Note the CLI `command` waits for background operations, but
   a check must tamper first to prove an operation ran (`tools/parity_ops.py rescore` does).
3. **UI gaps**: write dry-run output appears only in the event log (Python toasted counts and what would be
   kept); chip-style team/livery editing; drag-a-folder-to-scan and the taskbar progress bar are not visually
   verified; tray hide/restore and single-instance behaviour are not exercised; review refresh is still
   refetch-per-action rather than event-driven for edits.
4. **Accuracy** (unchanged): 9% plate-text and 8% number disagreement vs RapidOCR, RapidOCR's angle classifier not
   ported, plates run on CPU because DirectML rejects a `Resize` node, faces/eyes unvalidated on real portraits,
   pan/shake/focus-miss unchecked. Stale local fixtures (`detector_local`, `faces_local`, `vision_local`) pass by
   skipping; rebuild with `tools/gen_faces_local.py`, `tools/gen_vision_local.py` via `tools/local_frames.py`.
   Opt-in real-photo gates: `cargo test --release -p conrod-vision -- --ignored`.
5. **Idiom review still open**: `Result<_, String>` everywhere (keep `String` at the Tauri boundary, type the library
   crates where callers branch; `VlmError` is the model); `rows()` builds `serde_json::Value` for every query, so the
   review payload should be typed `Serialize` structs; generate the TypeScript types from the Rust argument structs;
   decide whether Pillow-exact resize/crop in `conrod-vision::imageops` is still needed (the parity numbers depend
   on it: re-measure before changing). `import_entries` writes a new timestamped file per import and never removes
   the old ones.
6. **CLI** lacks Python's `review` (there is no web server by design) and `--map/--vlm-*/--detect-*` flags; use
   `command save_settings` instead, or add flags if a headless path must survive.

## 3. Route parity (`conrod/server.py`)

| Python | Rust command | State |
| --- | --- | --- |
| settings, pick-folder | `bootstrap`, `save_settings`, Tauri `choose_folder` | done |
| setup, health, setup/fix | `health`, `install_models` | done; compare with `conrod/setup_check.py` for missing checks |
| scan (stage, recursive, resume) / stop / pause / resume / log | `scan`, `stop`, `pause`, `resume_scan`, `status.log` | done |
| jobs list, delete, rename, summary, cover, filling | `jobs`, `delete_job`, `rename_job`, `summary`, `cover`, `filling` | done, parity unchecked |
| frames, detections, mark, edit, bulk edit | `review`, `mark`, `edit_detection`, `bulk_edit` | done, parity unchecked |
| group, regroup, rescore, pick | `group`, `regroup`, `rescore`, `pick_keepers` | done, parity unchecked |
| cache, cache/clear, resets | `cache_info`, `cache_clear`, `reset_*` | done (deletes only files Conrod cut in `cache/native`) |
| known list/save/delete, seed, csv | `known`, `save_known`, `delete_known`, `seed_known`, `export_known`, `import_known` | done |
| entries | `import_entries` | done |
| training (status, label, undo, train, forget), taste | `training_status`, `train_label`, `undo_label`, `train_model`, `forget_model`, `train_taste` | done |
| write | `write` (`dryRun`, `embedInRaw`) | done |
| watch | `watch_status`, `set_watch` | done and verified live |
| update check/install | `check_update`, `install_update` | done; installer path unverified |
| browse, browse/count | none | not needed (native picker) |

## 4. Release (see RELEASING.md)

Ready: `assets-v1` published, pinned manifest `rust/scripts/assets.json`, `stage-assets.ps1`,
`fetch-release-assets.ps1`, `check-version.mjs`, `.github/workflows/release-rust.yml` (tag `rust-v*`, pre-release,
`SHA256SUMS.txt`, runs the shipped exe's selftest), `check.yml` (Rust job plus the Python job), the installer (built and
tested locally, unsigned by decision) and the version `1.0.0-beta.1`. Outstanding: commit and push, then tag
`rust-v1.0.0-beta.1`, watch the workflow, install the resulting setup.exe on another machine and unzip the portable build.

## 5. Replacing the Python app (cutover checklist)

1. Sign off parity (sections 2 and 3) on the same album in both apps.
2. Data: Rust opens the same `~/.conrod` (`conrod.db`, `settings.json`, `entries/`). Open a **copy** made from a
   Python-written DB and confirm nothing is lost, and that Python can still open a Rust-touched copy while both coexist.
3. **Migration path already exists**: Python's own updater installs a stable release's `Conrod-<v>-win64.zip` by
   swapping its `Conrod` folder for the zip's `Conrod/` folder (keeping `Conrod-previous`), which is the layout of the
   native portable zip, and `~/.conrod` carries over. So a stable `v1.0.0` (not pre-release, number above Python's
   0.8.x, zip + setup.exe + `SHA256SUMS.txt`) upgrades existing Python installs in place. Details and the cutover
   change list: RELEASING.md "Cutover". Native betas stay invisible to Python (pre-release, `rust-v` prefix).
4. Then delete, in one reviewed change: `conrod/` (incl. `web/`), `main.py`, `cli.py`, `conrod.spec`,
   `requirements.txt`, `smoke_test.py`, Python `tests/`, `.github/workflows/release.yml` and the Python job in
   `check.yml`; rewrite the root `README.md` and `docs/`. Keep `tools/*.py` while parity is re-measured (they import the
   Python package). Since the Python app goes away, the "one library opens in either app" constraint on
   `settings.json` keys and the database can be relaxed after the cutover.
5. Make the Rust workflow the main release path: also trigger on `v*` (publishing that tag as a normal release and
   letting `check-version.mjs` accept it); the updater already accepts `v*`.

## 6. How to verify

```bash
cd rust && node scripts/check-version.mjs && cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
(cd frontend && npm ci && npm run build && npm test)
cargo build --release -p conrod-app --features tauri/custom-protocol   # needs frontend/dist first
./target/release/Conrod.exe --selftest <a-jpeg>
```

- **App QA**: launch the release exe with `CONROD_HOME=<temp>`, `WEBVIEW2_USER_DATA_FOLDER=<temp>` and
  `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222`; drive it over CDP with
  `window.__TAURI_INTERNALS__.invoke('command', {action, args})`, and key events on `document.body`. Pass folder paths
  with forward slashes (the Bash tool eats backslashes). Copy a few JPEGs from job 15 to `%TEMP%`; never scan or write
  on `D:\`.
- **Updater against a fake release**: serve `/releases?per_page=15` (a `rust-v9.9.9` pre-release with
  `Conrod-9.9.9-win64-setup.exe` and `SHA256SUMS.txt`) on localhost, copy the release exe to a folder with a dummy
  `uninstall.exe` beside it, and start it with `CONROD_UPDATE_API=http://127.0.0.1:<port>`.
- **Parity on real data**: `python tools/parity_ops.py pick|pick-py|group|rescore <job>` (needs
  `cargo build --release -p conrod-cli`); it backs the real DB up into `%TEMP%` read-only and never writes to `~/.conrod`.
  Jobs: 15 fully identified (17,587 detections), 38 cull-only Museum (4,713 CR3, Python picks), 39 (Python groups),
  95 (only one with stored features).

## 7. Hard rules and quirks

- Never touch the installed Python Conrod in Downloads. **Both apps are named `Conrod.exe`**: stop only the pid you
  launched, never by name. Never modify `~/.conrod` (open it `mode=ro`, work on copies). Never run `write` on original
  photographs (the Museum folder holds 4,713 Python-written sidecars). Use at most one sub-agent at a time (a 13-agent
  fan-out burned the owner's usage limit in 20 minutes).
- Windows quirks: the Bash tool collapses `\\` and chokes on long heredocs, so write patch scripts to a file and run them
  (sources are CRLF: `replace("\r\n","\n")`, write back with CRLF); PowerShell's safety check rejects scripts containing
  `C:\Windows` or `/1MB`; cargo waits on the build-directory lock while another build runs; `vite build` empties
  `frontend/dist`, so do not run it while `cargo build --features tauri/custom-protocol` is embedding it.
