export type Job = { id: number; label: string; root: string; status: string; total: number; done: number; created_at: number };
export type Frame = {
  id: number; path: string; status: string; thumb_path: string | null; preview_path: string | null;
  width: number; height: number; rating: number | null; manual_stars: number | null; rejected: number;
  burst_key: number | null; error: string | null;
};
export type Detection = {
  id: number; image_id: number; cls: string; x1: number; y1: number; x2: number; y2: number;
  sharpness: number; panning: number; number: string | null; plate: string | null;
  cull_reason: string | null; attributes: string | null; burst_pick: number; region_type: string | null;
};
export type Task = { id: number; label: string; detail: string; state: string; done: number; total: number; elapsed?: number; eta: number | null; error: string | null };
export type Status = { revision?: number; tasks: Task[]; log: string[]; activeJob: number | null; operations: string[] };
export type Review = { frames: Frame[]; detections: Detection[] };

export type SettingValue = string | number | boolean;
export type Settings = Record<string, SettingValue>;
export type ModelInfo = { name: string; file: string; ready: boolean };
export type Bootstrap = { jobs: Job[]; settings: Settings; models: ModelInfo[]; status: Status };

export const ATTRIBUTE_KEYS = ['plate', 'race_number', 'make', 'model', 'colour', 'team', 'plate_state', 'body_type'] as const;
export type AttributeKey = (typeof ATTRIBUTE_KEYS)[number];
export type Attributes = Partial<Record<AttributeKey, string>> & { colour_hex?: string; sponsors?: string[] };
export type KnownVehicle = { plate: string; make?: string; model?: string; colour?: string; team?: string; race_number?: string };
export type Training = { labels: number; active?: boolean; validation?: unknown };

export const REGIONS = ['vehicle', 'person', 'face', 'eye'] as const;
export type Region = (typeof REGIONS)[number];
export type Page = 'Library' | 'Scan' | 'Review' | 'Train' | 'Known vehicles' | 'Settings';
export type View = 'review' | 'all' | 'picks' | 'rejected';
export type Sort = 'review' | 'best' | 'worst' | 'frame' | 'pick';
export type Facet = { kind: 'number' | 'plate'; value: string };
export type ScanArgs = { root: string; label: string; profile: string; recursive?: boolean; stage?: 'index' | 'cull' | 'all'; readPlates?: boolean; readNumbers?: boolean } | { jobId: number };
export type MarkValues = { stars?: number | null; rejected?: boolean };
export type Toaster = (message: string, options?: { tone?: 'error' | 'ok'; ms?: number }) => void;

export const PROFILES: readonly { id: string; title: string; hint: string }[] = [
  { id: 'motorsport', title: 'Motorsport', hint: 'Cars, bikes & the perfect pan' },
  { id: 'portrait', title: 'Portrait session', hint: 'People, faces & eyes' },
  { id: 'event', title: 'Event', hint: 'Every part of the occasion' },
  { id: 'mix', title: 'Mixed shoot', hint: 'People and vehicles together' },
];
