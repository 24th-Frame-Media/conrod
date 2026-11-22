# Settings reference

**November 2026**

[Home](Home) · Previous: [Lightroom integration](Lightroom-integration) · Next: [Recommended setups](Recommended-setups)

Settings has five tabs: **Culling**, **Identification**, **Metadata**, **Lightroom**, and **System**. Most values have global defaults and per-album overrides. The vehicle registry is here too.

## Culling

- **Blur strictness** — lenient, moderate, strict, or critical. Per album.
- **Burst gap** — seconds between frames of one camera that still belong to one burst. Per album, default 1.5.
- **Sharp at / blurred below** — the measurement scale behind the star verdict. Changing this retires older thresholds rather than reinterpreting them.
- **Auto-reject below (stars)** — optional automatic cull.
- **Cull blurred frames** — whether the blur verdict writes a cull reason.
- **Default shoot type** — what new imports start on.
- **Cars / Bikes / Trucks / People** — the detector lanes. Cars, bikes, and trucks are one vehicle class for rating; the switches decide what is looked for.
- **Detection confidence** and **smallest subject** — how readily a box is accepted.
- **Vehicles per frame** — a cap for busy frames.
- **Moments**: judge with **Vision model** or **Decision model**, **moment confidence** (default 0.85), and **where the decision model runs** (Auto, Local, OpenRouter).

## Identification

### AI models card

Each model row shows its job, where it runs, its state, and a switch:

| Row | What it does | Where it runs |
|---|---|---|
| **Car & person detector (YOLO)** | Finds cars, bikes, trucks, and people; everything else starts here | This PC (GPU if present); always on |
| **Plate finder + plate reader** | Finds a plate and reads its characters | This PC |
| **Number/text reader (OCR)** | Reads race numbers, signs, and sponsor text; fills a number only when the vision model missed one | This PC |
| **Faces/eyes** | Finds faces and checks eyes; can help suggest names | This PC; needs People on |
| **Sharpness/quality** | The main sharpness measurement, plus an optional quality tiebreak | This PC |
| **Vision model** | Reads make, model, colour, team, and race number from a crop | This PC (Ollama), or the cloud (OpenRouter, OpenCode Go) |
| **Decision model** | Answers calibrated yes/no moment questions and picks a burst peak | This PC's graphics card |

The vision row takes **+ Add server** (Ollama, OpenRouter, or OpenCode Go), a model name, and a switch. **Use a vision model** is the global on/off. The decision row takes **Use the decision model**, a precision choice (8-bit or 4-bit), and, for the OpenRouter transport, a key and model.

See [AI models in Conrod](../user/ai-models.md) for provider setup, prices, and privacy.

### Readers and text

- **Read licence plates** — per album.
- **Read race numbers** — per album.
- **Read other text** — signs and incidental text.
- **Read action** — moment tags.
- **Use known vehicles** — borrow make/model/colour from the registry by plate.
- **Tidy make and model names** — normalise model spellings.
- **Entry list** — a CSV such as `number,driver,team,...`; a listed number is trusted over OCR.
- **Load balancing** — least busy, round robin, or random across vision servers.

### Workflow

- **Workflow mode** — both, cull only, or identify only.
- **Identify scope** — keepers or everything.

These two change what the next scan or identify run does; they do not regroup an album by themselves.

## Metadata

Controls what **Write** puts in the sidecar: rating, label, keywords, keeper/cull/burst prefixes, whether to embed in RAW, and XMP fields. A dry run logs proposed keywords without changing files or written timestamps.

## Lightroom

- Stars, picks and colour labels written to your files (the main path).
- The optional plug-in: its switch (off by default), status card (installed, version, needs update, Lightroom running), and **Install plug-in** / **Remove plug-in**.

See [Lightroom integration](Lightroom-integration).

## System

- Model files and downloads, including the decision model card.
- Cache information and clearing: thumbnails, previews, crops, orphaned files.
- Library data: album and frame counts, reset actions, and the statistics cards.
- Update check and install.

## Per-album overrides

Open an album's **Shoot settings** to override a value. Clearing an override follows the photographer's default, then the shoot type's policy. Each row shows the active default.

Rules:

- Changing a guard/grouping value regroups the album in place.
- Changing **workflow mode** or **identify scope** does not regroup; it affects the next run.
- An album takes a copy of the wizard defaults when it is created or its shoot type changes. Changing the global profile later does not move existing albums.
- Older albums have no copy and keep their previous behaviour.

## Where settings live

`%USERPROFILE%\.conrod\settings.json`, with the library database beside it. API keys are stored there **without encryption**. Use a provider key with a credit limit, and delete the key on the provider's site if you share the PC or the file.
