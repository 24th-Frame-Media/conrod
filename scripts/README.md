# Release scripts

| script | who runs it | what it does |
| --- | --- | --- |
| `stage-assets.ps1` | maintainer, once per asset change | gathers the exact model files and ExifTool the port was tested with, writes `assets.json` (pinned SHA-256 + licence per asset), fills `crates/conrod-app/resources/` for a local build, and leaves `dist/assets-v1/*` to publish as a GitHub release named `assets-v1` |
| `fetch-release-assets.ps1` | CI | downloads the assets from that release into `resources/` and refuses any whose hash differs from `assets.json` |
| `check-version.mjs` | CI and developers | fails unless the workspace, `tauri.conf.json` and `frontend/package.json` agree on the version (and, in CI, the tag) |

The app resolves models in `~/.conrod/models` first, then `resources/models` beside the executable
(`conrod_core::models`), so a developer checkout does not need any of this.
