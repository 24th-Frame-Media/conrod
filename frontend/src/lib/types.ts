export type Job = { id: number; label: string; root: string; status: string; total: number; done: number; copies?: number; created_at: number; scan_profile?: string; include_people?: boolean; settings_json?: string | null; burst_gap?: number };
export type Frame = {
  id: number; path: string; status: string; thumb_path: string | null; preview_path: string | null;
  width: number; height: number; rating: number | null; manual_stars: number | null; rejected: number; picked: number;
  burst_key: number | null; dupe_key?: number | null; burst_pick?: number | null; error: string | null;
  /** Keep-together set (bracket / focus / interval): one stack, never culled alone. */
  set_id?: number | null;
  set_kind?: 'bracket' | 'focus' | 'interval' | null;
  /** Measured exposure: a flag, and `dark_frame` is the only exposure cull. */
  exposure_verdict?: 'ok' | 'highlights' | 'shadows' | 'dark' | null;
  highlight_clip?: number | null;
  shadow_clip?: number | null;
  dark_frame?: number | null;
  /** Level, only where the preset asks: property, interiors, landscape. */
  tilt_deg?: number | null;
  converge_deg?: number | null;
  level_verdict?: 'level' | 'tilted' | 'keystone' | 'unknown' | null;
  /** The camera's autofocus area, as fractions 0..1 of the upright photo; null when the payload has none. */
  af_x1?: number | null;
  af_y1?: number | null;
  af_x2?: number | null;
  af_y2?: number | null;
  /** Expression across the frame's faces, 0..1; null without the models. */
  eye_open_min?: number | null;
  smile_max?: number | null;
  /** Learned taste, 0..1: a sort signal inside a burst, never a pick. Null until `taste_rank` runs. */
  taste?: number | null;
  /** Rodeo/equestrian only: how much the animal is rearing under its rider. */
  buck_score?: number | null;
  /** Composition, measured on demand: where the subject sits and what the frame reads as. Null until `composition` runs. */
  comp_placement?: number | null;
  comp_mode?: 'centre' | 'thirds' | 'phi' | null;
  /** -1 heavy left .. 1 heavy right. */
  comp_balance_x?: number | null;
  /** -1 top .. 1 bottom. */
  comp_balance_y?: number | null;
  comp_symmetry?: number | null;
  /** 0..1, 1 = busiest background. */
  comp_clutter?: number | null;
  /** How many leading lines the frame reads as, 0..7. */
  comp_lines?: number | null;
  comp_subject_share?: number | null;
  /** EXIF shot data, read with the file (or backfilled on the first `shoot_stats`); null when absent. */
  camera?: string | null;
  lens?: string | null;
  iso?: number | null;
  /** Bytes on disk. */
  file_size?: number | null;
  /** Seconds: 0.001 is 1/1000. */
  exposure_time?: number | null;
  /** EV, signed. */
  exposure_bias?: number | null;
  f_number?: number | null;
  /** Millimetres. */
  focal_length?: number | null;
  /** Metres. */
  focus_distance?: number | null;
  /** Epoch seconds, in local time when displayed. */
  taken_at?: number | null;
};
export type Detection = {
  rejected?: number; bystander?: number; stars?: number | null;
  id: number; image_id: number; cls: string; x1: number; y1: number; x2: number; y2: number;
  sharpness: number; panning: number; number: string | null; plate: string | null;
  crop_path?: string | null;
  rating?: number | null; rating_verdict?: string | null; sharp_end?: string | null;
  cull_reason: string | null; attributes: string | null; burst_pick: number; region_type: string | null;
  /** NIMA technical quality, 0..1; null when the model is off or absent. */
  quality?: number | null;
  /** Framing of this subject's box; `framing_score` ranks the keeper where the preset applies. */
  cut_sides?: number | null;
  subject_frac?: number | null;
  lead_room?: number | null;
  framing_score?: number | null;
  reviewed?: number; group_key?: number | null; group_size?: number | null; group_agreement?: number | null;
  known_match?: KnownVehicle & { similarity?: number };
  known_person_match?: { name: string; country?: string; similarity?: number };
};
export type Task = { id: number; label: string; detail: string; state: string; done: number; total: number; elapsed?: number; eta: number | null; error: string | null };
export type Status = { revision?: number; tasks: Task[]; log: string[]; activeJob: number | null; operations: string[] };
export type Review = { frames: Frame[]; detections: Detection[]; tasteHead?: boolean };

