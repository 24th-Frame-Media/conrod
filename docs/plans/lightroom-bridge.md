# Conrod <-> Lightroom Classic bridge (2026-11-19)

Two-way, file-based, versioned. Owner: conrod (Rust). Consumer: a Lightroom Classic Lua plugin
(`lightroom-plugin/ConrodBridge.lrplugin`). Neither side needs the other running: packages are
written atomically and picked up later, so a 3,000-frame import cannot time out the way the old MCP
bridge did. Live selection is a plugin menu action, not a server.

## Folders

All under `%USERPROFILE%\.conrod\bridge\` (the app's data root; the plugin resolves the same path
from `%USERPROFILE%`).

| folder | written by | read by | purpose |
|---|---|---|---|
| `out\` | conrod | plugin | verdicts for Lightroom to apply (export) |
| `in\` | plugin | conrod | the photographer's flags/ratings (import) |
| `done\` | either | — | packages consumed, renamed `<name>.<unix>.done` |
| `failed\` | either | — | packages that could not be read, kept for support |

A reader only accepts `*.json`; `*.tmp` and anything else is ignored. Writes are
`<name>.json.tmp` then `fs::rename` / `LrFileUtils` rename to `<name>.json`, so a reader never sees
a partial file. After a successful read the package moves to `done\`.

## Envelope (both directions)

```json
{
  "format": "conrod.lightroom.bridge",
  "version": 1,
  "direction": "to-lightroom",
  "album": {
    "job_id": 109,
    "label": "Bathurst 1000 - Race Week",
    "root": "D:\\24th Frame Media\\02_Projects\\2026-10-06_Bathurst-1000\\01_RAW"
  },
  "created_at": "2026-11-19T16:00:00Z",
  "producer": { "app": "conrod", "version": "1.0.0-beta.8" },
  "collection": "Conrod keepers - Bathurst 1000",
  "photos": [ { ...photo... } ]
}
```

- `direction`: `"to-lightroom"` or `"to-conrod"`. A reader rejects a package for the other direction
  into `failed\`.
- `version`: integer; a greater version than the reader knows goes to `failed\` (never half-applied).
- `collection`: optional. When present and non-empty the plugin creates/updates that collection in a
  collection set named `Conrod` and adds the picked photos to it.
- Unknown fields are ignored. Photos are applied in order; a bad photo entry is skipped and counted,
  never aborts the package.

## Photo entry

```json
{
  "path": "D:\\24th Frame Media\\...\\20261006_B1000_0202.CR3",
  "rating": 5,
  "pick": 1,
  "label": "green",
  "keywords": ["Conrod keeper", "Conrod burst 145"],
  "stars": 5,
  "verdict": "sharp",
  "burst": 145,
  "keeper": true
}
```

| field | type | meaning |
|---|---|---|
| `path` | string | absolute path as the album stores it. Matching is case-insensitive, separators normalised. Required. |
| `rating` | int 0-5 \| null | Lightroom rating. `to-lightroom`: applied. `to-conrod`: read from the catalog. |
| `pick` | -1 \| 0 \| 1 \| null | Lightroom flag: -1 rejected, 0 unset, 1 picked. |
| `label` | `"red"\|"yellow"\|"green"\|"blue"\|"purple"\|null` | Lightroom colour label name. |
| `keywords` | string[] | `to-lightroom` only: keywords to add (never removed). |
| `stars`, `verdict`, `burst`, `keeper` | optional | conrod's own facts, carried for the plugin's summary dialog and future use; the plugin must not interpret them. |

## Matching (conrod, `to-conrod` packages)

1. Normalise the path and look for an album frame whose stored `path` equals it. If the path is not
   unique within the album (copies), apply only when exactly one row matches.
2. Fallback when no path matches: same file name **and** capture time within 2 s, applied only when
   exactly one album frame qualifies; otherwise count as `unmatched`.
3. Report counts: `matched`, `unmatched_photos`, `album_frames_without_catalog_entry`, and per-photo
   rejects. Nothing is guessed silently.

## What conrod does with an import

- Stores `images.catalog_rating`, `catalog_pick`, `catalog_label`, `catalog_at` (new columns,
  migration). Never overwrites conrod's own `stars`/`picked`/`rejected`: the catalog is a second
  opinion and a training label, not a command.
- "Matched your Lightroom picks" = share of keepers (effective keeper, as identification defines it)
  that carry `catalog_pick = 1`, reported per album in Shoot statistics and in readiness output.
- Training: `catalog_pick` + `catalog_rating` are the preferred labels for the taste head and the
  sharpness trainer when present.

## What conrod puts in an export

- Exactly the visible, non-copy frames of the album, each with `rating` (conrod's stars, 0-5),
  `pick` (picked 1, rejected -1, else 0), `label` (green for keeper, red for rejected, yellow for
  "review" when the frame has `needsLook`), `keywords` (`Conrod keeper`, `Conrod cull`,
  `Conrod burst <n>` when in a burst), and `stars`/`verdict`/`burst`/`keeper` for the dialog.
- Written after "Write XMP" so sidecars and catalog agree; the package is additive.

## Plugin behaviour

Menu (under `Library > Plug-in Extras`):

| item | effect |
|---|---|
| **Send flags to Conrod** | Reads `rating`, `pickStatus`, `colorNameForLabel` for the selection, or for the whole folder when nothing is selected, writes `in\<album>-<unix>.json`, shows a summary. |
| **Apply verdicts from Conrod** | Reads the newest `out\*.json` (or the one chosen), shows `matched / missing / skipped` first, then applies on confirmation inside one `withWriteAccessDo` per chunk of 50 photos, with a progress dialog and Cancel. Skipped (missing on disk) photos are counted, not fatal. |
| **Show pending packages** | Lists `out\` with album label, photo count and age. |

Rules: never delete or move files; never touch develop settings; never write a rating unless the
package carries one (a null leaves the catalog's value alone); tolerate `.lrdata` previews missing;
work on Lightroom Classic 6 and later; one write chunk at a time so the catalog lock is never held
for long; all dialogs use `LrDialogs`, progress uses `LrTasks` + `LrProgressScope`.

## Install

Copy `lightroom-plugin/ConrodBridge.lrplugin` into Lightroom's Modules folder (or add via
`File > Plug-in Manager > Add`). No signing, no network, no credentials. `README.md` next to it
carries the steps and the exact package folder path.

## Not in scope

Writing the `.lrcat` directly (unsupported, locks while Lightroom runs). Develop settings, exports
and publishing services. The cloud version of Lightroom: Classic only.
