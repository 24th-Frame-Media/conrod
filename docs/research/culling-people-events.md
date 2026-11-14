# Culling people & event genres: research notes

Scope: what photographers keep and reject per genre, how they shoot and select, and what Conrod can measure. Sourced from vendor guides (Aftershoot, Narrative Select, FilterPixel, Imagen AI, Selekt), photographer forums and editorial/photojournalism guidance. Items marked *(convention)* are common practice that I did not verify in a source this pass. Vendor accuracy claims are marketing, not independent measurements.

Shared rule across genres: cull in passes. Pass 1 removes technical rejects (blink, missed focus, black/flash-failed, test frames). Pass 2 picks one best per moment and checks coverage.

---

## 1. Weddings

**Ceremony**
- Keepers: vows and ring exchange with both faces readable; first kiss with eyes visible or intentionally closed; processional bride/entrance framed clean; officiant mid-sentence only if expression works.
- Rejects: guests' backs/phones in frame, blinks on the couple, hands covering rings, flower girl mid-blink, crowd heads blocking the couple.
- Shooting: continuous bursts on vows, kiss and exit; 1/250 s+ for walking; ISO and flash tolerance vary by venue.
- Selection: one per moment, keep a second only if it's a different beat (wide vs tight). Work backward through each sequence; the strongest frame is often last.
- Signals: face count, eye-openness on the couple, closed-eye group with kiss exceptions.

**Portraits (formals, couple, family)**
- Keepers: every shot-list combination has one frame with all eyes open and facing camera; natural smiles; sharp faces.
- Rejects: one person blinking/looking away, missed focus on far-side subject, hands/bouquet obscuring faces.
- Shooting: burst on each combo; family groups often shot in 5–10 frame bursts to get eyes-open composite.
- Selection: pick the frame with most eyes open; if none, composite (see Group photos). Check coverage against VIP list (parents, grandparents, wedding party).
- Signals: per-person eye-open/face-sharpness across the combo; coverage of named people.

**Reception**
- Keepers: first dance, speeches, cake, toasts with a readable face; genuine laughter.
- Rejects: motion blur on dance floor unless intentionally stylised; mid-chew guests; flash-shot faces with harsh shadows; underexposed/blown speaker frames.
- Shooting: mixed ambient and flash; low shutter on dance floor; ISO 3200–12800 common.
- Selection: Aftershoot's blur sensitivity should be lowered when the photographer uses intentional motion blur on dance floors.
- Signals: subject sharpness vs background; blur sensitivity setting; clipped-highlight %.

**Candids (guests, details)**
- Keepers: interaction between two people, spontaneous emotion, detail shots (rings, florals, dress).
- Rejects: awkward guest expressions, backs of heads, repeated near-duplicate candids.
- Selection: group duplicates by scene, keep one per scene plus variety across the day.
- Metadata: caption/keyword the day section (getting ready, ceremony...) and key people.

**Volume and benchmarks**
- Aftershoot 2026 guide: 70–100 delivered photos per coverage hour; full 6–8 h day 450–800 delivered; 3,000–6,000 frames typical.
- Narrative: 50–75 per hour (8 h ≈ 400–600). Reddit: ~30% of shots reach clients.
- Cull-to-target: pick a delivery count before culling (Aftershoot allows 10–60% of album).

---

## 2. Engagement & couples

- Keepers: a genuine moment between the two people; eyes on each other or camera; kiss with a readable face; hair/wind not hiding faces.
- Rejects: half-blink in a laugh; couple looking at the wrong spot; focus on the background tree when the faces are soft.
- Shooting: continuous bursts while couple moves; shoot the walk/interaction more than static poses.
- Selection: Aftershoot treats engagements like weddings (key faces, closed eyes, emotion). Kisses need a manual check: the closed-eye detector can flag intentional closed eyes.
- Signals: closed-eye flag on a kiss (allow override), face sharpness vs background, gaze-to-camera vs gaze-to-partner (pose) to separate "posed" from "candid".

