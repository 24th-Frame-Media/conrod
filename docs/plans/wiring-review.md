# Wiring review: 8210b8f, dba683d, 470940f, 5420c1a

Reviewed `git diff 9bea587..5420c1a`. Severity: HIGH = wrong behaviour in the
real app or a wrongly culled frame; MEDIUM = latent or order-dependent;
LOW = polish. "Fixed" items carry tests and are in the commit that follows
5420c1a.

## Regression causes (readiness)

Method: a worktree of 9bea587 (and d18efcf) built into the repo target dir, same
`export_recall` harness, per-frame diff of `frames.csv` / `subjects.csv`.

**Cruise 50.0%: not caused by the wiring.** All 108 frames match between
9bea587 and 5420c1a in rating, burst key, burst size, pick and cull reason (0
diffs), and d18efcf picks the same. The metric is 4 scored bursts (all size 2):
IMG_0059 (0.950 vs 0.941) and IMG_0117 (0.936 vs 0.937) are sub-band
rating near-ties, so 2/4 = random 2/4. The earlier "READY" was a different
sample/commit. No code change is warranted; do not tune for it.

**Portrait 42.9% -> 35.7%: one frame, IMGC0088 (burst 21 of 2).** 9bea587
misses 8 bursts, 5420c1a misses the same 8 plus IMGC0088. Before, OCEC called
both faces closed, they were equally culled, and the earlier frame (0088, the
delivered one) won the tie. Now the blendshape mesh rescues 0089 (eye-open
>= 0.20) and reads 0088 at 0.168, so 0089 is the only uncull frame. Both
frames are visibly downcast, near-closed eyes (checked on the embedded
previews); the photographer kept the slightly more closed one. This is a
borderline mesh read on an ambiguous pose, not a logic error. Retuning the
0.20 threshold for it would overfit n=14.

## Findings

### HIGH (fixed)

1. **`blur_explicit` is always true in the app, so the preset's
   `default_blur` never applies** (`frontend/src/screens/Import.tsx`,
   `crates/conrod-engine/src/desktop.rs` scan args). The Import sheet always
   sent `blurStrictness`, which set `blur_strictness` and (as written) was
   treated as the album's choice. Readiness and `export_recall` use
   `Settings::default()` (flag false, preset default applies), so readiness
   measured a behaviour the app never ran. Fix: the sheet sends strictness
   only once the slider is touched; every writer (scan args, `update_job_settings`,
   `save_settings` when the value changes) sets `blur_explicit`; one helper
   `Settings::effective_blur()` is used by scan, `find_people`, `rescore`,
   `evaltest`, `pantest`. Albums saved before the flag keep any non-"moderate"
   value as a choice. Test: `the_preset_default_blur_applies_until_the_album_chooses`
   and the scan-args test now asserts `blur_explicit`.
2. **`deliver_most` never culled a blurred subject** (`lib.rs`, cull reason).
   It compared `verdict == "blurred"` but `verdict` is `rating_for`'s word
   ("poor"/"fair"/"good"), so condition reports and landscape kept even
   clearly out-of-focus frames; it also ignored held pans. Fix: shared
   `deliver_most_reason()` using `sharpness::cullable`. Test:
   `a_condition_report_culls_only_clearly_blurred_subjects`.

### MEDIUM

3. **Dark-ok presets hard-coded** (`passes.rs`: `matches!(preset, Astro |
   Concert)`) instead of `PresetPolicy`; Event shows (stage blackouts) were
   missing, and the `bright` ranking key penalised dark frames even where dark
   is the subject. Fixed: `PresetPolicy::dark_ok` (Astro, Concert,
   EventShows), used for both the cull and the `bright` key. Test:
   `a_dark_frame_loses_the_pass_except_where_the_preset_shoots_in_the_dark`.
