# Lightroom integration

**November 2026**

[Home](Home) · Previous: [Identify](Identify) · Next: [Settings reference](Settings-reference)

The Conrod Bridge plug-in connects Conrod and Lightroom Classic in both directions. It is file-based: neither app has to be running while the other works, so a 3,000-frame import cannot time out.

## It installs itself

The bridge is **on by default**. On start, and whenever you save Settings, Conrod copies the bundled plug-in into Lightroom's Modules folder:

`%APPDATA%\Adobe\Lightroom\Modules\ConrodBridge.lrplugin`

- Settings > **Lightroom** shows whether the plug-in is installed, its version, and whether a newer bundled version is waiting.
- **Install plug-in** copies it now; **Remove plug-in** removes only Conrod's folder.
- **Restart Lightroom Classic** after an install or update.
- A start only updates a stale plug-in when the bridge is on. It never removes anything.

If you prefer, add it by hand: **File > Plug-in Manager > Add** and choose the `ConrodBridge.lrplugin` folder. Lightroom Classic 6 or later is supported.

## The three menu items

Under **Library > Plug-in Extras**:

### Send flags to Conrod

Reads the selected photos' rating, pick flag, and colour label, and writes them to Conrod's `bridge\in` folder. Use it when you have culled or starred in Lightroom and want Conrod to know.

### Apply verdicts from Conrod

Lists packages Conrod wrote, newest first. **Check only** validates without changing anything. **Apply**:

- matches by absolute path (lower-case, slashes normalised), master photos only;
- writes a rating, pick flag, or colour label only when the package carries one;
- adds keywords, never removes them;
- finds or creates a collection in a set named **Conrod** and adds the picked photos.

Ratings and colour labels are written only when the photo has none, unless the package allows overwriting. A pick flag is set only on an unflagged photo and is never cleared. A bad entry is counted, not fatal.

### Show pending packages

Lists packages waiting in `out\` and reveals the folder.

## From Conrod to Lightroom

In Review, **Export to Lightroom** writes one package to:

`%USERPROFILE%\.conrod\bridge\out\<album>-<unix>.json`

It carries one entry per visible, non-copy, done frame:

- `rating` — Conrod's 0–5 stars.
- `pick` — 1 for a hand pick, -1 for a reject or an auto-cull under *respect culling*, else 0.
- `label` — green for a keeper, red for rejected, yellow for "needs look".
- keywords — `<prefix> keeper`, `<prefix> cull`, `<prefix> burst <n>`.
- a collection name.

Suggested stars, picks, and colour labels therefore land in the catalog without retyping. The package is written to a temporary name and renamed, so the plug-in never sees half a file.

## From Lightroom to Conrod

Two ways:

1. **Send flags to Conrod** writes `bridge\in\<album>-<unix>.json`. In Conrod, the album's Lightroom import consumes it and applies the flags to matching frames. Consumed packages move to `done\`; unreadable ones move to `failed\` with a reason.
2. **Read a catalog** directly: point the import at a Lightroom Classic `.lrcat`. It is opened read-only. Matching is by normalised path, then file name plus capture time within two seconds, and only when exactly one album frame qualifies.

The import reports matched, unmatched, missing, the percentage of effective keepers carrying a pick, skipped, and failed. Only catalog rating/pick/label columns are written, and only when a value changes.

## Folder layout and TO-IMPORT

Conrod does not require a folder layout, but this one keeps albums and Lightroom collections predictable:

```
P:\24th Frame Media\02_Projects\2026-11-22_Bathurst-1000\
    TO-IMPORT\        cards copied here first
    01_RAW\           the files Conrod scans
    02_Selects\       exports and picks
    ...
```

**TO-IMPORT workflow**

1. Copy each card into `...\YYYY-MM-DD_Name\TO-IMPORT`.
2. Point the Conrod album at that folder, or at the project folder with subfolders on.
3. Cull and identify in Conrod.
4. Import to Lightroom from the same paths, or apply the Conrod package.
5. Move files out of `TO-IMPORT` once both apps have read them. Keep the paths stable until then.

Matching is by path, so renaming or moving files between Conrod and the Lightroom apply breaks the match. Move files before scanning, or re-sync afterwards.

## Safety

- The plug-in never deletes, renames, or moves a photo.
- It never changes develop settings.
- It never writes a rating, flag, or label the package does not carry.
- Catalog write locks are held for 50 photos at a time, never for a whole package.
- Offline photos are counted, not fatal.

## If a package will not apply

- Confirm the photos are imported in the same catalog and their paths match.
- Look in `bridge\failed\` for the reason.
- Re-export from Conrod after changing the album; old packages are not updated.
- A package for another album is left in `in\` (or skipped) rather than applied to the wrong frames.
