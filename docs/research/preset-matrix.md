# Preset matrix: proposed tree, signals, exceptions

Sources: the four research files in `docs/research/`, `profile.rs` (ShootPreset, ScanProfile), `settings.rs` (field names). Status is judged from names only, so verify before relying on it. Effort S/M/L. Value H/M/L for this photographer (motorsport, car meets, property condition and real estate, wildlife/birds, some portraits and events, possible sport). "unsourced" means the research gave no delivery ratio.

Existing ids are kept. New parent profiles proposed: `sport`, `nature`, `property`, `commercial` (each needs a `ScanProfile` arm with its own priority, pan and faces rules). Aviation, trains and drag go under the `motorsport` family, since they share pan rules.

## 1. Preset tree

### 1a. Motorsport, transport, event, portrait, mix

| id | label | parent | decisive subject | keeper | reject | blur / pan | group unit | faces | VLM targets | delivered |
|---|---|---|---|---|---|---|---|---|---|---|
| motorsport | Motorsport | motorsport | car/moto/truck box | car sharp, pan streak, number visible | soft car, nose/wheel cropped | moderate / yes | one corner pass or overtake | opt | race no., class, livery, sponsor | unsourced |
| motorsport-burnouts | Burnouts | motorsport | car, rear wheels, smoke | car sharp, backlit smoke | smoke hides car | lenient / yes | one burnout | n | car no., class | unsourced |
| motorsport-track | Track days | motorsport (min_box x0.7) | car | corner pan, car squared | soft car, cropped | moderate / yes | same car, same corner = one | opt | car no., driver, team | unsourced |
| motorsport-rally | Rallies | motorsport (min_box x0.7) | car + dust | airborne/sliding, dust lit, crew visible | dust-only, nose cropped, frozen flight | lenient (dust) / yes | one feature (jump, hairpin) | opt | car no., co-driver, stage | unsourced |
| motorsport-meet | Car meets | motorsport (max 16) | car; detail subject | detail sharp at plane, rolling pan, badge legible | wrong-layer focus, glare | moderate, stack for depth / yes (rolling) | one detail; one rolling pass | n | make, model, year, colour, mods, plate | unsourced |
| motorsport-drag (new) | Drag racing | motorsport | car at launch, flame | launch, flame, wheelstand | past-launch small car | lenient (flame) / yes 1/160-1/250 | one run | n | class, lane, run no. | unsourced |
| motorsport-moto (new) | Motorcycle, speedway, karting | motorsport | motorcycle + rider; kart | lean/knee, rider head sharp, roost lit | rider off-frame, bike cropped, roost hides rider | lenient on roost; rider head is the sharpness test / yes | one corner or one jump | opt | rider/kart no., heat, class | unsourced |
| motorsport-aviation (new) | Aviation | motorsport | airplane (COCO 4) | jet frozen, prop disc blurred, level attitude | frozen prop (flag only), soft canopy, tail cropped | per subject; prop-disc test / yes runway 1/40 | one pass | n | tail/registration, type, display team | unsourced |
| motorsport-rail (new) | Trains | motorsport | train (no COCO class) | sharp train on streaked bg, or frozen | soft train, cropped ends, glare | moderate / yes 1/30-1/60 | one pass | n | headcode, operator, class | unsourced |
| event | Events | event | person (eye > face > person) | face sharp, eyes open, activity | blink, missed focus | moderate / yes | one moment (time + framing burst) | y | people, setting, activity | unsourced |
| event-shows | Shows, concerts, festivals, theatre | event | performer, limbs | peak gesture, face sharp, limbs complete | missed focus, face in shadow, limb cut, blown | lenient (intentional motion) / yes | one song or tableau | y | performer, song, stage | concert keepers 2-3% (Selekt) |
| event-parties | Parties | event | person | lit face, laughter | flash misfire, black frame, red-eye | moderate / no | one moment | y | people, atmosphere | unsourced |
| event-wedding (new) | Weddings, receptions | event | couple, face group | vows/kiss readable, all eyes open | blink, guest in frame, hands over rings, flash-faces | moderate; dance floor motion allowed / no | vows = one; combo = burst | y, closed-eye on, kiss override | VIP and key-person names | 70-100/hr delivered (Aftershoot); ~30% reach clients (Reddit) |
| event-conference (new) | Conference, corporate | event | speaker person | mouth closed, gesture, logo visible | mid-word, blocked slide, "oo" mouth | moderate / no | one speaker moment | y | speaker, sponsor, slide text | unsourced |
| event-graduation (new) | Graduations, awards | event | handshake: 2 hands + 2 faces | both faces, name card readable | hands-only, blink, clutter touching | strict on handshake / no | one graduate, 5 s window | y | graduate name (roster), programme | 1 per graduate |
| mix | Mixed | mix | all | per-frame main subject | none forced | moderate / yes | per moment | opt | none forced | unsourced |
| mix-sequence (new) | Timelapse | mix | whole frame | steady exposure and framing | flicker spike, shift, gap | n/a / no | ONE sequence, never per frame | n | interval, start/end | keep whole sequence |

