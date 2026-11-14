# Sports culling requirements by genre

Purpose: concrete culling rules per sports genre so Conrod presets can be tuned. Sources are photographer guides, forum threads, agency and wire caption guides, and culling-vendor blogs. Vendor blogs (FilterPixel, Aftershoot, Imagen) are marketing and are flagged as such. Numbers marked **(inferred)** are my synthesis, not a quoted source. Shutter figures are starting points; the sources agree that the right value depends on subject speed, lens length and light.

## Cross-genre baseline

- **Sharpness of the subject beats overall sharpness.** Reject a frame when the athlete is soft even if the background is crisp. (FilterPixel, "Sports Photography Workflow"; imagic "Burst Shot Culling for Sports Photography")
- **Peak action is one instant in a burst.** Shoot bursts, then pick the apex frame. Sources describe the peak as the catch, the apex of the jump, or the contact moment. (FilterPixel; Michael Berkeley Photography, cricket and rugby guides)
- **Anticipation frames are valid keepers in some sports.** Football photographers describe the "pre-peak" frame (hands out for the ball, tension before the catch) as a keeper in its own right. (BYU Photo, "How To Photograph Football Part 1")
- **Eyes or face visible matters.** Digital Photography School: "include the eyes of the subject"; back-of-player frames are rejects unless the story needs them.
- **Duplicates collapse to one moment.** Burst groups should yield one selected frame per decisive instant, with the rest flagged as duplicates.

---

## 1. Soccer / football (association)

**1. Keeper**
- Peak touch or contact: header, shot, tackle, keeper save, goal celebration.
- Ball visible and near the contact player. Eye-level headers and knee-high action are cited as the best frames because the face is above horizontal. (DPS "8 Tips On How to Photograph Sports")
- Goalkeeper frames count: "don't forget to photograph the goal keeper." (DPS)
- Emotion: goal celebration, reaction, injury distress.
- Player identifiable (number or face).

**2. Rejects**
- Ball out of frame when the story is the pass or shot.
- Back of player, or player turned away with no face.
- Focus on a defender when the ball carrier is the subject.
- Frames before the touch with the player still running and no contact.
- Heads cut off by the top edge in headers.

**3. Pattern**
- Long lens (70–200 on half-pitch, 300–600 on full pitch) plus wide for warm-up and atmosphere.
- Continuous AF, bursts of 6–10 frames at the moment of contact.
- Shutter 1/1000 to 1/2000 for running players; ball-in-kick frames need 1/1250–1/1600 (Kevin Lisota, NFL guide, transferable).
- Aperture f/2.8–f/4 with ISO allowed to float.

**4. Subject priority & grouping**
- Decisive subject is the ball carrier or shooter. The goalkeeper is a second subject for save sequences.
- Group by play: one attack or set piece (corner, free kick) equals one moment; select one or two frames from it.

**5. ID needs**
- Shirt number and name for every selected frame, plus team and competition.
- Caption template (see Section "Caption standards" below).

**6. Conrod signals**
- COCO `sports ball` (32) inside or touching a `person` bbox, or within about one person-height of its centre.
- Face or eye detection on the nearest `person`, with eyes open.
- Pose keypoints: arms raised or head below shoulder line for headers (inferred).
- Subject sharpness on the person bbox, relative to the burst median.
- Reject when `person` bbox height is under about 6% of frame height (too far) (inferred).
- Blur tolerance: reject subject blur above about 1/1000 s equivalent (inferred from 1/1000 floor).

---

## 2. Australian rules football

**1. Keeper**
- Mark, tackle, kick at the ball, goal, ruck contest.
- Athlete in the air is a strong frame.
- Ball in the hand or boot. Ball visible in the mark frame.
- Emotion: celebration after goal, team huddle at quarter break.

**2. Rejects**
- Crowded pack (ruck or pile-up) where no individual is readable.
- Subject with the back to camera.
- Obstruction by umpires or players when the subject is a separate figure.

**3. Pattern**
- Shutter at least 1/1000, with 1/2000 used for action (Facebook group thread on AFL photography).
- Canon forum starter settings: Tv 1/500–1/1000, ISO auto, AI Servo.
- Ovals are wide, so a 100–400 or 70–200 is common. Aperture f/2.8 to f/5.6 (Australian Photography, "Secrets of shooting sports part two"). The article recommends f/5–f/7.1 for coverage as more usable frames.
- Bursts are long; burst 6–10 frames on the mark.

**4. Subject priority & grouping**
- The player with the ball is the subject. The mark or kick is the moment.
- Group by possession/contest. Goals and marks each get one selection.

**5. ID needs**
- Jumper number and name; club and league. Full name in caption.

**6. Conrod signals**
- `sports ball` plus a `person` in the air (feet raised above the ground line in pose, bbox top edge above the crowd).
- Number read via OCR on the chest or back of the jumper, when the bbox is large enough.
- Crowd density: many `person` boxes with low individual size means reject as crowd frame unless it is a huddle.

