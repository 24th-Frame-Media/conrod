# Scan → cull flow audit

Read-only audit, 2026-11-15. Mock walk on `vite` port 5176 (`?mock=1&page=…`). Screenshots in `target/flow/01…10-*.png`. The mock has no scan-progress state (`devStart` in `frontend/src/lib/mock.ts:8-12` has no scan param), so the 3,000-frame progress path is code-only.

## 1. Flow map

| # | User action | Screen / modal | Command (action) | Blocks? | Persisted |
|---|---|---|---|---|---|
| 1 | Opens app | Library (`App.tsx:117`) | `bootstrap`, `jobs`, `cover` per card | no | — |
| 2 | "+ Import photos" or drag folder | ImportDialog (`screens/Scan.tsx:18`) | `chooseFolder` (native picker) | no | — |
| 3 | Picks folder, label | same dialog | — | no | draft only (`App.tsx:28`) |
| 4 | Picks "What did you shoot?" (group) + preset select | same | — | no | `profile` in App state; seeded from `scan_profile` once (`App.tsx:46`) |
| 5 | Ticks targets (plates, numbers, people) | same (`Scan.tsx:95-105`) | — | no | per-dialog state only |
| 6 | Chooses stage (Import only / Import + suggest culls) and optional auto-identify | same (`Scan.tsx:70-71`) | — | no | — |
| 7 | "Import photos" | dialog closes, page → Review (`App.tsx:68-81`) | `scan` → `start_scan` (`desktop.rs:188`) | `await` only until job id returns; scan runs on a worker | job row; `scan_profile`, `include_people` on job |
| 8 | (on finish) | toast + taskbar | `regroup(job, settings.burst_gap)` (`desktop.rs:628`) | background | `burst_key` on images |
| 9 | Review grid fills | Review (`screens/Review.tsx:223`) | `review` (`useReview.ts:19`) | refresh on each change | — |
| 10 | "Suggest culls (N)" split button | Review toolbar (`Review.tsx:316-345`) | `scan` with `{jobId}` (`App.tsx:87`, `onCull`) | background | sharpness, rating, `cull_reason` |
| 11 | Browse stacks / flat; arrows, P, X, 0-5 | grid, loupe `E`, compare `C` | `mark`, `bulk_edit` | per key | `picked`, `rejected`, `manual_stars` |
| 12 | "Identify kept photos (N)" | same split button | `identify` (`App.tsx:82`) | background, banner + Skip (`Review.tsx:115-150`) | plates, numbers, make/model |
| 13 | "Accept all identifications" | menu | `bulk_edit` + `seed_known` (`App.tsx:137-143`) | — | `reviewed` |
| 14 | "Write XMP" / "Dry run" | menu | `write` (`App.tsx:83-86`) | background | XMP sidecars |
| 15 | Change preset/detection options | "Shoot settings & preset…" modal (`ShootSettingsModal.tsx`) or Manage-shoot menu toggles (`Review.tsx:438-467`) | `update_job_settings` **and** `save_settings` (global) | — | per-album and **global** |
| 16 | Re-measure | Workflow menu: Re-score, Rescan from pixels, Re-pick keepers, Re-group vehicles | `rescore`, `rescan`, `pick_keepers`, `group` | background | — |
| 17 | Export | no in-app Lightroom send; XMP only (step 14) | — | — | — |

Clicks to "keepers flagged" from Library with a folder ready: about 9 (Import, Browse, preset, Import, then Suggest culls, plus waiting). Then at least 3 more to write XMP. Culling itself has no keyboard "next unculled" shortcut.

State lost on navigation: `App.tsx:116` uses `key={page}`, so `ReviewScreen` remounts on every Library↔Review switch. Lost: grouping (`Review.tsx:225` resets to bursts), expanded stacks (`StackGrid` state), loupe/compare view, the confirm dialogs. Kept: `useReview` filters, because it lives in App.

## 2. Mock walk (what I saw)

