export type Job = { id: number; label: string; root: string; status: string; total: number; done: number; created_at: number; scan_profile?: string };
export type Frame = {
  id: number; path: string; status: string; thumb_path: string | null; preview_path: string | null;
  width: number; height: number; rating: number | null; manual_stars: number | null; rejected: number;
  burst_key: number | null; burst_pick?: number | null; error: string | null;
};
export type Detection = {
  rejected?: number; bystander?: number; stars?: number | null;
  id: number; image_id: number; cls: string; x1: number; y1: number; x2: number; y2: number;
  sharpness: number; panning: number; number: string | null; plate: string | null;
  crop_path?: string | null;
  cull_reason: string | null; attributes: string | null; burst_pick: number; region_type: string | null;
  reviewed?: number; group_key?: number | null; group_size?: number | null; group_agreement?: number | null;
  known_match?: KnownVehicle & { similarity?: number };
  known_person_match?: { name: string; country?: string; similarity?: number };
};
export type Task = { id: number; label: string; detail: string; state: string; done: number; total: number; elapsed?: number; eta: number | null; error: string | null };
export type Status = { revision?: number; tasks: Task[]; log: string[]; activeJob: number | null; operations: string[] };
export type Review = { frames: Frame[]; detections: Detection[] };

export type SettingValue = string | number | boolean;
export type Settings = Record<string, SettingValue>;
export type ModelInfo = { name: string; file: string; ready: boolean };
export type Bootstrap = { jobs: Job[]; settings: Settings; models: ModelInfo[]; status: Status };

export const ATTRIBUTE_KEYS = ['plate', 'race_number', 'make', 'model', 'colour', 'team', 'driver', 'country', 'plate_state', 'body_type'] as const;
export type AttributeKey = (typeof ATTRIBUTE_KEYS)[number];
export type Attributes = Partial<Record<AttributeKey, string>> & { colour_hex?: string; sponsors?: string[]; person_name?: string };
export type KnownVehicle = { plate: string; old_plate?: string; make?: string; model?: string; colour?: string; team?: string; race_number?: string; driver?: string; country?: string };
export type Training = { labels: number; active?: boolean; validation?: unknown };

export const REGIONS = ['vehicle', 'person', 'face', 'eye'] as const;
export type Region = (typeof REGIONS)[number];
export type Page = 'Library' | 'Scan' | 'Review' | 'Train' | 'Known vehicles' | 'Settings';
export type View = 'review' | 'all' | 'picks' | 'rejected' | 'kept' | 'stacks' | 'vehicles';
export type Sort = 'review' | 'best' | 'worst' | 'frame' | 'pick';
export type Facet = { kind: 'number' | 'plate'; value: string };
export type ScanArgs = { root: string; label: string; profile: string; recursive?: boolean; stage?: 'index' | 'cull' | 'all'; readPlates?: boolean; readNumbers?: boolean } | { jobId: number };
export type MarkValues = { stars?: number | null; rejected?: boolean };
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