### 1b. Portrait, sport, nature, property, commercial

| id | label | parent | decisive subject | keeper | reject | blur / pan | group unit | faces | VLM targets | delivered |
|---|---|---|---|---|---|---|---|---|---|---|
| portrait | Portraits | portrait | eye > face | eyes open, sharp face | blink, missed focus on eye | strict on eye / no (bokeh) | one setup | y | none | unsourced |
| portrait-group | Group photos | portrait (min_box x0.5, max 24) | every face | all eyes open, facing | any blink, soft edge face | strict / no | 5-10 burst = composite base | y per face | names L to R | composite |
| portrait-couple | Couples | portrait | 2 faces | connection, kiss face readable | half-blink, back to camera | strict, kiss exempt / no | one moment | y | none | unsourced |
| portrait-pets | Pets | portrait | animal eye | nearest eye sharp | mid-blink, play blur, muzzle cropped | strict on eye / no | one moment | animal eye (MISSING) | breed, name | unsourced |
| portrait-headshot (new) | Headshots | portrait | face | catchlight, eyes open, natural smile, same crop | any blink, glasses glare | strict / no | one subject, 4-8 frames | y | subject name/ID | 1 per subject |
| portrait-school (new) | School, volume | portrait | face + QR/ID card | eyes open, neutral, facing | blink, hair over face, odd pose (warning bucket) | strict / no | one per student | y, gaze, head pose | name from QR/roster | 1 per student |
| portrait-newborn (new) | Newborn | portrait | face (often asleep) | sharp face, relaxed, hands/feet in | fidget blur, parent face cut | moderate / no | one per pose | y, closed-eye check OFF | none | unsourced |
| sport-field (new) | Field sports: soccer, rugby, AFL, US football, cricket | sport | person nearest ball; batter/bowler | ball at contact, ball in hand, header above horizontal, wicket | back to camera, ball out, pile-up, head cut at top | strict (1/1000 subject) / pre-peak kept | one play, possession or delivery | y (ID by number) | jersey no., name, team, score, wicket state | 15-20 per game (wire) |
| sport-court (new) | Basketball, netball, tennis, badminton | sport | person with ball or racket | apex, ball in hands, racket at contact | back to camera, ball hidden, no face | strict 1/500; noise tolerant / yes | one possession or point | y | jersey, name, score | unsourced |
| sport-combat (new) | Boxing, MMA | sport | 2 persons, head proximity | glove on face, impact, knockdown | miss, back to camera, clinch hides face | moderate (1/400-1/500 ok) / no | one round impact | y | fighter, round, result | unsourced |
| sport-athletics (new) | Athletics, swimming, water polo | sport | person in stride; swimmer at breakout | finish, hurdle apex, face on breath | from behind, bib hidden, glare | strict (1/1000; swim 1/800) / opt | one heat, dive, or hurdle | y (breath), bib OCR | bib/lane, name, time | unsourced |
| sport-cycling-skate (new) | Road/track cycling, BMX/MTB, skate, snow | sport | bicycle, skateboard, snowboard, skis + person | road: rider sharp, streak; BMX: board visible at apex | mid-rotation clip, board missing, face-less | road: pan; BMX/skate: strict 1/500+ / road yes | one pass or one trick | y (helmet: closed-eye off) | rider, team, trick | unsourced |
| sport-equestrian (new) | Equestrian, rodeo, horse racing | sport | horse (17) or cow (19) + rider overlap | apex over fence, peak buck, rider face | early focus lock, rider off, face turned | strict 1/1000 jump, 1/800 rodeo / racing yes | one fence or one ride (peak + bucked-off) | y (rider) | rider, horse/bull name, class, score | peak + bucked-off per ride |
| sport-surf (new) | Surf, open water | sport | surfboard (37) + person | surfer inside wave, spray frozen | surfer hidden in lip, underexposed, back | strict 1/1000+ / no (long shutter if style=motion) | one wave | opt | surfer, beach, event | unsourced |
| nature-birds (new) | Birds, perched and flight | nature | bird (14) | eye sharp, wings at apex, head to lens | eye soft, wing-tip cropped, back-focus | strict on eye; wing tips tolerated / opt | one flight sequence | animal eye (MISSING) | species (confidence-gated), sex | 1-3 per behaviour |
| nature-wildlife (new) | Wildlife, safari | nature | animal: cat, dog, horse, sheep, cow, elephant, bear, zebra, giraffe | eye sharp, action, isolated bg | eye soft, cropped, in shadow | strict on eye; pan for running / yes (running) | one behaviour | animal eye | species (critical), sex, age | 1-3 per behaviour |
| nature-macro (new) | Macro, focus stacks | nature | user/saliency box (no class) | sharpest plane holds key feature; halo-free merge | halo or ghost, wind blur, missing stack frame | strict within plane / no | ONE STACK, never per frame | n | species, magnification | merged output |
| nature-underwater (new) | Underwater | nature | fish, diver (person) | subject sharp, close, no backscatter | backscatter specks, murk, colour cast | moderate / no | one encounter; school = one | opt | species, site, depth | unsourced |
| nature-landscape (new) | Landscape, sunrise/sunset | nature | none (horizon, foreground) | horizon within 1 deg, foreground sharp, sky unblown, silky water ok | tilt over 1 deg, blown sky, soft foreground | strict on static edges; water exempt / no | bracket, focus stack, or exposure set = ONE | n | location, landmark, time | unsourced |
| nature-astro (new) | Astro, star trails | nature | none | pinpoint stars (NPF), Milky Way core sharp, trail stack | elongated stars not in a trail, hot pixels, colour noise | NPF limit / no | one session; trail stack = one | n | location, date, time | keep all stack frames |
| property-listing (new) | Real estate: interiors, exteriors, twilight, drone | property | room region; sky for drone | verticals level, windows not blown, lights on, full room | tilt, mirror reflection, people/text/logos, ghosting, duplicate view | strict on ghosting; tilt <=1 deg / no | bracket set kept until merged; one final per room | n (people are a reject) | room label, address, MLS violations, amenity | 25-40 finals (sourced) |
| property-hospitality (new) | Airbnb, hospitality | property | amenity object | amenity visible and on; cover candidate | dark room, covered amenity, heavy HDR halo | moderate / no | one per amenity; cover ranked | n | amenity vs listing text, room | 30-60 finals (sourced) |
| property-architecture (new) | Architecture, interiors | property | structure, verticals | strong verticals, clear light source, consistent WB | tilt, cables/tripod in frame, WB drift | strict / no | hero + 1-3 per space | n | project, space, credit | 5-15 finals (sourced) |
| property-condition (new) | Condition, inspection, insurance, progress | property | template item (room + item) | wide + close per defect, readable, timestamped | unreadable defect, mirror reflection | legibility on defect region, not aesthetic / no | ALL frames kept, grouped by room and item | n | room, item, condition code, timestamp (mandatory) | ~100% kept (coverage) |
| commercial-dealer (new) | Dealership, broker stock | commercial | car + angle class | fixed angle set complete; odometer lit; VIN legible | dark dash, glare, missing cargo, plate glare, wrong car | moderate / no | one per angle per VIN | n | VIN (check digit), odometer, stock no. | 15-25 per vehicle (sourced) |
| commercial-product (new) | Product, e-commerce | commercial | product mask on white | white bg >=95%, fill >=85%, label sharp | grey/cream bg, shadow, dust, text on main, soft label | strict on label / no | one per angle per SKU | n | SKU, variant | 5-7 per SKU |
| commercial-food (new) | Food | commercial | food hero region | sharp hero, fresh, true colour | soft hero, wilted, blown sauce, crumb | strict on hero / no | one per dish per angle; drop pre-finish | n | dish, venue | 1-2 per angle (sourced) |