---

## 3. Family & newborn

**Family**
- Keepers: everyone eyes open and facing the camera; mix of smiles; natural movement (kids running with smiles).
- Rejects: any one person blinking, turned away, or with hand over face; kids mid-blink; motion blur on running children.
- Shooting: 3–6 frame bursts per pose, multiple poses for kids; use a "count to three" cue to reset blinks *(convention)*.
- Selection: Aftershoot's family genre is for multi-person foreground with clear background; the photographer picks the frame with the most open eyes per person, and composites the rest if needed.
- Signals: per-face eye-open; gaze-to-camera per face; count of faces; face-size spread (kids vs adults).

**Newborn**
- Keepers: relaxed baby with soft, natural expression; hands/feet tucked; sharp face and eyelashes if eyes are open.
- Rejects: baby's eyes closed (asleep is the usual shot, so not a reject in itself); fidgeting that blurs the face; parent's face cropped.
- Shooting: babies are often asleep; closed-eye detection is turned off for newborn genre (Aftershoot recommends it for newborns and toddlers).
- Selection: do not use closed-eye rejection for sleeping babies; select by pose quality and sharp face.
- Signals: face/body sharpness, closed-eye check disabled, hands/feet in frame, background clutter.

---

## 4. Headshots & corporate portraits

- Keepers: eyes sharp with catchlights; expression natural and on-brand; head angle and crop consistent across the set.
- Rejects: any blink or mid-blink; forced grimace; glare on glasses blocking eyes; motion between frames.
- Shooting: 4–8 frames per subject; vary expression, not the whole pose; consistent lighting and backdrop.
- Selection: pass 1 eyes open and technically OK; pass 2 expression, then angle, then technical. Two equal frames: pick either, move on. Final consistency sweep for crop and lighting. Selekt estimates 3–5 s per subject for a clean session, 10–20 s typical (vendor planning figure).
- Delivery: one winner per person; do not over-deliver options.
- Metadata: subject name/ID as file name or keyword; roster check (every subject has exactly one pick).
- Signals: per-subject group; eye-open score; face sharpness; crop/face size consistency across set; brightness/colour-temp deviation vs set median.

---

## 5. School & volume portraits

- Keepers: eyes open, neutral-to-smile, face toward camera, in-focus; clean school-appropriate pose.
- Rejects: weird pose or not looking at camera (Aftershoot marks these as warnings, not rejects); blinks; hair over face; flash misfire.
- Shooting: 3–5 frames per student; QR cards or ID cards used as subject separators (Aftershoot reads QR codes as the next-subject marker).
- Selection: one per student; pass 1 technical; pass 2 expression; use a secondary "warning" bucket for weird-pose frames that are sharp.
- Metadata: student name/ID from QR or roster; keep roster checks.
- Signals: QR/barcode detection, gaze and head pose (weird pose), eye-open, one-face-per-group.

---

## 6. Group photos (any genre)

- Keepers: every face eyes open, looking at camera, expression roughly matched, no one mid-sentence.
- Rejects: any one person blinking, looking away or with mouth in an odd shape; someone out of focus at the edge.
- Shooting: 5–10 frame bursts; sit people down in rows; use a cue to start all at once; some pros close eyes first to reset blink timing *(convention; forum tip)*.
- Selection: best-composite logic: take the base frame with the fewest problems, then replace each faulty face from another frame where that person is open and in focus. Photoshop's Auto-Align Layers is the common tool (Adobe help article). Conrod scope: pick the best base + list faces to swap.
- Keep variety: do not drop an entire burst if the best frame has one fault; keep the composite candidate.
- Metadata: names left-to-right in group (CUNY guideline) when captioning.
- Signals: per-face eye-open score (eye aspect ratio), per-face gaze/pose, face sharpness, face count, best-frame per-person index so a composite can be proposed.

---

## 7. Concerts & live music

