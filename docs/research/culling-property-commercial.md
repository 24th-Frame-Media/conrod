# Culling research: property, automotive and commercial genres

Scope: what a keeper looks like, typical rejects, shooting pattern, grouping, metadata, and signals Conrod could measure, per genre. Sources are listed at the end. Vendor blogs and forum posts are weaker evidence than MLS/FAA/CFR text; these are marked where it matters. Numeric thresholds in section 6 of each genre are proposals, not sourced values, and need tuning on real Conrod output.

Cross-cutting facts that affect the whole app:

- Bracket count disagrees across sources: 3 frames at ±2 EV is the common baseline; 5 frames (±2 to ±4 EV) is recommended for extreme contrast. Lightroom's HDR merge requires every bracket set in a multi-set merge to have identical EV offsets (Adobe), so Conrod should record the offset pattern per set, not assume one.
- Auto Align matters for handheld brackets; tripod sets usually merge with it off.
- MLS minimums vary by board (640x480 to 2048x1536 long edge). Do not hardcode one size; the spec is the board's, not ours.

---

## 1. Real-estate listing photography (interiors and exteriors)

**1. Keeper**
- Vertical edges straight (walls, door frames, cabinet edges). Camera held level, not tilted up or down.
- Landscape orientation; the first photo is the front exterior (MLS rule, e.g. CRMLS/NWMLS practice).
- Windows show exterior detail, not blown white; room interior not crushed. Lights on in every room (interior lights on for twilight).
- Full room coverage, with the shot list complete (see §4).
- No people, pets, personal items, licence or agent contact text, logos or watermarks (Stellar MLS, NWMLS, Bright, Airbnb-style rules).

**2. Rejects**
- Converging verticals or leaning walls, especially on wide lenses.
- Photographer or tripod in a mirror or window reflection; flash hot spots on glossy surfaces.
- Clutter, cables, bins, personal photos, unmade beds.
- Ghosting (double edges) from a bracket set where something moved between frames, or a tree or curtain moving.
- Wrong-exposure frame in a set (a bracket frame that will not be merged); duplicate framing of the same view.
- Heavy edits that misrepresent the property (Stellar MLS rule: photos must show all features truly).

**3. Shooting pattern**
- Tripod, at roughly chest or waist height, level, same height for every room.
- Bracket 3 frames at ±2 EV (or 5 at ±2 EV steps for mixed windows and dark corners). Set-up is ISO 100, aperture f/7.1–f/8, shutter-priority or manual.
- Flambient (flash + ambient blend) is the current premium default for interiors: one ambient set for windows, one flash frame, blended. Shoot one flash exposure per composition as well as brackets.
- Twilight: start about 15–20 min after sunset, bracket 3–5 frames per composition, all interior lights on, f/8–f/11, 10–30 s exposures; keep shooting through blue hour.
- Typical delivery: 25–40 final images per listing (agent sites show 20-35 is the common best-practice band). Deadline is usually same-day or next-day for MLS.

**4. Grouping and selection**
- A bracket set is one composition: group frames with the same framing, same focal length, same few-second time window, and different ExposureTime. Keep all frames of the set together until the merge is done; do not cull a set down to one frame before merging, because the merge needs them.
- After merge, keep one final per composition and per room; a room keeper count of 1–3 is typical.
- Shot-list completeness: each room, the front exterior, back exterior, every bathroom, kitchen, and any listed amenity must have at least one keeper. Flag missing rooms rather than silently dropping them.
- Reject duplicate compositions even if both are technically fine; keep the cleaner one.

**5. Metadata**
- Room label (kitchen, bath, bed 2, etc.), property address, ordering (exterior first, then living, kitchen, bedrooms, bathrooms, outdoor), and captions where the portal allows them.
- Export: JPEG, sRGB, long edge 2048 (safe for all published MLS limits), under 3–10 MB depending on board.
- Track whether a photo was digitally altered (object removal); some MLSs require disclosure.

