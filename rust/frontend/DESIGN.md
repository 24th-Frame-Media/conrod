# Conrod desktop UI: design notes

Ground truth: `conrod/web/style.css`, `index.html`, `app.js` (the Python UI). The React
app in `src/` re-creates that look and keeps its own engine calls (`src/lib/api.ts`).

## Tokens (from Python `:root`, copied 1:1 into `src/styles/base.css`)

| token | value | use |
| --- | --- | --- |
| `--bg` | `#0b0b0d` | canvas, scrollbar track edge |
| `--surface` / `-2` / `-3` | `#141417` / `#1b1b20` / `#232329` | bars, cards, hover/active |
| `--line` / `--line-soft` | `#2a2a31` / `#202027` | borders, dividers |
| `--text` / `-dim` / `-mute` | `#f2f3f5` / `#a1a5ad` / `#6f747d` | copy tiers |
| `--accent` / `-hover` | `#2f6df6` / `#4880ff` | the ONE accent: primary, focus, selection, stars |
| `--good` `--scan` `--warn2` `--bad` | `#35c48b` `#2ee6a8` `#e5b543` `#f0616b` | verdicts, progress glow, warnings, danger |
| radii | 10 (`--radius`), 14 (`--radius-lg`), 8 controls, 999 pills | |
| shadow | `0 8px 28px rgba(0,0,0,.45)` | popover, dialogs, toast |

Type: `"Segoe UI Variable Text","Segoe UI",system-ui`, 14px/1.5, antialiased. Headings 20/600
(`-.01em`); section labels 11px uppercase `.09em` mute; mono `"Cascadia Mono",Consolas`.
Controls: 8px radius, 8x11 padding, surface-2 fill, 1px `--line`; primary = solid accent, white
600; danger = red text + `#4a2b30` border; ghost = transparent; small = 4x10 / 12px.
Scrollbars 11px, `#303038` thumb, 3px bg border. Toast: pill, surface-3, bottom centre.

## Metrics

Topbar 52px (`--surface`, bottom `--line-soft`). Rail 330px (right), facets 250px (left),
viewer/inspector side 300px. Cards: 3:2 thumb, radius 10, grid gap 10, min column 190.
Album cards `minmax(290px,1fr)` gap 22, cover 16:10. Pane padding 24 28; content max 780/1100.
Card left edge = verdict colour (good/fair/poor); `current` = 2px accent ring; culled/rejected dim.

## Screens (Python screen -> React)

| Python | React (`src/screens`) | notes |
| --- | --- | --- |
| home (job cards + rail) | `Library` | cards with status cover, next-step buttons (Review / Identify / Resume), Delete; rail = totals + model setup |
| scan (form on home + live rail) | `Scan` | folder + name + scan type (profile) + Start; live progress rail while a scan runs |
| review (toolbar, facets, grid) | `Review` | view tabs, search, star filter, sort, Numbers/Plates facets, virtual card grid, inspector with inline editing |
| frame dialog | `Viewer` | native `<dialog>`, full preview, fractional detection boxes (B toggles), prev/next, stars/reject |
| train | `Train` | subject over its frame with box, 1-5/Pan/Can't tell/Undo, learn/forget |
| known cars (in settings) | `Known` | search, add row, inline-editable table |
| settings | `Settings` | setting groups (label + hint + input), Save, "Learn from my ratings" |
| album/setup | folded into Library/Scan rails | no dedicated screens: the Rust engine has no album-sheet or install actions |

Shell: `TitleBar` = brand + tabs + stats + scan-type chip + status pill + window controls. The
status pill (top right) opens a popover: tasks + progress, Pause/Resume, Stop, cancel operations,
event log. Splash card while bootstrapping.

## Keymap

Review / Viewer: `J` `K` or arrows move; `0`-`5` stars; `X`/`Del` reject; `U` reset (stars cleared,
un-rejected); `Enter` open viewer; `B` boxes (viewer); `Esc` close; `?` shortcut list.
Train: `1`-`5` rate + next; `X` can't tell; `P` pan; `U` undo; `B` box; `Z` 100% crop; `J` `K` move.
Inputs/selects swallow keys. Ctrl/Meta/Alt combos are ignored.

## Native flare (on top of the web look)

- Custom title bar: topbar is the drag region (`data-tauri-drag-region`), double click toggles
  maximise (Tauri built-in), min/max/close buttons via `@tauri-apps/api/window`. Shown only when
  the window reports `decorations:false`, so it degrades to the OS frame until the conf changes.
- Toast when a scan finishes / fails / is stopped (status transition), replaces the error banner.
- Drop a folder on the window: full-window drop overlay, then the Scan page is pre-filled
  (`onDragDropEvent`); nothing starts until the user chooses a scan type and presses Start.
- `prefers-reduced-motion` turns transitions/animations off; smooth 120-200ms transitions elsewhere.
- Pill light breathes while busy; new cards fade in; selection scrolls into view smoothly.
- `color-scheme: dark` (native form controls, scrollbars), `accent-color`, `::selection` in accent.
- Needs Rust: persist window size/position (window-state plugin), taskbar progress
  (`set_progress_bar`), close-to-tray already a setting; see the hand-off notes.

## Dev mock

`?mock=1` under `vite dev` swaps `call()` for `src/lib/mock.ts` (fake albums/frames, SVG data-URI
thumbs). `&page=review|scan|train|known|settings`, `&viewer=1`, `&popover=1`, `&scan=1`
(running scan) jump straight to a state. Guarded by `import.meta.env.DEV`; not in `dist`.
