# Competitor culling signals: what commercial tools score, and where Conrod has gaps

Research date: 2026-11-15. Sources are vendor docs, vendor marketing, and independent reviews. Each claim is tagged **[doc]** (vendor help/manual or a documented spec), **[mkt]** (marketing copy or vendor blog, not verifiable), or **[rev]** (independent reviewer's account). Where a source is weak or unclear, that is stated.

## Per-tool findings

### Aftershoot (Selects, Culling AI)
- **Signals:** Score 1–100 per image [doc]. Buckets: Selected, Highlights, Duplicates, Blurry, Closed Eyes, Maybes (Extreme / Cull-to-Target only) [doc]. Closed-eye and blur detection [doc]. Blown exposure flagged [doc]. Composition and "lighting, background clearness" called subjective signals [doc]. Support says it can prioritise emotion over perfect focus [doc]. "30+ factors" [mkt].
- **UI:** Filter groups in a sidebar; sliders for strictness per group; Blurry strictness and Duplicate group size (Few/More) [doc]. Ratings written back to Lightroom Classic / Capture One; star ratings and colour labels preserved [doc].
- **Duplicates:** Grouping sensitivity (Lenient / Moderate / Strict / Extreme) and "selection in duplicate sets" (Less = 1 of N, More = 3 of 10) [doc].
- **Highlights:** An older blog says the Highlights group uses an algorithm that "monitors popular photos shared on social media" [doc, dated 2022]. Current docs call it "best of the selected" [doc]. Treat the social-media claim as unverified now.
- **Learning from user:** Culling is marketed as "learns what you like, what you skip" [mkt]. The support docs describe learning only for the *editing* profile: min 2,500 edited images, 5,000 recommended, up to 15 catalogs [doc]. No documented mechanism for culling personalisation. "90%+ match by third or fourth shoot" is marketing [mkt].
- **Speed:** "20–30 min vs 3–5 h" [mkt].

### Imagen (AI culling + Personal AI Profile)
- **Culling signals:** Keepers (best of each duplicate group), Standalones, Duplicates, Low-rated (blurry, accidental, improperly exposed) [doc]. Analysis based on "sharpness, exposure, and overall image quality" [doc]. Sneak peeks are for weddings only [doc].
- **Modes:** "Keep best of each group" vs "Cull to exact number" (works best at 15–30% keep, min 1,000 photos) [doc]. Limit: 20,000 photos per project [doc].
- **Learning:** The Personal AI Profile learns *edits*, not culls [doc]. It needs at least 2,000 edited photos with the same camera profile; Imagen's own guidance is about 2,000–5,000 originals, with retraining needing half as many new edits as the original set (min 2,000) [doc]. Lite profile from a preset plus survey; no photos needed [doc]. Training takes up to 24 h [doc]. Culling personalisation is not documented.
- **Speed:** Not in the docs I read beyond marketing copy.

### Narrative Select (face and focus assessments)
- **Signals:** Per-face focus score 1–10 (bottom curve). Eye/expression ellipse with "17+" assessments, including eye state with context (blink vs. kissing vs. looking down) [doc]. Scene grouping. AI First Pass: five categories within each scene (Best in scene, Above average, Average, Below average, Undesirable) with stated reasons [doc]. Key Element Detection for non-face frames (Mac only) [doc]. Key Subject highlighting for wedding/event (people who "matter most") [doc]. Background faces are deliberately not assessed [doc].
- **Visibility:** Green/yellow/orange-red indicators; strictness setting for First Pass (Ruthless / Balanced / Cautious) [doc]. Assessments never delete or reject [doc].
- **Duplicates:** Scene grouping; duplicates stack automatically from v2.8.0 (Nov 2026) [doc].
- **Learning from user:** Personal Color/B&W AI Presets retrain after 1,000 corrections (Premium) or 250 (Ultra) [doc]. This is for editing presets. No documented personalisation for culling.
- **Speed:** RAW import "about three seconds" [mkt].

### FilterPixel (Basic Cull + DeepCull)
- **Signals:** DeepCull is genre-specific, with 10 criteria per genre (sports: peak action, ball visibility, athlete expression, subject fill, separation, sharpness) [doc/mkt]. A "Score & Reason" per frame [doc]. Frame-in-sequence ranking (picks 1–3 of a 12-frame burst) [doc].
- **Inconsistency to note:** One page says "six criteria", another "10 criteria" [doc, inconsistent].
- **Learning:** A blog post title says "AI Photo Culling That Learns Your…" but I could not verify the mechanism from the body [unverified].
- **Speed/accuracy:** "1,800 photos culled in 11 minutes", "92% accuracy vs photographer selections" [mkt, no methodology].
- **Processing:** Cloud [doc]; up to 5,500 photos per DeepCull shoot [doc].

### Excire Foto 2027 / Excire Search 2026 (local AI, Lightroom plug-in)
- **Signals:** Aesthetic score 0–100 per image [doc]. Vendor KB says the aesthetic model was trained to imitate ~200 photographer ratings per photo, ~25M parameters [doc]. Sharpness, face sharpness, eye sharpness, eyes open, smile, expression [doc]. Rejection: blurry, over/under-exposed, all eyes closed [doc]. Culling profiles can define these settings and be saved/exported [doc].
- **Grouping:** People, content (keywords), visual similarity (with slider), capture date, photo series (with length) [doc]. Duplicates: binary-identical and near-identical [doc].
- **UI:** Survey view, focus peaking, 3×3 grid, star/colour/flag [doc]. Keys match Lightroom (1–5, 6–9, P, X) [rev].
- **Learning from user:** None documented; custom profiles are manual [doc].
- **Speed:** Reviewer: 179,733 images initial analysis took 9 h 52 min; adding 4,454 RAWs took 13 min [rev]. Reviewer: 487 bird RAWs culled in 6 min [rev].

### Peakto (macOS media manager, CYME)
- **Signals:** Templates: deduplication, culling, portrait culling, collection culling. Rules: file type (RAW first), size, rating, face quality, aesthetic score, expression, eyes open/closed. Rules are user-ordered [doc]. Aesthetic analysis (technical and appeal) [doc].
- **UI:** Similarity slider defines clusters; A/B compare with zoom; pick/reject per cluster [doc].
- **Scope:** Works across all catalogs, drives and NAS (since v2.6, Jan 2026) [doc].
- **Learning from user:** Not documented; the user sets the rules.

### Adobe Lightroom Classic / Lightroom (Assisted Culling, GA June 2026)
- **Signals:** Subject Sharpness (0–100, default threshold 50), Eye Sharpness (per eye, aggregated per face), Eyes Open (open / closed / "Can't tell"), exposure issues (sensitivity slider), misfires, documents/receipts [doc]. Per-face Face View with eye sharpness and eyes-open scores (June 2026) [doc]. Adobe's design blog: subject score blends blur distribution, whether blur reads as intentional, subject position, and aesthetics [doc].
- **Stacking:** Auto-stack by time interval or visual similarity, both slider-driven [doc].
- **Visibility:** Select/Reject badges; Culling Scores in the Info panel (reviewer found the green ticks vanish when that panel is open) [rev].
- **Learning from user:** Not documented.
- **Speed/load:** Reviewer: ~2,600 photos pushed an M1 Mac Mini to 74 °C [rev].

### Capture One (Assisted Review, beta; Cull mode)
- **Signals:** Tags for closed eyes, missed focus, exposure problems [doc]. Face frames coloured green / yellow / red / grey (grey = unable to evaluate) [doc].
- **Visibility:** Tags are filterable, usable in Smart Albums, combined with ratings and colour tags [doc]. Can run during tethering (16.8.6, Sept 2026) [doc].
- **Manual cull:** Group Overview with similarity slider; Face Focus magnifier [doc].
- **Learning from user:** None documented.
- **Desktop only;** beta [doc].

### ON1 Photo RAW 2027 MAX (AI Culling Assistant)
- **Signals:** Visual similarity grouping, time between bursts, maximum group size, subject focus threshold, eyes-closed detection [doc].
- **UI:** Review a whole burst side-by-side with synced pan/zoom; Actions apply ratings/presets/export to keepers and rejects in one batch [doc].
- **Local processing** [doc]. MAX-only ($169.99 licence / $79.99 yr subscription) [doc].
- **Learning from user:** None documented; "the photographer decides" [doc].

### Photo Mechanic (no AI; manual speed)
- **Signals:** None; the photographer decides. Single-key ratings (0–5) or colour classes (0–8), tags (T), auto-advance after a change, arrow-key navigation, filters on the contact sheet, F3 for tagged view [doc].
- **Speed:** Testimonials only, e.g. "reducing by half the time" [rev/mkt]. No benchmark.
- **Learning:** Not applicable. Writes XMP to other editors [doc].

### Apple Photos / Google Photos (consumer)
- **Apple:** Bursts show grey dots on frames Apple suggests keeping; user taps keepers; "Keep only" removes the rest [doc]. Live Photos: choose a key frame [doc]. Eyes-open preference is in a third-party guide only, not in Apple's docs [unverified].
- **Google Pixel Top Shot:** Camera picks the best frame from a burst or motion photo [doc]. Google Photos shows a "pick a better shot" chip [rev, 2021].
- **Learning from user:** No documented personalisation.

### Luminar Neo
- **Signals:** No dedicated culling scorer. A competitor (imagic) states this; the claim is second-hand and comes from a competitor [mkt, biased].

### Optyx (budget autocull, now Zenfolio)
- **Signals:** Autogroup and Autocull; face analysis (expression, blink, focus); "sharpness, composition, exposure" in marketing [mkt]. Shotkit review: good grouping on most shoot types; the AI "doesn't know what happened before or after" [rev].
- **Output:** XMP ratings and colour labels [doc].
- **Pricing:** Free (no AI), Pro $6.99/mo annual / $9.99 monthly [doc].
- **Learning from user:** None documented.

### Others (2025–2026 new)
- **Excire Foto 2027:** Survey view, focus peaking, Smart selection, map view [doc]. Already covered above.
- **Narrative 2.8.0:** Duplicate auto-stacking, faster scan [doc].
- **Lightroom Assisted Culling:** Face View, GA in June 2026 [doc].

## Learning from the user's own picks: what is actually documented

| Tool | Learns from user's choices? | Documented data needed | Status |
|---|---|---|---|
| Aftershoot | Editing profile: yes. Culling: marketed, not documented | 2,500 min / 5,000 rec. | Editing doc; culling mechanism unknown |
| Imagen | Editing profile only | 2,000 min edits; ~2,000–5,000 originals | Editing doc |
| Narrative | Editing presets only | Retrain after 1,000 / 250 corrections | Editing doc |
| FilterPixel | Title suggests yes | Unknown | Unverified |
| Excire, Peakto, Lightroom, Capture One, ON1, Optyx, Photo Mechanic | None documented | n/a | Fixed rules / sliders |

Conclusion: nobody documents personalised culling trained on a user's own keep/reject history. Aftershoot and Imagen personalise the edit, not the cull. This is the least-crowded space in the market.

## Gap table (Conrod has vs. competitors)

Conrod HAS: subject sharpness via edge blur, panning detection, faces + closed eyes, burst/moment grouping, duplicates, keeper pick per burst (top 3), VLM reading of numbers/plates/sponsors/make-model, per-shoot presets, blur strictness.

| Signal | Who has it (documented) | Does Conrod have it? |
|---|---|---|
| Subject sharpness with intent (intentional blur vs. miss) | Lightroom (Subject Sharpness, blur intent), Aftershoot (blur strictness + emotion-over-focus), Narrative (per-face focus 1–10), Excire (sharpness) | **Yes** (edge blur + panning). Gap: no 0–100 score users can slide, no "intentional blur" model beyond panning |
| Eye sharpness, separate from eyes-closed | Lightroom (per eye + per face), Excire (eye sharpness), Narrative (focus per face), Optyx (focus) | **Partial.** Conrod has eyes-closed; eye focus is not separated |
| Eyes open with a "can't tell" state | Lightroom, Capture One (grey), Excire (eyes-open), Narrative (eye context) | **Partial.** Likely yes/no; "can't tell" is a gap (sunglasses, motion) |
| Expression / smile | Narrative (expression ellipse), Excire (smile, expression), Optyx, Peakto (expression), Aftershoot (Key Faces), FilterPixel (athlete expression) | **No** |
| Aesthetic / composition score | Excire (0–100, trained on ~200 ratings/photo), Aftershoot (1–100), Optyx, Peakto (aesthetic score), FilterPixel (composition), Lightroom (aesthetics in subject score) | **No** |
| Exposure / misfire check | Lightroom (sensitivity slider), Excire (over/under), Aftershoot (blown), Capture One (tags), Imagen (improperly exposed) | **In progress** |
| Scene-relative ranking (best in scene vs. the rest) | Narrative (5 tiers per scene), Aftershoot (1 keeper per group), Imagen (keepers vs. standalones), Lightroom stacks | **Partial.** Top-3 per burst exists; no tiered rank for the whole scene |
| Hero / highlight tier | Aftershoot (Highlights), Imagen (sneak peeks), FilterPixel (hero / front-page), Narrative (key subjects) | **No** |
| Per-person grouping / key-person filter | Narrative (People filter, key subjects), Excire (group by person), Peakto (face quality) | **No** (faces detected, not identity-grouped) |
| Personal taste learning from the user's keep/reject history | Marketed only: Aftershoot (culling). Documented for editing only: Aftershoot, Imagen, Narrative | **No** (per-shoot presets are a manual analogue) |
| Tunable duplicate / similarity strictness | Aftershoot (Few/More), Excire (slider), Capture One (slider), Lightroom (auto-stack slider), ON1 (similarity + time gap) | **Yes** (blur strictness; duplicates). Gap: user-facing similarity slider |
| Burst grouping by time gap | ON1 (time between bursts), Lightroom (time interval) | Check Conrod's implementation |
| Per-frame reason text | FilterPixel (Score & Reason), Narrative (hover reason), Lightroom (Culling Scores), Capture One (tags) | Unknown; a VLM could emit this cheaply |
| Sports peak-action / ball-in-frame | FilterPixel DeepCull only (marketing claims) | **No** |
| Rejects for receipts / documents | Lightroom, Excire (not confirmed) | Unknown; VLM could detect |

## What this suggests for Conrod
1. **Biggest documented gap:** expression/smile and aesthetic score. Five-plus competitors have both, and both are cheap to add on top of face detection.
2. **Cheapest differentiator:** personalised culling. Nobody documents it. Conrod's per-shoot presets are a manual start. Learning from keep/reject decisions would be novel; the minimum data (how many decisions) is not documented by any vendor, so it needs a local experiment.
3. **Eye sharpness and "can't tell":** separate per-eye focus and a third eyes-open state. Lightroom has both; users notice.
4. **Hero tier:** a per-scene ranking (Narrative-style five tiers) is more useful than top-3 per burst.
5. **Speed:** the only solid numbers are Excire's (reviewer-measured). Conrod should benchmark on the same 4,454-RAW shoot to compare.

## Source URLs
- Aftershoot tech answers: https://support.aftershoot.com/en/articles/10601968-technical-answers-about-aftershoot
- Aftershoot culling get-started: https://support.aftershoot.com/en/articles/13928734-get-started-with-aftershoot-culling
- Aftershoot preferences: https://support.aftershoot.com/en/articles/6508163-setting-your-ai-automated-culling-preferences-in-aftershoot
- Aftershoot Selects (marketing): https://aftershoot.com/selects/
- Aftershoot Highlights (2022 blog): https://aftershoot.com/blog/understanding-filters-in-aftershoot
- Aftershoot profile training (2,500 min): https://support.aftershoot.com/en/articles/6673009-how-many-images-should-i-use-to-train-a-profile
- Aftershoot professional profile: https://support.aftershoot.com/en/articles/9189902-how-to-build-your-first-professional-ai-profile
- Imagen culling explained: https://support.imagen-ai.com/hc/en-us/articles/6816483048221-Imagen-Culling-Explained
- Imagen culling project: https://support.imagen-ai.com/hc/en-us/articles/29925087795357-Create-a-new-culling-project
- Imagen culling method: https://support.imagen-ai.com/hc/en-us/articles/35240170215965-Choose-your-culling-method
- Imagen Personal AI Profile: https://support.imagen-ai.com/hc/en-us/articles/6069711141009-What-is-a-Personal-AI-Profile
- Imagen training: https://support.imagen-ai.com/hc/en-us/articles/6069799037457-When-and-how-to-train-your-Personal-AI-Profile
- Narrative face assessments: https://help.narrative.so/en/articles/7337369-face-and-focus-assessments
- Narrative focus filter: https://help.narrative.so/en/articles/7337374-filter-by-focus-score
- Narrative Lite vs Standard: https://help.narrative.so/en/articles/13048836-what-s-the-difference-between-lite-and-standard-culling
- Narrative plans: https://help.narrative.so/en/articles/7337508-compare-narrative-plans-lite-standard-premium-and-ultra
- Narrative 2026 review (vendor blog): https://narrative.so/blog/best-photo-culling-software
- FilterPixel DeepCull: https://filterpixel.com/deepcull
- FilterPixel sports: https://trial.filterpixel.com/deepcull-sports
- FilterPixel sports guide: https://filterpixel.com/blog/sports-photo-culling-complete-guide
- Excire Foto 2027 facts: https://excire.com/en/facts/excire-foto-2027
- Excire aesthetics KB: https://support.excire.com/portal/en/kb/articles/how-does-the-ai-aesthetics-rating-work
- Excire culling profile (manual): https://learning-center.excire.com/s/excire-search/m/excire-search-2026-quickstart-en/a/execution-of-a-culling-project
- Excire review, speed (Amateur Photographer, 2026): https://amateurphotographer.com/review/excire-photo-2025-excire-search-2026-review-organising-photos-has-never-been-easier
- Excire 2027 review: https://amateurphotographer.com/review/excire-foto-2027-review-get-on-top-of-even-the-most-disorganised-photo-collection
- Peakto culling: https://desk.cyme.io/portal/en/kb/articles/culling-deduplication
- Peakto 2.6 press: https://cyme.io/en/press/peakto-introduces-ai-driven-culling-and-deduplication
- Lightroom Assisted Culling help: https://helpx.adobe.com/lightroom/desktop/organize-photos/assisted-culling.html
- Adobe design behind-the-scenes: https://adobe.design/ideas/behind-the-design-assisted-culling-in-adobe-lightroom
- Lightroom release notes: https://helpx.adobe.com/lightroom-cc/using/whats-new/release-notes.html
- Lightroom Classic GA thread: https://community.adobe.com/questions-675/p-assisted-culling-feedback-thread-lrc-1627467
- Lightroom review (Phoblographer): https://www.thephoblographer.com/2026/04/11/lightrooms-ai-assisted-culling-is-incredible-for-sports-photography
- Capture One Assisted Review: https://support.captureone.com/hc/en-us/articles/35841394167837-Assisted-Review-Beta
- Capture One 16.8.6 (DPReview): https://www.dpreview.com/news/capture-ones-latest-update-includes-automatic-culling-and-more-canon-support
- ON1 culling walkthrough: https://www.on1.com/videos/cull-your-shoot-in-minutes-with-photo-raw-max-2027
- ON1 2027 release: https://www.on1.com/press/on1-releases-photo-raw-2027
- ON1 pricing: https://www.on1.com/products/photo-raw
- Photo Mechanic prefs (Camera Bits): https://home.camerabits.com/setting-up-photo-mechanic-quickstart-guide-to-preferences
- Apple Photos bursts: https://support.apple.com/guide/photos/view-photo-bursts-pht8745d2677/mac
- Google Pixel Top Shot: https://support.google.com/googlecamera/answer/9937175?hl=en
- Google Photos chip (2021): https://9to5google.com/2021/08/27/google-photos-tests-new-top-shot-preview-chip-for-selecting-your-best-images
- Optyx site: https://www.optyx.app/
- Optyx review (ShotKit): https://shotkit.com/optyx-ai-photo-selection-tool
- Optyx pricing (Aftershoot blog, comparison; vendor-adjacent): https://aftershoot.com/blog/ai-photo-culling-tool-pricing/
- Luminar Neo comparison (competitor, biased): https://imagic.ink/compare/luminar-neo
- Capture One Face Focus / grouping: https://support.captureone.com/hc/en-us/articles/7185822431645-Culling-images
