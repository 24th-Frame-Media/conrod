# Releasing the native Conrod

The native app replaced the Python app on 20 Sept 2026; the Python code (up to v0.8.0) is on the
`legacy-python` branch. Releases are plain `v*` tags built by `.github/workflows/release.yml`; a tag with a suffix
(`v1.0.0-beta.1`) is published as a pre-release, which GitHub does not treat as "latest". The in-app updater only
offers releases that carry an installer, so the old Python zips are never offered.

## What a release contains

| file | what |
| --- | --- |
| `Conrod-<v>-win64-setup.exe` | NSIS installer, per-user (no admin), WebView2 bootstrapper embedded |
| `Conrod-<v>-win64.zip` | portable: `Conrod/Conrod.exe`, `Conrod/resources/{models,exiftool}` |
| `SHA256SUMS.txt` | `sha256sum` format, both files |

The app looks for models in `~/.conrod/models` first, then `resources/models` beside the exe
(`conrod_core::models`), and for ExifTool in `resources/exiftool`, then `PATH`.
Missing models and ExifTool are installed at run time from the SHA-256-pinned manifest. Ollama and its vision model are configured separately.

## The pinned assets (published 20 Sept 2026 as `assets-v1`)

Models and ExifTool are pinned by SHA-256 in `scripts/assets.json`; CI refuses anything else. `assets-v1` is
published (8 files, each re-downloaded from its public URL and checked against the manifest; a first run on an empty
library installed the missing models from it). To publish a new set:

```powershell
powershell -File scripts/stage-assets.ps1        # collects the tested files, rewrites assets.json
gh release create assets-v2 dist/assets-v2/* --title "Conrod assets v2" --notes-file notes.md `
  --prerelease --latest=false
