# Shoot types: what Conrod does for each

Generated from the policy table in `crates/conrod-core/src/profile.rs` (`ShootPreset::policy`) and the moment vocabularies in `crates/conrod-core/src/moments.rs`. The summary the app shows when you pick a type comes from the same table (`preset_policy`), so this page cannot drift from it: `presets_match_the_frontend_copy` fails when the table changes.

Every detector and reader below is a per-album switch in **both** directions (import sheet, album *Shoot settings*). A switch sets an override in the album's `overrides`; clearing it follows the type again. A reader needs its detector: numbers, plates, make/model and livery need *cars & bikes*; names need *faces & eyes*; *let moments lift ranking* needs *spot moments*.

## The table

| Type | Looks for | Reads | Moments | Other | Why |
|---|---|---|---|---|---|
| **Motorsport** (`motorsport`) | cars & bikes, people | race numbers, make & model, sponsors & livery | tyre smoke, flames, sparks, kerb strike, wheel lift, close racing, contact, crash or spin, chequered flag, podium | pan forgiven, blur critical | Circuit racing: the car decides the frame. Numbers and livery identify it; race cars carry no plates; drivers sit behind helmets, so faces are off until you ask (`include_people`). |
| **Burnouts** (`motorsport-burnouts`) | cars & bikes, people, faces & eyes | race numbers, number plates, make & model | smoke cloud, flames, tyre burst, sparks, crowd reaction | pan forgiven, blur lenient | Burnout comps run numbered street cars, so numbers and plates both; the driver, the crowd and the smoke are part of the shot, so people and faces stay on. |
| **Track days** (`motorsport-track`) | cars & bikes, people, faces & eyes | race numbers, number plates, make & model, sponsors & livery, names (from faces) | tyre smoke, sparks, kerb strike, wheel lift, close racing, contact, crash or spin, pit lane work | pan forgiven, blur critical | Track days are road cars with taped numbers, so plates and numbers. Paddock, pit lane, crew, owners and kids at the fence are normal, so people and faces are on; the car still decides action frames. |
| **Rallies** (`motorsport-rally`) | cars & bikes, people, faces & eyes | race numbers, number plates, make & model, sponsors & livery, names (from faces) | airborne, dust, water splash, sideways, wheel lift, crash or spin, crowd reaction | pan forgiven, blur lenient | Door numbers and plated cars; crews, service park and spectators are common, so people and faces on. |
| **Car meets** (`motorsport-meet`) | cars & bikes, people, faces & eyes | number plates, make & model | owner with car, badge detail, engine bay, interior, wheel detail | no pan forgiveness, blur moderate | Meets and shows are parked cars with owners: plates and make/model, never race numbers, no sponsor-livery reading; owners and detail shots keep people on. |
| **Cycling** (`sport-cycling`) | bicycles, people | race numbers | airborne, sprint finish, crash or spin, peloton, climbing effort | pan forgiven, blur moderate | The bicycle (COCO class 1) is the subject, never a car. Bib numbers are read; helmets hide faces. |
| **Action sports** (`sport-action`) | bikes, people | race numbers | airborne, trick at its peak, spray or dust, crash or spin, grab | pan forgiven, blur moderate | Bikes (bicycle and motorcycle classes) and riders; numbers on bibs and boards; faces off (helmets, goggles). |
| **Dealership stock** (`auto-dealer`) | cars & bikes | number plates, make & model | none | no pan forgiveness, blur moderate | Stock photography: one car per angle, no people. Plate and make/model identify the stock; no numbers, no livery. |
| **Aviation** (`aviation`) | aircraft | registrations, aircraft type, operator & livery | vapour, afterburner, formation, gear down, landing flare, banked hard | pan forgiven, blur moderate | Aircraft only (COCO airplane). Registration rides the plate lane, type the make/model lane, operator the livery lane; no people, no race numbers. |
| **Portraits** (`portrait`) | people, faces & eyes, animals | nothing | genuine smile, laughter, eye contact, candid moment | no pan forgiveness, blur strict | People and faces; eyes decide the frame. Animals stay on (a dog in the shot); no vehicles, no readers. |
| **Group photos** (`portrait-group`) | people, faces & eyes, animals | nothing | everyone looking, genuine smile, laughter | no pan forgiveness, blur strict | As portraits, with a smaller minimum box so every person in a group counts. |
| **Couple photos** (`portrait-couple`) | people, faces & eyes, animals | nothing | embrace, laughing together, eye contact, kiss | no pan forgiveness, blur strict | As portraits. |
| **Pets** (`portrait-pets`) | people, faces & eyes, animals | nothing | eye contact, playful action, happy face | no pan forgiveness, blur strict | As portraits; the animal is the subject. |
| **Events** (`event`) | people, faces & eyes, animals | names (from faces) | speaker gesture, crowd reaction, presentation, laughter | pan forgiven, blur moderate | General events: people first, animals where they turn up, names from faces. Cars off by default; switch on for a car-heavy event. |
| **Shows** (`event-shows`) | people, faces & eyes, animals | names (from faces) | performer at peak, crowd reaction, animal in motion, animal portrait, prize moment | pan forgiven, blur lenient, dark ok | Agricultural and stage shows: animals, riders, performers, crowd. Cars are off by default (a show is not a car meet); dark frames are fine on stage. |
| **Parties** (`event-parties`) | people, faces & eyes | names (from faces) | laughter, toast, dancing, hug, candid moment, cake and candles | no pan forgiveness, blur moderate | People and faces, names. No vehicles, no animals by default. |
| **Weddings** (`wedding`) | people, faces & eyes | names (from faces) | first kiss, ring exchange, vows, first dance, confetti, tears or laughter, bouquet toss, walk down the aisle | no pan forgiveness, blur moderate | Couple, guests, expressions: people and faces, names; no vehicles. |
| **Concerts** (`concert`) | people, faces & eyes | names (from faces) | jump, crowd surfing, pyro, guitar raised, singer and crowd | pan forgiven, blur lenient, dark ok | Performers in blackout and stage light: people and faces, dark frames fine, lenient blur. |
| **Field sports** (`sport-field`) | people | nothing | ball contact, goal or try, celebration, tackle, peak of a jump | no pan forgiveness, blur moderate | Players and the ball; faces off at field distance; numbers are not read (jersey numbers on a moving player are unreliable); names off. |
| **Court sports** (`sport-court`) | people, faces & eyes | names (from faces) | shot release, dunk or spike, celebration, block, peak of a jump | pan forgiven, blur strict | Players and the ball, closer in: faces on; names on. |
| **Rodeo** (`sport-rodeo`) | people, faces & eyes, animals | names (from faces) | rider airborne, bucking peak, dismount or fall, rodeo clown | no pan forgiveness, blur moderate | Rider and animal; no numbers; names off faces. |
| **Equestrian** (`sport-equestrian`) | people, faces & eyes, animals | names (from faces) | jump apex, all hooves off ground, landing, rider focus | pan forgiven, blur moderate | Horse and rider; pans forgiven; no numbers (saddle cloths are a gap). |
| **Mixed** (`mix`) | cars & bikes, people, faces & eyes, animals | race numbers, number plates, make & model, sponsors & livery, names (from faces) | airborne, tyre smoke, flames, laughter, celebration, crowd reaction | pan forgiven, blur moderate | Unknown shoot: look for everything, read everything. |
| **Property listing** (`property-listing`) | nothing (whole frame) | nothing | none | no pan forgiveness, blur strict, level check | Rooms: nothing to detect or read; level check, blur strict. |
| **Condition report** (`property-report`) | nothing (whole frame) | nothing | none | no pan forgiveness, blur strict, level check, keeps most | Condition report: as listing; keeps nearly everything. |
| **Birds** (`wildlife-birds`) | animals | nothing | in flight, catch, eye contact, wings at apex, interaction | pan forgiven, blur moderate | Animals only; no people, no readers. |
| **Landscape** (`landscape`) | nothing (whole frame) | nothing | golden hour, dramatic light | no pan forgiveness, blur strict, level check, keeps most | Nothing to detect; level check; keeps nearly everything. |
| **Astro** (`astro`) | nothing (whole frame) | nothing | milky way core, meteor, aurora | no pan forgiveness, blur lenient, dark ok | Nothing to detect; dark frames are the subject; lenient blur. |