---

## 3. Rugby league and rugby union

**1. Keeper**
- Tackle with the ball carrier visible, offload, try, conversion kick, scrum or lineout contest.
- Ball visible on the try and in the hands.
- Emotion: try celebration, referee interaction, injury.
- Matt Hughes Photo: "shutter speed is the most important setting."

**2. Rejects**
- Scrum or ruck that is a pile of bodies; no clear face.
- Frame with the ball carrier's face hidden by a tackler.
- Frames shot from behind at the try line when the player is not turned.

**3. Pattern**
- Michael Berkeley: f/2.8 open, 1/1000, ISO auto. Matt Hughes: "around 1/1000" with ISO auto.
- Long lens (100–600) and close-in positioning. Shoot low for drama.
- Bursts on the tackle contact.

**4. Subject priority & grouping**
- Ball carrier or tackler, not the crowd.
- A sequence (ruck to tackle to offload) is one moment per phase. Select one frame per phase where the outcome is visible.

**5. ID needs**
- Number and name. Club names. The try/score as caption detail.

**6. Conrod signals**
- Multi-person contact: two or more `person` bboxes with overlapping boxes at the same coordinates (tackle). Score the frame highest where one box is the ball carrier (ball inside its bbox).
- Shutter floor: reject frames under 1/800 for contact subjects (inferred from the 1/1000 guidance).

---

## 4. American football

**1. Keeper**
- Quarterback release, catch, tackle, sack, touchdown celebration, sideline reaction.
- Ball visible on throw or catch.
- Emotion: coach, bench, sideline celebration.
- Peak: ball in the hand at release.

**2. Rejects**
- Back-to-camera QB frames when the other side is available.
- Frames with the ball and receiver both out of focus.
- Subject obstructed by linemen.

**3. Pattern**
- 1/1000 minimum; 1/1250–1/1600 for the ball in a throw or kick (Lisota).
- Aperture f/4 on zoom lenses, f/2.8 indoors.
- Monopod and IS on for tele work. Second body with 70–200 for close action.
- JPEG vs RAW: bursts of 10+ fps in the end zone.

**4. Subject priority & grouping**
- Quarterback, receiver, or tackler, depending on the play. Ball carrier is the subject when the ball is visible.
- One play = one moment. A kick is one moment.

**5. ID needs**
- Jersey number and name in caption; team and league; down/yard context is optional.

**6. Conrod signals**
- Ball as `sports ball` must be within the QB or receiver `person` bbox for the throw/catch.
- Ball size in frame: low pixel size means too far. Inferred threshold: ball bbox at least about 0.4% of frame area.
- Number OCR on the jersey.

---

## 5. Cricket

**1. Keeper**
- Bowler delivery at the release point, batter's contact (bat on ball), wicket (bails off, stumps), catch, celebrations.
- Ball visible at contact or in the wicket frame.
- Michael Berkeley: "you will want to freeze the ball in almost every case."

**2. Rejects**
- Batter's back when facing away; bowler's back at release.
- Fielders obscured by bowler or umpire.
- Frames before the ball is near the batter when the story is the wicket.

**3. Pattern**
- Michael Berkeley: "burst of 6–8 images," start before the ball reaches the batter.
- Shutter at least 1/1000. ISO 5000 on Sony A9II at 600mm is an example (Berkeley photo caption).
- Long lens 200–600 plus wide for crowd.
- Wide aperture to blur crowd and advertising hoardings.

**4. Subject priority & grouping**
- Batter and ball are the subject for a shot; bowler for the release; wicketkeeper and slips for catches.
- One ball delivery = one moment. The wicket delivery is the decisive moment.

**5. ID needs**
- Player name and number; team and match status (wicket, runs); event name, date, venue.

**6. Conrod signals**
- `sports ball` near the batter's `person` bbox at contact (within one person-height).
- Stumps and bails are hard to detect with COCO; flag wicket for OCR/manual check (inferred).
- Burst gaps: deliveries are about 1 per 5–6 seconds; group bursts by time gap under about 1 s (inferred).

---

## 6. Basketball and netball (indoor)

**1. Keeper**
- Dunk, layup at the rim, block, shot release with ball in hand, celebration, face visible.
- Ball in frame near the athlete. Ball in mid-air at the apex is a keeper.
- Emotion: celebration, coach reaction, defensive contest.

**2. Rejects**
- Back-to-camera; ball hidden behind bodies.
- Ball not in the subject's hands.
- Noise so high the face is smeared.

**3. Pattern**
- Exposure guidance: f/2.8 with ISO up to 3200–6400 (Photzy); 1/1000 for stopping action; 1/500 minimum for most (Exposure Guide). One source: 1/200 minimum indoors on a bright arena (exposureguide) (conflicting).
- Shoot from the baseline or under the rim. Burst mode.
- Noise: "Grain over motion blur is preferable" (ShotKit). ShotKit: ISO 400–800 ideal, but accept higher.
- Netball follows the same pattern with a lower frame rate; similar guidance (inferred).