**6. Measurable signals**
- EXIF `ExposureBiasValue` (EV offset) and `ExposureTime` to form bracket sets; sets with identical focal length and f-number and a gap under ~5–10 s. Confirm with feature matching (ORB/SIFT) so two rooms shot back-to-back are not merged.
- Vertical tilt: detect near-vertical lines (Hough, angle within ±15° of vertical), take median deviation. Flag if median deviation over about 1°. Also flag converging verticals (left and right line angles disagree in sign).
- Highlight clipping: percent of pixels with max channel ≥ 254 inside window regions of the merged or reference frame; flag over ~2–3%.
- Shadow crush: percent of pixels with luma < 5 in room regions.
- Room/scene classification (kitchen, bath, bedroom, exterior, twilight by sun-altitude from GPS+timestamp).
- Presence of people (face or body detection) and text/logos (OCR) in frame, to flag MLS rule violations.

---

## 2. Property condition reports / inspection photography

This is documentary work. The client wants full coverage, not selection. The photographer's real output is often every frame, with captions, and is delivered almost unfiltered. Keep this genre's culling rules different: the goal is coverage and evidence, not beauty.

**1. Keeper**
- Every room and every item on the template has at least one photo; a wide shot from the doorway plus close-ups of defects.
- Defect frames are sharp, well-lit, and show both the defect and where it is (a wide frame plus a close-up).
- Date and time stamp present (EXIF and/or burned-in metadata).

**2. Rejects**
- Blurred or dark frames of a defect (a defect photo that cannot be read is worse than no photo; retake).
- Duplicates of the same defect from the same angle, unless the second one adds scale or context.
- Photographer or reflection in a mirror or glass.

**3. Shooting pattern**
- Consistent path: enter, then move clockwise through each room, wide shot first, then items, then floor (common practice from condition-report guides).
- A minimum of about 4 photos per room; a 3-bedroom house often produces 60–100 photos. Defect photos are often shot twice: wide and close.
- Use a scale reference (ruler, coin) for defects where size matters.
- Keep the same camera height and lens across the job so items compare cleanly.

**4. Grouping and selection**
- Do not cull on aesthetics. Group by room and by item in the template, and keep all frames that document the item.
- Within an item, keep at least one wide + one close-up. Drop near-identical duplicates only.
- Completeness checks: every template item present; flag any with zero photos.

**5. Metadata**
- Room, item, condition code, and a caption describing the defect. Timestamp is mandatory and must be preserved, never stripped.
- Keep originals unaltered; any crop or exposure change must be a separate derived file.

**6. Measurable signals**
- Template completeness: map each photo to room and item (VLM or manual tag) and count coverage per room.
- Sharpness on the defect region (not whole frame): Laplacian variance inside a user-marked or detected region; flag below a threshold.
- Duplicate detection: perceptual hash or feature matching within the same room and item; keep the one with best sharpness and a close-up.
- Scale-reference presence (detect a ruler or coin; optional).
- Timestamp and GPS present in every frame; flag frames missing them.

---

## 3. Architectural and interior design photography

**1. Keeper**
- Strong verticals, straight horizon, a clean line of the architecture, and the building as the subject, not the photographer.
- Mood-true light: the photo has a clear light source (window, lamp) and is not flattened by flash.
- Editorial clarity: deliberate framing, minimal stray objects, consistent colour across a series.

**2. Rejects**
- Tilt and converging verticals, even if mild; distortion from ultra-wide lenses without correction.
- Visible tripod, cables, or stray clutter in frame.
- Inconsistent white balance across a series of the same space.

**3. Shooting pattern**
- Tripod, long-ish lens (24–50 mm equivalent), shot at a level height.
- Brackets (3–5) for high-contrast spaces; flash-ambient blend where the interior is artificially lit.
- Delivery is curated: often 5–15 final images per project, plus wide and detail sets.

**4. Grouping and selection**
- Group a space's frames by composition; each space keeps a hero plus 1–3 supporting frames (detail, alternate angle).
- Keep brackets together for merge, as in real estate.
- Shot completeness per space: wide, detail, and a view from the other corner.

