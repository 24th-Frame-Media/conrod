# Ingest live screen: engine contract

The unified INGEST screen (`frontend/src/screens/Ingest.tsx`) reads two existing events, `scan-progress` and `identify-progress`.
This document lists the **optional** fields the screen can draw once the engine sends them. Types live in `frontend/src/lib/types.ts`
(`LaneFrame`, `LaneBox`, `LaneStep`, `PipelineWorker`, `ScanRecentFrame`). Every field below is optional: the screen degrades to
today's payload (file name, stage, and the thumbnail of the lane's file once it is in `recent`). A sample of the full payload is played by
the dev mock, `frontend/src/lib/mockIngest.ts` (`?mock=1&page=ingest`; add `&legacy` to see today's payload only).

Nothing here changes `identify-progress`. Its `active` / `recent` / `workers` already drive the vision lanes.

## 1. `scan-progress` → `pipeline.workers[]` (one entry per lane)

```ts
type PipelineWorker = {
  stage: string;            // existing. The stage the lane is in now: read | detect | sharpness | faces
  worker_index: number;     // existing
  state: 'idle' | 'busy' | 'waiting' | 'paused';   // NEW value: 'paused'
  waiting_for: 'gpu' | 'detector' | 'faces' | 'lock' | 'writer' | 'disk' | 'ollama' | 'paused' | null;  // NEW values: detector, faces, writer, disk
  file: string | null;      // existing. File name only
  step: string | null;      // existing free text; shown only when `current` is absent
  ms_in_step: number;       // existing
  lane?: number;            // NEW
  current?: LaneFrame | null;  // NEW
};
```

| Field | Set when | Notes |
|---|---|---|
| `state: 'paused'` | The run is paused and the lane holds a frame | Today the lane keeps saying `busy`. The UI also treats `waiting_for: 'paused'` and the global pause (task state `paused`) as paused, so this is a nicety, not a blocker. Paused beats busy. |
| `state: 'waiting'` + `waiting_for` | The lane holds a frame but is blocked: `detector` (`gpu` is accepted as a synonym) waiting for a detector session, `faces` for the face Mutex, `lock`/`writer` for the DB, `disk` for IO | The card says "waiting for detector" and stops animating. The face and expression Mutex waits are not reported today (see ingest-review.md section 2): reporting them as `faces` is the most useful new value. |
| `state: 'idle'` | The lane has no frame | The card dims. Send `current: null`. |
| `lane` | Always, if you can | A stable 0-based id that survives the lane changing `stage`, so a card does not jump around the grid as the frame moves from read to faces. Without it the card is keyed by `stage:worker_index`, which remounts the card at every stage change (the frame image reloads, boxes pop in again). Lane ids should be dense: 0..N-1 for N cull lanes. |
| `current` | While `state` is `busy`, `waiting` or `paused` | See 2. Send `null` when the lane is idle. |

Group and pick are barriers and are not lanes. If the engine lists lanes for `group` / `pick` the UI hides them.
The scan's `identify` lanes are not shown as cards either: the vision lanes come from `identify-progress`.

## 2. `PipelineWorker.current` (the frame the lane holds)

```ts
type LaneFrame = {
  image_id: number;
  file: string;                       // file name only
  thumb_url?: string;                 // same kind of path as ScanRecentFrame.thumb_url
  step: 'read' | 'decode' | 'detect' | 'sharpness' | 'faces' | 'persist';
  boxes?: { cls: string; conf: number; box: [number, number, number, number] }[];
  af?: [number, number, number, number];
  stars?: number;                     // 0..5
  verdict?: 'sharp' | 'soft' | 'blurred';
  panning?: boolean;
};
```

Boxes are `[x1, y1, x2, y2]` as **fractions (0..1) of the upright frame** (after EXIF orientation), the same frame the thumbnail shows. The
thumbnail is drawn at its own aspect ratio, so the fractions map straight onto it. `conf` is 0..1.

When the engine sets each field (the screen draws whatever is present, in any combination):

| Field | Set when | Order within a frame |
|---|---|---|
| `image_id`, `file`, `step` | The lane picks the frame up, and `step` on every step change | `step` walks `read` → `decode` → `detect` → `sharpness` → `faces` → `persist`. Skipped steps (e.g. no faces for the shoot type) may be skipped. The step ladder under the picture lights the current one. |
| `thumb_url` | As soon as a picture exists: after `decode` (the embedded preview) is fine. The final thumbnail written in `persist` may replace it | If it is missing the card shows a placeholder and the step. |
| `boxes` | After `detect`. Send the **kept** detections (after the min-box and class filters), not the raw model output. May be sent growing, one at a time, but one array after detect is enough | Class names as the detector reports them: `car`, `person`, `motorcycle`... The tag reads `car 92%`. |
| `af` | After the AF area is known. Today the scan does not read the focus point at all (ingest-review.md headline "AF is not part of ingest"). Send it once that exists, as soon as it is read, before `sharpness` | The dashed amber box. Omit for frames with no AF data. |
| `stars`, `verdict`, `panning` | After `sharpness` | `verdict`: `sharp` / `soft` / `blurred`. The card shows `★★★★☆ Sharp · pan`. `panning` is a flag, not a verdict. |

When a frame finishes, the lane's `current` becomes the next frame (or `null`). The finished frame goes to `recent` (next section).

## 3. `ScanProgress.recent[]` (newest first, up to 24; unchanged shape, new optional fields)

```ts
type ScanRecentFrame = {
  image_id: number; thumb_url: string; subject: string;
  verdict: 'sharp' | 'soft' | 'blurred';   // NEW value: blurred
  panning?: boolean; label?: string | null;
  file?: string;                           // NEW
  stars?: number;                          // NEW
  boxes?: LaneBox[];                       // NEW, same as LaneFrame.boxes
  af?: [number, number, number, number];   // NEW
};
```

- `file` lets today's lanes find their last finished thumbnail (`lane.file === recent[i].file`) and is the caption on the strip card.
- `stars`, `boxes`, `af` fill the **detail view** a click on a strip card opens (picture, boxes, AF box, rating facts). Without them the view says
  "No boxes were kept" and shows no stars.
- Keep the list at 24 or more: the strip shows 40 cards when it can.

## 4. Update rate

- `scan-progress` stays at about 4 per second (every 250 ms); `identify-progress` at a few per second. The UI renders every event: React
  reconciles by `lane` / `stage:worker_index`, so each event must carry the **whole** `workers` array (not a delta).
- Do **not** send more often when a step changes. A lane's step can change many times per second at 50+ frames/s, so the ticker's snapshot (last state
  wins) is the right granularity. A frame that passes through a lane between two ticks is simply never seen on that lane; it still arrives in `recent`.
- Payload size: `thumb_url` is a path, not bytes. Boxes are small (at most `max_vehicles_per_frame` per lane). Estimate: under 3 KB per lane per event.
- `recent` needs only the frames finished since the last event plus the previous 23: send the whole list as today.

## 5. What the UI derives (nothing for the engine to do)

- Overall bar: the scan's `overall.fraction`; with a vision phase 70% scan + 30% identify (`ingestFraction`). A scan that lists an `identify` stage
  with `total > 0` is treated as having a vision phase.
- Phase word: `Scanning` / `Grouping` (scan `stage: 'group'`) / `Picking` / `Identifying` (identify run running, or scan `stage: 'identify'`) / `Complete`.
- Time left: the scan's smoothed ETA during the scan; the identify run's own pace (`elapsed_ms`, `done`, `total`) during vision.
- Flowchart node text: `done / total` from `pipeline.stages[]`, busy/total workers, the stage `model`. The VISION node's model is `identify-progress.model`.
- Clicking a flowchart node filters the grid to that stage's lanes (by `PipelineWorker.stage`).

## 6a. What the engine sends now (U2)

Engine and UI both implement the contract; the following goes beyond or narrows it:

- `pipeline.workers[]` has `lane` (dense 0..N-1 over every lane listed) and `current` (`null` when idle). Lanes publish into the shared lane slot
  (`Progress::lanes`), which `snapshot()` clones: no DB query, no decoding. The slot is dropped when the lane moves to another file or goes idle.
- `current.step` is `read` | `decode` | `detect` | `sharpness` | `faces` | `persist`; `persist` is the write of the frame's row. `detect:wait` and `faces:wait`
  report as `detect` / `faces` with the lane `waiting` and `waiting_for` `detector` / `faces`.
- `current.thumb_url` appears right after the preview is decoded: the lane writes the thumbnail (`cache/native/thumb-<id>.jpg`) at that moment and
  `persist_frame` does not write it a second time. `current.af` (+ `af_kind`: `point` | `grid`) is sent with it, from the makernote already in memory.
- `current.boxes` are the detector's kept detections as fractions of the upright frame; `stars` / `verdict` (`sharp` | `soft` | `blurred`) / `panning`
  follow the frame's rating (display only; the same values the row stores).
- `recent[]` gains `file`, `stars`, `boxes`, `af`, `af_kind`; `verdict` can be `blurred`. (`found.soft` still counts everything short of sharp.)
- `pipeline.stages[]` gains `errors`: frames that failed inside that stage this run. A failed frame leaves that stage's and later stages' `total`, and a hand-rejected
  frame leaves all of them, so a finished stage reads `done == total`. A resumed album counts the frames its earlier run finished in both `done` and `total`.
  `group` / `pick` carry `total` = frames in the album and settle to `done == total` when their pass ends.
- Not sent: `state: 'paused'` (lanes finish their frame and idle before pausing, so a paused scan shows idle lanes; the UI uses the task state), `waiting_for` `writer` /
  `disk` (the DB and disk waits are not instrumented).

## 6. Compatibility notes

- A lane `state: 'paused'` is unknown to older UIs; they treat unknown states as idle. Ship the UI first (this change), the engine second.
- `PipelineStage` and `identify-progress` are unchanged.
- The scan's one-shot identify uses the same `identify-progress` event as a standalone run; the UI picks it up by `job_id`.
  If the scan reports its `identify` stage as running before the first `identify-progress` arrives, the page shows VISION as active and an empty vision grid for a moment.
