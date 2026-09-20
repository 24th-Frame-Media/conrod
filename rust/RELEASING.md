# Releasing the native Conrod

The native app replaces the Python app (owner, 20 Sept 2026). Until the cutover Python keeps shipping `v*`
releases (`.github/workflows/release.yml`; its latest is v0.8.0) and the native app releases on tags `rust-v*`,
always as a **pre-release**, which the Python app's "latest release" updater cannot see. At the cutover the native
app publishes plain `v*` releases (see "Cutover" below). The native updater accepts both tag styles, and only
releases that carry an installer.

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

Models and ExifTool are pinned by SHA-256 in `rust/scripts/assets.json`; CI refuses anything else. `assets-v1` is
published (8 files, each re-downloaded from its public URL and checked against the manifest; a first run on an empty
library installed the missing models from it). To publish a new set:

```powershell
powershell -File rust/scripts/stage-assets.ps1        # collects the tested files, rewrites assets.json
gh release create assets-v2 rust/dist/assets-v2/* --title "Conrod assets v2" --notes-file notes.md `
  --prerelease --latest=false
git add rust/scripts/assets.json && git commit -m "Pin release assets"
```

**Always `--prerelease --latest=false`.** The Python app's updater reads GitHub's *latest* release; an assets release
that became "latest" would be offered to every Python install as an update. Repeat (editing `release` in the script)
only when a model or ExifTool changes.
Licences are recorded per asset in `assets.json`; ExifTool's own licence files ship inside
`resources/exiftool/exiftool_files`.

## Cut a beta

1. Bump the version in **three** places: `rust/Cargo.toml` (`[workspace.package]`),
   `rust/crates/conrod-app/tauri.conf.json`, `rust/frontend/package.json` (and `package-lock.json`).
   `node rust/scripts/check-version.mjs` must pass; CI runs it too. The native line continues above the Python
   app's 0.8.x, so the betas are `1.0.0-beta.N` and the first stable release is `1.0.0`.
2. Merge to `main`, then `git tag rust-v1.0.0-beta.1 && git push origin rust-v1.0.0-beta.1`.
3. `.github/workflows/release-rust.yml` fetches the pinned assets, builds the frontend, runs fmt, clippy and
   every test, builds the installer, assembles the portable folder, **self-tests the shipped `Conrod.exe`**
   (`Conrod.exe --selftest report.txt`: models found and loaded, a frame through detector + sharpness + OCR,
   ExifTool, database), checksums, and publishes the pre-release. A failing self-test fails the release.
4. Check by hand on a clean profile: install, scan a folder, review, identify, write XMP on a copy.

Locally: `stage-assets.ps1`, `cargo build --release -p conrod-app --features tauri/custom-protocol`, copy
`target/release/Conrod.exe` next to `crates/conrod-app/resources`, run `Conrod.exe --selftest out.txt`.
Producing the installer locally needs the Tauri bundler, which downloads NSIS on first use.

## Cutover: replacing the Python app

The Python app's updater reads GitHub's latest release (never a pre-release), offers the first `.zip` asset whose
name contains `win`, checks it against `SHA256SUMS.txt`, and swaps its `Conrod` folder for the archive's `Conrod/`
folder (keeping the old one as `Conrod-previous` and rolling back if the move fails), then reopens `Conrod.exe`.
The portable zip has exactly that layout, so a **stable** release moves existing Python installs onto the native
app, and `~/.conrod` (database, settings) carries over. It needs:

- a tag starting with `v` whose number is above the Python line (`v1.0.0`; the updater compares numbers only), not
  marked pre-release;
- assets `Conrod-<v>-win64.zip` (with `Conrod/Conrod.exe` inside), `Conrod-<v>-win64-setup.exe` and a
  `SHA256SUMS.txt` covering both (Python installs without a check when the file is missing; never ship without it).

Nothing publishes a stable release by accident: `release-rust.yml` triggers on `rust-v*` and marks it pre-release.
When the owner decides to cut over, in one reviewed change: delete the Python app (`conrod/`, `main.py`, `cli.py`,
`conrod.spec`, `requirements.txt`, `smoke_test.py`, `tests/`, `.github/workflows/release.yml`, the Python job in
`check.yml`), let `release-rust.yml` also trigger on `v*` and publish that tag as a normal release, and rewrite the
root README. The frozen Python updater cannot be pointed at a test server, so the first stable release is the real
test of that path; the Python swap keeps the old build as `Conrod-previous` if it fails.

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

From `rust/frontend`, run `npm ci` and `npm run tauri -- build --ci`.
The build hook runs TypeScript checking and Vite before compiling the app. NSIS is downloaded
by the official Tauri bundler on first use. Output: `rust/target/release/bundle/nsis/`.
The release executable is `rust/target/release/Conrod.exe`.

Settings → Maintenance provides setup checks, missing-model installation and update checking.
Update installation is available only for an installed copy, requires idle work, verifies the
release checksum and requests app exit after launching the installer helper. Portable builds
remain manual downloads. The silent upgrade and relaunch were exercised on 20 Sept 2026 with the real NSIS
installer served by a local fake release server (`CONROD_UPDATE_API`).

