# Albums and scanning

**November 2026**

[Home](Home) · Previous: [Getting started](Getting-started) · Next: [How culling works](How-culling-works)

## An album is a folder

Each album points to one folder tree. Its name starts as the folder name; renaming the album does not rename the folder. The album stores its shoot type, switches, grouping choices, and results. Conrod never moves or changes the original photographs.

**New scan** creates and starts an album. Reopen it to resume, rescan, redetect, regroup, identify, or write without importing frames again.

## Shoot types

The shoot type decides:

- what Conrod looks for (cars, bikes, people, faces, animals);
- what it reads (race numbers, plates, make/model, livery, names);
- which moments matter (tyre smoke, a first kiss, a jump apex);
- how strictly blur is judged and whether panning is forgiven.

See the full table in [Shoot types](../shoot-types.md). Key defaults:

- **Motorsport** types look for cars and people. Faces are off until you ask, because helmets hide drivers. People stay on: crowds, pit lane, paddock.
- **Field and court sports** read jersey numbers from person boxes. Court sports also read names from faces.
- **Shows, events, parties, portraits, weddings, concerts, rodeo, equestrian, wildlife, property, landscape, and astro** leave vehicles off by default. Switch cars on for a car-heavy event.

You can change any switch for one album, or clear it to follow the shoot type again.

## The scan queue

Only one album scans at a time. Other albums wait in the queue:

- The Library shows each album's status: *queued*, *scanning*, *stopped*, or *done*.
- The queue runs first come, first served.
- **Unqueue** takes a waiting album out of the line. It keeps its frames and results and goes back to *stopped*, ready to start by hand.
- Asking again for an album already in the queue keeps its place; it does not jump the line.
- If an album in the queue is deleted before its turn, it is skipped.

The same queue is visible in `status.queue` for scripts and the CLI.

## Pause, resume, stop

While a scan runs:

- **Pause** stops work but keeps everything already measured.
- **Resume** continues from where it stopped.
- **Stop** ends the run. Frames already done stay done; the rest can be scanned later.

Stopping an album does not lose its results.

## Unfinished scans

If Conrod closes or crashes mid-scan, the next start finds unfinished albums and returns them to *stopped*. Open one and press **Resume** to finish it.

## Folder watch

**Folder watch** scans new frames as they settle while cards copy. It uses the album's selected stage: a cull album stays cull-only, an identify album stays identify-only. Disabling the watch removes its saved configuration.

## Copies and re-imports

The same frame can arrive twice under different names. **Collapse copies** hides the duplicate; bursts, picks, counts, and writes ignore it. **Restore copies** shows it again and regroups.

## Re-running work

Run one pass again without re-importing:

- **Rescan** re-reads every photo and measures sharpness inside the stored boxes.
- **Redetect** runs the detector lower on frames where it found nothing.
- **Regroup** rebuilds embeddings, groups, and consensus.
- **Identify new only** reads only subjects nobody has read yet; **Identify** under the album's scope reads the eligible subjects again.
- **Reset identifications**, **Reset detections**, or **Reset album** clears results without deleting photographs.

## Album settings worth knowing

- **Blur strictness**: per album, from lenient to critical.
- **Burst gap**: seconds between frames of one camera that still belong to one burst (default 1.5).
- **Grouping sensitivity**: off, gentle, normal, or eager — how readily one time burst is split into moments.
- **Workflow mode**: both, cull only, or identify only.
- **Identify scope**: keepers or everything.

The exact defaults and meanings are in the [settings reference](Settings-reference) and [shoot types](../shoot-types.md).