## 2. Signal inventory

Status key: EXISTS / PARTIAL / MISSING. "Presets" lists the main users.

| signal | presets | status | effort | value |
|---|---|---|---|---|
| COCO person, vehicle, animal detection | all | EXISTS | - | H |
| Extra COCO classes wired (sports ball, racket, bird, horse, cow, surfboard, skis, snowboard, skateboard, airplane) | sport-*, birds, aviation | PARTIAL (vehicle_classes and animals only) | S | M |
| Ball/racket/bird within person box or ~1 person height | sport-field, court, surf, birds | MISSING | S | M |
| Subject-edge sharpness, per direction | all | EXISTS | - | H |
| Subject sharpness on box vs burst median | all subject presets | EXISTS | - | H |
| Pan score: background streak coherence | motorsport*, cycling, equestrian racing, rail, wildlife | EXISTS | - | H |
| Pan axis match (streak aligned to subject motion) | pan presets | PARTIAL | S | H |
| Per-preset blur threshold (replaces global blur_strictness only) | all | PARTIAL | S | H |
| EXIF shutter gate (per-preset floor) | sport, birds, aviation, equestrian | MISSING | S | M |
| Prop-disc arc-blur test (flag, not reject) | motorsport-aviation | MISSING | M | L |
| Focal-area score for subject-less frames | landscape, astro, property | EXISTS | - | M |
| Face detect and closed-eye | portrait*, event*, sport-court | EXISTS | - | H |
| Closed-eye toggle per preset (off: newborn, helmet, boudoir; kiss exempt) | newborn, cycling, wedding | MISSING | S | H |
| Animal eye detect and sharpness | birds, wildlife, pets | MISSING | M | H |
| Face luma vs background (shadow, flash failure), black frame | parties, shows, wedding | MISSING | S | M |
| Mouth open, smile, expression | conference, shows, school | MISSING | M | M |
| Gaze and head pose (facing camera) | school, group, portrait | MISSING | M | M |
| Pose keypoints (arms up, knee down, lean, header) | sport-field, moto, aviation | MISSING | L | M |
| Body or head cut at frame edge (bbox touches edge) | all subject presets | MISSING | S | H |
| Subject size vs frame (min_box_fraction) | all | EXISTS | - | M |
| Crowd density (count of small persons) | sport-field pile, shows | PARTIAL (max_subjects) | S | L |
| Main subject (largest person or vehicle) and priority list | all | EXISTS | - | H |
| Burst grouping by time gap and framing (burst_gap 4 s) | all | EXISTS | - | H |
| Finer moment split (~1 s gap, per subject) | sport-field, motorsport-track | PARTIAL | S | H |
| Near-duplicate pHash | all | EXISTS | - | H |
| Keeper ranking within group | all | EXISTS | - | H |
| EXIF bracket detect (ExposureBiasValue, focal, f-number, gap <=5-10 s) + feature-match confirm | property-*, architecture, landscape, sunrise | MISSING | M | H |
| Focus-stack detect (monotonic FocusDistance or file pattern) | macro, landscape, meet | MISSING | M | M |
| Timelapse detect (regular interval, same settings) + flicker and shift | mix-sequence | MISSING | M | L |
| Star-trail stack detect (consistent sky vector) | astro | MISSING | M | L |
| Highlight clip % (subject box, sky, window) | property, motorsport white cars, wildlife, events | MISSING | S | H |
| Shadow crush % (room, face) | property, shows, parties | MISSING | S | M |
| Tilt and verticals (Hough median, converging check) | property, architecture, landscape horizon | MISSING | M | H |
| Sky clip vs house exposure split | property-listing (drone) | MISSING | S | M |
| Smoke, dust, roost texture region | burnouts, rally, moto | MISSING | M | M |
| Water region | sport-surf, athletics (swim) | MISSING | M | L |
| Backscatter specks, colour cast | underwater, food, product | MISSING | M | L |
| Noise tolerance relative to ISO | shows, astro, indoor sport | MISSING | S | M |
| Airborne (wheel gap vs ground line) | moto, rally, skate | MISSING | L | M |
| Apex from bbox trajectory across burst | equestrian, skate, birds | MISSING | M | M |
| Two-subject pan (both sharp, bg streak) | car-to-car | PARTIAL | S | L |
| Person-to-person head proximity (overlap at face) | combat, couple, handshake | PARTIAL (boxes exist) | S | L |
| Handshake (2 hands + 2 faces) | graduation | MISSING | M | L |
| Per-subject identity across burst | youth sport, graduation, school | MISSING | M | M |
| Composite best-frame per person | portrait-group | MISSING | L | M |
| Race/car number OCR + entry_list join | motorsport* | PARTIAL (read_numbers, entry_list) | S | H |
| Plates, make, model, colour, team | motorsport-meet, motorsport* | EXISTS (read_plates, identify_make_model/colour/team) | - | H |
| Sponsor and livery text | motorsport, conference | PARTIAL (read_text) | S | M |
| Species name (confidence-gated) | birds, wildlife, macro | MISSING | M | H |
| Room, angle, amenity, dish classify | property, dealer, food | MISSING | M | H |
| VIN OCR, check digit, cross-check | dealer | MISSING | M | M |
| Odometer OCR | dealer | MISSING | M | M |
| Tail registration OCR | aviation | PARTIAL (generic read_text) | S | L |
| Bib, lane, heat OCR | athletics | PARTIAL (read_numbers) | S | M |
| Roster / shot-list coverage (room, angle, item, child, graduate) | property, condition, dealer, product, school | MISSING | M | H |
| Scene or room classification | property, architecture | MISSING | M | H |
| GPS presence and clustering | nature, travel | MISSING | S | M |
| Blur strictness setting | all | EXISTS | - | H |
| NIMA quality tiebreak | all | EXISTS (use_quality_score) | - | L |
| Keeper write-back (rating, label) | all | EXISTS (write_rating, write_label) | - | H |

