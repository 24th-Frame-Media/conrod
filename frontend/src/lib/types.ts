export type Job = { id: number; label: string; root: string; status: string; total: number; done: number; created_at: number; scan_profile?: string; include_people?: boolean; settings_json?: string | null; burst_gap?: number };
export type Frame = {
  id: number; path: string; status: string; thumb_path: string | null; preview_path: string | null;
  width: number; height: number; rating: number | null; manual_stars: number | null; rejected: number; picked: number;
  burst_key: number | null; dupe_key?: number | null; burst_pick?: number | null; error: string | null;
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
  reviewed?: number; group_key?: number | null; group_size?: number | null; group_agreement?: number | null;
  known_match?: KnownVehicle & { similarity?: number };
  known_person_match?: { name: string; country?: string; similarity?: number };
};
export type Task = { id: number; label: string; detail: string; state: string; done: number; total: number; elapsed?: number; eta: number | null; error: string | null };
export type Status = { revision?: number; tasks: Task[]; log: string[]; activeJob: number | null; operations: string[] };
export type Review = { frames: Frame[]; detections: Detection[] };

export type SettingValue = string | number | boolean;
export type Settings = Record<string, SettingValue>;
export type ModelInfo = { id?: string; name: string; file: string; ready: boolean; detail?: string };
export type Bootstrap = { jobs: Job[]; settings: Settings; models: ModelInfo[]; status: Status };

export const ATTRIBUTE_KEYS = ['plate', 'race_number', 'make', 'model', 'colour', 'team', 'driver', 'country', 'plate_state', 'body_type'] as const;
export type AttributeKey = (typeof ATTRIBUTE_KEYS)[number];
export type Attributes = Partial<Record<AttributeKey, string>> & { colour_hex?: string; sponsors?: string[]; person_name?: string; plate_bbox?: [number, number, number, number]; model_suggested?: boolean; model_evidence?: string };
export type KnownVehicle = { plate: string; old_plate?: string; make?: string; model?: string; colour?: string; team?: string; race_number?: string; driver?: string; country?: string };
export type Training = { labels: number; active?: boolean; validation?: unknown };

export const REGIONS = ['vehicle', 'person', 'face', 'eye'] as const;
export type Region = (typeof REGIONS)[number];
export type Page = 'Library' | 'Scan' | 'Review' | 'Train' | 'Known vehicles' | 'Settings';
export type View = 'all' | 'keepers' | 'undecided' | 'rejected' | 'top3' | 'review' | 'needsLook';
export type Sort = 'review' | 'best' | 'worst' | 'frame' | 'pick';
export type Facet = { kind: import('./facets.mjs').FacetKind; value: string };
export type ScanArgs = { root: string; label: string; profile: string; recursive?: boolean; stage?: 'index' | 'cull' | 'all'; readPlates?: boolean; readNumbers?: boolean; includePeople?: boolean } | { jobId: number };
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

/** One worker lane: what that worker is doing this instant. */
export type PipelineWorker = {
  stage: string;
  worker_index: number;
  state: 'idle' | 'busy' | 'waiting';
  /** Why it is waiting: `gpu`, `lock`, `ollama` or `paused`; null unless waiting. */
  waiting_for: 'gpu' | 'lock' | 'ollama' | 'paused' | null;
  /** The file name, to show beside a thumbnail. */
  file: string | null;
  step: string | null;
  ms_in_step: number;
};

/** A frame that just finished, newest first in `ScanProgress.recent`. */
export type ScanRecentFrame = {
  image_id: number;
  /** The same path `Frame.thumb_path` holds; run it through `asset()` as usual. */
  thumb_url: string;
  subject: string;
  verdict: 'sharp' | 'soft' | 'panning';
  /** What the frame reads as once identified: a race number or plate. */
  label?: string | null;
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
};

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

export type UpdateInfo = {
  ok: boolean;
  current: string;
  latest?: string;
  newer?: boolean;
  tag?: string;
  size?: number;
  notes?: string;
  installable?: boolean;
  error?: string;
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

export type VlmProvider = 'ollama' | 'openai' | 'anthropic' | 'gemini';
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