/**
 * The `global_stats` command's payload: the whole library's totals, plus gear and exposure
 * gathered only from the albums whose every visible frame has complete shot data.
 */
export type GlobalStats = {
  albums: number;
  complete_albums: number;
  frames: number;
  complete_frames: number;
  copies_hidden: number;
  size_bytes: number;
  cameras: import('./shot.mjs').StatName[];
  lenses: import('./shot.mjs').StatName[];
  most_used: {
    camera?: string | null;
    lens?: string | null;
    f_number?: number | null;
    shutter?: number | null;
    iso?: number | null;
    focal_length?: number | null;
  };
  f_numbers: import('./shot.mjs').StatValue[];
  shutters: import('./shot.mjs').StatValue[];
  isos: import('./shot.mjs').StatValue[];
  focal_lengths: import('./shot.mjs').StatValue[];
  pans: number;
  bursts: number;
  blurred: number;
  soft: number;
  sharp: number;
  af: { frames: number; missing: number; off_subject: number; no_subject: number; soft: number };
  keepers: number;
  rejected: number;
  /** Sum of every timed album's first-to-last frame span, in seconds. */
  shooting_seconds: number;
  /** Per timed album. */
  average_shoot_seconds: number;
  longest_shoot_seconds: number;
  timed_albums: number;
};

export type SettingValue = string | number | boolean;
/** Global defaults; an album's `settings_json` overrides the same keys. */
export type Settings = Record<string, SettingValue> & {
  /** What identification reads; the album's own value wins over this default. */
  identify_scope?: Scope;
  workflow_mode?: string;
};
export type ModelInfo = { id?: string; name: string; file: string; ready: boolean; detail?: string };
export type Bootstrap = { jobs: Job[]; settings: Settings; models: ModelInfo[]; status: Status };

export const ATTRIBUTE_KEYS = ['plate', 'race_number', 'make', 'model', 'colour', 'team', 'driver', 'country', 'plate_state', 'body_type'] as const;
export type AttributeKey = (typeof ATTRIBUTE_KEYS)[number];
export type Attributes = Partial<Record<AttributeKey, string>> & { colour_hex?: string; sponsors?: string[]; person_name?: string; plate_bbox?: [number, number, number, number]; model_suggested?: boolean; model_evidence?: string; number_source?: string; number_conf?: number; vlm_conf?: number; action?: string[] };
export type KnownVehicle = { plate: string; old_plate?: string; make?: string; model?: string; colour?: string; team?: string; race_number?: string; driver?: string; country?: string };
export type Training = { labels: number; active?: boolean; validation?: unknown };

export const REGIONS = ['vehicle', 'person', 'face', 'eye'] as const;
export type Region = (typeof REGIONS)[number];
export type Page = 'Library' | 'Scan' | 'Identify' | 'Review' | 'Train' | 'Known vehicles' | 'Settings';
export type View = 'all' | 'keepers' | 'undecided' | 'rejected' | 'top3' | 'review' | 'needsLook';
export type Sort = 'review' | 'best' | 'worst' | 'frame' | 'pick' | 'taste';
/** Which subjects identification reads: the suggested keeper of each burst, or every subject. */
export type Scope = 'keepers' | 'everything';
export type Facet = { kind: import('./facets.mjs').FacetKind; value: string };
export type ScanArgs = { root: string; label: string; profile: string; recursive?: boolean; stage?: 'index' | 'cull' | 'all'; readPlates?: boolean; readNumbers?: boolean; includePeople?: boolean; scanProfile?: string; blurStrictness?: string; burstGap?: number; groupingStacks?: boolean; groupingSensitivity?: string; groupingPause?: number; groupingKeepPans?: boolean; groupingFocusAnchor?: boolean; workflowMode?: string; identifyScope?: Scope } | { jobId: number };
export type MarkValues = { stars?: number | null; rejected?: boolean; picked?: boolean };
export type Toaster = (message: string, options?: { tone?: 'error' | 'ok'; ms?: number }) => void;

