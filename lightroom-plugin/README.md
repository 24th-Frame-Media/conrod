# Conrod Bridge — Lightroom Classic plug-in

A two-way, file-based bridge between Lightroom Classic and the conrod
photo-culling app. Neither app has to be running while the other works:
packages are written atomically and picked up later, so a 3,000-frame import
cannot time out.

The plug-in lives in `ConrodBridge.lrplugin/`. It is plain Lua (Lightroom
Classic's Lua 5.1): no network, no credentials, no signing, no external
modules.

## Install

Conrod installs the plug-in for you: **Settings → Lightroom → Install plug-in**
copies this folder into `%APPDATA%\Adobe\Lightroom\Modules\` (macOS:
`~/Library/Application Support/Adobe/Lightroom/Modules/`) and keeps it current
when the app updates. Restart Lightroom Classic after an install or update.

By hand:

1. In Lightroom Classic: **File > Plug-in Manager > Add**, then choose
   `lightroom-plugin/ConrodBridge.lrplugin`.
2. Or copy the `ConrodBridge.lrplugin` folder into
   `%APPDATA%\Adobe\Lightroom\Modules\` and restart Lightroom Classic.
3. The three commands appear under **Library > Plug-in Extras**.

Requires Lightroom Classic 6 or later. `Info.lua` targets `LrSdkVersion = 6.0`
and `LrSdkMinimumVersion = 6.0`, so the same bundle runs on current releases
without claiming newer APIs.

## Folders

All under `%USERPROFILE%\.conrod\bridge` (resolved from `USERPROFILE`; if that
is missing the plug-in falls back through `LrPathUtils` to the catalog's
folder). The plug-in creates `in\` and `out\` when needed.

| folder | written by | read by | purpose |
|---|---|---|---|
| `in\` | this plug-in | conrod | the photographer's ratings, flags and labels |
| `out\` | conrod | this plug-in | verdicts to apply to the catalog |
| `done\` | either | — | consumed packages, renamed `<name>.<unix>.done` |
| `failed\` | either | — | packages that could not be read or validated |

Only `*.json` is read; `*.tmp` is ignored, so a partial write is never
mistaken for a package. After a successful import the package is moved to
`done\`. A package that fails validation is moved to `failed\` with a message.

## What each menu item does

### Send flags to Conrod

Reads the current selection (`catalog:getTargetPhotos()`; with nothing selected
Lightroom returns the whole filmstrip/folder), reads `path`, `rating`,
`pickStatus`, `colorNameForLabel` (plus `fileName` and `dateTimeOriginal`),
and writes `in\<album>-<unix>.json` with `direction: "to-conrod"`.

- Every photo entry carries `path`, `rating`, `pick`, `label`; unknown values
  are JSON `null`, never invented. `colorNameForLabel` values `none`/`grey`
  become `null`.
- One entry per file: if a master and a virtual copy of it are both selected,
  the master's flags are sent, so conrod's path matching is never ambiguous.
- Entries are sorted by normalised path, so package diffs stay readable.
- Written as `<name>.json.tmp` and renamed into place.
- Shows `Wrote N flags (M picked, K rejected, R rated)` and the file path.
- Writes nothing to the catalog and touches no develop settings.

### Apply verdicts from Conrod

Lists `out\*.json` newest first (album label, photo count, age) and lets you
choose one; the newest is preselected. **Check only** validates and reports
without changing anything.

On **Apply** the plug-in:

1. Validates `format`, `version` (1) and `direction: "to-lightroom"`. Anything
   else is moved to `failed\` with the reason — never half-applied.
2. Matches by absolute path (lower-case, `/` and `\` normalised). Only master
   photos are matched; virtual copies share their master's path and are
   skipped so the verdict lands on the photo you see.
3. Shows a preflight: **matched / not in this catalog / skipped (no verdict)**.
4. Applies in chunks of 50 inside
   `catalog:withWriteAccessDo("Conrod verdicts", …, { timeout = 60 })`, with a
   progress dialog and Cancel between chunks.
   - `rating` only when the entry carries one.
   - `pickStatus` (-1/0/1) only when the entry carries one.
   - `colorNameForLabel` only when the entry carries one.
   - `keywords` are added (`photo:addKeyword`), never removed.
   - `collection`, when present: found or created inside a collection set named
     `Conrod`, and the **picked** photos of the chunk are added to it.
5. On success moves the package to `done\<name>.<unix>.done`.
   On failure or Cancel it stays in `out\` and the dialog reports exactly what
   was applied. Re-running is safe: setting the same values again and
   re-adding keywords/photos is idempotent.

Per-photo errors are caught and counted; one bad entry does not abort the
package.

### Show pending packages

Lists `out\` newest first with album label, photo count and age, then reveals
the folder in Explorer.

## Package format

```json
{
  "format": "conrod.lightroom.bridge",
  "version": 1,
  "direction": "to-conrod",
  "album": { "label": "Bathurst 1000 - Race Week", "root": "D:\\…\\01_RAW" },
  "created_at": "2026-11-19T16:00:00Z",
  "producer": { "app": "conrod-bridge", "version": "1.0.0" },
  "photos": [
    { "path": "D:\\…\\20261006_B1000_0202.CR3",
      "rating": 5, "pick": 1, "label": "green" }
  ]
}
```

`to-lightroom` packages may also carry `collection`, per-photo `keywords`, and
conrod's own `stars`, `verdict`, `burst` and `keeper` fields. The plug-in
ignores `stars`/`verdict`/`burst`/`keeper`; the rest are applied as described
above. Unknown fields are ignored. The full definition is
`docs/plans/lightroom-bridge.md` in the conrod repository.

## Self-test

`Json.lua` (encoder/decoder) is covered by `Selftest.lua`:

```powershell
lua Selftest.lua        # or luajit Selftest.lua
```

It prints one line per test and exits non-zero on failure. To run the same
tests inside Lightroom, temporarily add this to `LrLibraryMenuItems` in
`Info.lua`, reload the plug-in in the Plug-in Manager, and use the menu item:

```lua
{ title = 'Run self-test', file = 'MenuSelftest.lua' },
```

## Safety

- Never deletes, renames or moves a photo file.
- Never changes develop settings, and never writes a rating, flag or label the
  package does not carry.
- Catalog write locks are held for 50 photos at a time, never for a whole
  package.
- Missing previews or offline photos are not fatal: they are counted, and only
  the photos present in the catalog are changed.