**4. Subject priority & grouping**
- Ball carrier or shooter. For a rebound or dunk: the one who gets the ball.
- One possession, one moment for a dunk.

**5. ID needs**
- Jersey number, player name, team, score.

**6. Conrod signals**
- Noise tolerance: accept higher noise (ISO 6400+) when the subject is sharp. Use per-ISO thresholds and reject frames above the sensor's ceiling (inferred).
- Shutter: 1/500 floor for most; reject below 1/320 for dunks (inferred).
- `sports ball` above the rim or within the athlete bbox at the apex.

---

## 7. Tennis

**1. Keeper**
- Serve at contact, forehand or backhand at contact, volley, celebration, emotion after point.
- Ball in frame at contact (racket face).
- Face visible in the follow-through.

**2. Rejects**
- Players on the far baseline with no face.
- Ball not visible at contact.
- Shot where the racket is blurred and the player is not sharp.

**3. Pattern**
- Shutter 1/1000 to 1/2000 (Photzy "rarely below 1/1000"); 1/8000 used outdoors (Facebook thread). ISO 500 example.
- Long lens from the end (70–200 or 200–600). Shoot from the sides for serve action.
- Back-of-court positions favour the serve and return.

**4. Subject priority & grouping**
- Player at contact; ball is the second subject.
- One point = one moment. Key points (break point, match point) get more selects.

**5. ID needs**
- Player names, country flags; match name, round, score.

**6. Conrod signals**
- `tennis racket` (38) and `sports ball` (32) near the `person` bbox at contact.
- Pose: arm extension at contact. Face detection on the player.
- Key points from the player's position relative to the court.

---

## 8. Athletics / track and field

**1. Keeper**
- Finish line dip or breast-tape, athlete mid-stride with face visible, hurdle clearance (apex), jump or throw release, celebration.
- Emotion at finish (pain, joy).
- Athlete frames with the full body in the air.

**2. Rejects**
- Athlete from behind during a running shot.
- Lane number or bib obstructed.
- Frames of the wrong athlete when the race is the story.

**3. Pattern**
- Long lens 200–600 on track; wide for start and crowd.
- Shutter 1/1000 minimum (inferred from the sprint guidance).
- Panning for sprint: 1/60–1/125 at track speed, requires practice (Canon "Panning Techniques"). Panning is a deliberate choice, not a default.
- Bursts at finish.

**4. Subject priority & grouping**
- Leading athlete and finish frames. For heats, one group per heat.
- A sequence of hurdle clears: one frame per hurdle is the cull; choose the apex frame.

**5. ID needs**
- Bib number and name; event, heat, placing, time.

**6. Conrod signals**
- Bib number OCR. Bib is in the athlete's chest; OCR region on person bbox.
- Finish line detection: frames near the line plus a body-lean pose.
- Panning detection: subject sharp, background streaked (directional blur high in background, low in subject bbox).

---

## 9. Cycling (road, track, BMX, MTB)

**1. Keeper**
- Road/track: rider sharp with background streaked (panning) when speed is the story; rider face and helmet visible for portraits.
- BMX/MTB: jump apex, whip, landing, dust/roost explosion.
- Peloton: sprint or crash.
- Helmet sharp (helmet is the focal point per Canon/Digital Camera World).

**2. Rejects**
- Subject sharp and background sharp when panning was intended (no sense of motion).
- Panning where the subject is blurred too (hit rate, see Car Photo Tutorials).
- Rider back or helmet down in a tuck with no face.

**3. Pattern**
- Panning: 1/30–1/8 in beginner guides; 1/60–1/125 for fast riders; start at 1/30 and work down (Instagram guide). Expect 25–50% hit rate early (Car Photo Tutorials; applies to panning generally).
- Freeze: 1/1000–1/1600 for BMX/MTB jumps (inferred from MTB/MX guidance; MX guidance: 1/1600+ for roost).
- Track and road: 300–600 mm; panning at 1/125–1/320 as middle ground (Digital Camera World).
- Continuous AF and burst.

**4. Subject priority & grouping**
- Lead rider or breakaway. Panning frames of the same rider group into one moment per pass.
- Group by time; select one frame per pass.

**5. ID needs**
- Team, rider name, race, stage, place. Helmet design identifies riders in some cases (Digital Camera World).

**6. Conrod signals**
- `bicycle` (1) bbox plus `person` bbox overlap.
- Panning detection: subject sharpness high, background gradient energy low in subject-adjacent regions. Panning frames are keepers, not rejects, for road/track when the genre setting asks for motion.
- Shutter floor: reject non-panning frames under 1/500 (inferred from freeze guidance).

---

## 10. Swimming and water polo

**1. Keeper**
- Start (dive), breakout from water, turn at wall, finish at the touch, celebration.
- Face visible on breath for breakout or head-on frames.
- Ball visible in water polo; goal celebration.

**2. Rejects**
- Swimmer's back or underwater frames with no face.
- Glare or reflection obscuring the athlete (pool lights).
- Frame before the swimmer is in view.

