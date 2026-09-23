# Conrod

Vehicle keywording for motorsport and car photography. Point it at a folder of
frames; it finds cars, bikes and people, measures focus on the subject, picks
the keeper of each pass, reads competition numbers, plates and livery text,
works out make, model and colour, and writes it all into XMP for Lightroom,
Bridge, Photo Mechanic and Capture One.

Conrod is a native Windows app (Rust, Tauri and React). The earlier Python
version, up to v0.8.0, lives on the
[`legacy-python`](../../tree/legacy-python) branch.

## Install

Download the latest release from [Releases](../../releases): the
`-win64-setup.exe` installer (per user, no admin rights) or the portable
`-win64.zip` (unzip, run `Conrod.exe`). Builds are not code-signed, so Windows
SmartScreen asks you to confirm.

The models and ExifTool come with the release. If one is ever missing Conrod
downloads it on the next scan, checked against a pinned SHA-256. Settings →
Maintenance checks the setup, installs what is missing and looks for updates.

Optional: [Ollama](https://ollama.com/) and `ollama pull qwen2.5vl:7b` to name
make, model, colour and team. Without it Conrod still culls, and reads plates,
numbers and livery text.

Coming from the Python version: your library in `%USERPROFILE%\.conrod`
(database, settings, trained models) opens as it is.

## The entry list

Optional. A CSV with a `number` column; every other column becomes a keyword,
so a bare two-column grid and a full entry list both work with no
configuration (there is an example in [`samples/`](samples/entries-example.csv)):

```csv
number,driver,team,class,sponsor
88,Broc Feeney,Triple Eight Race Engineering,Supercars,Red Bull
```

A frame with car 88 gets keyworded `88`, `#88`, `Car 88`, `Broc Feeney`,
`Triple Eight Race Engineering`, `Supercars`, `Red Bull`. A cell can hold
several values separated by `;` or `,`. Where the entry list names a number,
it beats anything read off the car's own panels.

## Good to know

- **Non-destructive.** RAW frames get an `.xmp` sidecar; the original file is
  never touched. Only JPEGs are written to directly. Keywords are replaced on
  every write, so scanning the same shoot twice does not stack duplicates. A
  dry run shows what would be written first.
- **Trainable sharpness.** The Train tab shows crops and asks how sharp the
  car is, 1-5 (judge the car, not the background; a crisp car on a streaked
  pan is a 5). After about 60 ratings, *Learn from my ratings* fits a small
  model, checks it against ratings it has not seen, and only switches it on if
  it beats the built-in measure.
- **Canon-first.** Tested throughout on `.cr3`/`.cr2`. `.jpg`/`.jpeg` are
  fully supported. Other RAW is not read yet; open an issue with a few sample
  frames if you shoot something else.
- **Folder watch.** Point it at a card that is still copying and it picks up
  each frame once it has stopped changing.
- **Data lives in `%USERPROFILE%\.conrod`**: previews, the job database,
  settings and trained models.

## Models

Each stage uses the model that is good at it, rather than asking one model to
do everything:

| Task | Model | Why |
|---|---|---|
| Vehicle detection | YOLO11s | Small and fast |
| Plate detection | [open-image-models](https://github.com/ankandrew/open-image-models) | 7.5 MB; also boxes competition-number roundels, which read far better than OCR across the whole car |
| Plate OCR | fast-plate-ocr | Trained on plates specifically: 18/18 test crops correct vs 5/18 for general OCR, and about 40x faster |
| Number and livery text | PP-OCRv4 (the RapidOCR models) | General-purpose, for anything that is not a plate |
| Faces | YuNet | For portrait sessions |
| Grouping one car across a burst | dinov2-small (quantized) | Cheap visual similarity to merge crops of the same vehicle |
| Make, model, colour, team | qwen2.5vl:7b via Ollama | See below |

### Why qwen2.5vl:7b

Every vision-language model that fits in 8 GB VRAM, same 13 real crops, same
prompt:

| model | sharp crops correct | per crop |
|---|---|---|
| **qwen2.5vl:7b** | **11 / 11** | 2.7 s |
| gemma3:4b | 4 / 11 | 5.6 s |
| minicpm-v:8b | 3 / 11 | 6.4 s |
| qwen3-vl:8b | worse | 2.6 s |

Not close. qwen3-vl is newer and worse here, and puts its answer in a
`thinking` field a normal reader sees as empty. The vision model also cannot
read plate characters at any resolution tried, which is why plate reading is
a separate detector and OCR pair rather than one more thing asked of the VLM.

## Build

Needs Rust (MSVC), the Visual Studio C++ build tools, Node.js and WebView2.

### TL;DR: run, test and compile

Run these from the repository root in PowerShell:

```powershell
npm --prefix frontend ci
npm --prefix frontend run tauri -- dev       # launch a development build

cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
npm --prefix frontend test
npm --prefix frontend run build              # type-check and compile the UI

npm --prefix frontend run tauri -- build --ci # release installer
```

The installer is written under `target/release/bundle/nsis/`.

### Build from the frontend directory

```powershell
cd frontend
npm ci
npm run dev                    # Vite on http://localhost:1420
npm run tauri -- build --ci     # installer: target/release/bundle/nsis/
```

[`API.md`](API.md) lists every command the desktop frontend and CLI use; [`RELEASING.md`](RELEASING.md) details release tagging and bundling.

## Contribute

```
scan -> preview -> detect -> plate/number/text -> identify (VLM) -> merge -> review -> write
```

Everything runs per vehicle, not per frame, which is what stops a trackside
banner being keyworded onto every car that passes it. The crates in
[`crates`](crates): `conrod-core` (pure logic), `conrod-vision`
(detector, plates, OCR, faces, similarity), `conrod-io` (RAW, ExifTool, VLM,
assets, updates), `conrod-store` (SQLite), `conrod-engine` (the operations),
`conrod-app` (the Tauri app) and `conrod-cli`.

Issues and PRs welcome.

### Releasing

Push a tag and CI builds the Windows installer and portable zip, self-tests the
shipped exe and attaches them, with checksums, to a GitHub Release:

```bash
git tag v1.0.0 && git push origin v1.0.0
```

A tag with a suffix (`v1.0.0-beta.1`) is published as a pre-release. See
[`RELEASING.md`](RELEASING.md).

## Licensing

Detection uses Ultralytics YOLO, which is **AGPL-3.0**: anyone distributing a
build must make source available on the same terms. The plate detector
([open-image-models](https://github.com/ankandrew/open-image-models)) is MIT.
Licences of every downloaded model and ExifTool are listed in
[`scripts/assets.json`](scripts/assets.json).