export type ProfileOption = { id: string; title: string };
export type ProfileGroup = ProfileOption & { hint: string; children: readonly ProfileOption[] };

export const PROFILE_GROUPS: readonly ProfileGroup[] = [
  {
    id: 'motorsport', title: 'Motorsport', hint: 'Cars, bikes & the perfect pan', children: [
      { id: 'motorsport-burnouts', title: 'Burnouts' },
      { id: 'motorsport-track', title: 'Track days' },
      { id: 'motorsport-rally', title: 'Rallies' },
      { id: 'motorsport-meet', title: 'Car meets' },
      { id: 'sport-cycling', title: 'Cycling' },
      { id: 'sport-action', title: 'Action sports' },
      { id: 'auto-dealer', title: 'Dealership stock' },
      { id: 'aviation', title: 'Aviation' },
    ],
  },
  {
    id: 'portrait', title: 'Portraits', hint: 'People, faces & eyes', children: [
      { id: 'portrait-group', title: 'Group photos' },
      { id: 'portrait-couple', title: 'Couple photos' },
      { id: 'portrait-pets', title: 'Pets' },
    ],
  },
  {
    id: 'event', title: 'Events', hint: 'Every part of the occasion', children: [
      { id: 'event-shows', title: 'Shows' },
      { id: 'event-parties', title: 'Parties' },
      { id: 'sport-field', title: 'Field sports' },
      { id: 'sport-court', title: 'Court sports' },
      { id: 'sport-rodeo', title: 'Rodeo' },
      { id: 'sport-equestrian', title: 'Equestrian' },
      { id: 'wedding', title: 'Weddings' },
      { id: 'concert', title: 'Concerts' },
    ],
  },
  {
    id: 'mix', title: 'Mixed', hint: 'Nature, property & no dominant subject', children: [
      { id: 'property-listing', title: 'Property listing' },
      { id: 'property-report', title: 'Condition report' },
      { id: 'wildlife-birds', title: 'Birds' },
      { id: 'landscape', title: 'Landscape' },
      { id: 'astro', title: 'Astro' },
    ],
  },
];

export const MIXED_PROFILE = { id: 'mix', title: 'Mixed', hint: 'Global, unspecialised defaults' } as const;

export function profileParent(id: string): string {
  return PROFILE_GROUPS.find((group) => group.id === id || group.children.some((child) => child.id === id))?.id ?? MIXED_PROFILE.id;
}

export function profileLabel(id: string): string {
  if (id === MIXED_PROFILE.id) return MIXED_PROFILE.title;
  for (const group of PROFILE_GROUPS) {
    if (group.id === id) return group.title;
    const child = group.children.find((option) => option.id === id);
    if (child) return `${group.title} · ${child.title}`;
  }
  return id;
}

/** One stage node of the scan pipeline (the flowchart on the scan screen). */
export type PipelineStage = {
  /** `read` | `detect` | `sharpness` | `faces` | `identify` | `group` | `pick`. */
  id: string;
  label: string;
  /** The model actually loaded: e.g. `yolo11s-960.onnx`, `qwen2.5vl:7b via ollama`. */
  model: string;
  /** Where it runs: `DirectML`, `CPU`, `ollama`. */
  device: string;
  workers_total: number;
  workers_busy: number;
  queue_len: number;
  done: number;
  total: number;
  rate_per_s: number;
  eta_s: number | null;
};

/** A step a scan lane moves a frame through, in order. See docs/plans/ingest-live-contract.md. */
export type LaneStep = 'read' | 'decode' | 'detect' | 'sharpness' | 'faces' | 'persist';

