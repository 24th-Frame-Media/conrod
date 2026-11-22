# How culling works

**November 2026**

[Home](Home) · Previous: [Albums and scanning](Albums-and-scanning) · Next: [Identify](Identify)

Conrod may suggest, order, and annotate frames, but composition or moments never demote a sharp frame. You can override its decisions.

## Sharpness is measured, not guessed

The main sharpness check measures edge detail on the subject. It needs no model download, so it works with AI features off.

- The subject is measured at native resolution, so a soft frame and a sharp frame at the same distance are told apart.
- A held pan is treated as **sharp subject over a streaked background**, not as a blurred frame. The preset says whether pans are expected.
- **Blur strictness** is per album: lenient, moderate, strict, or critical.
- A frame below the album's bar gets a reason such as `too blurred`, `2 stars`, or a cut-off description.

**A sharp frame is never demoted.** Composition, framing, moments, and burst suggestions can reorder sharp frames but cannot pull one below the keeper bar. The inspector explains each change.

## The main subject

The camera's autofocus area helps Conrod decide what you pointed at.

1. If the autofocus centre sits inside a vehicle, that vehicle is the **main subject**.
2. If the focus area sits on no vehicle, the focus area is the subject and every detection becomes background.
3. If there is no usable autofocus data, the **sharpest vehicle** is the main subject, with ties going to the larger box.

Every other car in the frame is a **background car**. It is dimmed in Review and drives no rating, pick, cull, or identification. A parked car behind the subject cannot become the keeper just because it is sharp.

A focus grid is a depth-of-field map, not a point. Conrod trusts it only when the detector found only small objects, such as a distant truck or spectator. The scan reads focus areas on Canon bodies; other cameras fall back to the sharpest car. See the [roadmap](Roadmap).

## Composition ranks, never culls

Composition measures subject placement, edge cuts, balance, and background clutter. These values order frames within a burst but never lower a sharp frame's rating or cull it.

## Bursts and stacks

A burst contains frames from one camera taken close together in time. The per-album **burst gap** defaults to 1.5 seconds.

- **Grouping sensitivity** decides how readily a time burst is split into moments: off keeps one stack per burst, gentle is looser, normal is the measured default, eager splits more.
- A **pause** (0.8 s by default) starts a new moment when the picture also changed. Set it to 0 to disable pause cuts.
- **Merge look-alike bursts** joins neighbouring time bursts of one camera that look like the same subject a few seconds apart.
- **Stacks** gives every frame its own stack and suggests no keeper when you turn it off; a "best of stack" pick that marks every frame says nothing.

The suggested keeper is the sharpest frame above the bar. Burst peak and framing break ties. A hand pick suppresses the suggestion for that burst.

The separate **burst pipeline** — scanning a burst as a unit and reading it until two reads agree — is planned, not built. See the [roadmap](Roadmap).

## Moments

A moment is a shoot-specific event, such as tyre smoke, a first kiss, a jump apex, or a chequered flag. Each shoot type has a short vocabulary.

Two switches per album:

- **Spot moments** — ask the model and show tags.
- **Let moments lift ranking** — allow a sure moment to break ties.

**Threshold**: a moment at or above **0.85** (album setting) is *sure* and may lift a frame. Between 0.5 and the threshold it shows as **Maybe** and changes no ranking.

**What a moment can do**

- Break ties inside the same sharpness band.
- Give a burst's peak frame the tie-break.

**What it cannot do**

- Rescue an unsharp or culled frame.
- Demote a sharp frame.
- Change anything with ranking off.

### Who judges moments

- **Vision model** (default): the model reading the cars is also asked about the shoot's tags when they are clearly visible. It gives no calibrated probability: a confident answer becomes p = 0.9, an unsure one p = 0.6 (a maybe). These reads are marked *unverified*.
- **Decision model**: Perplexity's decision model returns calibrated probabilities. It asks one yes/no question per tag plus one overall strength question, and for a burst it compares up to eight downscaled frames to pick the peak.

### Where the decision model runs

- **Auto** (default): uses this PC when **Use the decision model** is on and ready; otherwise OpenRouter when a key is set; otherwise skips the lane with a note. Skipping never demotes a frame.
- **Local (this PC)**: force the on-device model.
- **OpenRouter**: force the OpenRouter transport.

The decision model sees the whole frame and the car crop, and it runs live during identify. Its questions and probabilities appear on the DECIDER node of the Ingest flowchart. On a small card (8 GB or less) running the 8-bit model, a decision call waits for the scan's detector and face passes instead of loading beside them.

A missing key, a timeout, or a broken reply leaves the frame unjudged. Nothing is demoted and the scan does not wait.

## Your decisions win

A hand star, pick, or reject is yours. **Identify new only** and the suggested keeper respect hand picks. Rescoring and regrouping preserve hand ratings. A rejected frame stays rejected until you clear it.