- 01 Library: four albums, Import card, Setup rail.
- 02-03 Import dialog: folder via Browse, Events preset selected. Preset radio + preset `<select>` + a second preset list = three controls for one choice.
- 04 Import click: the dialog closed and the mock opened Review on album 3 with **13 burst stacks**, primary "Suggest culls (3)". Toast was empty in the DOM check. Plain DOM clicks worked; Playwright clicks on the dialog button timed out until a DOM click was used.
- 05-06 Stacks: repeated titles "MZD23 · #23" across five stacks, two stacks titled by filename only (IMG_4231.CR3). Same car fragmented into many bursts.
- 07 Flat grouping: Left/Right move one frame. Down moved 4207→4209, skipping 4208, so Down is not a burst jump in grid view. P adds `picked` class; X moves selection to the next non-culled frame (`useReview.ts:54-57`).
- 08 Filter "Rejected": still 10 stacks (same as "Picks"), and the expanded Camaro stack still showed all 5 frames. Filters narrow the stack list but not the frames inside.
- 09 Settings: Detection tab has "Blur strictness", "Burst gap (seconds)", "Counts as sharp at" as global fields.
- 10 Shoot settings modal opened from `.scan-type` pill (`Review.tsx:526-534`).
- Loupe: E opens the viewer; Up/Down and Left/Right change the score readout. The viewer does not show the filename, so I could not prove the cross-burst jump from the DOM.
- Console: 2 errors on load in mock (not investigated).

## 3. Friction list, ranked

1. **Burst filters and stacks are disconnected from the grid** (`Review.tsx:198`, `:191`, `:209`). `BurstStacks` builds stacks from `rv.review.frames` (all frames), not the filtered list. Result: "Rejected" and "Picks" both show 10 stacks, stack counts and the filmstrip ignore the filter, and the keeper may be a hidden frame. Verified in mock (step 08).
2. **Stack click can select a frame that the current filter hides** (`Review.tsx:41` calls `rv.setSelected(group.pick.id)`). `useReview.ts:36-38` then snaps selection back to `frames[0]`, so the view jumps to the top. Likely cause of "it jumps around".
3. **Keyboard: Up/Down do not follow the burst model in grid view** (`Review.tsx:279-280`: `stepBurst` calls `rv.step` when `view === 'grid'`; `useHotkeys.ts:11` documents it as burst jump). Verified: Down behaves like a flat step in flat grid. In bursts grid, arrows step the flat list, so the selection can land inside a collapsed stack with no visible cue except `isSelected` on the stack.
4. **Burst keeper "P" doesn't matter** (`lib/review.ts:76` pick = `burst_pick` only; `frame.picked` is ignored). P changes the card badge but not the stack's pick, the "Burst picks" filter, or the sort. Verified: P adds the `picked` class; the stack pick is unchanged in code.
5. **Burst gap setting is misleading and not re-applied** (`settings.rs:193` default 4.0; `bursts.rs:14` const 4.0; `bursts.rs:19` default 1.5; `desktop.rs:33` maps 4.0 to 1.5, so an explicit 4.0 is silently ignored; `desktop.rs:628` regroups only at scan end). Changing "Burst gap" in Settings does nothing to existing albums. Only "Rescan from pixels" (`housekeeping.rs:322` nulls `burst_key`) recomputes, and it is a destructive-sounding action.
6. **Grouping mode is lost on screen switch and is not persisted** (`Review.tsx:225`; remount from `App.tsx:116`). Users re-pick "Group: Burst stacks" after every trip to Library.
7. **Global settings written from album-level toggles** (`Review.tsx:447`, `:462`, `:572-578`: `save_settings` with the full settings object). Changing "Read race numbers" for one album silently changes the global default for every future album. Shoot settings modal does the same.
8. **Three ways to pick a preset** (Scan.tsx:74-93 radios + nested select; ShootSettingsModal:79-96 one select with optgroups; Review:525-535 pill). Duplicated control and naming drift: "Scan type", "preset", "shoot type", "Shoot settings".
9. **Naming drift across screens:** "Import" (dialog), "Scan" (`startScan`, `scan` action, "Subject rating started"), "Suggest culls" (cull), "Identify kept photos" (identify), "Rescan" vs "Re-score" vs "Re-pick keepers" vs "Re-group vehicles", "Rename shoot" via `window.prompt` (`Review.tsx:497`).
10. **Burst stacks are not virtualised** (`Review.tsx:209` renders every stack; `PhotoGrid.tsx:59` windows only the flat grid). A 3,000-frame shoot mounts every thumbnail on group change.
11. **Burst fragmentation** visible in mock (repeated car titles across five stacks). Stacks split by time gap alone; the vehicle grouping step ("Re-group vehicles") is the only merge, and it is a separate menu action.
12. **Primary split button changes meaning silently** (`Review.tsx:316-345`): Suggest culls → Identify → Accept all → Write XMP depending on counts. Users cannot predict the click.
13. **Scan blocking:** the Import button shows "Preparing import…" only during `scan` dispatch; there is no progress inside the dialog. Progress is in the activity menu (`App.tsx:82-85` toasts say so). Review can open early (`App.tsx:76`), but Review shows "Waiting for the first frames…" (`Review.tsx:661`) only when the frame list is empty.
14. **Culling has no keyboard "next unculled" or compare-next.** Culled frames sort to the end (`lib/review.ts` `visibleFrames`), which is good, but there is no key to step to the next undecided frame.
15. **Dead-end:** "Accept all identifications" and "Skip" both call `bulk_edit` and mark everything reviewed without a preview. Risky with no undo.