/** One detection on a lane's frame: class, confidence and a box as fractions (0..1) of the upright frame, [x1, y1, x2, y2]. */
export type LaneBox = { cls: string; conf: number; box: [number, number, number, number] };

/** The frame a scan lane holds right now. Every field but the identity is optional and fills in as the steps finish. */
export type LaneFrame = {
  image_id: number;
  file: string;
  /** Same kind of path as `ScanRecentFrame.thumb_url`; run it through `asset()`. */
  thumb_url?: string;
  step: LaneStep;
  boxes?: LaneBox[];
  /** The autofocus area, [x1, y1, x2, y2] as fractions of the upright frame. */
  af?: [number, number, number, number];
  /** 0..5 */
  stars?: number;
  verdict?: 'sharp' | 'soft' | 'blurred';
  panning?: boolean;
};

/** One worker lane: what that worker is doing this instant. */
export type PipelineWorker = {
  stage: string;
  worker_index: number;
  /** `paused` is a lane that holds a frame but is frozen by Pause; older engines report it as `busy`. */
  state: 'idle' | 'busy' | 'waiting' | 'paused';
  /** Why it is waiting: `gpu`/`detector`, `faces`, `lock`, `writer`, `disk`, `ollama` or `paused`; null unless waiting. */
  waiting_for: 'gpu' | 'detector' | 'faces' | 'lock' | 'writer' | 'disk' | 'ollama' | 'paused' | null;
  /** The file name, to show beside a thumbnail. */
  file: string | null;
  step: string | null;
  ms_in_step: number;
  /** A stable lane id (0-based) that survives the lane changing `stage`; without it the card is keyed by stage + worker_index. */
  lane?: number;
  /** The frame this lane holds, with what has been found on it so far. */
  current?: LaneFrame | null;
};

/** A frame that just finished, newest first in `ScanProgress.recent`. */
export type ScanRecentFrame = {
  image_id: number;
  /** The same path `Frame.thumb_path` holds; run it through `asset()` as usual. */
  thumb_url: string;
  subject: string;
  verdict: 'sharp' | 'soft' | 'blurred';
  /** A pan is a flag, not a verdict: a panned car can still be soft. */
  panning?: boolean;
  /** Older payloads folded the pan into the verdict. */
  legacy?: never;
  /** What the frame reads as once identified: a race number or plate. */
  label?: string | null;
  /** The file name; lets a lane find its last finished thumbnail. */
  file?: string;
  /** 0..5 */
  stars?: number;
  boxes?: LaneBox[];
  af?: [number, number, number, number];
};

export type ScanFound = {
  subjects: number;
  soft: number;
  closed_eyes: number;
  numbers: number;
  plates: number;
  bursts: number;
  keepers: number;
};

/** The payload of the `scan-progress` event, ~4 per second while a scan runs. */
/** The whole scan in weighted work units: the header's bar and time left. */
export type ScanOverall = {
  done: number;
  total: number;
  /** 0..1 across every stage, weighted by each stage's cost per item. */
  fraction: number;
  rate_per_s: number;
  eta_s: number | null;
  /** False until every stage with real throughput is at least 20 items in. */
  eta_confident: boolean;
};

export type ScanProgress = {
  job_id: number;
  state: 'running' | 'done' | 'stopped' | 'error';
  /** The stage the header shows, and its place in `stage_count`. */
  stage: string;
  stage_index: number;
  stage_count: number;
  done: number;
  total: number;
  rate_per_s: number;
  eta_s: number | null;
  recent: ScanRecentFrame[];
  found: ScanFound;
  pipeline: { stages: PipelineStage[]; workers: PipelineWorker[] };
  /** The whole scan, weighted across stages; prefer it over deriving one. */
  overall: ScanOverall;
};

/** One subject the identify pass is reading, or has just finished reading. */
export type IdentifyStage = 'read' | 'crop' | 'vision' | 'merge' | 'done';

/**
 * `identify-progress` `current` and each `recent` entry: the car, the exact question and reply,
 * the parsed fields, and the trace lines the engine wrote while deciding them.
 */
