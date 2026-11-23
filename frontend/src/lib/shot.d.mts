/** One label/value line: the full-frame viewer's shot rows. */
export type ShotRow = { label: string; value: string };

/** How many frames used one value: an f-stop, shutter, ISO or focal length. */
export type StatValue = { value: number; count: number };
/** How many frames came from one camera or lens. */
export type StatName = { name: string; count: number };

/** The frame fields these helpers read; all optional and null until the EXIF pass finds them. */
export type ShotFrame = {
  camera?: string | null;
  lens?: string | null;
  iso?: number | null;
  file_size?: number | null;
  exposure_time?: number | null;
  exposure_bias?: number | null;
  f_number?: number | null;
  focal_length?: number | null;
  focus_distance?: number | null;
  taken_at?: number | null;
};

/** The `shoot_stats` command's payload: album-wide counts and the top values per bucket. */
export type ShootStats = {
  frames: number;
  copies_hidden: number;
  size_bytes: number;
  /** How many frames this call had to read EXIF for; 0 once the album is backfilled. */
  backfilled: number;
  cameras: StatName[];
  lenses: StatName[];
  most_used: {
    camera?: string | null;
    lens?: string | null;
    f_number?: number | null;
    shutter?: number | null;
    iso?: number | null;
    focal_length?: number | null;
  };
  f_numbers: StatValue[];
  shutters: StatValue[];
  isos: StatValue[];
  focal_lengths: StatValue[];
  pans: number;
  bursts: number;
  blurred: number;
  soft: number;
  sharp: number;
  af: { frames: number; missing: number; off_subject: number; no_subject: number; soft: number };
  keepers: number;
  rejected: number;
  /** How many keepers carry Lightroom's pick, when the bridge has imported a catalog; absent today. */
  catalog_match?: import('./types').CatalogMatch;
  stars: StatValue[];
  taken_from: number | null;
  taken_to: number | null;
};

/** "1/1000 s", "0.5 s", "1.3 s". */
export function formatShutter(seconds: number | null | undefined): string | null;
/** "ƒ/5.6", "ƒ/4". */
export function formatAperture(f: number | null | undefined): string | null;
/** "400 mm". */
export function formatFocal(mm: number | null | undefined): string | null;
/** "24.3 MB", "12.4 GB". */
export function formatBytes(bytes: number | null | undefined): string | null;
/** "+0.3 EV", "0.0 EV", "-0.7 EV". */
export function formatEv(ev: number | null | undefined): string | null;
/** "12.5 m", "1.2 km". */
export function formatDistance(m: number | null | undefined): string | null;
/** A short local date and time, or null. */
export function formatTaken(epoch: number | null | undefined): string | null;
/** "2 h 14 min". */
export function formatDuration(seconds: number | null | undefined): string | null;
/** The frame's shot details, present values only, in the viewer's order. */
export function shotRows(frame: ShotFrame | null | undefined): ShotRow[];
