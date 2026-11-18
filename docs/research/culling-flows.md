# Culling flows: how competitors get from folder to keepers

Scope: UI and flow only. Scoring/model quality is in `competitor-scoring.md`.
Method: vendor help centres, vendor blogs and third-party reviews, found via web search and a few fetches. Click counts are my reconstruction from the docs, not timed tests. Anything I could not confirm is marked **[unverified]**.

---

## 1. Aftershoot (most detail)

Source: https://support.aftershoot.com/en/articles/5223473-get-started-with-aftershoot-culling (main guide, fetched); https://support.aftershoot.com/en/articles/9189985-using-grid-loupe-and-survey-mode-views ; https://support.aftershoot.com/en/articles/13928681-aftershoot-keyboard-shortcuts ; https://support.aftershoot.com/en/articles/7053496-spray-can-mode ; https://support.aftershoot.com/en/articles/7048858-how-to-export-your-culled-images-from-aftershoot ; https://support.aftershoot.com/en/articles/6673181-culling-editing-workflow-with-aftershoot

**Step by step (about 8–10 clicks from folder to delivery):**

1. **Create album / import.** Home > `+ New Album`. Drag a folder in or browse. Asked: format, include sub-folders, optional backup when ingesting from a card.
2. **Pick the shoot genre** (wedding, portrait, family, boudoir, sports, school, newborn, other). Asked. Album can be renamed here.
3. **Choose cull type.** Cull tab > `Start Culling` > *AI Automated* or *AI Assisted*. (Also a *One-Click Cull & Edit* option that combines cull and edit; RAW/JPEG only.) Asked.
4. **Preferences.**
   - Automated: *Auto Target* (one strictness slider; default category presets) or *Cull to Target* (percentage slider, 10–60%, replaces Highlights with Maybes, no duplicate groups).
   - *Customize* exposes per-filter toggles and sliders: Highlights, Duplicates (Few/More), Blurry (sensitivity), Closed Eyes, Maybes. Star/colour mapping per filter via `Update`.
   - Assisted: one slider only (how strictly to group duplicates).
   - Defaulted: category presets, default star/colour mapping, filter groups on.