- Keepers: performer's face sharp, peak expression or gesture (apex of motion), instrument/mic interaction; one keeper per song or moment.
- Rejects: missed focus (check at 100%); motion blur beyond stylistic tolerance; blown highlights with no detail; crushed shadows; autofocus locked on the mic stand; backs of heads; lens flare (unless intentional).
- Shooting: ISO 1600–12800 common; shutter 1/250–1/500 s for movement (vendor and photographer advice); aperture wide; noise is not a reject.
- Selection: split by song or set (Selekt uses 1/5/15/30 min time gaps); one keeper per burst, two only if different. Variety check: wides, mediums, tight faces, interaction between members, quiet moments. Delivery: a 45–90 min set typically yields 30–75 finals (Selekt planning figure); 3-song pit shoot ≈ 30 selects from 800 frames.
- Keeper ratio often 2–3% (Selekt).
- Signals: face sharpness vs body sharpness; motion blur on face region; clipped-highlight %; ISO/noise tolerance (do not penalise noise); gaze to crowd/band; mouth open (singing) and hand/mic position.

---

## 8. Theatre, dance & performing arts

*(convention, not verified in a source this pass; check before shipping logic)*
- Keepers: peak pose/gesture with clear line; face readable in dance; actor's expression during key line; ensemble with no one hidden.
- Rejects: legs/arms out of frame; faces in shadow or blown by follow spot; motion blur on the face (accept on limbs for dance); flash-lit frames in houses where flash is banned.
- Shooting: dress/rehearsal or sanctioned shots; bursts on peak poses; high ISO with fast lens; often no flash.
- Selection: one per tableau; prefer whole-cast frames for ensemble numbers.
- Signals: body-part detection (limbs cut off at frame edge); face sharpness; face in shadow (luma in face box vs frame); mouth/pose.

---

## 9. Corporate events & conferences

- Keepers: speaker with a closed or pleasant mouth, mid-gesture, eyes open; audience reacting; clear sponsor/logo; handshake with both faces visible.
- Rejects: speaker mid-word with awkward expression; blinks; test frames; moving audience member crossing the frame; blocked slides; mouth agape.
- Shooting: fast bursts on gesture; pause on applause/laughter (Lauren Lindley tip); keep clean light, avoid mixed colour cast.
- Selection: one best per speaker moment; for panel, prefer frames where multiple speakers are engaged. Mouth shapes: photographers avoid "oo/oh" and F/R sounds (SXSW guide).
- Priority: sponsor/executive coverage and roster check (FilterPixel conference mode claims it prioritises sponsor logos and speaker visibility; vendor claim).
- Signals: mouth-open ratio (speaker); eye-open; gaze to audience; slide/logo detection (OCR or template); audience-occlusion (motion blur of someone crossing); face sharpness.

---

## 10. Parties & nightlife (flash)

- Keepers: genuine laughter; faces lit, not blown; group reaction shots; dance-floor energy with a readable face.
- Rejects: flash misfire (faces dark, background bright); black/underexposed frames; hard shadows; red-eye; subjects blinking from flash; blurred faces from low shutter.
- Shooting: on-camera flash with bounce; bursts for groups; ambient-dominated frames at high ISO.
- Selection: separate flash-failed frames from ambient; keep a flash-OK frame per moment. Aftershoot's "School Events" genre handles messy backgrounds and subjects not looking at camera (candid event mode).
- Signals: flash-failure check (face region luma vs background, face underexposed while background bright), black frame (mean luma), clipped highlight in face region, eye-open, red-eye.

---

## 11. Festivals

- Keepers: crowd energy with a readable performer; wide establishing frames; artist moment (same as concerts); signage/branding when client needs it.
- Rejects: missed focus; crowd heads blocking; lens cap / hands over lens; over-bright stage frames.
- Shooting: multi-artist days; pass credentials with time windows; 3-song rule for pit.
- Selection: group by artist, then by set; 15–30 images per artist for a 20–30 min slot (Selekt planning figure); crowd/wide shots for venue clients.
- Signals: per-artist grouping (time gap + stage/face identity); crowd density; face size; wide vs tight classification.

