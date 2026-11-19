# The detector's small-subject hole: a tiled second pass

Status: follow-up, not implemented. The gated focus-grid rule (8fb3f4e) buys
the 0202 case back without it; this is the general fix.

## What the detector misses

Album 109's 0202 is a Torana filling about a third of the frame. YOLO11s at the
scan's 960-edge input finds it as `car 0.380` and a background `truck 0.492`;
the album's confidence cut is 0.55, so both are dropped and the frame is rated
on the truck that *did* clear the cut on the neighbouring frame. 0201 gets
`truck 0.649`, misclassed but kept.

Measured on 0202 (detectdiag, the network run raw with all 80 classes at a 0.05
floor):

| pass | boxes >= 0.05 | car | truck |
|---|---|---|---|
| full frame, edge 960 | 2 | 0.380 | 0.492 |
| full frame, edge 1280 | 2 | 0.123 | 0.409 |
| full frame, edge 1536 | 2 | 0.117 | 0.438 |
| 2x2 + centre tiles, edge 960 | 12 | 0.834, 0.716 | 0.620 |
| 2x2 + centre tiles, edge 1280 | 9 | 0.771, 0.601 | 0.598 |

Two facts matter. A bigger input edge does not help: the car is already the
largest thing in the box and the 960 net resizes away what little the body's
edges need. A crop does help, because the car then spans the tile, which is the
scale the network was trained on. Cost is about +1.1 s a frame (five tiles,
network only; read and decode are the same), on frames that take ~1 s now.

## Why the focus grid is only a stop-gap

The rule in 8fb3f4e trusts the camera's in-focus cells only when every
vehicle/person detection is under 5% of the frame, so it can stand in for a
subject the detector missed. On album 109 it acts on 14 of 847 frames (12
ratings change, 0202 0.690 -> 0.924). It cannot help a frame where the detector
found something real *and* missed the subject (0202's truck was 3%, so it
qualified; a 20% bystander would not), and it cannot help a body with no AF
grid or no AF metadata at all. The detector fix helps every body and every
frame.

## Plan

1. Second detection pass, cropped, only where it can matter: when the first
   pass's best vehicle/person detection is under `dominant_subject_fraction`,
   or when the frame has no subject at all but the AF grid is usable. Five
   overlapping crops (four quadrants with 10% overlap, plus the centre half)
   at the scan's input edge, mapped back to frame coordinates, class-wise NMS
   at IoU 0.45.
2. Merge with the first pass before anything else reads detections, so the
   subject list, the frame rating and the burst pick see one set of boxes.
   Boxes that touch a crop edge are partial: drop a merged box that came only
   from a crop edge unless the first pass has it too (they are what made the
   tile output look good at conf 0.83 while covering only part of the car).
3. Budget: +1.1 s a frame on the frames that qualify. Gate it behind a
   setting (`detect_tiles`, default on for motorsport/wildlife presets) and
   keep the whole pass inside the existing per-frame worker, not a second
   queue.
4. Measure on album 109 and the readiness suite: keeper rate on 0201/0202,
   pick vs random, and the false-positive count from tiled backgrounds (crowd
   faces, parked cars). The tiles at 960 also found 12 boxes where the full
   pass found 2, most of them small and low-confidence; the class and
   confidence cuts, not the tile pass, must keep them out.

## Open questions

- Does the tiled pass need its own confidence floor (the full pass is cut at
  0.55; the tiles produced 0.834 and 0.716 for the car, so 0.55 is fine, but a
  background-heavy tile may not be)?
- Should the second pass run only when the AF grid or the frame's own sharpness
  says the picture is worth another 1.1 s? A culled frame does not need it.
- `detect_imgsz` is per album; a smaller edge with tiles may be both faster and
  better than 960 full-frame, which is worth a sweep before shipping.
