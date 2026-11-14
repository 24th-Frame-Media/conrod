# Culling research: motorsport in depth, nature, and misc genres

Purpose: input for Conrod's genre presets and keeper logic. Each genre lists (1) keeper criteria, (2) typical rejects, (3) shooting pattern, (4) grouping into moments, (5) metadata/ID needs, (6) measurable signals Conrod could use.

Confidence: sources are mostly photographer blogs, forums and culling-software docs, not agency rulebooks. Where a point rests on one thin source or on general reasoning it is marked **(thin)** or **(inferred)**. Shutter numbers are starting points, not rules.

---

## Part 1: Motorsport in depth

### 1.1 Circuit racing (Supercars, GT, formula)

1. **Keepers**: car sharp with a clean tyre/nose or livery; a panned frame where the car is sharp and the background streaks; driver visible through the windscreen or helmet; race-start and overtaking moments; cars squared to the lens rather than cropped.
2. **Rejects**: soft car (focus on the background or a wall behind the car); a car cropped at the wheel or nose; a moment where the car is hidden by another car or by marshals; a blurred frame with no streak (motion blur without panning technique reads as camera shake); overexposed white bodywork with no detail.
3. **Pattern**: shutter priority, 1/60 to 1/250 for panning (see 1.2 for detail); 1/1000 or faster for frozen-action shots at corners; continuous AF, high-speed burst on the apex or the exit. Pre-focus on a corner marker and let the car enter the zone.
4. **Moment**: one corner pass by one car, or one overtaking sequence. A pass of the same car through the same corner in consecutive bursts is one moment. Keep the best one or two per moment, and keep a wide and a tight framing if both exist.
5. **Metadata**: race number (primary key for matching to the entry list), driver names, team, class (GT3 vs GT4, Supercars brand), sponsor livery. Race numbers are usually legible at native resolution on the door or the nose.
6. **Signals**: race number and livery read by the vision model; subject sharpness on the car using native-resolution edge blur; panning score (sharp subject, streaked background); burst groups split at lap time or corner entry.

### 1.2 Panning (cross-genre; applies to most motorsport)

- **Shutter range**: practitioners give roughly 1/30 to 1/250 depending on speed and framing. Common starting points: 1/60 to 1/125 for slow cars or bikes, 1/125 to 1/250 for fast cars through a corner. Motorcycle sources cite 1/125 to 1/160 for a pan that keeps the rider sharp **(thin, forum)**.
- **Technique**: pan from the hips, use continuous AF, and keep the subject in the same place in the frame. A sharp subject with a streaked background is a keeper; a streaked subject with a sharp background is a reject.
- **Measurement**: the subject region has low edge blur along the direction of motion, while the background has directional streak (high anisotropy in the direction of motion). Conrod already measures per-direction edge blur, so a pan is "subject sharp in the axis of motion, background streaked along the same axis".

### 1.3 Rally and rallycross (dust, gravel, trees)

1. **Keepers**: car airborne or sliding with a visible dust plume lit from behind or the side; car sharp, dust motion-blurred; the crew (driver, co-driver) visible; trees and terrain framing the car; tyre sidewall detail in gravel.
2. **Rejects**: dust-only frames where the car is lost in the cloud; the car's nose cropped at the frame edge; a frozen flight (car in the air with dust stopped looks unnatural and the sense of motion is gone); dark car in shadow with no detail.
3. **Pattern**: 1/250 to 1/500 for a car with dust frozen; 1/60 to 1/125 for panning the car with the dust streaked. Fast burst at jumps. Stage-specific pre-focus on a jump lip or a bend.
4. **Moment**: one car through one feature (a jump, a hairpin, a ford). Cars in the same feature across several runs are different moments only if the car or light changed; otherwise group by feature and keep the best.
5. **Metadata**: car number, driver and co-driver names, stage name and location (for the moment key), rally name. Sponsor decals on the car.
6. **Signals**: dust plume as a high-frequency low-contrast texture region (distinct from clean background); airborne detection (see 1.7); car number from the vision model; stage/feature from location metadata or GPS if present.

### 1.4 Drifting (smoke, angle)

1. **Keepers**: car at a clear angle (roughly 30 to 60 degrees of yaw) with the front wheels turned and the car sharp; smoke from the rear tyres lit and visible; the car's side or three-quarter view; the driver's face when visible.
2. **Rejects**: car pointing straight with no angle; wheel rotation dominates the frame (a wheel frozen can look wrong, a wheel blurred hides the tyre); smoke cut off; the car cropped at the front tyre.
3. **Pattern**: 1/125 to 1/200 for panning the car with smoke; faster (1/640 to 1/800) for a frozen car. Pan from a corner exit or a track-side angle.
4. **Moment**: one car through one corner. Keep frames of the same car with different angles as separate options, not as duplicates.
5. **Metadata**: car number, driver, event class, sponsor livery.
6. **Signals**: yaw angle from the wheel-to-body axis (inferred, hard); smoke volume as a large low-texture bright region around the rear wheels; subject sharpness on the body.

