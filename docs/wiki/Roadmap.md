# Roadmap

**November 2026**

[Home](Home) · Previous: [Troubleshooting](Troubleshooting)

This page separates what is **built** from what is only **planned**. Plans change; nothing here is a promise of a date.

## Built and in main

- **Scan queue** — one album at a time, waiting albums visible, **Unqueue**.
- **Start-up splash** with real steps and unfinished-album detection.
- **Vision inheritance** — an album follows the global switch or sets its own — and an explicit reason when vision is off (`album`, `settings`, `no_service`, `lanes`).
- **AI models Settings card** — every model's job, location, state, and switch.
- **OpenRouter and OpenCode Go vision**, with a key test that spends no model call.
- **Decision-model transports** — local and OpenRouter, with Auto choosing from what is ready.
- **OCR demotion** behind the vision model, entry-list checks, and a race-number tiebreak across panels and a burst.
- **Identified albums** — skip identify, and identify new subjects only.
- **Detector fixes** — sharp AF frames kept, cars/trucks/buses as one vehicle class, a confidence floor for big cars, the AF-selected car as main subject, background cars dimmed, composition never culls.
- **Decision model sees the whole frame and the crop**, and runs live during identify.
- **Lightroom plug-in auto-install**, with suggested stars, picks, and colour labels applied to the catalog.

## Planned, not built

### Burst pipeline

Scan a shoot as bursts rather than frame by frame: group by camera and time from EXIF first, then detect and measure each unit, and read a car until two reads agree or three attempts are spent. It also formalises the main/background role per box.

Design: [burst pipeline](../plans/burst-pipeline.md).

### Built-in Qwen2.5-VL

Ship Qwen2.5-VL 7B inside Conrod so no Ollama install is needed. Expected download about 5.2 GB and about 6 GB of video memory; it becomes the default only when its readings match Ollama's.

### A smaller decision model

A smaller on-device decider for cards that cannot hold the current one beside the detector.

Design: [small decider](../plans/small-decider.md).

### JEV / OpenCode decider

An alternative decision-model route through OpenCode/JEV is being developed on a branch. It is not in main and is not documented as available yet.

### Other-camera autofocus

The scan reads the focus area on Canon bodies today. Nikon, Sony, and other bodies fall back to the sharpest car. Reading their focus metadata is on the backlog; until then the fallback is deliberate, not a bug.

## Research, not scheduled

A top-down detector for drone shots, a wildlife detector beyond COCO's classes, a saddle-cloth number crop for equestrian and rodeo, and calibration of vision-model moment probabilities. The research notes live under `docs/research/`; the open gaps are listed at the end of [Shoot types](../shoot-types.md).

## How to read the plan documents

Files under `docs/plans/` describe designs, including alternatives that were not chosen. They are not user documentation. When a plan ships, its behaviour is described here and in the API reference.