**3. Pattern**
- Shutter at least 1/1000 (Jeff Cable: "at least 1/1000 sec"; PM Photo: 1/1000 starting point, 1/800 in dark pools).
- 1/2000 on Adorama guide for freeze; panning around 1/15.
- Burst 3–14 fps (Jeff Cable), servo AF.
- Fast lens f/2.8; higher ISO accepted (Photzy: ISO 2500–3200 indoors).
- Indoor glare: lens filter or exposure compensation (Facebook thread).

**4. Subject priority & grouping**
- Swimmer at the breakout or the finish. For starts: one frame per dive.
- Water splashing counts as context but not as subject.

**5. ID needs**
- Lane number and swimmer name; event, heat, time.

**6. Conrod signals**
- Pool/water region: Conrod can detect water by colour; face detection on breakout frames.
- Glare detection: highlight clipping in the subject bbox above a threshold reject.
- Shutter floor: reject below 1/800 for indoor swim (inferred from PM Photo).

---

## 11. Surf and water sports

**1. Keeper**
- Barrel or tube with the surfer visible inside; aerial with clear arc; carve with spray.
- Face visible in the frame or board clearly identified.
- Spray in the frame adds drama but is not a keeper on its own.

**2. Rejects**
- Surfer hidden in the lip; wave face empty.
- Frames where water is blurred and the surfer is soft.
- Surfer's back on a take-off with no face.

**3. Pattern**
- 1/1000 to 1/4000 to freeze spray (Chris Bray; UW Photography Guide; Facebook thread used 1/4000).
- Long slow shutter (1/8 s on a tripod) for milky water is an alternative style; do not cull these as blur unless the style is "motion" (Chris Bray).
- Burst and fast cards.

**4. Subject priority & grouping**
- Surfer at the barrel or manoeuvre. One wave = one moment.

**5. ID needs**
- Surfer name; event; heat; location (beach).

**6. Conrod signals**
- `surfboard` (37) near `person`.
- Exposure: reject underexposed frames in shore-break (inferred from UW Photography Guide advice to dial exposure down).
- Shutter floor 1/1000 for spray-freeze.

---

## 12. Rodeo, bull riding and bronc riding

**1. Keeper**
- Rider and animal in the same frame at peak: bull at full buck with rider still on; bronc rider in the air; rider's face visible.
- Rider's face visible when the bull turns to camera (Wyoming News tip: "click right before they turn around facing you").
- Crooked sunglasses, flying hat or details (WTE photo editor).
- Ejection or landing with the dust cloud.

**2. Rejects**
- Rider off the animal (only a keeper if the story is the bucking-off frame).
- Bull or horse blur with rider sharp (or reverse) where the subject separation is lost.
- Rider with back to camera and no face.