### 1.5 Drag racing (launch, wheelstands, night)

1. **Keepers**: car at the launch or at the moment of maximum launch; flames from exhaust (visible in frame); wheelstand (front wheels lifted); parachute deployment (rare, usually from the spectator side); driver in helmet; crew reaction at the finish.
2. **Rejects**: frames past the launch where the car is already at speed and small in frame; a frozen wheel and frozen flames at 1/1000 (the launch energy disappears); night frames with heavy noise at high ISO; cars cut off at the front bumper.
3. **Pattern**: the reference source says start around 1/160 to 1/250 and treat as a panning shot, because at the launch the car is not moving fast and flames plus wheel rotation read better with motion **(motorsportphotographer.com, Jun 2026)**. Burst plus AF-C. Time the Christmas tree: the first yellow is the cue.
4. **Moment**: one run (start to finish of one pass). Keep the launch frames and one or two frames of wheelstand. Multiple passes by the same car are separate moments; the same car in the same run is one.
5. **Metadata**: class (Top Fuel, Funny Car, Pro Stock, Stock/Super), driver, lane, run number, elapsed time if available.
6. **Signals**: wheelstand = front wheel contact patch gap (hard; use the track and car silhouette); flame as high-luminance orange region at the rear; night exposure: high-noise score must not penalise a deliberate low-light frame.

### 1.6 Burnouts and skids (smoke volume)

1. **Keepers**: heavy smoke with the car sharp or cleanly panned; tyre heat or rubber visible; the car lined up with the sun or the light source to backlight the smoke; driver visible through smoke.
2. **Rejects**: smoke cutting off the car; smoke so thick the car is lost (or too little smoke to be a burnout); car cropped at the back wheels.
3. **Pattern**: the existing Burnouts preset probably already does this. 1/125 to 1/250 pan; 1/500 to 1/1000 for frozen smoke texture. Backlight with a compensation of plus 1/3 to plus 2/3 for white smoke.
4. **Moment**: one burnout (start to finish of the burnout). Keep a wide and a close option, no more.
5. **Metadata**: car number, driver, class.
6. **Signals**: smoke as large low-contrast bright region around rear wheels; subject sharpness; a burst where smoke density rises then falls is a single moment.

### 1.7 Motocross, supercross and enduro (airtime, whips)