Cars are an option on every type but on by default only where vehicles are the subject or common: motorsport (all), meets, cycling and action sports (their own bike classes), dealerships, aviation (aircraft), mixed. They are off for shows, events, parties, portraits, weddings, concerts, field/court sport, rodeo, equestrian, wildlife, property, landscape and astro.

## What changed

| Type | Race numbers | Plates | Cars by default |
|---|---|---|---|
| Burnouts | same | - -> P | same |
| Track days | same | - -> P | same |
| Rallies | same | - -> P | same |
| Cycling | same | same | - -> V |
| Action sports | - -> N | same | same |
| Events | same | same | V -> - |
| Shows | same | same | V -> - |
| Field sports | N -> - | same | same |
| Court sports | N -> - | same | same |
| Mixed | - -> N | - -> P | same |

`N` numbers on, `P` plates on, `V` vehicles on, `-` off. Other changes: *sport-field* and *sport-court* no longer read numbers (jersey numbers on moving players are unreliable) and read names only on court; *aviation* reads registration, type and operator but no race numbers; track days, rallies and burnouts look at people and faces; the model is never asked about a car when the crop is an aircraft or a bicycle.

## Sources

Background reading for what photographers shoot at each event; the tag sets and defaults are our synthesis of these and of the photographer's own practice, not a quotation. The links resolved when checked.