**3. Pattern**
- Shutter: 1/800 minimum (DR Photos, "my lowest speed setting is 1/800"); 1/1000–1/1600 as baseline (Improve Photography, Olie's Images: 1/1600+ for peak). Some guides: 1/500 minimum (Cairns Photo Courses).
- Aperture wide open (f/2.8–f/4). Auto ISO with shutter floor is standard for arena light.
- Flash is avoided at chutes because it spooks stock (Cairns Photo Courses).
- Bull riding: stand close to the chutes. Bronc: stand where horse opens through the turn.
- Very high frame count: 3,500–4,000 photos per rodeo day (WTE).

**4. Subject priority & grouping**
- Rider is the decisive subject, with the animal as second. Bull riding: rider's face.
- One ride = one moment group: chute open to ride end. Select one peak frame per ride, plus one "bucked off" frame.

**5. ID needs**
- Rider name, animal name if known, event, placing, score. Bull names are important to event sales.

**6. Conrod signals**
- Animal detection: COCO has `horse` (17) and `cow` (19); bulls may be detected as `cow` but verify on the dataset (inferred; COCO has no bull/bronc class).
- Rider `person` box overlapping the animal bbox (rider-on-animal test).
- Rider face visible: `person` keypoints for head and shoulders.
- Blur: reject frames where the rider box and animal box are both soft.

---

## 13. Equestrian (show jumping, dressage, eventing, racing)

**1. Keeper**
- Show jumping: horse and rider at the apex over the fence, with clean form, rider's face visible.
- Racing: horse head-on at the finish line; jockey's face visible; stride extended.
- Dressage: extended trot, pirouette, canter stride, with clean form.
- Clear, well-timed apex frame (Budget Equestrian: "peak of the jump is the most dramatic moment").

**2. Rejects**
- Lock-on to the jump too early: frame with rider behind the horse (Clark Stinson on AF grabbing the fence).
- Rider face turned away; horse's head cut off.
- Fence front-on with horse hidden behind it.

**3. Pattern**
- Shutter 1/1000 or faster for jumps (Budget Equestrian; Photo.net 1/1200–1/2000; Facebook: 1/1200 minimum).
- Aperture f/2.8 with Auto ISO.
- Continuous AF with tracking; moderate tracking sensitivity (Clark Stinson).
- Burst is useful for action sequence but many photographers prefer single shots (Katie Mortimore, "Behind the magic"). Burst is a tool, not a rule.
- Racing uses panning at lower speeds for motion (Michael Berkeley).

**4. Subject priority & grouping**
- Rider + horse as one subject; face and form decide keepers.
- One fence or one stride pattern = one moment group. Select apex frame per fence.

**5. ID needs**
- Rider name, horse name, event, class, placing, sponsor. Show jumping results often in caption.

**6. Conrod signals**
- `horse` (17) bbox with `person` (rider) overlap.
- Apex detection: horse bbox top at or near its minimum vertical speed (use bbox sequence across burst).
- Shutter floor 1/1000 for jumps (inferred from sources).
- Face detection on the rider (above horse's top edge).

---

## 14. Motorsport complements (motocross, supercross, drift)

**Motocross/supercross (brief)**
- Keeper: rider in air at apex with the bike visible; roost (dirt) flying; face or goggles visible; rider leaning in a berm.
- Shutter: 1/1600–1/2500 for roost (Motocross Action); 1/800–1/1000 for wheels to show spin (Martin Duffy). Panning: 1/160 approx (Martin Duffy). Fstoppers: 1/1000 default, 1/20 for panning.
- Reject: rider at the top of the frame with no bike, or rider hidden by dust.
- Conrod: `motorcycle` (3) plus `person`; roost detection is hard.

**Drift and road racing (brief)**
- Panning at 1/125–1/320 with the wheel spin visible (Digital Camera World, Canon). Hit rate matters: expect 25–50%.
- Conrod: `car` (2) or `motorcycle` (3) with background blur; panning flag.

---

## 15. Combat sports (boxing, MMA)

**1. Keeper**
- Punch contact (glove on face), sweat or spray at impact, knockdown, referee interaction, celebration.
- Face visible and punch line clear.
- Motor drive is not a substitute for timing: "the glove flattening the face is only a nanosecond" (DPS ringside guide).

**2. Rejects**
- Punch thrown but glove not on face (miss).
- Fighter's back to camera.
- Clinch where the face is obscured.

**3. Pattern**
- Shutter: 1/400–1/500 for the controlled-blur look; 1/1000+ to freeze impact and sweat (Photocrowd; Facebook thread).
- Aperture f/1.8–f/3.2; ISO up to 3200 (Photocrowd).
- Ringside, flash for knockdowns and post-fight (FM Forums). Flash caution: rules vary by promoter.
- Burst helps but timing matters most.

**4. Subject priority & grouping**
- The fighter with the punch or the knockdown. Sequences: one round sequence = one group; select the impact frame.

**5. ID needs**
- Fighter names, weight class, round, result, event.

**6. Conrod signals**
- Glove contact: `person` bboxes with close head proximity and reduced separation (two person bboxes overlap at face). Gloves are not COCO classes (inferred).
- Face visible on both fighters.
- Shutter floor 1/500 for impact frames (inferred from Photocrowd).

---

## 16. Skate and snow action sports (skateboarding, snowboarding, freeski)

**1. Keeper**
- Trick at apex: kickflip, grind, air, spin with board visible; face or helmet visible.
- Snow: spray and board clearly in the air.
- Clean landing as a second keeper.

**2. Rejects**
- Subject mid-rotation with board or body clipped.
- Frames where the board is missing from the frame.
- Subject only silhouetted with no face when the story needs it.

**3. Pattern**
- Shutter at least 1/500 (Digital Photography School); 1/1000+ for freeze.
- Wide aperture f/2.8–f/5.6, high-speed burst (Take Tack-Sharp video).
- Trick lasts under a second; anticipate and shoot just before the peak (DPS).
- Flash for night or harsh light (DPS flash video).

**4. Subject priority & grouping**
- Rider at apex. One trick attempt = one moment; select the best attempt.

**5. ID needs**
- Rider name, event, location, trick name where relevant.

**6. Conrod signals**
- `skateboard` (36), `snowboard` (31), `skis` (30) near `person` bbox.
- Apex detection from bbox trajectory across burst.
- Shutter floor 1/500 (DPS).

---

## 17. Junior and school sport

**1. Keeper**
- Child's face clear, action clear, with a visible emotion. Team or individual.
- Bench shots and reactions are part of the set (Click Love Grow).
- Ball or ball contact in the frame when the sport uses a ball.

**2. Rejects**
- Child not identifiable (back to camera, helmet over face).
- Background clutter (parking lot, parents, bins) (Click Love Grow).
- Frames where other children are unidentifiable and the child is the sale subject.

**3. Pattern**
- Shutter 1/1000 minimum for running kids (Click Love Grow); 1/250 for slow-moving.
- ISO up to 400+ accepted; "I'd rather have grain than blur."
- Burst for action; many shots per child.

**4. Subject priority & grouping**
- Each child is a subject for parent sale. Group per child per game (not per play).
- Coverage: every player should appear (gallerylink guide: "parents notice if their child isn't in the gallery").

**5. ID needs**
- Child's name (if permitted), team, number, school. Privacy: consent policies vary; check league rules (DPReview thread; Facebook group).

**6. Conrod signals**
- Coverage metric: count of distinct child face identities per gallery. Warn if any roster member is absent (inferred; requires roster input).
- Reject frames with no identifiable face on the subject.
- Keep the ISO/grain tolerance high; reject blur first.

---

## Caption standards (wire, agency, editorial)

These apply across genres. Sources: Sideline Captions (AP, Icon Sportswire), Multimedia Toolkit (AP), Getty.

- **AP style**: present tense; first sentence names who (team, position, name, number in parentheses), what is happening, where, and when. Example format: "Team position Name (number), action". (sidelinecaptions.com AP guide; multimediatoolkit.wordpress.com)
- **Icon Sportswire**: `CITY, ST - MONTH DAY:` then `Team position Name (number)`, action, league game phrase, full date, venue. (sidelinecaptions.com Icon Sportswire guide)
- **Getty**: captions must meet editorial accuracy standards with human review (Getty editorial policy). A photographer forum post says Getty requires the IPTC Personality field for all athletes in the image; treat as a user claim, verify before relying on it.
- **Sports caption checklist**: league, full team names, sport name, player position, name, number, action, game period (quarter/half/period), date, venue, city.
- **Wire delivery**: half-time selects 6–8 uploaded within minutes; full game 15–20 selects in about 30–45 minutes (Scott Kelby workflow; FilterPixel vendor blog). Olympic delivery windows of 14 minutes were cited by Fstoppers.

## Conrod implementation notes (cross-genre)

- **Genre preset chooses subject class**: ball sports weight `sports ball` proximity; animal sports weight rider-on-animal overlap; swimming/surf weight water and face; youth sport weights roster coverage.
- **Shutter floor per genre**: 1/1000 default, 1/800 for rodeo and swim (sources), 1/500 for skate and combat impact (sources), panning genres (cycling road/track, motorsport, horse racing) use 1/60–1/320 with panning-flag scoring.
- **Blur tolerance**: judge on subject bbox, not frame. Panning-mode frames pass with sharp subject and streaked background.
- **Burst grouping**: group by time gap under about 1 s (inferred) and by subject identity; one selected frame per decisive instant.
- **OCR**: jersey, bib, race number. Run only on the subject bbox; flag unreadable as "identify manually."
- **Face/eye**: required for most genres except rodeo (rider face preferred), surf and motorsport (helmet or board may be the decisive element).
- **Rejects**: ball out of frame, back to camera, crowd-only frame, obstruction, soft subject, head cut off.

## Summary table

| Genre | Decisive subject | Key keeper signal | Key reject | Blur tolerance (subject) | ID needs |
|---|---|---|---|---|---|
| Soccer | ball carrier / shooter / keeper | ball contact, face above horizontal (headers) | ball out of frame, back to camera | ≤ 1/1000 equivalent (inferred) | number, name, team |
| Aussie rules | ball carrier / mark | mark in air, ball in hand | pile-up, back to camera | ≥ 1/1000 (forum) | number, club |
| Rugby | ball carrier / tackler | tackle contact, offload, try | pile, face hidden | ~1/1000 (Hughes, Berkeley) | number, name, score |
| American football | QB / receiver / tackler | release, catch with ball visible | back to camera, linemen obstruction | ≥ 1/1000, 1/1250+ for ball | jersey number, name |
| Cricket | batter / bowler / keeper | bat-ball contact, wicket | back to camera, fielder hidden | ≥ 1/1000 (Berkeley) | name, number, match state |
| Basketball / netball | shooter / dunker | apex, ball in hands | back to camera, ball hidden | 1/500 floor; ISO tolerant | number, name, score |
| Tennis | player at contact | ball at racket face, face visible | far-baseline, ball not visible | 1/1000–1/2000 | name, round, score |
| Athletics | athlete in stride / finish | finish, hurdle apex, jump release | athlete from behind, bib hidden | 1/1000 (inferred); panning optional | bib, name, event, time |
| Cycling | lead rider / helmet | panning sharp subject + streak; apex on BMX | blurred rider in panning, back to camera | Panning 1/60–1/125; freeze 1/1000+ | team, rider, race |
| Swimming / water polo | swimmer at breakout / finish | face on breath, touch | glare, back to camera | ≥ 1/800–1/1000 | lane, name, time |
| Surf | surfer in barrel / manoeuvre | surfer visible inside wave, clean spray | surfer hidden, soft surfer | 1/1000–1/4000 freeze | surfer, event |
| Rodeo (bull/bronc) | rider + animal | rider in peak with animal at full buck, face visible | rider off, face hidden | ≥ 1/800 (DR Photos), 1/1600 common | rider, animal, event, score |
| Equestrian | horse + rider at apex | clean apex over fence, face visible | early focus lock, face turned | ≥ 1/1000 (jumps) | rider, horse, class, place |
| Motocross | rider in air / berm | roost, rider face/goggles | rider at frame edge, dust-hidden | 1/1600–2500 (roost), 1/800–1000 (wheels) | rider, number |
| Boxing / MMA | fighter with punch | glove contact, impact | miss, fighter back to camera | 1/500–1/1000 | fighter, round, result |
| Skate / snow | rider at apex | trick visible, board in frame | mid-rotation clip, board missing | ≥ 1/500 | rider, trick, event |
| Junior / school | each child | child face, emotion | child unidentifiable, clutter | ≥ 1/1000 for running | child, team, number |

## Sources

- FilterPixel, "Sports Photography Workflow: Cull 5K Photos Fast" — https://filterpixel.com/sports-photography-workflow (vendor blog)
- Sony Alpha Universe, "10 Tips For High-Impact Sports Photography" — https://alphauniverse.com/stories/10-tips-for-high-impact-sports-photography
- Imagen AI, "Sports & Action: Culling the Keepers" — https://imagen-ai.com/valuable-tips/sports-and-action-culling (vendor blog)
- imagic, "Burst Shot Culling for Sports Photography" — https://imagic.ink/blog/sports-photography-burst-culling (vendor blog)
- Digital Photography School, "8 Tips On How to Photograph Sports" — https://digital-photography-school.com/8-tips-on-how-to-photograph-sports
- Photos by Chip, "How I process sports photos quickly" — https://www.photosbychip.com/how-i-process-sports-photos-quickly
- Reddit r/sportsphotography, "Sports shooters: same-day or next-day delivery" — https://www.reddit.com/r/sportsphotography/comments/1pzqvwi/sports_shooters_how_do_you_handle_sameday_or
- Australian Photography, "Secrets of shooting sports (Part two)" — https://www.australianphotography.com/photo-tips/secrets-of-shooting-sports-part-two
- Canon 60D forum, "Starter settings for Australian Rules Football" — https://forums.cameratips.com/discussion/690/starter-settings-for-australian-rules-football
- BYU Photo, "How To Photograph Football – Part 1" — https://www.youtube.com/watch?v=4ZyWLzWwABE
- Kevin Lisota Photography, "Photographing an NFL football game from the sidelines" — https://kevinlisota.photography/2019/10/photographing-an-nfl-football-game
- Matt Hughes Photo, "Rugby Photography Techniques" — https://www.matthughesphoto.com/r-photography/rugby-photography-techniques.html
- Michael Berkeley Photography, "How to Photograph Rugby" — https://www.michaelberkeleyphoto.uk/new-blog/how-to-photograph-rugby
- Michael Berkeley Photography, "How to Photograph Cricket" — https://www.michaelberkeleyphoto.uk/new-blog/how-to-photograph-cricket
- Michael Berkeley Photography, "How to Photograph Eventing" — https://www.michaelberkeleyphoto.uk/new-blog/how-to-photograph-eventing-dressage-show-jumping-amp-cross-country
- Ravin Photography, "Cricket Photography Tips" — https://www.ravinphotography.ca/blog/cricket-photography-tips-camera-settings-positioning-timing
- Exposure Guide, "Basketball Photography Tips" — https://www.exposureguide.com/basketball-photography-tips
- ShotKit, "Basketball Photography Tips, Ideas & Best Settings" — https://shotkit.com/basketball-photography
- Photzy, "My Top Tips for Improving Your Indoor Sports Photography – Volume 2" — https://photzy.com/my-top-tips-for-improving-your-indoor-sports-photography-volume-2
- Photzy, "Top Tips for Improving Your Outdoor Sports Photography" — https://photzy.com/top-tips-for-improving-your-outdoor-sports-photography
- Canon Pro, "Tom Jenkins' 10 Top Tips For Better Sports Photography" — https://www.canon.com.cy/pro/stories/tom-jenkins-better-sports-photography-tips
- MPB, "Guide to Tennis Photography" — https://www.mpb.com/en-us/content/tips-and-techniques/tennis-photography
- Photocrowd, "Sports Photography 101 – How to Shoot a Boxing Match" — https://www.photocrowd.com/blog/182-sports-photography-101-how-shoot-boxing-match
- Digital Photography School, "Tips on Shooting Ringside" — https://digital-photography-school.com/tips-on-shooting-ringside-an-introduction-to-boxing-photography
- FM Forums, "Boxing Photos – Advice" — https://www.fredmiranda.com/forum/topic/476158
- Chris Bray Photography, "Surf and Wave Photography Tutorial" — https://chrisbrayphotography.com/tips/surf_photography.php
- Underwater Photography Guide, "5 Tips for Surf Photography" — https://www.uwphotographyguide.com/5-tips-for-surf-photography
- Jeff Cable, "Photographing a Swim Meet" — https://blog.jeffcable.com/2015/07/photographing-swim-meet-proper-camera.html
- PM Photo, "Splash into Success: Perfecting Your Swim Meet Photography" — https://www.pmphoto.net/Blog-entries/Perfect-swim-pictures
- 42West / Adorama, "A Guide to Swimming Photography" — https://www.adorama.com/alc/swimming-photography
- Cole's Classroom, "Rodeo Photography: The Ultimate Beginner's Guide" — https://colesclassroom.com/rodeo-photography-the-ultimate-beginners-guide
- DR Photos, "Tips on taking rodeo photos" — https://www.drphotos.ca/rodeo/tips-on-taking-rodeo-photos
- Cairns Photo Courses, "How to shoot a rodeo" — http://www.cairnsphotocourses.com/2013/07/photographing-a-rodeo
- Improve Photography, "7 Tips For Rodeo Photography" — https://improvephotography.com/41653/7-tips-rodeo-photography
- WTE / Wyoming News, "WTE photo editor offers tips for getting great rodeo shots" — https://www.wyomingnews.com/news/local_news/wte-photo-editor-offers-tips-for-getting-great-rodeo-shots/article_4bb4bc1c-6dda-11e7-af3b-8b6f851a9d90.html
- Olie's Images, "Rodeo Photography Settings for Barrels, Roping and Chute Action" — https://oliesimages.com/rodeo-western/rodeo-photography-settings
- Heinemann Images, "Freezing the Chaos of Bull Riding" — https://www.heinemannimages.com/post/freezing-the-chaos-of-bull-riding
- Budget Equestrian, "Tips for Taking Great Photos at a Horse Show" — https://www.budgetequestrian.com/tips-for-taking-great-photos-at-a-horse-show
- Clark Stinson, "Technical Settings for Equestrian Photography (Horse Jumping)" — https://www.clarkstinson.com/resources/technical-settings-for-equestrian-photography-horse-jumping
- Katie Mortimore Photography, "Behind the Magic – Taking a good action photo of a horse" — https://katiemortimore.com/behind-magic-taking-good-action-photo-horse
- Photo.net, "Horse Show Jumping Photography – Beginner Questions" — https://www.photo.net/forums/topic/522040-horse-show-jumping-photography
- Canon Snapshot Asia, "Motorsports Photography Techniques (1): Panning Techniques" — https://snapshot.asia.canon/en/article/motorsports-photography-techniques-1-panning-techniques
- Digital Camera World, "How to capture perfect panning shots in motorsport photography" — https://www.digitalcameraworld.com/tutorials/how-to-capture-perfect-panning-shots-in-motorsport-photography
- Motorsport Photographer, "How To Master Slow Shutter Speed Panning" — https://www.motorsportphotographer.com/how-to-master-slow-shutter-speed-panning
- Car Photo Tutorials, "Make panning shots like a pro" — https://www.carphototutorials.com/make_panning_shots_like_a_pro.html
- Motocross Action Magazine, "Ten things about how to become a motocross photographer" — https://motocrossactionmag.com/tenthings-how-to-become-a-motocross-photographer
- Fstoppers, "Tips on How to Photograph Motocross" — https://fstoppers.com/education/tips-how-photograph-motocross-277694
- Martin Duffy Photography, "The Ultimate Guide to Motocross Photography" — https://martinduffyphotography.co.uk/mastering-motocross-photography
- Click Love Grow, "10 Tips For Youth Sport Photography" — https://clicklovegrow.com/10-tips-for-awesome-kids-sport-photos
- Gallery Link, "Youth Sports Photography Business Guide (2026)" — https://gallerylink.live/blog/youth-sports-photography-business (vendor blog)
- DPReview, "youth sports – parents object to photos of their kids on a website" — https://www.dpreview.com/forums/threads/youth-sports-parents-object-to-photos-of-their-kids-on-a-website.1961255
- Aftershoot, "Best Tools for AI Culling in Sports Photography (2026)" — https://aftershoot.com/blog/ai-culling-in-sports-photography (vendor blog)
- Sideline Captions, "How to caption a photo in Associated Press style" — https://sidelinecaptions.com/associated-press-style
- Sideline Captions, "How to Write a Caption in Icon Sportswire Style" — https://sidelinecaptions.com/how-to-caption-in-icon-sportswire-style
- Multimedia Toolkit, "AP Style Photo captions" — https://multimediatoolkit.wordpress.com/ap-style-photo-captions
- Getty Images, "Editorial Policy" — https://www.gettyimages.com/company/editorial-policy
- Fstoppers, "Olympic Photographers Face 14 Minute Deadlines" — https://fstoppers.com/sports/olympic-photographers-face-14-minute-deadlines-8140
- Scott Kelby, "My Sports Photography Workflow (so far)" — https://scottkelby.com/my-sports-photography-workflow-so-far
- Adobe Community, "Add non-IPTC standard Getty 'Personality' field" (user claim) — https://community.adobe.com/feature-requests-676/p-add-non-iptc-standard-getty-personality-field-665970