---

## 12. Graduations & award ceremonies

- Keepers: handshake moment with both faces visible and in focus; diploma with face readable; name on stage; award with clear trophy; genuine reaction.
- Rejects: handshake with hands only; the graduate's back; blocked by other grads; blink on handshake; background distraction (people or stage props touching the subject).
- Shooting: burst on handshake (5-second window per graduate; photographers note this as a one-shot opportunity); distance from background matters; "shoot on TWO" with a laugh cue (Syracuse tip); do multiple rounds.
- Selection: one per graduate for the handshake; name identification per graduate.
- Metadata: name captioning is the key deliverable (per-graduate file/IPTC). Use the programme/roster; IPTC `Iptc4xmpExt:PersonInImage` for names; left-to-right for group captions (CUNY guideline).
- Signals: handshake detection (two hands with a face each); face count (1 or 2 in frame); roster/OCR for name card; background clutter at face; eye-open.

---

## 13. Fashion & runway

- Keepers: look fully visible from head to hem (vertical), face sharp, pose at apex of the walk when the foot lands; full-length look per model.
- Rejects: model mid-step with feet blurred; face in shadow; hair/makeup mid-movement; flash in shows where banned.
- Shooting: verticals; shutter 1/250–1/400 s; f/2.8–f/4; burst at the step; one frame per look.
- Selection: pick the best frame per look (face + outfit + pose); keep close-up detail shots (embroidery, seams) as separate bucket; backstage/fittings as separate bucket.
- Signals: full-body detection; face sharpness; foot/ankle sharpness; pose at apex (limb extension); look/outfit match across burst.

---

## 14. Boudoir & glamour (brief)

- Keepers: eyes open or artistic closed eyes; sharp focus on the face and the intentional part (skin, lingerie, hands); comfortable expression.
- Rejects: blinks in an open-eyes frame; blur on the face; uncomfortable/unflattering poses.
- Shooting: small sets, controlled light; burst for transitions.
- Selection: Aftershoot advises turning closed-eye detection off for boudoir (artistic closed eyes).
- Signals: closed-eye check toggle; face sharpness vs body; skin tone/clipping; body-part detection.

---

## 15. Street photography

*(convention; no primary source fetched this pass)*
- Keepers: decisive moment with gesture/expression; clean frame lines; subject readable; context.
- Rejects: frozen-moment-without-subject; crops of hands; blur on subject; distracting background merges.
- Shooting: burst on movement; prefer 35/50 mm; fast shutter.
- Selection: one per moment; variety in framing; consider consent/ethics, not a culling rule.
- Signals: subject sharpness, face detection (optional), gesture detection, background clutter.

---

## 16. Documentary & photojournalism

- Keepers: honest moment with clear context; expression that matches the event; sharp main subject; correct caption-able people.
- Rejects: staged or posed-looking frames if the brief is documentary; misidentified people; blurred key subjects; frames with unknown names when names are required.
- Shooting: burst on the key moment; wide plus tight.
- Selection: one primary per story beat; no manipulation.
- Metadata (strict): spell names as the person spells them; identify only when sure; do not describe emotion or motive not evident (HSM Univ. guidelines); IPTC caption, creator, credit, keywords; names left-to-right for groups.
- Signals: face recognition only with user confirmation; caption-status flag (missing names); emotion label as a suggestion, not a fact.

---

## 17. Pets (owners & pet portraits)

- Keepers: eyes sharp on the nearest eye; ears/face in frame; pet looking at camera or owner; natural expression.
- Rejects: mid-blink; motion blur from play; missed focus on nose or body; owner's face cut off in a "together" shot.
- Shooting: AF-C and burst; Animal Eye AF where available (Canon, Sony); fast shutter 1/500 s+; low angle.
- Selection: one per moment; owner-and-pet frames need both faces sharp; eye-openness on the pet is the same signal as humans.
- Signals: pet detection; pet eye-open; owner face detection; focus on nearest eye (eye-level sharpness) vs body.