- **Motorsport**: https://digital-photography-school.com/3-tips-taking-better-motorsport-photos/; https://en.wikipedia.org/wiki/Track_day
- **Burnouts**: https://digital-photography-school.com/7-tips-taking-better-photographs-cars/; https://digital-photography-school.com/take-better-action-photos/
- **Track days**: https://en.wikipedia.org/wiki/Track_day; https://digital-photography-school.com/3-tips-taking-better-motorsport-photos/
- **Rallies**: https://en.wikipedia.org/wiki/Rallying; https://digital-photography-school.com/take-better-action-photos/
- **Car meets**: https://digital-photography-school.com/7-tips-taking-better-photographs-cars/
- **Cycling**: https://digital-photography-school.com/top-5-tips-for-extreme-sports-photography/; https://digital-photography-school.com/take-better-action-photos/
- **Action sports**: https://digital-photography-school.com/top-5-tips-for-extreme-sports-photography/
- **Dealership stock**: https://digital-photography-school.com/7-tips-taking-better-photographs-cars/
- **Aviation**: https://digital-photography-school.com/air-show-photography-a-guide-for-beginners/; https://digital-photography-school.com/seven-tips-aviation-photography/
- **Portraits**: https://digital-photography-school.com/11-tips-for-better-candid-photography/
- **Group photos**: https://digital-photography-school.com/11-tips-for-better-candid-photography/
- **Couple photos**: https://digital-photography-school.com/11-tips-for-better-candid-photography/
- **Pets**: https://digital-photography-school.com/6-tips-for-photographing-dogs-in-action/
- **Events**: https://digital-photography-school.com/tips-event-photography/; https://digital-photography-school.com/event-photography-etiquette-avoid-embarrassment-10-tips/
- **Shows**: https://digital-photography-school.com/6-tips-on-how-to-do-horse-show-photography-for-parents/; https://digital-photography-school.com/tips-event-photography/
- **Parties**: https://digital-photography-school.com/tips-event-photography/; https://digital-photography-school.com/12-tips-better-nighttime-event-photography/
- **Weddings**: https://digital-photography-school.com/wedding-photography-21-tips-for-for-amateur-wedding-photographers/; https://digital-photography-school.com/5-tips-for-doing-candid-wedding-photography/
- **Concerts**: https://digital-photography-school.com/how-to-take-better-concert-photos/
- **Field sports**: https://digital-photography-school.com/take-better-action-photos/
- **Court sports**: https://digital-photography-school.com/indoor-sports-photography/
- **Rodeo**: https://en.wikipedia.org/wiki/Rodeo
- **Equestrian**: https://en.wikipedia.org/wiki/Show_jumping; https://digital-photography-school.com/6-tips-on-how-to-do-horse-show-photography-for-parents/
- **Property listing**: https://digital-photography-school.com/real-estate-photography-a-guide-to-getting-started/
- **Condition report**: https://digital-photography-school.com/real-estate-photography-a-guide-to-getting-started/
- **Birds**: https://digital-photography-school.com/tips-for-photographing-birds-in-flight/; https://digital-photography-school.com/10-tips-for-improving-your-wildlife-photography/
- **Landscape**: https://digital-photography-school.com/11-surefire-tips-for-improving-your-landscape-photography/
- **Astro**: https://digital-photography-school.com/how-to-do-milky-way-photography-a-comprehensive-tutorial/

## Judging moments