export type IdentifyItem = {
  detection_id: number;
  image_id: number;
  path: string;
  thumb_path?: string | null;
  preview_path?: string | null;
  crop_path?: string | null;
  stage: IdentifyStage;
  /** The exact prompt sent to the vision model; null when the shoot has no VLM. */
  prompt?: string | null;
  /** The model's raw JSON reply; null when only OCR / the entry list answered. */
  reply?: Record<string, unknown> | null;
  /** Newest first, as the engine wrote them. */
  trace?: string[];
  number: string | null;
  /** `vlm`, `ocr`, `entry`, `ocr+entry`, … */
  number_source?: string | null;
  /** 0..1. */
  number_conf?: number | null;
  plate?: string | null;
  make?: string | null;
  model?: string | null;
  team?: string | null;
  colour?: string | null;
  /** The identify worker reading this car, 0-based. */
  worker?: number;
  /** Visible action effects: airborne, dust, tyre_smoke, spray, flames, sparks, contact, wheel_lift. */
  action?: string[];
  failures?: string[];
  /** How long this car took, in milliseconds. */
  ms?: number | null;
  /** Epoch seconds. */
  at?: number | null;
};

/** The payload of the `identify-progress` event, a few times a second while identification runs. */
export type IdentifyLive = {
  job_id: number;
  state: 'running' | 'done' | 'stopped' | 'error';
  done: number;
  total: number;
  /** Epoch seconds. */
  started_at?: number | null;
  elapsed_ms?: number | null;
  stage: IdentifyStage;
  /** The vision model this run calls, for the VISION node; empty when off. */
  model?: string;
  /** How many identify workers run at once: one panel per worker. */
  workers?: number;
  /** Null before the first car is read. */
  current: IdentifyItem | null;
  /** Every car a worker is inside right now, one per identify worker, newest first. */
  active: IdentifyItem[];
  /** Finished cars only, newest first, up to 16. */
  recent: IdentifyItem[];
  errors: string[];
};

/** One detection the identify pass has read: the light row behind the identifications list. */
export type IdentifiedRow = {
  id: number;
  image_id: number;
  cls: string;
  region_type: string | null;
  crop_path: string | null;
  number: string | null;
  plate: string | null;
  number_conf: number | null;
  plate_conf: number | null;
  /** A JSON string: make, model, team, driver, sponsors, confidences. */
  attributes: string | null;
  quality: number | null;
  reviewed: number;
  rejected: number;
  frame_path: string;
  frame_thumb: string | null;
};
export type Identified = { rows: IdentifiedRow[]; total: number; unreviewed: number; shown: number };

/** What `regroup` (and an `update_job_settings` gap/preset patch) returns. */
export type RegroupCounts = { bursts: number; moments: number; keepers: number };

/** One pool's worker count and its cap; `live: false` means "applies next scan". */
export type WorkerLimit = { current: number; min: number; max: number; default: number; live: boolean };

/** The `worker_limits` command: per-stage worker counts and their caps. */
export type WorkerLimits = {
  cores: number;
  cull: WorkerLimit;
  detect: WorkerLimit;
  preview: WorkerLimit;
  identify: WorkerLimit;
  note: string;
};

export type OllamaModel = {
  name: string;
  size: number;
  vision: boolean;
  parameter_size?: string;
  modified_at?: string;
};

export type OllamaModelsResponse = {
  ok: boolean;
  host: string;
  online: boolean;
  models: OllamaModel[];
  error?: string;
};

export type VlmProvider = 'ollama' | 'openai' | 'anthropic' | 'gemini' | 'opencode' | 'opencode-go';
/** `provider_models`: the models a provider key can call. */
export type ProviderModelsResponse = { models: { id: string; name: string }[]; error?: string };
export type VlmService = {
  id: string;
  provider: VlmProvider;
  model: string;
  host?: string;
  api_key?: string;
  key_kind?: string;
  enabled: boolean;
};
export type VlmStrategy = 'least_busy' | 'round_robin' | 'random';

