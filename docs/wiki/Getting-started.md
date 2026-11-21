# Getting started

**November 2026**

[Home](Home) · Next: [Albums and scanning](Albums-and-scanning)

## 1. Install

Download the latest release:

- **Installer** (`Conrod-<v>-win64-setup.exe`): per-user install, no admin rights.
- **Portable** (`Conrod-<v>-win64.zip`): unzip and run `Conrod.exe`.

Builds are not code-signed, so Windows SmartScreen asks once. Choose **More info > Run anyway**.

Conrod keeps its database, settings, models, and Lightroom bridge packages in `%USERPROFILE%\.conrod`.

## 2. First launch

The splash reports real work, not a decorative progress bar:

1. **Library** — opens the database and applies updates.
2. **Unfinished albums** — albums a crash or a close left half-scanned are put back to *stopped* and named, so you can resume them.
3. **Models** — checks which model files are on disk and starts downloading missing ones.
4. **ExifTool** — finds the bundled ExifTool.
5. **Graphics card** — reads its memory so Conrod can plan vision and decision work.

The detector, face, similarity, and quality models are not loaded at start-up; they load on the first scan. The splash never pretends otherwise.

## 3. The setup wizard

On first run the wizard asks:

- which shoot types you photograph (motorsport, portraits, weddings, and so on);
- how strict the blur check should be;
- who is in your pictures, for each motorsport type;
- whether moments should be spotted and whether they may lift ranking.

Your answers are stored as global defaults and copied into each new album. A skipped wizard changes nothing and never opens by itself again. **Settings > Set up Conrod again** runs it later.

## 4. Your first scan

1. Press **New scan**.
2. Choose the folder. Subfolders are included by default.
3. Pick the **shoot type**. This decides what Conrod looks for, what it reads, and which moments it understands.
4. Leave **Scan and cull** on for a normal first pass. **Cull only** skips identification; **Identify only** skips the automatic cull.
5. Press **Start**.

The **Ingest** tab shows the live flowchart and the scan log. When the scan ends, **Review** opens on the suggested keepers. The first scan of a large album is the slow one; later passes reuse what is stored.

## 5. What you get

- Sharpness and stars for the main subject, with a reason when a frame is culled.
- One suggested keeper per burst, unless you pick your own.
- Detections you can edit: make, model, colour, team, race number, plate, sponsors, names.
- Plates and race numbers even with the vision model off.
- XMP sidecars from **Write**, or a package for the Lightroom plug-in from **Export to Lightroom**.

## 6. Optional: Lightroom

Conrod writes stars, picks, and colour labels into your files by default, so Lightroom sees them after **Read Metadata from File** or on import. The plug-in is optional: turn on Settings > **Lightroom** > **Install the Conrod Bridge plug-in**, restart Lightroom Classic, then use **Library > Plug-in Extras**. See [Lightroom integration](Lightroom-integration).

## 7. Optional: a vision model

For make, model, colour, team, and stronger race-number reads, add a vision server:

- **Ollama** on the same PC (`ollama pull qwen2.5vl:7b`), or
- **OpenRouter** or **OpenCode Go** with a key.

Open **Settings > Identification > AI models**. The model guidance, costs, and privacy notes are in [AI models in Conrod](../user/ai-models.md).

Without a vision model, nothing breaks: Conrod still finds cars, measures sharpness, groups bursts, suggests keepers, and reads plates and numbers with OCR.
