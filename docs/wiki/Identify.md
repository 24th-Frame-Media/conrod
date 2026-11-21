# Identify

**November 2026**

[Home](Home) · Previous: [How culling works](How-culling-works) · Next: [Lightroom integration](Lightroom-integration)

Identify is the second lane: it reads what a culling run found. It can run automatically after a scan, on demand from Review, or on its own for an album scanned with **Cull only**.

## What it reads

- **Race numbers** — from the car's panels, with a check against your entry list.
- **Number plates** — found and read by dedicated plate models.
- **Make, model, colour, team** — from the vision model.
- **Sponsors and livery** — as visible text, split into sponsors and team.
- **Names** — from faces, on portrait, event, court sport, and wedding types.
- **Jersey numbers** — on field and court sports, read from person boxes by the players lane.

On a motorsport type the car is the subject. On a people type the person is. On aviation the registration, type, and operator ride the plate, make/model, and livery lanes.

## Vision on and off

If a vision model runs, it reads the crops. If vision is off, Conrod still finds cars, measures sharpness, groups bursts, suggests keepers, and reads plates and race numbers with OCR. Make, model, colour, and team stay empty, and numbers are marked as read by OCR only.

The identify screen says why vision is off:

| Reason | Meaning |
|---|---|
| `album` | Switched off for this album. |
| `settings` | Switched off in Settings, and the album did not choose otherwise. |
| `no_service` | On, but no vision server is enabled. |
| `lanes` | This shoot type has nothing to ask the vision model. |

The album can follow the global switch or set its own. An album's own choice wins.

## Vision services

Add one or more servers in **Settings > Identification > AI models**:

- **Ollama** on this PC or another machine on the network.
- **OpenRouter** with an API key. **Test key** checks the key without spending a model call.
- **OpenCode Go** with a subscription key.

You can enable several. **Load balancing** chooses least busy, round robin, or random. The model named on the identify screen is the one that will answer.

A built-in vision model is planned, not shipped. See [Recommended setups](Recommended-setups) and [AI models in Conrod](../user/ai-models.md).

## OCR's role

Whole-crop OCR is the weakest number read. It can take digits out of a sponsor's livery, so it is demoted:

- If the vision model ran, the shoot is not a race shoot, and OCR disagrees with the model, the OCR number is discarded.
- If you have an entry list, an OCR number that is not on the list is treated as livery or sponsor text, not a number.
- With vision off, OCR is the only number reader; its read is kept and clearly marked as OCR only.

The plate reader is separate from race numbers and keeps working either way.

## Race numbers and the tiebreak

A car can show several numbers: door, nose, windscreen. Conrod reconciles them:

1. An **entry-list** number the vision model read is trusted on its own.
2. An OCR number the list does not contain is rejected.
3. If the model's number and OCR agree, confidence rises, but never above the model's own ceiling.
4. A unique **driver name** on the car can correct the number: car 2's nose board saying 1 loses to "Wood" and "Evans" on the windscreen. A team alone proves nothing, because a team runs two cars.
5. Across a burst, the number read most often on the most prominent panels settles a tie. This keeps one car's identity consistent through the pass.

Every read leaves a trace in the identify feed: what the model said, what OCR said, what the entry list matched, and what was merged.

## Scopes

- **Keepers (suggested)** — the default. Reads the frame you picked by hand, or, when a burst has no hand pick, the engine's suggested keeper. A hand star alone is not a keeper.
- **Everything** — every unreviewed, non-bystander subject, culled or not. Use it when you want metadata on frames you did not keep.

The scope is an album setting, and a single run can override it.

## Identify new only

After a redetect, a re-import, or **Skip identify**, some subjects are already read and some are not. **Identify new only** reads only the subjects nobody has read yet. A full identify reads the eligible subjects again.

## Skip identify

**Skip identify** accepts the album's identifications as they stand: every unread vehicle or person subject is marked read without a model call, so Review opens. Nothing is deleted. Subjects found later still count as unread and can be read with **Identify new only**.

## Watching a run

The Identify screen draws one panel per worker and a flowchart: LOAD, CROP, READ, VISION, ENTRY LIST, RESULT. The DECIDER node shows the decision model's questions and probabilities while the cars are read.

The feed survives after the run so you can read the summary. The next run replaces it. Failures are collected once, not repeated per crop.

## Reset

- **Reset identifications** removes identification results and keeps detections and hand work.
- **Reset detections** removes detections and crops and returns frames to pending.
- **Reset album** returns one album to just after import, keeping photographs, XMP sidecars, settings, the registry, and training labels.