5. **Start.** Import and cull run; you get an on-screen and email notice when done.
6. **Review in Grid** (the default landing view). See section on states below.
7. **Save / export.** Edits are saved with `Save Changes` (Cmd/Ctrl+S). Export: pick a Cull Filter group, click `Export`, then Lightroom / Capture One (opens that app's import with picks) or `Export to Folder`.

**Progress while it scans.** Aftershoot's blog says you can "wait for the import process to complete… or click Start Culling to proceed", so you are not blocked during import. Browsing results while the cull runs: **no** (confirmed by the photographer, 2026-11-15) — Aftershoot shows the images only once all AI analysis has finished.

**The culled view.**
- Grid view opens with *Selected* and *Highlights/Maybes* shown; *Duplicates* are stacked.
- Left side Cull Filters, grouped as: AI Selections (Selected, Highlights/Maybe) — 5-star by default; For Review (Blurred, Closed Eyes) — 0-star by default; My Selections (P-flagged); Duplicates; Unrated (no stars or colour). Sort/filter controls in the top bar; People filter for key faces.
- Stars and colour labels are written by Aftershoot: 5★ for selects, 0★ for review groups, colours per filter (configurable).
- Stacked duplicates show a count badge top-right of the thumbnail. Click it, or use `,` / `.`, to cycle. A right drawer shows the duplicates and their sort options; a filmstrip mode is also available.

**Duplicates and swapping the pick.** Stacked group, pick on top. `S` swaps the pick with a clicked duplicate (moves the 5★ across). `A` marks an image Selected in a set. `Z` in Loupe rejects a whole set. Survey (`N`) puts a set side by side; the help centre says up to 16 (the 2024 blog said 8 — **[unverified which is current]**), zoom synced across the set, `Cmd/Ctrl`+click removes a frame until one remains. Spray Can (`Cmd/Ctrl+Opt+K`, Grid and Survey only) turns mouse clicks into rate/accept/reject actions.

**Keyboard model.**
- Arrows move between images (flat across the grid, not by stack).
- `,` / `.` move within a duplicate set.
- `1`–`5` stars, `6`–`9` and `-` colours, `0` or `=` clears, `P` flag.
- `X` clears stars and colour (help centre) — note the Lightroom-style `X` = reject is *not* Aftershoot's meaning.
- `E`/Enter = Loupe, `G` = Grid, `N` = Survey, Space = zoom, `Alt`+←/→ = next/prev key face, `I` = image score, `/` hides drawer. Full cheatsheet via the keyboard icon. Remappable; Lightroom or Photo Mechanic key presets exist.

**Face / eye zoom.** Loupe shows *Key Faces* down the right side; click one to zoom to it. Slider sets face size. Space zooms (100%, +/- steps).

**Override and how changes apply.**
- Stars, colours, swaps, flags: applied instantly in the app.
- Persisted to disk only with `Save Changes`; Lightroom/C1 see them as XMP sidecars.
- Re-cull with different settings: **[unverified]** in the Aftershoot docs I could read; the "Update" button only changes star/colour mapping in the article. Treat re-cull as not confirmed.

**Export.**
- *Save Changes* writes `.xmp` sidecars next to originals (non-destructive; RAW ratings go to sidecars).
- *Export → Lightroom Classic / Capture One* opens that app's Import with the picks; Aftershoot's own step writes ratings, and Lightroom only sees them via XMP ("Read Metadata from Files") if the catalog is already open.
- *Export to Folder*: specific folder or same folder, subfolder, "Divide by Rating" subfolders (AI groups / stars / colour / flags), renaming (original or sequence + custom), JPEG/TIFF/RAW, quality, colour space, sizing.

---

## 2. Narrative Select

Sources: https://narrative.so/select/lessons/quickstart-guide ; https://help.narrative.so/en/articles/11069214-narrative-quickstart-tutorial ; https://help.narrative.so/en/articles/7337363-getting-started-with-narrative ; https://help.narrative.so/en/articles/9035849-getting-your-images-from-narrative-into-lightroom-windows ; https://shotkit.com/how-to-use-narrative-select

- **Steps:** create project from folder (or memory card: Quick Ingest) → wait for Scenes + assessments → go scene by scene → rate (`T` tag, `1`–`5`) → `Ship + Edit` (`Ctrl+E`), choose Lightroom, optional AI Preset → drag the shown folder into the open Lightroom Library. About 6 steps; 1 extra drag in LR.
- **Defaulted vs asked:** First Pass strictness (Ruthless / Balanced / Cautious) is set in Preferences, not per shoot. It does **not** set a keeper count.
- **Groups:** *Scenes* (`S`) group similar frames; *First Pass* hexagons (blue = pick, grey = average, pink/red = weak) are relative to the scene, not absolute.
- **Keyboard:** `↑`/`↓` move between images, `←`/`→` move between scenes (stated in shotkit review). `Space` zooms to the detected face; while zoomed, `←`/`→` cycle faces and `↑`/`↓` move to the next image *keeping the same face in view*. `/` toggles the Close-Ups panel (per-face sharpness/eyes score). `N` Survey (up to 12).
- **Exit:** Ship writes XMP ratings and hands a folder to LR's import (Add / Copy / Move / Copy as DNG). Narrative says it never deletes photos.
- **Caveat:** Narrative's own FAQ says Auto-rate is optional and it does not cap a count.

---

## 3. FilterPixel

Sources: https://filterpixel.com/blog/how-to-use-filterpixel-the-complete-feature-walkthrough ; https://filterpixel.com/blog/posts/avoiding-analysis-paralysis-while-culling ; https://filterpixel.com/blog/review-face-survey-mode ; https://filterpixel.com/blog/filterpixel-4.0 ; https://filterpixel.com/deepcull

- **Steps:** drop folder → choose a mode (Basic Cull, DeepCull with genre, or Magic Number target) → set a target count (DeepCull) or sliders (Duplicate, Blink, Focus strictness: Strict/Moderate/Lenient) → culling runs 1–20 min → `Review Mode` → pick → export to LR or folder. About 5–6 clicks.
- **Asked up front:** target number (DeepCull / Magic Number) or three strictness sliders. Defaults: genre presets.
- **Grouping:** *Review Mode* (stack icon) groups bursts and brackets; each group shows its count. `Enter` opens a 3× larger gallery; `↑`/`↓` fly through stacks, `Esc` back.
- **Keyboard:** `P` pick, `X` reject, `U` unflag/tag duplicate, `Tab` skips all similar to the pick, `C` comparison, `G` grid, `S` Survey (up to 12–16), `Space` Face Mode (16× on the primary face, `←`/`→` cycles faces).
- **Exit:** exports selects with star ratings to Lightroom Classic or a folder. Never deletes.

---

## 4. Imagen (Culling Studio)

Sources: https://support.imagen-ai.com/hc/en-us/articles/29925087795357-Create-a-new-culling-project ; https://support.imagen-ai.com/hc/en-us/articles/13820521363997-Set-up-culling ; https://support.imagen-ai.com/hc/en-us/articles/29925174097821-Review-your-photos-after-culling ; https://imagen-ai.com/culling/

- **Steps:** new project → pick folders → name, photography type, original or edited preview (AI Profile; free for 60 days) → choose method (*Keep the best of each group* or *Cull to an exact number*) → upload → `Review Culling` opens Culling Studio → filter → optional export or Send to edit. About 6–7 clicks.
- **Asked:** method, photography type, similarity level (*Less / Similar (default) / Very similar*), rating method (stars, colour labels, or flags), whether to show low-rated.
- **Groups:** duplicates grouped; the best of each becomes *Keeper*; unique frames are *Standalones*; the rest are *Duplicates*; blurred/closed-eyed are *Low-rated*. Filter by Keepers + Standalones first.
- **Keyboard:** not documented in the pages I read **[unverified]**. Navigation between groups uses buttons or shortcuts above the image display.
- **Re-cull:** explicitly supported ("cull again with new settings").
- **Exit:** XMP sidecars or embedded metadata for the pick; "Send to edit" continues inside Imagen.

---

## 5. Excire Foto (culling module)

Sources: https://excire.com/en/photography-culling ; https://excire.com/en/best-culling-software ; https://excire.com/manuals/Excire-Foto-2027-Quickstart-(EN).pdf ; https://support.excire.com/portal/en/kb/articles/excire-foto-or-excire-search

- **Steps:** Culling Project from a folder → choose a culling profile (grouping: visual similarity, sequence, people, content, date; sharpness/eye/face/aesthetic) → Start → work groups → filter 3★+ → Compare → Selection bin → export. About 5–6 steps.
- **Asked:** culling profile (saved for reuse); profile changes apply only to new projects.
- **Keyboard:** `Ctrl`/`Cmd`+`Shift`+←/→ jumps group to group; `P`/`X`; `1`–`5`; `6`–`9`. Groups navigated with buttons or shortcuts.
- **Exit:** Selection folder; export originals; Excire never auto-deletes (rejects go to a Recycle Bin folder).

---

## 6. Photo Mechanic (ingest -> tag -> filter)

Sources: https://docs.camerabits.com/support/solutions/articles/48000207409-ingesting-photos-with-photo-mechanic ; https://home.camerabits.com/setting-up-photo-mechanic-quickstart-guide-to-preferences ; https://www.trunghoangphotography.com/for-photographers/photo-mechanic-lightroom-workflow-culling-faster-with-photo-mechanic ; https://www.ppa.com/ppmag/articles/photo-mechanic-power-tips

- **Steps:** Ingest (`Ctrl/Cmd+G`, or auto on card insert) → contact sheet opens → tag/colour/rate with single keys → filter the sheet to hide rejects → select → drag to Lightroom's Import (Add). About 4–5 steps. Work can start while ingest runs in background.
- **Asked up front:** ingest destination, folder naming, IPTC template, rename. Defaults are generous.
- **Keyboard:** single-key `T` tag; `0–5` rating or `0–8` colour (set in Preferences > Accessibility, Ctrl/Alt for the other). **Auto Advance** after any tag, colour or rating change (a Preview setting). `Z` zoom, `H`/`V` compare two.
- **Filter:** toggles on the bottom-right bar; click a swatch to hide it; `Alt`+click to show only that value.
- **Groups:** none. Burst handling is manual compare.
- **Why pros find it fast:** the whole pass is single keys on a grid with auto-advance; filtering is one click; nothing re-renders previews. Ingest writes files, not a catalog. Caveat from reviewers: rating/tagging *on a card* risks card corruption; copy first.

---

## 7. Lightroom Classic (import -> flag -> compare)

Sources: https://helpx.adobe.com/lightroom-classic/help/flag-label-rate-photos.html ; https://helpx.adobe.com/lightroom-classic/desktop/introduction-to-lightroom-classic/keyboard-shortcuts.html ; https://helpx.adobe.com/lightroom-classic/help/browse-compare-photos.html ; https://phlearn.com/tutorial/30-days-lightroom-day-3/ ; https://aftershoot.com/blog/cull-photos-in-lightroom/ (for the two-pass habit; vendor-written)

- **Steps:** Import (Add, Embedded & Sidecar previews) → Grid → Loupe → `P`/`X`/`U` with Auto Advance on → Attribute filter → Survey (`N`) or Compare (`C`) for bursts → `Shift+Tab` to clean up → `Refine Photos` → Delete Rejected. About 6 steps.
- **Asked:** import settings only. No cull question.
- **Keys:** `P`/`X`/`U`; `Shift`+`P`/`X` sets and moves to next; `1–5`, `6–9`; `Caps Lock` Auto Advance; `G`/`E`/`C`/`N`; `Z` 100%; `Ctrl/Cmd+Option+R` Refine Photos.
- **Groups:** none automatically. Stacks are manual. Compare = Select (left) vs Candidate (right); arrows step candidates; Up arrow promotes. Survey = all selected side by side; click the `X` on a thumbnail to remove it from the set (not from catalog).
- **Exit:** flags are in the catalog and, since LrC 13.2, written to XMP (Adobe help).
- **Assisted Culling** (focus/eyes/exposure scoring) exists in Lightroom desktop/Classic per the Aftershoot blog; I did not verify Adobe's own page **[unverified]**.

---

## 8. Capture One (Cull view)

Sources: https://support.captureone.com/hc/en-us/articles/7185822431645-Culling-images ; https://support.captureone.com/hc/en-us/articles/8265235276957-Cull-View ; https://lifeafterphotoshop.com/capture-one-23-cull-tool-how-does-this-new-feature-work-and-how-useful-is-it ; https://www.thedigitalstory.com/2022/12/culling-is-better-in-capture-one-pro-23.html ; https://www.captureone.com/blog/sessions-103-expert-knowledge

- **Steps:** Cull (toolbar) on a collection → Group Overview on → double-click → rate/colour → Done → filter by colour → select → delete or process. About 5 steps. Cull view needs no rendered previews.
- **Asked:** Group similarity slider (default 75%). Auto-advance is a menu setting (`Select > Select Next When`).
- **Groups:** Group Overview clusters by similarity; the group column and the thumbnail column are separate.
- **Keyboard:** `↑`/`↓` move **within** the current group; `←`/`→` move **between** groups; number keys rate. Face Focus (50/100/200%, limit to eye/face). `G` toggles grid/viewer.
- **Exit:** colour/rating filter, then select all of one colour and delete or process. Smart Albums collect by rating without moving files.

---

## Comparison (folder to keepers)

| Tool | Steps (approx) | Up-front questions | Groups | Burst keys | Exit |
|---|---|---|---|---|---|
| Aftershoot | 8–10 | genre, cull type, strictness/target, filters (Customize) | stacks, count badge, Survey | `,`/`.` in set; arrows flat | XMP save, Export to LR/C1 or folder |
| Narrative | ~6 | none per shoot | Scenes, face close-ups | ↑/↓ images, ←/→ scenes | XMP, Ship + Edit |
| FilterPixel | 5–6 | target count or 3 sliders | Review Mode stacks | ↑/↓ stacks, Tab skip group | export selects + stars |
| Imagen | 6–7 | method, similarity, rating method | Keepers/Standalones/Duplicates | buttons/shortcuts **[unverified]** | XMP or embedded, edit in-app |
| Excire | 5–6 | culling profile | similarity/sequence | Ctrl/Cmd+Shift+←/→ groups | Selection folder, export originals |
| Photo Mechanic | 4–5 | ingest options | none | single keys + auto-advance | XMP, drag to LR import |
| Lightroom Classic | ~6 | import only | manual stacks, Compare/Survey | P/X/U + Caps Lock | catalog + XMP, Refine/Delete |
| Capture One | ~5 | similarity % | Group Overview | ↑/↓ in group, ←/→ between | colour filter, select, delete/process |

---

## (a) Best-of target flow for Conrod (8 steps)

1. **Folder in** — drag a folder onto the home screen. No questions. Ingest starts immediately; browsing is allowed during it.
2. **One sheet, one question** — "Shoot type" (defaults to last used) and a single "How many keepers?" slider (% or count). Everything else is advanced and collapsed.
3. **Browse while it runs** — grid fills as results land; keyboard works from the first thumbnail.
4. **Landing view = Selected only**, duplicates stacked with a count badge, pick on top. Non-picks hidden by default.
5. **Stacks:** `↑`/`↓` between stacks, `←`/`→` within a stack, `Enter` expands a stack in place, `Esc` collapses. `S` promotes a duplicate to pick.
6. **Keys:** `P` pick, `X` reject (flags, so Lightroom reads them), `1`–`5` stars, `6`–`9` colours, `U` unflag, `Tab` skip the rest of the stack, Auto Advance on by default, `Space` zoom to the face, `Alt`+←/→ next face.
7. **Changes save instantly** to a sidecar as you go (no Save step), with an undo log; "Re-cull with new slider" is one button that keeps your manual picks.
8. **Export** — one button with three destinations: "Open in Lightroom" (XMP + import), "Copy keepers to folder" (optional subfolders by stars/colour), "Send to edit". Originals are never moved unless the user picks Move.

---

## (b) UI patterns to copy

| Pattern | From | Why it is fast |
|---|---|---|
| Stacks with pick on top and a count badge | Aftershoot, FilterPixel, Capture One | Collapsing hides near-duplicates; one pick per moment |
| ↑/↓ between stacks, ←/→ within (or Ctrl+Shift for groups) | Capture One, Excire | Two axes, no mode switch |
| Tab skips rest of group after picking | FilterPixel | One key ends a burst |
| Auto Advance after P/X/1–5 | Lightroom, Photo Mechanic, Capture One | Keeps hands on keys |
| Single-key pick/reject (P/X/U) plus numbers | Lightroom, FilterPixel, Photo Mechanic | Standard muscle memory |
| Compare with Select vs Candidate, promote with a key | Lightroom Compare | Two-way decisions, no grid thinking |
| Survey: all duplicates side by side, zoom synced | Aftershoot, FilterPixel | Same-spot comparison |
| Remove one frame from a survey with Cmd/Ctrl-click | Aftershoot | Elimination is quicker than choosing |
| Face zoom panel with next/prev face key | Narrative, Aftershoot, FilterPixel | Eyes-open checks without leaving the frame |
| Filter bar: click swatch to hide, Alt+click to isolate | Photo Mechanic | One click to get to picks |
| Ship/Export with "Add vs Copy vs Move" choice | Narrative, Aftershoot | Clear what happens to files |
| Per-shoot presets with strictness sliders | Aftershoot, FilterPixel, Imagen | Asks once, defaults the rest |
| Cull to a target count | Aftershoot, FilterPixel, Imagen | Matches a delivery number |

---

## (c) Burst / group navigation: the clearest model

Best model seen: **Capture One's Cull view + Aftershoot stacks**, with FilterPixel's Enter-to-expand.

- Collapsed stack, pick on top, count badge (Aftershoot, FilterPixel).
- `Enter` (or double-click) expands the stack in a larger view (FilterPixel: 3× gallery).
- `←`/`→` moves within the stack (Capture One and Aftershoot's `,`/`.` do this).
- `↑`/`↓` moves between stacks (Capture One, FilterPixel).
- `S` or `P` sets the pick; `Tab` skips the remaining frames of the group (FilterPixel).
- `Esc` collapses back to the stack.

Known failures to avoid: Aftershoot's arrow keys step across the flat grid, so a burst is only partly reachable by arrows; Lightroom has no stacks in the cull, so bursts are handled by Survey and a second pass; FilterPixel's Basic Cull and Excire both have groups but make you learn modifiers (Ctrl+Shift).

---

## (d) Export: XMP vs catalog vs folder

- **XMP sidecars** (Aftershoot Save Changes, Narrative ratings, Photo Mechanic, Imagen, Lightroom flags since 13.2): ratings and labels travel next to the RAW without touching the catalog. Lightroom needs "Read Metadata from Files" (Aftershoot, Narrative) or it picks up changes on folder import. Safe, reversible, editable in any app.
- **Writing into a Lightroom / Capture One catalog** (Aftershoot Export → LR/C1, Narrative Ship + Edit, Photo Mechanic drag-to-import): the picks become catalog entries; choose Add (references original location) vs Copy/Move. Conrod should default to Add.
- **Copying keepers to a folder** (Aftershoot Export to Folder, FilterPixel, Excire Selection, Photo Mechanic move): a physical sub-set for a client or a second editor. Moving is the only irreversible option; default to Copy and show the file count.
- Who does what: Aftershoot writes XMP and can open LR/C1 or copy; Narrative writes XMP and uses LR's own import; Imagen writes XMP and edits in-app; Photo Mechanic writes XMP and leaves import to you; Excire keeps a Selection folder and exports originals; Lightroom and Capture One hold flags in the catalog (C1 uses colour filters and Smart Albums).

---

## Unverified / not checked

- Aftershoot: (resolved: no browsing until all AI analysis finishes); whether re-culling with new settings keeps manual overrides; Survey's maximum (16 in help vs 8 in a 2024 blog).
- Imagen: keyboard shortcut list.
- Lightroom Assisted Culling details (only vendor-blog mentions).
- Excire keyboard list beyond what the blog gave.
- Click counts are reconstructed from docs, not timed.