---

## 18. Kids' sports days & youth sports

- Keepers: child in focus with a readable face; peak action (finish, jump, catch); group photo with all children; expression (grin, effort).
- Rejects: missed focus on the face (body in focus is OK for some sports); helmet-covered eyes (closed-eye detection is off by default for helmets); blurred running child.
- Shooting: burst for action; 1/1000 s+ for running; AF-C tracking; sideline positions.
- Selection: Aftershoot's sports genre looks for body sharpness, not face sharpness; closed-eye detection off by default for helmets. Duplicates: do not cluster frames far apart in time; burst frames can cluster.
- Signals: body sharpness vs face sharpness (choose per sport), helmet detection (disable closed-eye), time gap (for duplicate groups), motion blur measure, child count per group.

---

## 19. Additional common genres (brief)

- **Birthdays & milestones**: keep the honoree face-first; group cake/candle shots with all eyes open; treat as party (flash rules).
- **Religious & cultural ceremonies**: check for banned flash and restricted areas; keep ritual moments with clear faces; do not blur hands/objects used in rites *(convention)*.
- **General sport (adult)**: same as kids' sports with stronger shutter; use the sport-specific body vs face rule.

---

## Cross-genre summary

| Genre | Primary keeper signal | Main reject | Selection unit | Key Conrod signal |
|---|---|---|---|---|
| Wedding ceremony | Kiss/vows readable | Blink, guest in frame | One per moment | Eye-open on couple |
| Wedding portraits | All eyes open | One blink in group | One per combo | Per-face eye-open |
| Reception | Genuine laugh, face lit | Motion blur on face | One per beat | Subject vs background sharpness |
| Engagement | Connection, kiss | Half-blink | One per moment | Closed-eye flag with kiss override |
| Family | All open, facing camera | One away/blinking | Best composite | Face count, per-face gaze |
| Newborn | Sharp face, relaxed | Blur on fidget | One per pose | Closed-eye check off |
| Headshots | Catchlight, natural smile | Any blink | One per subject | Per-subject eye-open, consistency |
| School | Eyes open, straight face | Weird pose, blink | One per student | Gaze/pose, QR/ID |
| Group | Every face open | Anyone blinking | Composite base | Per-face eye-open index |
| Concert | Peak gesture, face sharp | Missed focus, blown | One per burst | Face vs body sharpness, clipping |
| Theatre/dance | Peak pose, line | Limb cut, shadow face | One per tableau | Edge-limb detection |
| Conference | Mouth closed, gesture | Mid-word, blocked slide | One per speaker moment | Mouth-open ratio |
| Party/flash | Lit face, laugh | Flash misfire, black | One per moment | Face vs background luma |
| Festival | Energy + performer | Blocked, over-bright | Per artist, 15–30 | Per-artist grouping |
| Graduation | Handshake, both faces | Hands-only, blink | One per graduate | Two-hand + face detection, name |
| Runway | Full look, apex | Blurred feet | One per look | Full-body, foot sharpness |
| Boudoir | Intentional closed eyes OK | Blur on face | One per frame | Closed-eye toggle off |
| Street | Decisive gesture | Blur on subject | One per moment | Subject sharpness |
| Documentary | Honest moment, names | Misidentified person | One per beat | Caption/name status |
| Pets | Nearest eye sharp | Mid-blink, play blur | One per moment | Pet eye-open, nearest-eye focus |
| Sports (kids) | Body/face sharp, action | Helmet-eyes, blur | One per action | Body vs face sharpness |

---

## Sources

