# Ingest flow review (2026-11-19)

Haiku read-only map of the ingest path (folder pick -> scan -> post-scan -> Review), spot-checked by Opus.
The section numbers match the original report. Effort: S/M/L. Accuracy risk relates to sharpness, burst and keeper results.

## Headlines
- **No card import.** Import indexes the folder in place (`desktop.rs:825`, `lib.rs:1061`). Nothing is copied, verified or ejected.
- **Re-importing a folder makes a second album.** `create_job` is unconditional (`desktop.rs:900`).
- **A crash mid-scan strands the album.** It stays `status='scanning'`, which hides Resume (`Library.tsx:30`) and still lets Review open (`App.tsx:53,123`).
- **One unreadable file fails the whole album.** `incomplete` counts `status!='done'` (`desktop.rs:1052-1062`).
- **Library Resume drops the stage.** `ScanArgs.stage` defaults to Cull, so identify is skipped (`commands.rs:89`, `desktop.rs:971,1014`).
- **The pipeline view is partly fiction.** read, detect, sharpness and faces all advance together after each frame (`lib.rs:1301-1304`), so per-stage rates are identical and `queue_len` is always 0. The worker counts shown are lane counts, not the real pools (2 detector sessions; one face Mutex).
- **AF is not part of ingest.** `focus_points` is never run by the scan and has no frontend caller. The default "Anchor on the focus area" grouping, and the AF rescue, are no-ops until someone runs it. Confirmed: job 109 has 0 of 847 frames with an AF box.

## 1. Current flow
```mermaid
flowchart TD
  A["Library: Import / drop folder"] --> B["ImportDialog (Import.tsx:34-148)"]
  B --> C["startScan -> invoke('command','scan') (App.tsx:89, api.ts:30)"]
  C --> E["start_scan (desktop.rs:781-905)"]
  E --> F["files_with_recursion: directory walk (lib.rs:76-119)"]
  F --> G["create_job: INSERT jobs, no duplicate-root check"]
  G --> H["add_images: INSERT OR IGNORE, one tx"]
  H --> I["jobs.status='scanning' (desktop.rs:918)"]
  I --> J["load models; detect_workers sessions; faces/expression/quality per scan (lib.rs:1103-1175)"]
  J --> K["N cull lanes, up to 16 (lib.rs:1229-1310)"]
  K --> L["cull_frame: raw::read -> full-size JPEG decode -> detector Mutex -> sharpness/streaks/framing/faces/exposure (lib.rs:699-1023)"]
  L --> M["persist_frame: thumb, then tx per frame (desktop.rs:1764-1799)"]
  M --> Z["post thread waits for lanes (poll 100 ms)"]
  Z --> R["regroup, writer lock held throughout (desktop.rs:1883-2107)"]
  R --> X{"one-shot and identify?"}
  X -- yes --> Y["identify (VLM)"]
  X -- no --> TA["taste::after_scan"]
  Y --> TA
  TA --> U["status done / error / stopped -> Review unlocks"]
```

```mermaid
sequenceDiagram
  participant UI
  participant TK as Tauri ticker 250 ms
  participant EN as Engine
  participant PR as Progress
  UI->>EN: invoke scan
  EN-->>UI: jobId at once
  loop every 250 ms
    TK->>PR: snapshot()
    TK-->>UI: scan-progress (7 nodes, lanes, recent, found, overall)
  end
  loop each frame
    EN->>PR: busy(stage, file) / waiting('detect') / recent(thumb, verdict)
  end
  EN->>PR: phase group, then pick (no per-item progress); identify; taste (no node)
  TK-->>UI: final state done/stopped/error
```

## 2. Concurrency as built
```mermaid
flowchart LR
  LN["Cull lanes x N (<=16)"] -- "per crop" --> D1[["Detector session 1"]]
  LN -- "per crop" --> D2[["Detector session 2"]]
  LN -- "every face" --> FC[["FaceDetector Mutex, all lanes"]]
  LN -- "every face" --> EX[["Expression Mutex, all lanes"]]
  LN -- "persist: SELECT id + tx per frame" --> WR[["DB writer Mutex"]]
  LN -- "eligibility SELECT per path" --> RD[["DB reader Mutex"]]
  RG["Post-scan regroup"] -- "holds the writer through thumbnail decodes + SQL" --> WR
  TW["Watcher"] -- "add_images" --> WR
```
Frames wait on the detector sessions (about 8 lanes per session at defaults), on the face and expression Mutexes (this wait is not reported), and on the writer lock while regroup runs. Paused lanes still show `busy`.

## 3. Pain points
Numbering follows the original report; the items not listed here are covered by the headlines.

