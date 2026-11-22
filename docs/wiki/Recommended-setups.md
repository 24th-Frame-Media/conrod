# Recommended setups

**November 2026**

[Home](Home) · Previous: [Settings reference](Settings-reference) · Next: [Troubleshooting](Troubleshooting)

For prices and limits, see [AI models in Conrod](../user/ai-models.md). Checked 22 November 2026.

## (a) A good graphics card: everything on this PC

For **12 GB of video memory or more**. Free to run; nothing leaves the PC.

1. Use **Ollama** with `qwen2.5vl:7b` as the vision server. The built-in vision model is planned; until it ships, Ollama is the local route.
2. With **16 GB or more**, also turn on **Use the decision model** and let it download. With 12–16 GB you can skip it and let the vision model judge moments.
3. Expect a local vision read around 10 seconds per car. A big shoot takes a long time; leave it running.

## (b) A laptop or a weak card: use the cloud

For **under about 6 GB of video memory**, or no graphics card. Cloud reads cars quickly and costs cents.

1. Add **OpenRouter** or **OpenCode Go** as the vision server.
2. Leave the on-device decision model off. You can select JEV through OpenCode Go for moment checks; otherwise the vision model may judge them or the lane is skipped. Neither choice demotes a frame.
3. Detection runs on the processor if there is no card, which is slower. Conrod tells you at start-up.

With no card and no key, Conrod still reads plates and race numbers with OCR, but not make or model.

## (c) Mixed: an 8 GB card with a cloud helper

For many gaming laptops. The vision and decision models are each too big to sit on the card together.

1. Put the **vision model in the cloud** (OpenRouter or OpenCode Go), leaving the card for the detector.
2. Put the **decision model on the card**, in the smaller 4-bit version if offered. Or skip it and let the cloud vision model judge moments.
3. With no key: keep vision local, and let the same model judge moments. It is slow but fits.

Conrod chooses between these when a scan starts and tells you what it chose.

## Cloud models

Try OpenCode Go first: it is probably the best bang for your buck. Its subscription allowance covers car reads and JEV moment checks (free or paid) with one key, instead of per-token billing.

- **OpenRouter**: `z-ai/glm-5.3-flash` is the cheapest sensible start. Prices per 1,000 photos are in the main guide.
- **OpenCode Go**: use **GLM-5.3-Flash** (`glm-5.3-flash`). Do not use DeepSeek V4.1 Flash for cars; it kept thinking and never answered in testing.
- Check a new model's first 20 readings before using it for a whole shoot.

## Privacy

- Photo files never leave the PC.
- Cloud vision sends one cropped car (or person) at a time, plus a short instruction.
- The decision model always runs on this PC.
- Keys are stored unencrypted in `settings.json`. Use a credit limit.

For weddings, children, or anything under a privacy agreement, keep vision local or off.

## What works with no AI at all

Plates, race numbers via OCR, sharpness, bursts, suggested keepers, hand rating, and stars/picks/colour labels written to XMP. You lose make/model/colour/team and calibrated moments.