- Aftershoot, How to Cull Wedding Photos Fast (2026): https://aftershoot.com/blog/cull-wedding-photos
- Aftershoot, Culling Genres (support): https://support.aftershoot.com/en/articles/10570203-aftershoot-culling-genres
- Aftershoot, Settings & Culling preferences: https://support.aftershoot.com/en/articles/6508163-setting-your-ai-automated-culling-preferences-in-aftershoot
- Aftershoot, Best AI culling tools for portrait photography (2026): https://aftershoot.com/blog/best-ai-culling-tool-for-portrait-photography
- Aftershoot, Runway photography guide: https://aftershoot.com/blog/runway-photography
- Narrative Select, portrait & family culling pages: https://narrative.so/use-cases/portrait-photographers ; https://narrative.so/use-cases/family-photographers
- Narrative, How many photos a wedding photographer should deliver: https://narrative.so/blog/how-many-photos-wedding-photographer-deliver
- Narrative, Photo culling checklist for beginners: https://narrative.so/blog/photo-culling-checklist-beginners
- Imagen AI, Corporate event culling: https://imagen-ai.com/valuable-tips/how-long-photo-culling-takes-for-corporate-event-photographers (403 on fetch; snippet only)
- Imagen AI, Headshot culling tips: https://imagen-ai.com/valuable-tips/photo-culling-tips-for-headshot-photographers
- Imagen AI, Concert culling software: https://imagen-ai.com/valuable-tips/photo-culling-software-for-concert-photographers-2026/
- Imagen AI, Pet culling software: https://imagen-ai.com/valuable-tips/photo-culling-software-for-pet-photographers-2026/
- Selekt, Concert culling workflow: https://selekt.photos/blog/concert-photography-culling-workflow
- Selekt, Volume headshot culling workflow: https://selekt.photos/blog/volume-headshot-day-culling-workflow
- FilterPixel, DeepCull genre modes (vendor claims, unverified): https://filterpixel.com/blog/deepcull-genre-modes
- Photoreka, Best AI culling software 2026 comparison: https://photoreka.com/blog/best-ai-culling-software
- Digital Photography School, Conference tips: https://digital-photography-school.com/7-tips-for-photographing-a-conference
- Lauren Lindley, Conference speaker photo tips: https://laurenlindley.com/conference-speaker-photo-tips-from-a-professional-event-photographer
- SXSW, How to photograph SXSW: https://sites.google.com/sxsw.com/howtophotographsxsw/conference/conference-tips
- Gradimages, Avoid background distractions in commencement handshake: https://resources.gradimages.com/avoid-background-distractions-in-commencement-handshake-photographs
- Syracuse University News, graduation photo tips (2026): https://news.syr.edu/2026/05/07/10-tips-for-taking-a-great-graduation-photo/
- Adobe Photoshop Help, Combine images into group portrait: https://helpx.adobe.com/photoshop/desktop/create-masks/blend-images/combine-multiple-images-into-group-portrait.html
- Paige Gribb Photography, Shoot a fashion show: https://paigegribbphotography.com/blog/how-to-shoot-a-fashion-show-4-tips-to-level-up-your-runway-photography
- Sigma, Pet photography tips: https://blog.sigmaphoto.com/2020/pet-photography-shooting-tips/
- Canon Europe, Pet photography tips (Animal Eye AF): https://canon-europe.com/get-inspired/tips-and-techniques/pet-photography-tips
- HSM University, Captions, accuracy and corrections (PHOT-320): https://hsmuniversity.org/us/course/phot-320-photojournalism-and-ethics/lesson-8-captions-accuracy-and-corrections
- HSM University, Working to deadline: editing, transmission and metadata: https://hsmuniversity.org/us/course/phot-320-photojournalism-and-ethics/lesson-7-working-to-deadline-editing-transmission-and-metadata
- CUNY, Photo filing and ethics guidelines: http://photo.journalism.cuny.edu/photo-filing-and-ethics-guidelines
- Reddit r/WeddingPhotography, delivery counts: https://reddit.com/r/WeddingPhotography/comments/17a2w06/how_many_photos_to_deliver

Not fetched or only snippet-checked: theatre/dance, street, nightlife flash, religious ceremonies, the Narrative/Imagen group-culling specifics. Treat those sections as convention until verified.