| # | Pain point |
|---|---|
| 5 | Files changed under the same name are never re-read (`INSERT OR IGNORE`, resume skips `done`). |
| 6 | Files added during a scan are missed: the watcher is off while scanning. |
| 7 | Stopped albums are never grouped but Review still opens. |
| 8 | Pause is not shown in the lane view. |
| 10 | `preview_workers` doesn't affect the scan. |
| 11 | Group and pick have no per-item progress, and taste has no node. |
| 13 | The worker default differs between `worker_limits` and `cull_workers`. |
| 15 | Dead time before the first thumbnail (model loads) and after the last frame (regroup, identify, taste). |
| 17 | Full-resolution decode per frame. |
| 18 | `raw::read` opens each file twice (rawler metadata). `focus_points` re-reads every RAW. |
| 19 | Detect is the serial stage, and its sessions are reloaded every scan. |
| 21 | One commit and one `SELECT id` per frame. No `cache_size` or `mmap_size` PRAGMAs. |
| 22 | Regroup holds the writer lock. |
| 25 | The watcher stats the whole tree every pass (11 s on 9,500 USB files). |
| 28 | `focus_points` falls back to exiftool on the bare PATH, with no presence check first. |

## 4. Proposed flow
```mermaid
flowchart TD
  subgraph Source
    A1["Card or folder"] --> A2{"Copy first? (optional)"}
    A2 -- yes --> A3["Copy to album dir + verify (size + hash)"]
    A2 -- no --> A4["Index in place"]
  end
  A3 --> B1
  A4 --> B1
  subgraph "Fast pass"
    B1["Header pass: EXIF, taken_at, AF box (native)"] --> B2["Batch INSERT (500/tx)"]
    B2 --> B3["Grid visible at once; provisional time bursts"]
  end
  subgraph "Streaming scan"
    C1["Reader pool: 1 read + 1 decode per file, bounded queue"] --> C2["Detect pool: K sessions"]
    C2 --> C3["CPU pool: sharpness, faces, exposure"]
    C3 --> C4["Batch writer: 50 frames/tx, checkpoint"]
  end
  B3 --> C1
  C4 --> D3["Regroup off the writer lock; identify queue; taste"]
  D3 --> E["Ready: Review unlocks"]
  C4 -. "failed file" .-> F["Done with N errors: Review opens, Retry list"]
```

```mermaid
stateDiagram-v2
  [*] --> Indexed
  [*] --> Scanning
  Indexed --> Scanning: Suggest culls
  Scanning --> Stopped: Stop
  Scanning --> Stopped: app restart finds stale 'scanning' (P1)
  Stopped --> Scanning: Resume, keeping the stage (P3)
  Scanning --> Grouping: all frames done
  Scanning --> DoneWithErrors: some frames failed (P2)
  DoneWithErrors --> Scanning: Retry failed
  Grouping --> Identifying: workflow wants identify
  Grouping --> Ready
  Identifying --> Ready
  Ready --> Scanning: watcher adds frames
```

| # | Proposal | Effort | Accuracy risk |
|---|---|---|---|
| P1 | On startup, reconcile stale `scanning` jobs to `stopped`; gate Review on status | S | none |
| P2 | Album finishes as done-with-errors; Review opens; Retry list for failed files | S | none |
| P3 | Resume keeps the album's stage | S | none |
| P4 | Honest per-stage counters and real pool sizes; identify shown `off` when unused; taste node; group/pick progress | S-M | none |
| P5 | Paused lanes shown as waiting | S | none |
| P6 | Wire `preview_workers` into the scan or remove it | S | none |
| P7 | Reader pool, then bounded detect pool, then CPU pool | M | none |
| P8 | Keep model sessions across scans | S | none |
| P9 | Measure before reducing decode size | S | high if changed |
| P10 | Header fast pass for an instant grid | M | none (provisional bursts only) |
| P11 | Batched writer, cached ids, PRAGMAs | M | none |
| P12 | Regroup off the writer lock; `look` at persist time | M | none |
| P13 | Re-import of a known root offers "add new frames" | S | none |
| P14 | Optional verified card copy (single-pass later) | M-L | none |
| P15 | AF box read natively at scan time, replacing the manual exiftool pass | M | low |
| P16 | Streaming group and pick per closed burst | L | low-med |
| P17 | Watcher stats only new names | S | none |
| P18 | Removable-drive awareness | S-M | none |

## 5. Top 8
1. P1 + P2: crash reconcile and done-with-errors.
2. P3: Resume keeps the stage.
3. P4 + P5: honest pipeline view.
4. P13: re-import guard.
5. P10 + P11: instant grid and batched commits.
6. P7 + P8: read/decode pool feeding the detect pool; models kept loaded.
7. P12: regroup off the writer lock.
8. P14, then P15: verified card copy, then AF in the scan.

Measure per-stage time per frame first; the honest counters in P4 provide those numbers.