git add scripts/assets.json && git commit -m "Pin release assets"
```

**Always `--prerelease --latest=false`.** The Python app's updater reads GitHub's *latest* release; an assets release
that became "latest" would be offered to every Python install as an update. Repeat (editing `release` in the script)
only when a model or ExifTool changes.
Licences are recorded per asset in `assets.json`; ExifTool's own licence files ship inside
`resources/exiftool/exiftool_files`.

## Cut a beta

1. Bump the version: `Cargo.toml`'s `[workspace.package] version` is the single source of truth (Tauri and
   the frontend package both omit `version` and fall back to it). Run `node scripts/release.mjs --beta` (or
   `--patch`/`--minor`/`--major`/`--set X.Y.Z`, add `--tag` to commit and tag) rather than editing it by hand.
   `node scripts/check-version.mjs` must pass; CI runs it too. The native line continues above the Python
   app's 0.8.x, so the betas are `1.0.0-beta.N` and the first stable release is `1.0.0`.
   With `--tag`, it also regenerates `CHANGELOG.md` and `frontend/src/lib/whatsNew.json` for the version
   just set (see below) and includes them in the release commit.
2. On `main`, `git tag v1.0.0-beta.1 && git push origin v1.0.0-beta.1`.
3. `.github/workflows/release.yml` fetches the pinned assets, generates the release notes (below), builds
   the frontend, runs fmt, clippy and every test, builds the installer, assembles the portable folder,
   **self-tests the shipped `Conrod.exe`** (`Conrod.exe --selftest report.txt`: models found and loaded, a
   frame through detector + sharpness + OCR, ExifTool, database), checksums, and publishes the release (a
   pre-release when the version has a suffix). A failing self-test fails the release.
4. Check by hand on a clean profile: install, scan a folder, review, identify, write XMP on a copy.

## Release notes and "What's new"

One generator, `scripts/release-notes.mjs`, drives three things from the same git history: the GitHub
release body, `CHANGELOG.md`, and the in-app "What's new" sheet (Settings → **What's new**, and once by
itself the first time a new version is opened). Only user-facing commits count: `feat` → New, `perf` and
`security` → Improved, `fix` → Fixed; `chore`, `ci`, `test`, `docs`, `refactor`, dev-only scopes
(`scripts`, `ci`, `deps`, ...) and merge commits are dropped automatically, as is wording that reads as
developer-only ("refactored", "lint", "CI", ...).

A commit steers its own line with a `Release-note:` trailer in the body (`Release-note: Cull with the
camera's focus point.`, or `Release-note: skip` to leave it out). A commit already made is steered instead
from `scripts/release-notes.overrides.json`, keyed by a commit hash (7+ hex characters): a string replaces
the wording, `"skip"` drops it, `{ "section": "New", "text": "..." }` can also move it between New,
Improved and Fixed, and an array gives several notes for one commit. The same file's `"versions"` map
gives each release its one-line `highlights` (and can `add` notes with no commit of their own, or override
a tag's date). **Write for the photographer using Conrod, not the code**: say what changed in their
workflow, never how it was implemented.

```
node scripts/release-notes.mjs --audit [--from <ref>]   # commits that still need wording in overrides.json
node scripts/release-notes.mjs --unreleased             # last tag -> HEAD, as Markdown
node scripts/release-notes.mjs --version 1.0.0-beta.9   # one release's notes (range ends at its tag, or HEAD)
node scripts/release-notes.mjs --all --write            # CHANGELOG.md + frontend/src/lib/whatsNew.json
```

Before tagging, run `--audit` and add any new commits to the overrides file. `scripts/release.mjs --tag`
runs `--all --write --release <version>` for you and includes the regenerated files in the release commit,
so CHANGELOG.md and the in-app sheet are correct as soon as the tag is pushed. `release.yml` runs
`--version <tag>` itself (the checkout fetches full history so it can walk the tag range) and uses the
Markdown as the GitHub release body, with the install instructions appended after it; the same text goes
into `latest.json`'s `notes` field for the updater's "Release highlights".

Locally: `stage-assets.ps1`, `cargo build --release -p conrod-app --features tauri/custom-protocol`, copy
`target/release/Conrod.exe` next to `crates/conrod-app/resources`, run `Conrod.exe --selftest out.txt`.
Producing the installer locally needs the Tauri bundler, which downloads NSIS on first use.

## Upgrading Python installs

The Python app's updater reads GitHub's latest release (never a pre-release), offers the first `.zip` asset whose
name contains `win`, checks it against `SHA256SUMS.txt`, and swaps its `Conrod` folder for the archive's `Conrod/`
folder (keeping the old one as `Conrod-previous` and rolling back if the move fails), then reopens `Conrod.exe`.
The portable zip has exactly that layout, so a **stable** release moves existing Python installs onto the native
app, and `~/.conrod` (database, settings) carries over. It needs:

- a tag starting with `v` whose number is above the Python line (`v1.0.0`; the updater compares numbers only), not
  marked pre-release;
- assets `Conrod-<v>-win64.zip` (with `Conrod/Conrod.exe` inside), `Conrod-<v>-win64-setup.exe` and a
  `SHA256SUMS.txt` covering both (Python installs without a check when the file is missing; never ship without it).

`release.yml` publishes `v1.0.0` (no suffix) as a normal release, so it becomes GitHub's latest and reaches every
Python install. Betas (`v1.0.0-beta.N`) stay pre-releases and are invisible to them. The frozen Python updater
cannot be pointed at a test server, so the first stable release is the real test of that path; the Python swap keeps
the old build as `Conrod-previous` if it fails.

## Rollback

Delete the pre-release and its tag. Python users were never offered it. A user who installed the beta
keeps their `~/.conrod` untouched apart from additive tables (`native_labels`, `native_label_undo`) that
the Python app ignores, so switching back is safe.

## What only the owner can supply

- **No code signing, by decision (20 Sept 2026).** Releases stay unsigned, so SmartScreen asks users to confirm;
  the release notes say so. Do not add signing steps.
- Authorising the commit and push that a release needs (nothing is committed yet), and the go-ahead for the first
  tag.

## Local build

From `frontend`, run `npm ci` and `npm run tauri -- build --ci`.
The build hook runs TypeScript checking and Vite before compiling the app. NSIS is downloaded
by the official Tauri bundler on first use. Output: `target/release/bundle/nsis/`.
The release executable is `target/release/Conrod.exe`.

Settings → Maintenance provides setup checks, missing-model installation and update checking.
Update installation is available only for an installed copy, requires idle work, verifies the
release checksum and requests app exit after launching the installer helper. Portable builds
remain manual downloads.

## QA and Verification

### Installer Testing on Windows
- When testing silent installation (`/S /D=<dir>`), pass **long paths** to `/D=`. An 8.3 short path (such as `KAPSIK~1`) can cause uninstaller shortcut comparisons to fail and leave orphaned Start Menu shortcuts.
- The NSIS uninstaller preserves `HKCU\Software\kapsikkum\Conrod` (the remembered install folder) unless the "delete app data" box is ticked. Clear that registry entry if testing temporary installation paths.

### Testing the In-App Updater Locally
- To test the updater against a mock release server, run a local HTTP server that responds to `/releases?per_page=15` with a release JSON containing `Conrod-<version>-win64-setup.exe` and `SHA256SUMS.txt`.
- Copy the release executable to a folder with a mock `uninstall.exe` beside it and start the app with:
  ```powershell
  $env:CONROD_UPDATE_API = "http://127.0.0.1:<port>"
  .\Conrod.exe
  ```

### Isolated QA and Headless CDP Automation
- Launch the release executable with isolated temporary directories and an open debugging port:
  ```powershell
  $env:CONROD_HOME = "$env:TEMP\conrod-qa"
  $env:WEBVIEW2_USER_DATA_FOLDER = "$env:TEMP\conrod-webview"
  $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9222"
  .\Conrod.exe
  ```
- Commands can then be driven via Chrome DevTools Protocol or directly via `window.__TAURI_INTERNALS__.invoke('command', {action, args})`.