1. **Keepers**: rider and bike at the peak of a jump (airtime) with the bike's wheels off the ground; whips and tabletops (bike rotated); rider's face visible; dust or mud lit; the bike sharp with the rider's body position readable.
2. **Rejects**: bike taking off with the front wheel in the lip (ambiguous); rider cropped at the top of the frame at the peak; the bike soft at 1/500 with the dirt sharp (motion blur on the wheels is fine but not the rider); frames at landing where the rider is already heading down.
3. **Pattern**: forum sources say start 1/1000 for motocross. Track-side, freeze the jump at 1/1000 to 1/1600; deliberate panning at 1/60 to 1/250 for rider-and-background streak. Burst with AF-C on the lip.
4. **Moment**: one jump by one rider. Keep the frames around the peak; the rest of the same jump is one moment.
5. **Metadata**: rider number and name (critical; numbers are on the plate and the rider's jersey), class, moto number (heat/main), sponsor logos.
6. **Signals**: airborne bike (wheels off ground) from the gap between tyre bottoms and the dirt surface (hard; approximate with an edge at the bottom of the wheel vs the background ground line, or use the lip and landing geometry); rider number from the vision model.

### 1.8 Speedway and dirt track (roost, night lighting)

1. **Keepers**: bike at a steep angle through a corner with the rider's knee or leg out; roost (dirt spray) lit by the flood lights; night lighting with the rider sharp; rider on the bike in the middle of a power slide.
2. **Rejects**: roost dominates and the rider is lost in the cloud; a flat-lit night frame with the bike soft from underexposure; ISO noise on the rider's helmet.
3. **Pattern**: 1/250 to 1/500 with the light; 1/60 to 1/160 for a deliberate pan. Pre-focus on the corner apex, which is where the rider holds the lean. Burst.
4. **Moment**: one corner pass by one rider. The same rider through the same corner in a later heat is a different moment.
5. **Metadata**: rider number, name, heat, race number, track name (corner ID).
6. **Signals**: roost as a dense textured region; night exposure should be judged on subject sharpness, not on the dark background.

### 1.9 Karting

1. **Keepers**: kart sharp with the rider's helmet visible; kart at a corner with a visible lean or wheel-slide; a clean pan with the background streaked; the kart's number visible.
2. **Rejects**: kart cropped at the nose; helmet hidden by another kart; a frame where the kart is small and off-centre.
3. **Pattern**: 1/125 to 1/250 pan; 1/500 to 1/1000 for freeze. Burst through the corner.
4. **Moment**: one corner pass; a pack of karts through the same corner is one moment per kart, or one moment for the pack if it is a battle.
5. **Metadata**: kart number (numbers are high-contrast on the nose), rider name, heat, class.
6. **Signals**: kart number from vision model; pack detection (multiple subjects in one frame) should not be treated as duplicates.

### 1.10 Motorcycle road racing (lean angle, knee down)

1. **Keepers**: rider with knee or elbow down at the apex; full lean with the bike's wheels visible (lean shows speed); braking-zone head-on frames with the rider sitting up; tucked-in straight-line shots; rider's face or helmet visible.
2. **Rejects**: rider looking away from the lens with no face; bike cropped at the rear wheel; rider soft (the panning technique rises and falls with the rider, so the panning frame needs the rider's head sharp, not the bike).
3. **Pattern**: continuous AF, burst. Pan at 1/125 to 1/160 through the holding part of the corner; freeze at 1/1000 for the head-on braking shot. Low to the ground at eye level with the rider.
4. **Moment**: one corner pass by one rider, or one sequence (braking, apex, exit). Keep one of each framing for the set.
5. **Metadata**: rider number, rider name, class (MotoGP, Superbike, Moto2), corner name (apex ID).
6. **Signals**: lean angle from the bike's roll in the frame (inferred, hard); knee-down detection (inferred; requires pose estimation); rider head sharpness as the primary sharpness signal for bike frames, not the whole bike.

### 1.11 Car meets and shows (detail shots, rolling shots)

1. **Keepers**: detail shots sharp at the subject (badges, wheels, interior stitching, engine bay, badges with the logo legible); static car with the front wheel turned; interior with the steering wheel and dash lit; rolling shot (car moving, background streaked, car sharp).
2. **Rejects**: detail shot with the focus on the wrong layer (badge soft, glass reflection sharp); wheel with the spokes blurred but the brake caliper sharp; interior with blown highlights in the window; rolling shot with a streak but no sharp car.
3. **Pattern**: detail shots at f/5.6 to f/8 with a focus stack if the subject has depth (engine bay, interior); rolling shots 1/60 to 1/125 with the car at a steady speed (the car's driver and passenger visible if the shot is a rolling one).
4. **Moment**: for detail shots, one subject (a badge, a wheel) is one moment; for rolling shots, one pass.
5. **Metadata**: make, model, year, colour, registration/plate (if in frame); show name, car class.
6. **Signals**: a "detail" tag from a small subject with a large near-sharp region; a focus stack (see 2.3); rolling shot = panning score.

### 1.12 Car-to-car rolling shoots

1. **Keepers**: camera car and subject car at a matched speed, both sharp, background streaked; subject car's livery and driver visible; a clean shot with no tow-line or cable in frame; camera car's own interior not visible.
2. **Rejects**: subject car drifting out of frame; camera car's window reflection; a shot with the cable or the mount visible; one car sharp and the other blurred because the speeds differ.
3. **Pattern**: 1/60 to 1/125 with a fixed speed; burst. Coordinate with the driver (speed and lane).
4. **Moment**: one rolling pass; one shot per speed setting.
5. **Metadata**: car numbers for both cars, location, speed if logged.
6. **Signals**: two subjects in one frame, both sharp, background streaked (a two-subject panning score).

### 1.13 Aviation (airshows, jets, panning, prop blur)

1. **Keepers**:
   - Prop blur: a visible arc of motion in the disc (a blur of the blades rather than a frozen propeller), with the aircraft sharp. Photographers in aviation sources prefer most of the disc blurred; one forum cites a blur over about 40 minutes of the clock face **(thin, forum)**.
   - Jets: frozen jet, with the sky and the wing sharp; or a deliberate pan with the runway streaked (1/40 to 1/125 for the runway).
   - Passes: the aircraft in a clean attitude (wings level, nose toward the lens).
2. **Rejects**:
   - **Frozen prop disc on a propeller aircraft in flight** is a common reject for editorial and stock use. One stock-focused source calls a frozen prop "one of the quickest ways to have an otherwise technically clean image rejected" **(professionalaviationphotography.com)**. Forum sources concur for editorial work **(thin, forum)**.
   - Motion-blurred jet with soft canopy; aircraft cropped at the tail; a streak on the aircraft with no sharpness at the nose.
3. **Pattern**:
   - Prop blur: sources give around 1/125 to 1/250 (a reel from an aviation photographer: 1/125 blurs props, 1/2000 freezes a jet, pan at 1/40 for runway streaks) **(thin, instagram)**. Too slow and the aircraft goes soft; pan with the aircraft.
   - Jets: 1/1000 to 1/2000 for a frozen jet on a pass; slower only for a deliberate pan.
   - Helicopters: sources note prop blur is less a factor than rotor blur; treat as its own case.
4. **Moment**: one pass; a formation or a display sequence is one moment per aircraft-pass, with the prop state recorded.
5. **Metadata**: aircraft **registration / tail number** (e.g. G-XXXX, N-numbers; visible on fuselage or tail), aircraft type, display team/squadron, airshow name and date. Tail numbers are a strong ID key and are often readable at native resolution on a fuselage.
6. **Signals**:
   - Prop-disc test: inside a circle centred on the spinner, measure the directional (arc-shaped) blur versus the blade shape edges. A frozen disc has sharp blade edges at a radius from the spinner; a blurred disc has a smear that is angularly directional. Compare with the fuselage sharpness; if the fuselage is sharp and the disc is frozen, flag as a reject candidate for propeller aircraft. Make the rule a flag, not an automatic reject, since some users want frozen props.
   - Shutter speed in EXIF: a prop aircraft at 1/500 or faster is a candidate for "frozen prop" check; a prop at 1/125 to 1/250 is a candidate for "good prop blur".
   - Jet frozen: subject sharpness high, with no spinning element.
   - Registration: OCR of the tail number from the vision model; match to a list.

### 1.14 Trains and railways

1. **Keepers**: train sharp with a streaked background (panning); train sharp with a frozen background (high speed); front-on (driver's cab) with a sharp windscreen; train number or class livery visible; train through a station or a bridge with leading lines.
2. **Rejects**: train soft in a pan (pivot from the arms rather than the hips); train cropped at the front or rear; windscreen glare (from a window shot); the pantograph and catenary cut off where they matter.
3. **Pattern**: shutter priority. Pan 1/30 to 1/60 for pronounced streaks (one train source gives 1/60 start and 1/30 for strong streaks **(thin)**); freeze 1/500 to 1/1000. Burst with AF-C. Shot from the trackside or from a bridge.
4. **Moment**: one pass of one train. Multiple carriages are part of one moment; a second train on the adjacent track is a separate subject.
5. **Metadata**: train number (headcode, UK), operator, class (e.g. LNER Azuma), location, direction of travel.
6. **Signals**: train as a long horizontal object; panning score; headcode/class from the vision model if legible.

---

## Part 2: Nature

### 2.1 Birds in flight and perched

1. **Keepers**: eye sharp (the single most important criterion); wing position interesting (upstroke, downstroke, dive, landing); bird's head oriented to the lens; background clean; feathers at the leading edge sharp.
2. **Rejects**: eye soft (even if the body is sharp); wings cropped at the frame edge; bird out of focus (back-focus); a wing in a blur at the tip (motion blur on wing tips is acceptable only if the bird is panned deliberately).
3. **Pattern**: sources give 1/1000 for perched or slow birds and 1/2000 to 1/3200 for birds in flight **(hawkwatch.org, instagram)**. Burst at 10+ fps; continuous AF; wide aperture. Panning at 1/30 to 1/125 is a creative option, but the eye must stay sharp.
4. **Moment**: one flight sequence (the bird leaves a perch, flies, lands). A burst of one take is one moment; a second bird in the same frame is a separate subject.
5. **Metadata**: species name (required for stock and for ID). Species ID from the vision model is a plausible target, with confidence gating. Location and date.
6. **Signals**: eye detection (small dark region with a bright specular at native resolution; the Conrod subject detector finds the bird; a secondary eye detector or a sharpness measure on the head region); wing-tip blur as a soft reject only when the eye is sharp (separate weighting); the bird's bounding-box area and its distance to the edge.

### 2.2 Wildlife and safari

1. **Keepers**: eye in focus; animal in an interesting action (mating, feeding, play, a stare); background isolated; the animal's body sharp at its leading edge; action with some tension.
2. **Rejects**: eye soft; animal cut at the frame edge; animal in the shadow of a tree with no detail; a hidden animal.
3. **Pattern**: sources recommend burst 5 to 10 fps, focus on the eye, and freezing action at 1/1000 or faster for mammals on the move **(passportandpixels.com, sabisands)**. Pan at 1/60 to 1/250 for a running animal.
4. **Moment**: one action sequence (a chase, a hunt, a drink). A still-standing animal over a long period is many moments; cull to the best 1 to 3 per behaviour.
5. **Metadata**: species name (critical), sex and age if known, location (park or reserve), date and time, camera distance.
6. **Signals**: eye sharpness as a separate score from body sharpness; subject detector (COCO has animals: bird, cat, dog, horse, sheep, cow, elephant, bear, zebra, giraffe); the species name from the vision model; burst groups split by inter-frame motion.

### 2.3 Macro and insects (focus stacking brackets)

1. **Keepers**: the subject's sharpest plane (eye of an insect, the front of a flower) sharp, with a merged focus stack that covers the depth; clean background; no motion blur from wind (macro is often wind-sensitive).
2. **Rejects**: a single frame where the sharp plane misses the important feature; a stacked frame with halos or ghosting (subject moved between frames); motion blur from wind on the insect; the focus stack with a missing frame.
3. **Pattern**: focus bracket at a fixed focus step; number of frames depends on magnification and aperture. Sources recommend around 18 frames for a small insect at high magnification **(wickeddarkphotography)**, but also suggest 2 to 3 frames at f/8 to f/9 for a wide lens **(zeiss lenspire)**. In-camera focus bracketing (focus shift) with 5 to 20 frames is common; a step size of 1 to 6 units is cited by Olympus-style menus. Shutter at 1/125 to 1/250 with flash, or on a tripod with a bulb-free speed.
4. **Moment**: **one focus stack = one moment** (all frames in the stack are kept together, the best is the merged output). Do not cull individual frames in a stack by sharpness alone; the frames are meaningful only as a group.
5. **Metadata**: species (if known), location, magnification, the focus stack flag.
6. **Signals**: focus-stack detection from EXIF: sequential frames with the same focal length, aperture, ISO, shutter speed, and a monotonic FocusDistance (or lens focus position) with a small step; or a file-name pattern from the camera. A stack is a set of frames; keep all or none. The stack's merged output (if present) is the keeper.

### 2.4 Underwater

1. **Keepers**: subject sharp with minimal backscatter (the white particles in water); strobe lighting that lights the subject's front; natural colour restored (red/orange at depth); subject close to the lens.
2. **Rejects**: backscatter (white dots); subject too far (colour loss, murkiness); flat, dull lighting; motion blur from current or from the diver's kick.
3. **Pattern**: strobes at the sides or slightly behind the housing; the shot is made close to the subject. Shutter at 1/125 to 1/250 with strobes; ISO as low as the strobes allow. Burst for fish and marine mammals.
4. **Moment**: one encounter with one subject. A school of fish is one moment; cull to the frames with the best backscatter-free area.
5. **Metadata**: species, dive site, depth, visibility conditions (if logged).
6. **Signals**: backscatter detector (small high-brightness isolated specks in a dark region; count and size); subject detection; colour cast (blue/green dominance) as a secondary reject signal; strobe-lit frames have bright foregrounds, so distinguish strobe glare from natural highlight.

### 2.5 Landscape (horizon, focus stacking, exposure brackets, long exposure)

1. **Keepers**: horizon level (within a degree); foreground and background sharp (single shot at a small aperture, or focus stack); clean sky and highlights; intentional long exposure (silky water, cloud streaks) that reads as deliberate.
2. **Rejects**: crooked horizon; blown sky; a shot where the foreground is soft because of the focus point; long-exposure water that looks like a camera shake (too slow for the water's flow or unsteady tripod).
3. **Pattern**: sources give waterfall shutter speeds of 1/60 to 1/4 s for texture, 1/4 to 5 s for classic silk, and longer for lake or sea **(venuslens, alphauniverse)**. Use an ND filter for daylight (3 to 10 stops). Exposure brackets (3 to 5 frames at 1 to 2 EV steps) for HDR or blending; focus stacks for depth at f/8 to f/11 (2 to 3 frames is often enough for a wide lens, per the Zeiss source).
4. **Moment**: one composition. An exposure bracket is one moment; a focus stack is one moment; a long-exposure set at different shutter speeds is a set of options (keep one of each speed that differs visibly).
5. **Metadata**: location (GPS), elevation, time, weather if available.
6. **Signals**:
   - Horizon angle: detect the dominant horizontal edge; tilt is a reject candidate if above ~1 degree.
   - Exposure bracket group: same focal length, aperture, ISO, focus distance; shutter speed differs by 1 to 2 EV steps in a regular pattern. Keep the group; blend candidate.
   - Long-exposure water: low-frequency smooth texture in the water region with long streaks; must not be penalised by the blur detector. Sharp stationary objects (rocks, trees) with smooth water is a keeper.
   - Long exposure with camera shake: stationary rock edges soft in both directions (not directional) vs a deliberate silk water where the rock is sharp.

### 2.6 Astro and night sky (star trails, pinpoint stars, noise)

1. **Keepers**: stars as points (pinpoint), not streaks, unless the frame is a deliberate star-trail stack; Milky Way core sharp; foreground lit and sharp; clean sky with controlled noise.
2. **Rejects**: elongated stars (trailing from too long a shutter speed) in a frame not intended as a trail; blurred Milky Way from tracking error; heavy colour noise; a hot pixel cluster.
3. **Pattern**:
   - 500 rule: shutter (seconds) ~ 500 / focal length (full-frame equivalent). The NPF rule refines this with pixel pitch and aperture; the 500 rule can overestimate safe time by 50 to 100% on modern high-resolution sensors **(photographylife, npfcalculator)**.
   - Star trails: a stack of many frames (e.g. 100 to 300 frames of 15 to 60 seconds with a gap of ~1 second), combined with a stacking tool; the trail frame set is a single moment.
   - Tracking mount: longer exposures allowed.
   - ISO typically 800 to 6400; bracket for noise reduction via dark-frame or averaging.
4. **Moment**: one exposure session; a star-trail stack is one moment (keep all frames). A timelapse of the night sky is a sequence (see 3.2).
5. **Metadata**: location (GPS), date and time (for the sky position), focal length, camera model (for pixel pitch and NPF calculation).
6. **Signals**:
   - Star shape: local elongation of point-like bright spots relative to the NPF-computed limit; a trailed star with a long-exposure intent is a separate "star-trail" flag.
   - Star trail stack: a sequence of frames with the same settings, identical framing, and consistent sky motion vector (arcs around the celestial pole).
   - Noise: high chroma noise is a reject only for a non-night-type shot; night shots need a noise threshold relative to ISO.
   - Hot pixels: small isolated bright specks on a dark background.

### 2.7 Sunrise and sunset brackets

1. **Keepers**: sun or the sky's glow held without clipping; foreground detail; a blended HDR-style result; the sunburst or the colour gradient; a silhouette where the foreground is intentionally black.
2. **Rejects**: blown sun disc with no detail in the sky; the foreground too dark with the sky correct (the bracket is incomplete); a bracket set with a missing frame.
3. **Pattern**: 3 to 5 frames at 1 to 2 EV steps; the darkest frame to hold the sun; the brightest for the foreground. Shooting on a tripod. Some sources note a dark exposure is needed at one end to keep the sun from blowing out **(dpreview)**.
4. **Moment**: **one bracket set = one moment**. Keep the set together; the blended output (if any) is the keeper.
5. **Metadata**: time of sunrise/sunset, location, weather.
6. **Signals**: bracket detection from EXIF (same focal length, aperture, ISO; shutter speeds in a regular EV progression with the same focus distance); a set should be kept together; sun clipping: count of pixels at 255 in the sun region; foreground black crush.

---

## Part 3: Other common genres

### 3.1 Travel

1. **Keepers**: a sharp subject (person, landmark, street scene) with a clear story; natural light; a recognisable place.
2. **Rejects**: blurred subject (handheld in low light); people in the frame with closed eyes; overexposed sky with the landmark dark; duplicate frames.
3. **Pattern**: no fixed; general street advice is 1/125 or faster for handheld; aperture f/5.6 to f/11 for landmarks.
4. **Moment**: one scene/location visit; keep one or two per composition.
5. **Metadata**: GPS location, landmark name (vision model), date.
6. **Signals**: landmark recognition (vision model), GPS clustering, face sharpness for people shots.

### 3.2 Timelapse sequences

1. **Keepers**: consistent exposure across the sequence (no flicker); clean framing (no camera shift); the sequence's first and last frames are meaningful.
2. **Rejects**: frames with a different exposure from the neighbours (flicker); frames with a camera bump (shift); missed intervals (gaps); a frame where a person or vehicle blocks the subject.
3. **Pattern**: manual exposure, fixed aperture, fixed shutter speed long enough to average light flicker (sources cite around 1/120 for mains-flicker light) **(uniquephoto, 153957)**; an intervalometer with a fixed interval (e.g. 2 to 10 s); adjust exposure in small steps for day-to-night ramps (ramping), or use a dedicated exposure-ramp feature.
4. **Moment**: **one timelapse sequence = one moment**. Never cull frames individually from a sequence without re-assessing the whole series; a missing frame breaks the sequence.
5. **Metadata**: interval, start and end time, location, total frames.
6. **Signals**: sequence detection from EXIF (same focal length, aperture, ISO, shutter speed, regular DateTimeOriginal spacing); flicker: per-frame mean luminance deviation from neighbours (a spike is a reject); camera shift: per-frame global translation (phase correlation) above a threshold.

### 3.3 Culling-software conventions (for consistency)

- Adobe Lightroom Classic's Assisted Culling groups photos into stacks and marks each as Pick, Unflagged, or Reject, with optional star rating and colour label **(helpx.adobe.com)**. A useful default mapping for Conrod: Pick = keeper, Reject = reject, Unflagged = needs human review.
- Two-pass culling is common in photographer practice: first pass, flag keepers; second pass, rate the strongest keepers **(reddit, photographers)**. Conrod's keeper picking could mirror this with a two-level output (keeper, best-of-moment).

---

## Cross-genre summary

| Genre | Keeper core | Typical rejects | Shutter pattern | Moment unit | Key IDs | Measurable signal |
|---|---|---|---|---|---|---|
| Circuit racing | Car sharp, pan streak, driver | Soft car, cropped nose | Pan 1/60–1/250; freeze 1/1000 | One corner pass | Race no., livery | Subject sharp + bg streak in motion axis |
| Rally | Car with dust, airborne | Dust lost car, frozen flight | 1/250–1/500 freeze; 1/60–1/125 pan | One feature (jump, hairpin) | Car no., stage | Dust texture, airborne |
| Drifting | Yaw angle, smoke | No angle, smoke cut | Pan 1/125–1/200; freeze 1/640–1/800 | One corner | Car no., driver | Smoke volume, yaw (hard) |
| Drag | Launch, flames, wheelstand | Past-launch, frozen launch | 1/160–1/250 pan | One run | Class, lane, run | Flame luminance, wheelstand (hard) |
| Burnout | Smoke, sharp car | Smoke hides car | Pan 1/125–1/250 | One burnout | Car no. | Smoke region |
| Motocross | Airtime, whip | Ambiguous lip, rider cropped | Freeze 1/1000+; pan 1/60–1/250 | One jump | Rider no. | Airborne bike (hard) |
| Speedway | Lean, roost, night | Roost hides rider | 1/250–1/500; pan 1/60–1/160 | One corner | Rider no., heat | Roost texture |
| Karting | Kart sharp, helmet | Kart cropped | Pan 1/125–1/250 | One corner | Kart no. | Pack detection |
| Motorcycle | Knee down, lean, head-on | Rider away, bike cropped | Pan 1/125–1/160; freeze 1/1000 | One corner/sequence | Rider no. | Head sharpness, lean (hard) |
| Car meet / detail | Badge/wheel sharp | Wrong layer soft | f/5.6–8; rolling 1/60–1/125 | One subject / pass | Make, plate | Focus-stack detection |
| Car-to-car | Both cars sharp | Drift out of frame | 1/60–1/125 | One pass | Both car nos. | Two-subject pan |
| Aviation | Prop blur (blurred disc), jet frozen | Frozen prop (for prop aircraft) | Prop 1/125–1/250; jet 1/1000–1/2000; runway pan 1/40 | One pass | **Tail/registration**, type | Disc blur test; EXIF shutter |
| Trains | Sharp train, streak bg | Soft train, cropped | Pan 1/30–1/60; freeze 1/500–1/1000 | One pass | Headcode, class | Panning score |
| Birds | Eye sharp, wing position | Eye soft, cropped | Perched 1/1000; flight 1/2000–1/3200 | One flight sequence | Species | Eye sharpness |
| Wildlife | Eye sharp, action | Eye soft, cropped | Freeze 1/1000+; pan 1/60–1/250 | One behaviour | Species | Eye sharp, species |
| Macro/focus stack | Stacked output sharp | Halos, wind motion | Focus bracket | **One stack** | Species | Focus-distance sequence |
| Underwater | Subject sharp, no backscatter | Backscatter, murky | 1/125–1/250 with strobe | One encounter | Species, site | Backscatter specks |
| Landscape | Level, sharp, intentional long | Crooked, blown sky | Silk 1/4–5 s; ND for day | One composition/bracket | GPS | Horizon angle, bracket group |
| Astro | Pinpoint stars, Milky Way | Trailed stars, noise | NPF/500 rule; trail stack | One session / stack | GPS, time | Star elongation vs NPF |
| Sunrise/sunset | Sun held, foreground detail | Blown sun, missing bracket | 3–5 frame brackets | **One bracket set** | Time, location | Bracket detection |
| Travel | Story subject sharp | Blur, closed eyes | 1/125+ handheld | One scene | GPS, landmark | Face sharpness |
| Timelapse | Consistent exposure | Flicker, shift, gaps | Manual, fixed; ~1/120 | **One sequence** | Interval, times | Flicker, shift, sequence EXIF |

---

## Implementation notes for Conrod

1. **Blurred-on-purpose allow-list**: prop disc (aviation), water long exposure (landscape), star trails (astro), panning (motorsport, trains, wildlife), roost and smoke. The blur detector must read the intent from the preset, not from the raw blur score.
2. **Stacks and brackets**: detect focus stacks (monotonic focus distance, same settings), exposure brackets (EV progression), timelapses (regular interval, same settings), and star-trail stacks. Keep the whole group together; never cull a single member.
3. **Airborne**: airborne bikes (motocross) and airborne rally cars are hard. Start with a manual flag from the user or a simple geometric heuristic (wheel-bottom gap to the ground line), and treat the result as advisory.
4. **ID**: race numbers, rider numbers, tail registrations, species names, and train headcodes are the main ID keys. Feed them into a moment-level metadata record so the grouping is by (event, subject ID, moment).
5. **Moment definition**: one moment = frames of the same subject in the same framing and the same action window (corner pass, jump, flight, behaviour, bracket, stack, sequence). Keep variety (wide and tight, pan and freeze) within a moment's best picks.

---

## Source URLs

- https://www.motorsportphotographer.com/how-to-photograph-top-fuel-drag-racing/
- https://www.motorsportphotographer.com/how-to-photograph-motorcycle-racing/
- https://www.exposureguide.com/motor-racing-photography-tips/
- https://racetagger.cloud/blog/NEW-2-Panning-Photography-Masterclass
- https://www.streetmachine.com.au/features/diy/car-photography-tips-how-to-take-better-photos-of-cars
- https://www.shootsmart.co.uk/how-to-photograph-aircraft/
- https://tamron-americas.com/blog/airshow-photography-tips-settings-lenses/
- https://www.instagram.com/reel/DfkiPVCNppI/ (aviation shutter reel)
- https://professionalaviationphotography.com/propeller-blur-shutter-speed/ (blog.professionalaviationphotography.com)
- https://airshowstuff.com/forum/viewtopic.php?t=99
- https://www.dpreview.com/forums/threads/would-you-reject-these-challenge-photos.4589753/
- https://digital-photography-school.com/tips-for-photographing-birds-in-flight/
- https://hawkwatch.org/how-to-photograph-birds-tips-for-capturing-stunning-shots/
- https://www.passportandpixels.com/safari-photography-tips-wildlife-photos/
- https://sabisandsgamereserve.com/tips-for-enhancing-your-sabi-sands-wildlife-photography/
- https://wickeddarkphotography.com/2021/05/23/stacking-for-macro-part-1-capture/
- https://lenspire.zeiss.com/photo/en/article/depth-of-field-challenges-bypass-the-limits-with-focus-stacking-near-or-far-macro-or-landscape/
- https://www.backscatter.com/reviews/post/How-To-Underwater-Strobe-Flash-Placement
- https://www.waterdogphotographyblog.com/backscatter-prevention/
- https://www.traintrackapp.de/en/blog/shutter-speed-train-photography-panning/
- https://andrewshenton.co.uk/panning-railway-photography/
- https://alphauniverse.com/stories/tips-and-techniques-long-exposure-how-to-for-moving-water/
- https://www.venuslens.net/waterfall-long-exposure-photography/
- https://photographylife.com/500-rule-vs-npf-rule
- https://npfcalculator.com/
- https://www.uniquephoto.com/community/qa/how-can-i-reduce-minor-exposure-flicker-in-a-time-lapse-sequence
- https://arne.delaat.net/timelapse_guide.html
- https://helpx.adobe.com/lightroom-classic/desktop/organize-photos-in-lightroom-classic/assisted-culling.html
- https://www.dpreview.com/forums/threads/strategy-for-sunrise-sunset-exposures.4673271/
- https://www.reddit.com/r/AskPhotography/comments/1lxl5la/how_to_get_sharp_photos_of_flying_birds/
- https://www.reddit.com/r/Planespotting/comments/1wfpix2/do_you_guys_like_blurred_or_frozen_props_in_your/
- https://www.facebook.com/groups/motorsportsphoto/posts/27556041664017886/ (motorcycle pan, forum)
- https://www.talkphotography.co.uk/threads/shutter-speed-for-motorcycle-racing.398941/
