# Phase 0 spikes

What was tried, on what, and what it decided. Numbers are from the owner's
laptop (RTX 3070 Ti Laptop, 20 threads) on real shoots, not synthetic data.

## C. Reading RAW files without ExifTool: go

`spikes/rawprobe`, using `rawler` 0.8. 100 files from one shoot: 60 CR3 from
an EOS R7, 40 CR2 from an EOS 80D.

| | CR3 | CR2 |
|---|---|---|
| Errors | 0 / 60 | 0 / 40 |
| Camera identity matches exiftool (model + serial, as `bursts.camera_of` builds it) | 60 / 60 | 40 / 40 |
| Capture time matches exiftool to the sub-second | 60 / 60 | 40 / 40 |
| Full-size embedded preview found | 6960×4640 | 6000×4000 |
| Open + metadata | ~20 ms | ~20 ms |

ExifTool today: ~40 ms per file for the tag read plus ~167 ms per file for
preview extraction. `rawler::preview_image` hands back a *decoded* image
(~200 ms at full size), so the port reads the JPEG bytes itself -- the CR3
sample offset from `stbl`, the CR2 IFD strip -- and decodes them scaled.
ExifTool stays for writes, per the plan.

## E. Frame to boxes, in Rust: go, on DirectML

`spikes/detbench`: `ort` 2.0.0-rc.13 (ONNX Runtime 1.28), `jpeg-decoder`,
the YOLO11s export from `tools/export_onnx.py`, 50 real frames, checked
against the 259 boxes ultralytics finds on them.

Inference plus post-processing, per frame, warm:

| Path | ms / frame |
|---|---|
| Python, ultralytics + torch, CPU (today) | 665 |
| Python, ONNX Runtime, CPU | 543 |
| Rust, ONNX Runtime, CPU | 130-148 |
| **Rust, ONNX Runtime, DirectML (GPU)** | **17-18** |
| Rust, CUDA | unavailable without the CUDA 13 runtime installed |

DirectML needs nothing installed (its DLL ships beside the exe) and runs on
NVIDIA, AMD and Intel GPUs, so it is the default device and CUDA is not
pursued.

Decode and letterbox, parallel over 20 threads:

| JPEG decode | frames / s | boxes matched (IoU ≥ 0.9 / ≥ 0.5) |
|---|---|---|
| full size | 15 | 255 / 255 of 259 |
| 1/2 (DCT scaling) | 24 | 249 / 254 |
| 1/4 | 35 | 237 / 250 |

**Decision:** decode at 1/2 for detection. It is within five boxes of the
reference at IoU 0.5, where 1/4 starts missing cars. With DirectML the
detector is no longer the bottleneck -- JPEG decode is -- so the cull lane is
bounded near 24 frames/s here, against ~1 frame/s today. A faster decoder
(SIMD `zune-jpeg`, or libjpeg-turbo) is the next lever if a slower machine
needs it.

The four boxes the full-size path misses at IoU 0.9 come from decoder and
resize rounding (`jpeg-decoder` vs libjpeg-turbo, float vs OpenCV's fixed
point bilinear); none is a missed car.

**Budget ratified:** cull throughput ≥ 10 frames/s holds with margin on this
machine. Re-check on an 8-thread laptop without a discrete GPU before
promising it generally.

## Not yet run

- **A. RapidOCR in Rust** -- identification lane, needed before Phase 2 step 8.
- **B. egui thumbnail grid** -- before Phase 5.
- **D. Faces and eyes** -- waiting on portrait sample frames from the owner.