Lower-severity: `useReview.ts:57` picks a new selection from a stale `frames` closure; `review.ts` `visibleFrames` sorts culled last even with a manual sort; the mock's identify banner and toast were not verified.

## 4. Burst grouping: suspected bugs (file:line)

- `Review.tsx:198`, `:191`, `:209`: stacks not filtered by the active filter; count and filmstrip show hidden frames.
- `Review.tsx:41`: stack click selects `group.pick.id`, which may be hidden → `useReview.ts:37` snaps to `frames[0]`.
- `useReview.ts:36-38`: any filter change that hides the selected frame jumps the selection and, with `PhotoGrid.tsx:82-94`, the scroll to the top.
- `Review.tsx:279-280`: `stepBurst` in grid view is a flat step; contradicts `useHotkeys.ts:11`.
- `lib/review.ts:76`: `pick` ignores `frame.picked`; P never changes burst keepers.
- `Review.tsx:178`: burst key is `burst-${burst_key}` from `review.frames`; after regroup the ids change but `StackGrid` `expanded` (`Review.tsx:35`) still holds the old key → expansion collapses after any refresh that regroups (e.g. after Rescan).
- `Review.tsx:225` + `App.tsx:116`: grouping state reset on remount.
- `desktop.rs:33`: `burst_gap` 4.0 sentinel silently becomes 1.5.
- `desktop.rs:628`, `housekeeping.rs:322`: burst gap changes never apply without a rescan; no UI says so.
- `useHotkeys.ts:37-40` plus `useReview.ts:40-48`: grid arrows step hidden frames inside collapsed stacks; selection invisible.
- `Review.tsx:541-542` vs `Review.tsx:655`: "Group: Burst stacks" is shown even when `filters.view === 'stacks'`, a legacy view id not in the dropdown.

## 5. Redesign suggestions (no code)

1. One filtered model: derive stacks from the already-filtered frame list, so counts, filmstrip and keeper all honour filters. Keeper chosen from visible frames.
2. Make burst grouping a persisted per-album view setting (grouping + sort + filter) that survives Library↔Review.
3. Separate "keeper" from "picked": P should set the stack's keeper or be removed from the stack model; show one badge.
4. Keyboard model in grid and loupe: Left/Right within burst, Up/Down across bursts, always; a visible focus ring on the active stack; "next undecided" key.
5. Burst gap: move it to album settings, apply it on change by re-grouping in place (no rescan), show the result count.
6. Collapse the three preset controls into one control in the import dialog; reuse the same component in the shoot-settings modal.
7. Album-scoped toggles must write album settings only; the global settings block should be reachable only from Settings.
8. Scan: show progress inside the import dialog's successor (an album header with ETA) and let Review start on the first 100 scored frames, with a "still scoring" banner.
9. Name the four passes once (e.g. Score, Identify, Group, Write) and use the same words in the split button, menus and toasts.
10. Add a summary step before Write XMP: counts of keepers, rejects, identifications, with a dry run as the default.

Competitor notes (`docs/research/competitor-scoring.md`): Narrative Select keeps per-scene grouping with keyboard flow and never rejects automatically; Imagen and Aftershoot ask for a keeper target count up front rather than many toggles; Lightroom shows culling scores inline in the info panel. Conrod's gaps are the disconnected filters and the unmanaged burst gap, not missing features.

## Caveats

- Mock data only; no real RAW scan. Scan-progress path was not exercised.
- The loupe cross-burst jump and the 2 mock console errors were not verified.
- Line numbers were checked by grep or read; a few (e.g. `Review.tsx:35`, `:178`) are approximate to the function, not the exact token.