**5. Metadata**
- Project name, space, architect or designer credit, and captions per image where a magazine or site needs them.
- Colour profile consistent (sRGB or the client's spec).

**6. Measurable signals**
- Vertical tilt estimate (as in §1).
- Colour-consistency across a series (mean RGB and white-balance drift per space).
- Scene classification (room type), and hero-candidate score combining sharpness, verticals, and clipping.

---

## 4. Airbnb / hospitality

**1. Keeper**
- Vertical or landscape per platform rules (Airbnb guidance: at least 800x1200, oriented vertically for its own listing grid, sharp, well-lit, natural colour, no text or logos, single image not collage, no screenshots or stock).
- Cover (hero) image with a strong wow factor; every amenity the listing advertises visible and working (hot tub with jets on and cover off; sofa and pull-out bed each in its own photo; fire lit if the listing claims it).
- Bright, true-to-life. Avoid heavy filters.

**2. Rejects**
- Photos that promise something the guest will not get (a feature that is covered, dark, or not visible).
- Dark rooms with no light; cluttered kitchens; photographed personal items.
- Heavy HDR halos, oversaturated or greenish cast from mixed lighting.

**3. Shooting pattern**
- Shot list per amenity and room; flambient for properties with views (interiors without washed-out windows).
- Brackets for interiors and twilight for exteriors when the view matters.
- Typically 30–60 final photos for a listing; platforms allow up to 100, but quality over quantity.

**4. Grouping and selection**
- One keeper per amenity/angle; keep the cover candidate set separate and rank it.
- Shot-list completeness: every amenity named in the listing text has a photo.

**5. Metadata**
- Room label and a short caption (Airbnb guidance: under 140 characters and only what is not visually obvious).
- Ordering: cover first, then exterior and arrival, then living, kitchen, bedrooms, bathrooms, amenities, neighbourhood.

**6. Measurable signals**
- Amenity presence detection (hot tub, fireplace, pool) via VLM or detector trained on labels; compare with the listing's amenity list if one is provided.
- Lighting-quality score: mean luma and clipping as in §1.
- Duplicate-angle detection between amenity shots.
- Cover candidate score: sharpness, colour, subject prominence.

---

## 5. Drone / aerial real estate

**1. Keeper**
- Clear, level, well-exposed aerial with the full property in frame; the sky not blown out; the property not in shadow.
- Clean composition: roof and boundary lines are straight; the horizon is level in a level-pitched shot.
- Sun angle that flatters the property (golden hour or overcast); no lens flare across the roofline.

**2. Rejects**
- Blown sky or dark house; haze and dust on a long lens.
- Motion blur from wind or rolling shutter in video frames.
- Neighbour properties clearly visible where they should be cropped or excluded (privacy; not an FAA rule, see below).
- People in frame who are not part of the operation.

**3. Shooting pattern**
- Take a bracket for the sky/house balance (3 frames, ±2 EV) when the sky is bright; a single frame is fine on overcast.
- Common altitudes: well under the 400 ft AGL limit for a single property.
- Capture a wide set (about 5–15 images) for each property plus a few oblique and nadir views.

**4. Grouping and selection**
- Group by flight or by view angle; keep one keeper per angle plus a bracket set where used.
- Completeness: a nadir, four oblique views (N/E/S/W), and a context view are the usual set.

**5. Metadata**
- Altitude (AGL) from XMP or EXIF, GPS, and the flight time. Note airspace authorisation if one was needed; some drone XMP includes gimbal pitch and relative altitude (check on Conrod's sample DJI files before relying on exact tag names).
- Keep the operator's Part 107 status out of image metadata; it is an operational record.

**6. Measurable signals**
- Sky clipping vs house exposure: separate sky region (top third or a sky mask) and house region; flag clipped sky over ~5%.
- Horizon or roofline tilt via dominant line angle; flag over ~1°.
- Altitude read from EXIF/XMP; flag any frame above 400 ft AGL as a compliance warning.
- Motion blur score (Laplacian variance) for frames shot from a moving platform.

**Regulation note (FAA Part 107).** Commercial real-estate flying requires a Part 107 remote pilot certificate. Operations are limited to 400 ft AGL (higher only within 400 ft of a structure), daylight or twilight with anti-collision lighting, within visual line of sight, and not over uninvolved people (waivers exist). Privacy is state/local law, not FAA: crop or blur neighbours where needed.

---

## 6. Automotive dealership and vehicle-broker stock

This is the most standardised genre. Buyers check a fixed sequence of angles; missing one is a buyer question left unanswered.

**1. Keeper**
- Every required angle present and in a fixed order.
- Exterior: front 3/4 (driver side), front 3/4 (passenger side), direct front, direct rear, both side profiles, rear 3/4s, wheels.
- Interior: front seats from the B-pillar, dashboard/cluster with ignition on (odometer readable), steering wheel/infotainment, rear seats, cargo area with tailgate open.
- Odometer legible, dash lights on, VIN and compliance plate legible (VIN plate on the dash visible through windscreen; compliance/door-jamb label on B-pillar).
- Honest condition: damage visible when present; no stock photo passed off as the car.

**2. Rejects**
- Dark, blurry, or glare-filled dash shots taken from the driver's seat.
- Wrong car, wrong colour, or a background that shows other stock.
- Number plate or VIN glare; reflection of the photographer in the paint.
- Missing cargo or rear-seat angle (skipped constantly on SUVs and vans).

**3. Shooting pattern**
- Standard angle set of about 11–25 images per vehicle; the common sequences list 8 exterior, 1–2 wheel, 1–3 feature, 2–3 dash, front and rear seat, 1–2 boot, and 2–4 close-ups. 15–25 total is typical for dealer listings; 20–35 is the best-practice band in some dealer guides.
- Position the car on a neutral background, shoot at a longer focal length (70–100 mm) for three-quarter views, and keep the car straight with wheels straight.
- Shoot the odometer with the ignition on so the display is lit.
- Brackets are optional here; use them only for dark interiors or harsh sun.

**4. Grouping and selection**
- Group by car (VIN or stock number), then by the fixed angle list. One keeper per angle.
- Completeness check per vehicle: a checklist of required angles; any missing angle is a flagged job, not a silent skip.
- Keep the damage and condition frames even when they are not pretty.

**5. Metadata**
- VIN, stock number, mileage (read from the odometer), and angle label. Captions per angle are useful for portals.
- Order: front 3/4 first (the hero), then the walkaround, then interior, then detail.

**6. Measurable signals**
- Angle classification: a detector or VLM labels each frame as front 3/4, side, rear, interior, dash, wheel, odometer, VIN plate, damage. Compare to the checklist.
- OCR on the odometer (digits and "mi/km" unit) and on the VIN (17 characters, 49 CFR 565 format: positions 1–3 WMI, 4–8 VDS, 9 check digit, 10 model year, 11 plant, 12–17 serial). A VIN that does not pass the check digit is a read error; flag it. Note: the check digit is in position 9 for North American VINs.
- Cross-check the VIN read from the door jamb and the dash plate; they must match (dealer guidance).
- Plate/label detection: locate the compliance label and VIN plate, then OCR within the box.
- Duplicate-angle detection: perceptual hash or feature match between frames of the same vehicle; keep one per angle.
- Background and car-area ratio, plus clipping on the car body, to flag glare.

---

## 7. Automotive editorial, rolling shots and car meets

**1. Keeper**
- A sharp car with a motion-blurred background (panning) or a deliberately blurred wheel with the car sharp (rolling shot).
- Wheels showing rotation in a pan; the car is sharp, not the background.
- Clean lighting: rain streaks or light trails where the story calls for it.

**2. Rejects**
- Wheels that look frozen (shutter too fast for the car speed).
- Background so blurred the scene is unreadable, or car blurred when it should be sharp.
- Missed focus on the car body or the badge.

**3. Shooting pattern**
- Panning: shutter-priority with a shutter speed matched to car speed, roughly 1/30–1/125 s for slow cars and 1/250 s or faster at speed, and a rule of thumb of 1/3 to 2/3 of car speed in mph; continuous autofocus and tracking; burst.
- Rolling (car to car): slower, often 1/10–1/15 s at night, on a high-ISO setting; use the camera's stabiliser in its active mode for rolling shots.
- Car meets: static portraits of cars in a bracket for the sky and dark interiors; detail shots.

**4. Grouping and selection**
- Group a rolling or panning burst by sequence and keep the 1–3 sharpest with the best wheel rotation.
- Keep one hero and a set of detail shots per car; reject bursts that failed completely.

**5. Metadata**
- Car make, model, year, owner or event name, location, and captions. Editorial captions often name the event.

**6. Measurable signals**
- Subject sharpness vs background blur: sharpness inside the car detection box vs the background; a pan is good when the car sharpness is high and background sharpness is low.
- Wheel-rotation estimate across a burst (optional; hard to do reliably).
- Motion-blur direction (dominant blur angle) to tell pans from camera shake.
- Shutter speed and focal length from EXIF; flag shots where shutter speed is too fast for a pan (wheels frozen) when the car detection says it is moving.

---

## 8. Product and e-commerce

**1. Keeper**
- Main image: pure white background (RGB 255,255,255), product fills about 85% of the frame, no text, no graphics, no watermark (Amazon main-image rule). Other images can use an environment at 50%.
- Sharp across the product; no jagged edges or halos from masking; accurate colour against a calibrated reference.
- Minimum 1000 px on the long side (1001+ enables zoom on Amazon).

**2. Rejects**
- Grey or cream backgrounds in the main image; visible shadows that break the white.
- Dust, fingerprints, or stray threads on the product.
- Soft focus on the label, logo, or key feature.
- Clipped highlights on glossy products.

**3. Shooting pattern**
- Tripod, fixed height, same lens and distance for a whole catalogue; lighting kit with diffusion; tethered shooting.
- Standard set: main (front), angles, detail, scale, in-use if relevant. Usually 5–7 per SKU, plus a video where the platform allows it.
- Bracket only when the product has very high contrast or a dark background.

**4. Grouping and selection**
- Group by SKU, then by angle. One keeper per angle.
- Completeness: every SKU has a main image and the required angles; flag SKU gaps.

**5. Metadata**
- SKU, product name, variant (colour/size), and angle. Alt text per image is useful for e-commerce.
- Background-cleaning masks are separate files; keep the originals.

**6. Measurable signals**
- Background purity: fraction of border pixels that are 255,255,255 (or within a small tolerance); flag below ~95%.
- Product-fill ratio: area of the non-background mask over frame area; flag under 85% for main images.
- Text/logo detection (OCR) on main images; flag any non-empty OCR.
- Sharpness in the product mask region (Laplacian variance); flag soft focus on labels.
- Clipping in the product mask (glossy highlight).

---

## 9. Food

**1. Keeper**
- Sharp on the hero element (the bite, the garnish, the crumb) with a controlled falloff; shallow depth of field often means f/2.8 to f/5.6 for a plate.
- Fresh, not wilted: the food is in the freshness window; the garnish is placed.
- Light from the side or back for texture and steam; steam only when visible and on a dark background.
- True colour; no grey or green cast.

**2. Rejects**
- Soft focus on the hero; the wrong element in focus.
- Dried, wilted, or unfinished food.
- Hands or cutlery in the wrong place; a stray crumb or fingerprint on the plate rim.
- Blown highlights on sauces and glossy surfaces.

**3. Shooting pattern**
- Tripod or tripod-head for overhead flat-lays; handheld for angled shots only when focus is locked.
- Shot list per dish: overhead, 45°, close-up, and a context shot.
- Often 20–50 frames per session with several takes of each dish; the freshness window limits this.

**4. Grouping and selection**
- Group by dish and angle; keep the 1–2 best per angle.
- Freshness is a selection criterion: discard frames from before the plate was finished.

**5. Metadata**
- Dish name, restaurant or client, shot list angle, and caption. Menu compliance matters where the image represents the dish served.

**6. Measurable signals**
- Focus on the hero region (sharpness inside a detected or user-marked box).
- Colour cast: mean colour in neutral regions (plate rim, background) vs reference.
- Specular clipping percentage on highlights.
- Duplicate-angle detection within the same dish.

---

## 10. Other commercial genres a working photographer culls

- **Corporate headshots and team sessions**: keeper = eyes open, face sharp, consistent colour and crop; reject blinks and off-brand poses. Signals: eye-state, face sharpness, background consistency. (Overlaps the wedding/portrait culling Conrod already does.)
- **Construction and progress documentation**: same logic as condition reports; timestamp and location are critical; keep every progress frame.
- **Insurance claims / damage documentation**: documentary, full coverage, scale references, no editing.
- **Events and conferences**: burst grouping plus face-quality checks; keep one per moment.
- **Restaurant and hotel interiors**: as in §1 and §4; brackets, flambient, and lighting checks.

---

## Cross-genre summary

| Genre | Keeper core | Main rejects | Bracket / pattern | Group & keep | Key metadata | Conrod signals |
|---|---|---|---|---|---|---|
| Real-estate listing | Level, straight verticals, windows not blown, full coverage | Tilt, mirror reflections, clutter, ghosting | 3–5 at ±2 EV; flambient; twilight 3–5 | Keep brackets together until merged; one final per room | Room, address, order, caption | `ExposureBiasValue`, tilt estimate, clipping %, room class |
| Property condition / inspection | Every item documented; wide + close-up | Blurred defects, duplicates, mirror | Not bracketed; consistent path | Group by room and item; keep all documenting frames | Room, item, condition, timestamp (mandatory) | Template coverage, defect sharpness, duplicate hash |
| Architecture / interior | Strong verticals, clear light source, consistent colour | Tilt, cables, WB drift | 3–5 brackets; flambient | Hero + 1–3 supports per space | Project, space, credit | Tilt, colour drift, hero score |
| Airbnb / hospitality | Cover wow, amenities visible and on | Promised amenity hidden, dark rooms, heavy HDR | Brackets; flambient with views | One per amenity; cover set ranked | Room, caption, cover first | Amenity detection, lighting score |
| Drone / aerial | Full property, level, sky not blown | Blown sky, haze, neighbours, motion blur | 3 at ±2 EV for bright sky | One per angle (N/E/S/W + context) | AGL altitude, GPS | Sky clipping, horizon tilt, altitude check |
| Dealership / vehicle | Fixed angle list complete; odometer and VIN legible | Dark dash, glare, missing cargo angle | Usually none; brackets for dark interiors | One per angle per vehicle | VIN, stock, mileage, angle | Angle classifier, odometer/VIN OCR, check digit, duplicate hash |
| Editorial / rolling / meets | Car sharp, background blurred or motion | Frozen wheels, unreadable blur, missed focus | Pan at matched shutter; burst | Best 1–3 per sequence | Make, model, event | Subject vs background sharpness, blur angle |
| Product / e-commerce | White background, 85% fill, no text | Grey background, dust, soft label | Usually none | One per angle per SKU | SKU, variant, alt text | Background purity, fill ratio, OCR-empty, product sharpness |
| Food | Sharp hero, fresh, true colour | Soft focus, wilted, cast | Tripod for flat-lays | Per dish per angle; freshness | Dish, venue, angle | Hero sharpness, colour cast, specular clipping |

**Priorities for Conrod** (most leverage first):
1. Bracket-set detection from `ExposureBiasValue` + focal length/f-number + time window, with feature-match confirmation, and a "keep set together" rule for real estate.
2. Shot-list completeness checks (rooms for real estate; angle list for dealership; SKU angles for product).
3. Tilt estimate and highlight-clipping % (both genre-agnostic and cheap).
4. OCR for odometer and VIN, with check-digit validation for North American VINs and cross-check of the two VIN locations.
5. Ordering and caption export per portal, with the MLS spec read from config, not hardcoded.

---

## Sources

- BoxBrownie, HDR tips for real estate: https://www.boxbrownie.com/b/fast-easy-hdr-photography-tips-for-real-estate
- HDRSoft learn (bracketing, verticals, twilight): https://www.hdrsoft.com/learn
- HDRSoft, twilight camera settings: https://www.hdrsoft.com/learn/camera-settings-twilight-real-estate-photography.html
- IACrea, HDR real estate: https://www.iacrea.com/en/blog/real-estate-photography/real-estate-hdr-photos-definition-and-how-to-succeed
- Bounti.ai, HDR real estate 2026: https://bounti.ai/blog/photography/hdr-photography-real-estate
- Imagen AI, real estate HDR 2026: https://imagen-ai.com/valuable-tips/hdr-real-estate-photography
- Zillow Premier Agent photo tips: https://www.zillow.com/pro/real-estate-photography-tips
- Giraffe360, common mistakes: https://giraffe360.com/blog/avoiding-common-real-estate-photography-mistakes
- 500px, real estate beginners guide: https://iso.500px.com/real-estate-photography-essential-guide-for-beginners
- PICSERA tips: https://picsera.com/real-estate-photography-tips
- Tov Studio, flambient and twilight 2026: https://tovstudiophoto.com/real-estate-photography-lighting-setup
- Regensburger Photography, HDR vs flambient: https://regensburgerphotography.com/hdr-vs-flash-ambient-blend-in-real-estate-photography
- realestate.com.au image requirements: https://help.realestate.com.au/hc/en-us/articles/115002640946-Image-and-video-requirements-for-realestate-com-au-listings
- Stellar MLS photo rules: https://www.stellarmls.com/photorules
- MLS spec roundup, 22 MLSs (roomagen): https://roomagen.com/blog/mls-photo-requirements-virtual-staging-disclosure-guide
- MLS photo specs 2026 (imagesystems): https://imagesystems.ai/blog/mls-photo-specs-the-complete-2026-technical-reference
- PropertyPixel MLS guide: https://propertypixel.app/blog/mls-photo-requirements-complete-guide
- Adobe, Lightroom HDR merge: https://helpx.adobe.com/lightroom-classic/desktop/process-and-develop-photos/hdr-photo-merge.html
- Adobe, Lightroom desktop HDR/panorama merge: https://helpx.adobe.com/lightroom-cc/using/hdr-panorama.html
- Condition report guides (ConditionHQ): https://conditionhq.app/blog/how-to-do-a-condition-report-fast
- RentAppeal, Atlanta condition reports: https://www.rentappeal.com/blog/property-condition-reports-for-atlanta-landlords-what-to-document-and-why-it-matters
- Photo-reports, real estate: https://photo-reports.online/en/docs/use-cases/real-estate
- Airbnb help, photo requirements: https://www.airbnb.com/help/article/3024
- Airbnb Homes guidelines (PDF): https://airbnb.orangelogic.com/AssetLink/4ptq561j70227otaqg1tw4ucv1g5d7hh.pdf
- BNB Mastery, Airbnb photo guide 2026: https://bnbmastery.com/videoblog/the-best-photo-strategy-for-airbnb
- FAA Part 107 summary: https://www.faa.gov/newsroom/small-unmanned-aircraft-systems-uas-regulations-part-107
- IACrea, drone rules and prices: https://www.iacrea.com/en/blog/real-estate-photography/real-estate-drone-regulations-prices-and-best-practices
- Peltier Photo Courses, Part 107 for real estate: https://www.peltierphotocourses.com/pages/do-you-need-part-107-license-real-estate
- ABJ Drone Academy, real estate drone: https://abjacademy.global/drone-blog/how-to-get-into-real-estate-drone-photography
- CarBG, car angles: https://carbackground.ai/blog/car-photography-angles
- Spyne, 12-angle dealership list: https://www.spyne.ai/blogs/car-photography-angles-dealership-inventory
- Autopix, interior angles: https://www.autopix.no/blog/car-interior-photography-tips
- Vehiso, listing photo counts: https://www.vehiso.com/blog/how-to-take-great-photos-of-cars
- Autolander, listing photo counts: https://autolander.ai/guide/how-many-photos-should-a-car-listing-have
- Cox Automotive, value story photos: https://www.coxautoinc.com/insights-hub/telling-your-value-story-through-photos
- 49 CFR 565.25 (VIN content): https://ecfr.gov/current/title-49/subtitle-B/chapter-V/part-565/subpart-C/section-565.25
- Scene, automotive photography guide: https://scene.sonyanz.com/articles-and-resources/a-guide-to-automotive-photography
- Capturing The Machine, panning tutorial: https://www.youtube.com/watch?v=bHaKPrrBLa0
- Car Photo Tutorials, panning: https://www.carphototutorials.com/make_panning_shots_like_a_pro.html
- Amazon Seller Central, product image guide: https://sellercentral.amazon.com/help/hub/reference/external/G1881?locale=en-US
- Amazon Seller Central, handmade image requirements: https://sellercentral.amazon.com/gp/help/external/GNRP6DDJV79DUHRD
- Selekt, food culling workflow: https://selekt.photos/blog/food-photography-culling-workflow
- Aftershoot, culling FAQ: https://aftershoot.com/culling-faq
- Narrative, Aftershoot comparison: https://narrative.so/blog/aftershoot-vs-narrative-which-is-better-culling-software
- Narrative Select features: https://narrative.so/select
