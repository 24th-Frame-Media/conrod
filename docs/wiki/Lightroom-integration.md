# Lightroom integration

**November 2026**

[Home](Home) · Previous: [Identify](Identify) · Next: [Settings reference](Settings-reference)

When an album finishes, Conrod writes stars, picks, and colour labels to XMP sidecars. Lightroom reads them with **Metadata → Read Metadata from File** or on import. Nothing needs installing.

The optional Conrod Bridge plug-in applies flags and labels in Lightroom and sends Lightroom's flags back without re-reading metadata. It uses files, so the apps need not run at the same time and large imports do not time out.

## Optional: install the plug-in

The bridge is **off by default**. Turn it on in Settings > **Lightroom** to copy the plug-in into Lightroom's Modules folder:

`%APPDATA%\Adobe\Lightroom\Modules\ConrodBridge.lrplugin`

- Settings > **Lightroom** shows whether the plug-in is installed, its version, and whether an update is available.
- **Install plug-in** copies it now; **Remove plug-in** removes only Conrod's folder.
- **Restart Lightroom Classic** after an install or update.
- Conrod updates a stale plug-in on start only when the bridge is on. It never removes anything.

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

Ratings and colour labels are written only when empty, unless the package allows overwriting. Picks are set only on unflagged photos and never cleared. Bad entries are counted.

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

Suggested stars, picks, and colour labels reach the catalog without retyping. Conrod writes the package to a temporary name, then renames it so the plug-in never sees a partial file.

## From Lightroom to Conrod

Two ways:

1. **Send flags to Conrod** writes `bridge\in\<album>-<unix>.json`. In Conrod, the album's Lightroom import consumes it and applies the flags to matching frames. Consumed packages move to `done\`; unreadable ones move to `failed\` with a reason.
2. **Read a catalog** directly: point the import at a Lightroom Classic `.lrcat`. It is opened read-only. Matching is by normalised path, then file name plus capture time within two seconds, and only when exactly one album frame qualifies.

The import reports matched, unmatched, missing, the percentage of effective keepers carrying a pick, skipped, and failed. Only catalog rating/pick/label columns are written, and only when a value changes.

## Folder layout and TO-IMPORT

Conrod does not require a folder layout. This example keeps albums and Lightroom collections predictable:

```
<library>\02_Projects\<shoot>\
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

Matching uses paths. Renaming or moving files between the Conrod scan and Lightroom apply breaks the match. Move files before scanning or re-sync afterwards.

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