4. **`beats()` reaches framing/smile only on an exact rating tie**
   (`passes.rs` ~line 135: `if self.score != other.score { return ... }` sits
   before the framing/smile keys; the later duplicate check is dead). The
   documented 0.10 margins therefore matter only when the float ratings are
   identical (the 1.0 ceiling). The 5420c1a "fix" for the Motorsport/Ford
   drops made these keys nearly inert rather than correctly placed. The cut-side
   gate before size is the part that does respect the evidence ("a cut-off
   subject never wins on size"); occlusion and lead room do not rank unless
   ratings tie. **Fixed**: `beats` is now one lexicographic key tuple
   (`Candidate::key`: stars, no cull, rating band, exposure, buck, cut sides,
   size, framing bucket, eyes-open bucket, smile bucket, exact rating, quality,
   sharpness, frame, row). Only the keys that used to compare by a pairwise
   margin are bucketed (framing/eyes/smile/buck at 0.10); the keys that were
   already exact (area, rating, quality, sharpness) stay exact, because
   bucketing them tied frames of genuinely different size and lost a motorsport
   pick (46.7 -> 40.0 on readiness; back to 46.7 with exact size). Tests:
   `the_key_ladder_is_a_total_order`,
   `within_the_rating_band_the_more_open_eyes_win`.
5. **Margin comparisons are not transitive** (framing 0.10, smile 0.10). The
   fold in `pick_of_pass` therefore depends on iteration order (`ORDER BY
   i.id`) for bursts of 3+. Deterministic, but a rescan with a different frame
   order can flip a pick. **Fixed** by the same bucket ladder: every key is an
   integer bucket or an exact total-ordered value, so the fold cannot depend on
   row order. Test: `the_keeper_order_does_not_depend_on_the_order_the_rows_arrive_in`
   (framing 0.50/0.56/0.63, the trio the old 0.10 margin split in two).
6. **Occlusion treats any covering box as in front** (`framing.rs::occlusion`).
   A small subject in front of a larger car/person gets 1.0 occlusion because
   the bigger box contains it. Count only boxes not larger than the subject, or
   whose bottom edge is lower in frame.
7. **Lead room is never measured**: `cull_frame` passes `motion = None` to
   `vis_framing::framing`, so `lead_room` stays `None` and the sport policy's
   lead-room term is dropped. The commit text and the sport policy imply it is
   used. **Fixed**: `streaks::background_pan` now returns the pan axis (the
   line the background is smeared along, degrees clockwise from image-right,
   from the same tiles the pan flag reads) beside the coherence;
   `pan_motion` passes it as `Motion::AxisDeg` on frames the pan flag calls a
   pan, and `None` elsewhere. One frame cannot say which way along that line
   the subject travelled (motion blur is symmetric), so the axis is scored on
   the side with the room and the lead term drops for a subject centred on the
   line (head-on cars). Tests: `the_pan_axis_is_the_streak_direction`,
   `an_axis_gives_lead_room_without_guessing_the_direction`,
   `a_panning_frame_gets_lead_room_and_a_still_one_gets_none`.
8. **Mesh can cull on its own when OCEC did not judge**
   (`lib.rs`, `!ocec_judged || ocec_closed`). The comment says the mesh "never
   invents" a closed eye, but for a face OCEC skipped (turned or small head)
   the mesh alone culls, which is the exact failure the comment describes.
   Prefer: no OCEC verdict => flag only (store `eye_open_min`), no cull.
9. **Regroup reads RAW headers under the DB lock and, for any album with no
   exposure EXIF stored (JPEGs, old albums), on every regroup**
   (`desktop.rs::keep_sets` / `regroup`). `update_job_settings` calls regroup
   synchronously on preset or burst-gap change, so a P: album stalls the UI for
   thousands of header reads. The `stored` test is "any row has exposure_time or
   bias", so an album that genuinely has none never becomes "stored". Move the
   header read out of the lock and into the scan (it is already stored there);
   for old albums do it once and write a sentinel.
10. **Set members are one pick unit, so the rest of a bracket reads as burst
    losers.** `set_id` is stored and exempts the cull, but nothing downstream
    keeps the whole set; only one frame gets `burst_pick`. Scan-time
    `cull_reason` is still written on set members (UI shows "too blurred" on a
    deliberately soft focus-stack frame). Decide whether sets export whole;
    until then the review UI should show the set, not demote it.
11. **Landscape gained `deliver_most`** (`profile.rs`). That disables the
    stars cull for the whole preset; probably intended for property, not
    landscape, where a soft frame is a genuine reject. Check with the owner.
12. Old albums: a user who explicitly saved "moderate" before the flag existed
    now gets the preset default on rescore (fix 1 cannot tell the difference).

### LOW

13. `beats()` compares `score` twice; the second is dead. NaN scores make the
    relation non-total (neither side wins); harmless because the first frame
    held stays, but order-dependent.
14. The `expression` Mutex is shared by all scan workers, so mesh calls
    serialise (about 14 ms per face with DirectML); fine for portraits, a
    bottleneck for large groups.
15. `smile` ranks for Event/EventShows/EventParties but `framing_policy` gives
    Event the portrait policy while `Concert`/`Wedding` differ; confirm that
    Wedding is meant to rank on smile but not on level.
16. Migrations: all `ALTER TABLE ... ADD COLUMN` entries are idempotent and
    nullable; existing albums open unchanged (NULL set_id, dark_frame 0,
    exposure_verdict NULL => "bright"). No findings.
17. Scan-time cost: exposure (downsampled grid), framing (boxes only), level
    (level presets only) are cheap; the new per-face mesh is the only
    measurable cost, and only for faces-on presets.

## Not wrong (checked)

- Exposure only culls `dark_frame` (mean luma < 0.04) and only outside
  sets; highlights/shadows are flags.
- `dark` verdict ranks below the rating band but above box size; correct.
- Closed-eye cull: face-level, and a shut eye box inside it never wins a
  pass; open-eyed frames win.
- Rodeo `buck` ranks only in rodeo/equestrian and only when both frames have
  one.