## 3. Intentional-blur and keep-together exceptions

| exception | presets | rule | detection idea |
|---|---|---|---|
| Panning | motorsport*, cycling, rail, equestrian racing, wildlife running | never cull for blur when pan score and axis match pass | streak coherence along subject motion axis; subject box sharp |
| Prop disc blur | motorsport-aviation | blurred disc is good; frozen prop is a flag | arc-blur test inside spinner circle |
| Water long exposure | landscape | smooth water with sharp static edges is a keeper | low-frequency water region; sharp rock edges |
| Star trails | astro | trailed stars inside a trail stack are kept | consistent sky vector across stack |
| Smoke, dust, roost | burnouts, rally, moto | do not reject the frame for soft texture | texture-energy region around wheels |
| Bokeh | portrait, events | soft background is not a reject | judge subject box only |
| Wing-tip motion | birds | eye sharp overrides wing blur | eye sharpness gate |
| Intentional closed eyes | newborn, kiss, boudoir | closed-eye check off or exempt | preset toggle; mouth-contact for kiss |
| Helmet, goggles, visor | cycling, moto, youth sport | closed-eye check off | helmet/goggle detection |
| Night noise | motorsport-drag, shows, astro | noise is not a reject | noise relative to ISO |
| Dance-floor motion | wedding, parties | lenient blur | lower blur weight on people boxes |
| Focus stacks | macro, landscape, meet | keep all frames together; output is the keeper | focus-distance sequence, same settings |
| Exposure brackets | property, architecture, sunrise | keep set until merged; never cull to one | ExposureBiasValue progression + ORB confirm |
| Timelapse | mix-sequence | never cull individual frames | regular interval, EXIF match; flicker and shift as sequence signals |
| Sunrise silhouette | landscape | black foreground is intentional | do not apply black-crush to silhouettes |
| Anticipation frame | sport-field | pre-peak frame valid | keep frames before apex in the same play |
| Grad handshake | graduation | one group per graduate in 5 s | handshake detect + time gap |

## 4. Top 10 implementation items (value over effort)

1. Preset tree in `profile.rs` with a per-preset policy table (blur multiplier, pan allowed, faces, group unit): S-M, H.
2. Edge-cut flag from person/vehicle boxes (limb, head, nose, wheel cut): S, H.
3. Highlight and shadow clip percent on subject, sky and window regions: S, H.
4. Per-preset blur threshold and pan axis match, replacing the global blur_strictness: S, H.
5. EXIF grouping for brackets, focus stacks, timelapses and sunrise sets, with keep-together rule: M, H.
6. Tilt and vertical-line estimator (Hough median, converging check) for property and architecture: M, H.
7. Roster and shot-list coverage report (room, item, angle, child): M, H.
8. Animal eye sharpness gate for birds, wildlife and pets: M, H.
9. Wire extra COCO classes (sports ball, racket, bird, horse, airplane) with ball-near-person proximity: S-M, M.
10. Dealer angle set with VIN and odometer OCR and check digit: M, M.
