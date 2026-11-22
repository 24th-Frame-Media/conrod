# Getting started

**November 2026**

[Home](Home) · Next: [Albums and scanning](Albums-and-scanning)

## 1. Install

Download the latest release:

- **Installer** (`Conrod-<v>-win64-setup.exe`): per-user install, no admin rights.
- **Portable** (`Conrod-<v>-win64.zip`): unzip and run `Conrod.exe`.

Builds are not code-signed. When SmartScreen appears, choose **More info > Run anyway**.

Conrod keeps its database, settings, models, and Lightroom bridge packages in `%USERPROFILE%\.conrod`.

## 2. First launch

The splash shows these steps:

1. **Library** — opens the database and applies updates.
2. **Unfinished albums** — albums a crash or a close left half-scanned are put back to *stopped* and named, so you can resume them.
3. **Models** — checks which model files are on disk and starts downloading missing ones.
4. **ExifTool** — finds the bundled ExifTool.
5. **Graphics card** — reads its memory so Conrod can plan vision and decision work.

The detector, face, similarity, and quality models load on the first scan, not at start-up.

## 3. The setup wizard

On first run the wizard asks:

- which shoot types you photograph (motorsport, portraits, weddings, and so on);
- how strict the blur check should be;
- who is in your pictures, for each motorsport type;
- whether moments should be spotted and whether they may lift ranking.

Your answers become global defaults copied into each new album. Skipping the wizard changes no defaults and it will not open again automatically. Run it later from **Settings > Set up Conrod again**.

## 4. Your first scan

1. Press **New scan**.
2. Choose the folder. Subfolders are included by default.
3. Pick the **shoot type**. This decides what Conrod looks for, what it reads, and which moments it understands.
4. Leave **Scan and cull** on for a normal first pass. **Cull only** skips identification; **Identify only** skips the automatic cull.
5. Press **Start**.

The **Ingest** tab shows the live flowchart and scan log. When the scan ends, **Review** opens on the suggested keepers. The first scan of a large album takes longest; later passes reuse stored results.

## 5. What you get

- Sharpness and stars for the main subject, with a reason when a frame is culled.
- One suggested keeper per burst, unless you pick your own.
- Detections you can edit: make, model, colour, team, race number, plate, sponsors, names.
- Plates and race numbers even with the vision model off.
- XMP sidecars from **Write**, or a package for the Lightroom plug-in from **Export to Lightroom**.

## 6. Optional: Lightroom

Conrod writes stars, picks, and colour labels into your files. Lightroom reads them with **Read Metadata from File** or on import. To use the optional plug-in, turn on Settings > **Lightroom** > **Install the Conrod Bridge plug-in**, restart Lightroom Classic, then open **Library > Plug-in Extras**. See [Lightroom integration](Lightroom-integration).

## 7. Optional: a vision model

For make, model, colour, team, and stronger race-number reads, add a vision server:

- **Ollama** on the same PC (`ollama pull qwen3-vl:8b-instruct-q4_K_M`, Conrod's local recommendation), or
- **OpenRouter** or **OpenCode Go** with a key.

Open **Settings > Identification > AI models**. The model guidance, costs, and privacy notes are in [AI models in Conrod](../user/ai-models.md).

Without a vision model, Conrod still finds cars, measures sharpness, groups bursts, suggests keepers, and reads plates and numbers with OCR.
