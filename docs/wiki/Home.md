# Conrod wiki

**November 2026**

Conrod is a Windows app for culling and keywording motorsport, automotive, and event photos. It scans RAW or JPEG folders, finds cars and people, measures sharpness, groups bursts, suggests keepers, reads race numbers and plates, and can identify make, model, colour, team, and names. It writes metadata to `.xmp` sidecars; RAW files stay unchanged.

Conrod works without AI models. Detection, sharpness, bursts, and suggested keepers run on the PC. The vision model adds make, model, colour, and better race-number reads; the decision model adds calibrated moments.

## Start here

- [Getting started](Getting-started) — install, first launch, the wizard, your first scan.
- [Albums and scanning](Albums-and-scanning) — shoot types, the scan queue, stop and resume, unfinished scans.
- [How culling works](How-culling-works) — sharpness, the autofocus subject, background cars, bursts, moments.
- [Identify](Identify) — vision services, OCR, race numbers, skip identify.
- [Lightroom integration](Lightroom-integration) — stars/picks/labels in your files, the optional plug-in, folder layout, TO-IMPORT.
- [Settings reference](Settings-reference) — the AI models card and per-album overrides.
- [Recommended setups](Recommended-setups) — local, cloud, and mixed.
- [Troubleshooting](Troubleshooting) — what the common messages mean.
- [Roadmap](Roadmap) — what is built and what is planned.

## What is new in this build

- Scan queue: start several albums; they scan one at a time. Remove a waiting album with **Unqueue**.
- Start-up splash with real steps: library, unfinished albums, models, ExifTool, graphics card.
- AI models card in Settings, with vision and decision model rows.
- Vision inheritance and an explicit reason when vision is off.
- OpenRouter and OpenCode Go vision providers; OpenRouter decision-model transport.
- OCR demoted behind the vision model, with a race-number tiebreak across panels and a burst.
- Identified albums can skip identification or run **Identify new only**.
- Detector fixes: sharp autofocus frames kept, cars/trucks/buses as one vehicle class, a confidence floor for big cars, AF-selected main subject, dimmed background cars, composition never culls.
- The decision model sees the whole frame and the crop, and runs live during identify.
- Conrod writes suggested stars, picks, and colour labels to your files; the Lightroom plug-in is optional.

Detailed model guidance, prices, and provider steps are in [AI models in Conrod](../user/ai-models.md).

## Requirements

Windows 10 or 11, 64-bit, with WebView2. A graphics card is optional but makes detection much faster. A vision model needs either Ollama on the same PC or a cloud key.
