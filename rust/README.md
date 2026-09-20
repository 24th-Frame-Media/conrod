# Conrod native

Rust + Tauri 2 + React desktop application. It replaced the Python app, which lives on the `legacy-python`
branch; the parity fixtures in `fixtures/` and the checks against real libraries were generated from it (see
`fixtures/README.md`).

## Build

Install Rust (MSVC), Visual Studio C++ Build Tools, Node.js and WebView2. Then:

```powershell
cd rust/frontend
npm ci
npm run tauri -- build --ci
```

The installer is under `rust/target/release/bundle/nsis/`; the executable is
`rust/target/release/Conrod.exe`. Bundled assets are staged by `scripts/stage-assets.ps1`.
Missing assets install automatically, verified against the pinned manifest.

## Development and checks

```powershell
cd rust/frontend
npm run dev
# in another terminal
npm run tauri -- dev
# from rust/
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
# from rust/frontend
npm run build
npm test
```

Use `?mock=1` in the Vite URL for browser-only layout work. Set CONROD_HOME to a temporary
library before running the app or CLI in tests. Never use original photographs for write tests.

## Workflow

Scan chooses a folder, optional entry-list CSV, subfolder recursion, subject profile and stage.
Index only creates an album; thumbnails fill when opened. Cull measures focus and finds keepers.
Cull and identify also reads plates/numbers and identifies cars. Review supports stars,
rejects, subject edits, bulk selection, grouping, rescoring, summary and persistent folder watching.
Known vehicles can be edited, imported/exported as CSV or built from identified albums.
Settings contains model health, installation, cache management, resets and updates.

## CLI

`conrod-cli jobs`, `scan <folder> [--stage index|cull|all] [--no-recurse]`,
`identify <job-id>`, `write <job-id> [--dry-run] [--embed-in-raw]`,
`install-models`, `selftest`, and `command <action> <json-args>` share the native engine.
`cull <folder> [--profile ...] [--limit N]` remains the JSON-lines parity benchmark.
See [API.md](API.md) for commands and [RELEASING.md](RELEASING.md) for publishing.