A *moment* is what makes a keeper at that kind of shoot: a first kiss, tyre smoke, a jump apex. Each type has a short vocabulary (6-12 tags, in `moments.rs` as data with a plain question and a weight per tag). A judge reads each keeper frame and says which moments are clearly visible and how sure it is.

- **Two switches per album**: *Spot moments* (`moments_detect`: ask and show tags) and *Let moments lift ranking* (`moments_rank`). Either can be off; ranking needs spotting. Both default on where the type has a vocabulary; property reports, listings and dealership stock have none.
- **Threshold** (`moments_threshold`, default **0.85**, album setting): a moment at or above it is *sure* and may lift a frame; from 0.5 up to it, it shows as **Maybe** and never changes ranking. The probability is stored per tag.
- **Ranking**: a sure moment (and a burst's peak pick) only breaks ties inside the same sharpness band, among frames that clear the sharpness bar and that no rule culled. It never rescues an unsharp or culled frame and never demotes a sharp one: with ranking off the order is exactly what it was. The inspector says why (`Lifted: first kiss`).
- **Backends** (`moments_backend`, Settings > Identification > Moments):
  - `vlm` (default): the vision model already used to identify vehicles, now asked for the type's tags only when clearly visible. It gives no probability: a model that says it is confident (>= 0.7) yields p = 0.9, one that is unsure p = 0.6 (a maybe). Readings are marked *unverified*. With the default threshold of 0.85 a confident vision-model tag counts.
  - `decider`: Perplexity's decision model (model card https://huggingface.co/perplexity-ai/pplx-decider-flash-v1.2), which returns calibrated probabilities. One request per frame: one `noul` question per tag (the tag's plain question) plus one `score` question for overall strength (levels: no moment, ordinary, good, peak) whose expected level scales the lift. Calibration is on the `noul` P(true) against the threshold. For a burst, one `choice` request over up to 8 downscaled frames asks which shows the peak; a clear winner (>= 0.5 and ahead of the rest) gets the tie-break.
  - Transports (same JSON, `DecisionTransport` in `conrod-core/src/decision.rs`): `openrouter` (`POST https://openrouter.ai/api/alpha/decisions`, model `perplexity/pplx-decider-flash-v1.3:free` by default, images sent as `image_url` parts inside `state`, because OpenRouter rejects a top-level `images` field; key in `moments_openrouter_key`) and `in_process` (the in-process model's seat, `InProcess::with(..)`; fails plainly until the decision-model crate fills it).
- **Failures never block**: a missing key, a timeout or a broken reply leaves the frame unjudged; nothing is demoted and the scan does not wait.

## Gaps: what Conrod cannot judge or read today

Not built; suggested approach in brackets.

- Helmet-obscured faces and eyes (track, rally, cycling, action): faces are skipped or low-confidence. [Judge the helmet/livery or the car instead; add a rider-pose model.]
- Motorcycle rider numbers on bibs versus fairings versus helmets: one `number` field. [Ask for `number_location` by kind and keep the fairing number as the identity.]
- Horse numbers and saddle cloths (equestrian, rodeo, field days): not read. [A saddle-cloth crop from the horse box into the OCR lane.]
- Livery text versus sponsors: both come from `visible_text`; the split is a guess. [Ask for sponsors and team separately.]
- Aircraft registrations versus tail numbers versus display numbers: all ride the plate lane with plate-shaped OCR. [An aircraft-registration pattern set per country.]
- Night and low light: dark frames are tolerated but the detector and OCR lose small subjects. [Exposure-lifted second pass for dark frames.]
- Drone shots: top-down vehicles and people look unlike COCO training data. [A top-down detector, or a lower threshold per album.]
- Pit boards and signage: read as incidental text. [A signage region type.]
- Animals beyond COCO's ten classes (kangaroo, emu, cattle breeds, poultry): not detected, so shows and wildlife rely on the whole-frame fallback. [A wildlife detector, or the vision model as a region proposer.]
- Moments the vision model cannot judge reliably: tears, genuine versus posed smiles, 'peak' timing, close racing (needs several vehicles), crowd reaction. [Use the decision model and, for timing, burst-peak detection: motion between consecutive frames plus the peak choice.]
- Burst peak with the vision model: it cannot compare frames, so only the decision backend picks a peak. [Frame-difference energy as a free peak signal.]
- Moment probabilities are not calibrated for the vision backend: p = 0.9 is a stand-in. [Calibrate on a labelled burst set.]
- Facet counts use the default threshold (0.85), not an album's own. [Carry the album threshold into the review payload.]
