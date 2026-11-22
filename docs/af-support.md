# Autofocus areas by camera brand

Conrod uses the camera's autofocus area to choose the main subject when the
detector misses (`crates/conrod-engine/src/afsubject.rs`). The reader,
`crates/conrod-io/src/af.rs`, runs `exiftool -n -json -G1` once per 200 frames,
strips the group from each tag name, and returns the area as fractions of the
upright frame.

Tag names and units were checked against the ExifTool 13.58 tag database
(`exiftool -listx` and the `Image::ExifTool` Perl tables).

| Brand | Files | Tags read | Supported |
|---|---|---|---|
| Canon | CR3 | `AFPointsInFocus` bitmap, `AFAreaXPositions`/`AFAreaYPositions`/`AFAreaWidths`/`AFAreaHeights`, `AFImageWidth`/`AFImageHeight`, `AFAreaMode`, `ValidAFPoints` | Yes, one point or the union of an in-focus grid |
| Nikon | NEF | `AFAreaXPosition`/`AFAreaYPosition`/`AFAreaWidth`/`AFAreaHeight` (Nikon AFInfo2, Z-series and contrast-detect bodies), `AFImageWidth`/`AFImageHeight`; older Canon-style `AFAreaXPositions`/`AFAreaYPositions` arrays as a fallback | Yes, one point |
| Sony | ARW | `FocusLocation` (`w h x y`, image pixels), `FocusFrameSize` (`w h flag`) | Yes, one point |
| Fujifilm | RAF | `FocusPixel` (`x y`, image pixels) | Yes, one point |
| Panasonic | RW2 | `AFPointPosition` (x y as 0..1 of the image) | Yes, one point |
| Olympus / OM System | ORF | `AFPointSelected` (four 0..1 fractions of the image: the target's corners) | Yes, one point or a target box |

## Geometry, per maker

- Canon: offset from the centre of the AF image, y upwards. Verified on an EOS
  R7; the bitmap is also read natively from the MakerNote
  (`crates/conrod-io/src/canon_af.rs`).
- Nikon: `AFAreaXPosition`/`AFAreaYPosition` are documented by ExifTool as
  "center of AF area in AFImage coordinates" (`Nikon.pm`, tag 0x14/0x16 and
  0x4a/0x4c for AFInfo2, 0x2e/0x30 for the Z-series). The fields are unsigned,
  so they are image pixels with a top-left origin, not a signed centre offset.
- Sony: `FocusLocation` is four `int16u` (`Sony.pm` 0x2027) whose first two are
  the image size; if no focus location could be stored the body writes the
  centre. `FocusFrameSize` 0x2037 is `width height flag`, flag zero meaning the
  size is unknown.
- Fujifilm: `FocusPixel` is `int16u[2]` in image pixels (`FujiFilm.pm` 0x1023).
- Panasonic: `AFPointPosition` is `rational64u[2]`, "X Y coordinates of primary
  AF area center, in the range 0.0 to 1.0" (`Panasonic.pm` 0x4d).
- Olympus / OM System: `AFPointSelected` is `rational64s[5]`; ExifTool drops a
  leading undefined value and prints `(x1%,y1%) (x2%,y2%)`, so `-n` gives four
  0..1 fractions of the image, top-left origin (`Olympus.pm` 0x0305). A single
  target is written as a degenerate box (all four values equal); Conrod then
  centres its default small square on that point. The packed `AFAreas` boxes are
  not read.

## Notable exclusions

- Sony `FocalPlaneAFPointLocation1`..`15` / `FocalPlaneAFPointArea` (old DSLRs):
  the coordinate units are undocumented, so they are not used.
- Olympus `AFTargetInfo` / `FocusInfo:AFPointDetails`: undocumented encodings,
  not used; `AFAreas` (packed 0..255 boxes) is not needed while
  `AFPointSelected` gives the target directly.
- Panasonic `AFAreaSize`: ExifTool describes it only as "relative to size of
  image" without saying whether that is a full or half extent, so the area is
  the point with a default size.
- Nikon `AFPointSelected` (menu/custom setting) and `AFPointsSelected` (a bit
  mask of sensor indices) carry no coordinates; the geometry is the AFInfo2
  fields above.

## Verification status

- Real file: Olympus E-M10 Mark IV ORF (raw.pixls.us, `PA040002.ORF`):
  `AFPointSelected` = `0.5 0.43125 0.5 0.43125`; the packed `AFAreas` box
  packs to x 118..137, y 101..119 of 0..255, a centre of (0.500, 0.431) — the
  same point, confirming the fractions and the top-left origin. A single
  degenerate target is also what that body wrote.
- Real file: Olympus E-M1 Mark III ORF (`_3160531.ORF`) has `undef` /
  zero AF tags and correctly yields no area.
- Real file: Panasonic DMC-LX3 RW2 (ExifTool test image): `AFPointPosition`
  `0.5 0.5`, read as a point at the frame's centre.
- Real file: Nikon D70 NEF and Fujifilm S5 Pro RAF (ExifTool test images) are
  stripped test files with no AF geometry, so Nikon and Fujifilm are verified
  by unit tests only. Sony has no small free sample in the ExifTool set; also
  unit-tested only.
- Unit tests per brand live in `crates/conrod-io/src/af.rs`; each passes a
  synthetic `-n -json -G1` record and checks the resulting fractions.

## When there is no area

A frame with no AF tags, an unknown body, a mirrored orientation, or a canvas
whose aspect no longer matches the frame (cropped after capture) has no area.
`af::read` answers `None` and the cull keeps the existing sharpest-car
behaviour; the camera's word is evidence, never a veto.
