import { filename, filterFrames, frameStars, frameExcluded } from '../review.mjs';
import type { AttributeKey, Attributes, Detection, Facet, Frame, Review, Sort, View } from './types';

export { filename, frameStars, frameExcluded };

/** What a card, facet list and search need to know about one frame, read once per review load. */
export type FrameFacts = {
  dets: Detection[]; plates: string[]; numbers: string[];
  /** The people (face clusters, by `group_key`) seen in the frame. */
  people: string[];
  plate: string | null; number: string | null; vehicle: string | null;
  colour: string | null; colourCss: string | null; team: string | null;
  panned: boolean; pick: boolean; cull: string | null; cullDetail: string | null; text: string;
  reviewed: boolean;
};
export type FacetItem = { value: string; count: number; who: string; thumb?: string | null };
export type Filters = { search: string; minStars: number; view: View; sort: Sort; facet: Facet | null };
export const defaultFilters: Filters = { search: '', minStars: 0, view: 'all', sort: 'frame', facet: null };

export function parseAttributes(d: Detection): Attributes {
  let attrs: Attributes = {};
  try {
    const parsed: unknown = JSON.parse(d.attributes || '{}');
    if (parsed && typeof parsed === 'object') attrs = parsed as Attributes;
  } catch { /* an unreadable blob is treated as no attributes */ }
  return { ...attrs, plate: attrs.plate ?? d.plate ?? undefined, race_number: attrs.race_number ?? d.number ?? undefined };
}

const first = <T,>(items: (T | null | undefined | '')[]): T | null => (items.find(Boolean) as T | undefined) ?? null;
const unique = (items: (string | null | undefined)[]): string[] => [...new Set(items.filter((v): v is string => Boolean(v)))];

/** Readable "why flagged" text for a tooltip: the cull reason plus the scores behind it. */
function cullDetail(frame: Frame, dets: Detection[]): string | null {
  if (frame.rejected) return 'Manually rejected';
  const culled = dets.find((d) => d.cull_reason);
  if (!culled) return null;
  const bits = [culled.cull_reason as string];
  if (culled.rating != null) bits.push(`rating ${culled.rating.toFixed(2)} (${culled.rating_verdict ?? 'unrated'})`);
  if (culled.sharp_end) bits.push(`sharp end: ${culled.sharp_end}`);
  if (culled.quality != null) bits.push(`quality ${culled.quality.toFixed(2)}`);
  return bits.join(' · ');
}

/** A vehicle detection with neither number nor plate read, or a shaky group match: worth a second look. */
const GROUP_AGREEMENT_LOW = 0.5;
function needsLook(x: FrameFacts | undefined): boolean {
  if (!x) return false;
  return x.dets.some((d) => {
    const isVehicle = (d.region_type ?? 'vehicle') === 'vehicle';
    if (!isVehicle) return false;
    const unread = !d.number && !d.plate;
    const shaky = d.group_agreement != null && d.group_agreement < GROUP_AGREEMENT_LOW;
    return unread || shaky;
  });
}

function factsFor(frame: Frame, dets: Detection[]): FrameFacts {
  const attrs = dets.map(parseAttributes);
  const pick = (key: AttributeKey) => first(attrs.map((a) => a[key]));
  const plates = unique(attrs.map((a) => a.plate));
  const numbers = unique(attrs.map((a) => a.race_number));
  const people = unique(dets.filter((d) => d.region_type === 'face' && d.group_key != null).map((d) => String(d.group_key)));
  const vehicle = [pick('make'), pick('model')].filter(Boolean).join(' ') || null;
  const colour = pick('colour');
  const team = pick('team');
  return {
    dets, plates, numbers, people, plate: plates[0] ?? null, number: numbers[0] ?? null, vehicle, colour,
    colourCss: first(attrs.map((a) => a.colour_hex)) ?? colour, team,
    panned: dets.some((d) => d.panning), pick: dets.some((d) => Boolean(d.burst_pick)) || Boolean(frame.burst_pick),
    reviewed: dets.length > 0 && dets.every((d) => Boolean(d.reviewed)),
    cull: frameExcluded(frame, dets) ? (frame.rejected ? 'Rejected · excluded from ML' : 'Culled · excluded from ML') : null,
    cullDetail: cullDetail(frame, dets),
    text: [frame.path, ...plates, ...numbers, vehicle, colour, team, ...attrs.map((a) => a.driver), ...attrs.map((a) => a.country), ...attrs.map((a) => a.person_name)].join(' ').toLowerCase(),
  };
}

export function buildFacts(review: Review): Map<number, FrameFacts> {
  const byFrame = new Map<number, Detection[]>();
  for (const d of review.detections) {
    const list = byFrame.get(d.image_id);
    if (list) list.push(d); else byFrame.set(d.image_id, [d]);
  }
  return new Map(review.frames.map((f) => [f.id, factsFor(f, byFrame.get(f.id) ?? [])]));
}

export function buildFacets(frames: Frame[], facts: Map<number, FrameFacts>): Record<Facet['kind'], FacetItem[]> {
  const bucket = { number: new Map<string, FacetItem>(), plate: new Map<string, FacetItem>(), person: new Map<string, FacetItem>() };
  const bump = (map: Map<string, FacetItem>, value: string, who: string | null) => {
    const item = map.get(value);
    if (item) { item.count += 1; if (!item.who && who) item.who = who; } else map.set(value, { value, count: 1, who: who ?? '' });
  };
  for (const f of frames) {
    const x = facts.get(f.id);
    if (!x) continue;
    x.numbers.forEach((n) => bump(bucket.number, n, x.vehicle));
    x.plates.forEach((p) => bump(bucket.plate, p, x.vehicle));
    for (const key of x.people) {
      const face = x.dets.find((d) => d.region_type === 'face' && String(d.group_key) === key);
      bump(bucket.person, key, face?.known_person_match?.name ?? null);
      const item = bucket.person.get(key);
      if (item && !item.thumb) item.thumb = face?.crop_path ?? null;
    }
  }
  return {
    number: [...bucket.number.values()].sort((a, b) => Number(a.value) - Number(b.value) || a.value.localeCompare(b.value)),
    plate: [...bucket.plate.values()].sort((a, b) => a.value.localeCompare(b.value)),
    person: [...bucket.person.values()].sort((a, b) => b.count - a.count || Number(a.value) - Number(b.value)),
  };
}

/** Frames that survive the toolbar. */
export function visibleFrames(frames: Frame[], facts: Map<number, FrameFacts>, f: Filters): Frame[] {
  const needle = f.search.trim().toLowerCase();
  const keep = (frame: Frame) => {
    const x = facts.get(frame.id);
    if (needle && !x?.text.includes(needle)) return false;
    if (f.facet && !(f.facet.kind === 'number' ? x?.numbers : f.facet.kind === 'plate' ? x?.plates : x?.people)?.includes(f.facet.value)) return false;
    if (f.view === 'rejected') return Boolean(x?.cull);
    if (f.view === 'kept') return !x?.cull;
    if (f.view === 'review') return !x?.cull && !x?.reviewed;
    if (f.view === 'picks') return !x?.cull && Boolean(x?.pick);
    if (f.view === 'needsLook') return !x?.cull && needsLook(x);
    return true;
  };
  const shown = filterFrames(frames, '', f.minStars, 'all').filter(keep);
  const ordered = f.sort === 'frame' ? shown : sortFrames(shown, f.sort, facts);
  return ordered.sort((a, b) => Number(Boolean(facts.get(a.id)?.cull)) - Number(Boolean(facts.get(b.id)?.cull)));
}

const uncertainty = (f: Frame) => (f.manual_stars != null ? 10 : Math.abs(frameStars(f) - 3));
const byName = (a: Frame, b: Frame) => filename(a.path).localeCompare(filename(b.path), undefined, { numeric: true });

export function sortFrames(frames: Frame[], sort: Sort, facts: Map<number, FrameFacts>): Frame[] {
  const list = [...frames];
  if (sort === 'review') return list.sort((a, b) => uncertainty(a) - uncertainty(b));
  if (sort === 'best') return list.sort((a, b) => frameStars(b) - frameStars(a));
  if (sort === 'worst') return list.sort((a, b) => frameStars(a) - frameStars(b));
  if (sort === 'pick') return list.sort((a, b) => Number(facts.get(b.id)?.pick) - Number(facts.get(a.id)?.pick) || frameStars(b) - frameStars(a));
  return list.sort(byName);
}

/** Pixels of the full frame, or already fractions; both come out as 0..1. */
function toFraction([x1, y1, x2, y2]: [number, number, number, number], frame: Pick<Frame, 'width' | 'height'>): [number, number, number, number] | null {
  const fractional = x2 <= 1.0001 && y2 <= 1.0001;
  const w = fractional ? 1 : frame.width, h = fractional ? 1 : frame.height;
  if (!w || !h) return null;
  const c = (v: number) => Math.max(0, Math.min(1, v));
  return [c(x1 / w), c(y1 / h), c(x2 / w), c(y2 / h)];
}

/** Detection boxes are pixels of the full frame, or already fractions; both come out as 0..1. */
export function boxOf(d: Detection, frame: Pick<Frame, 'width' | 'height'>): [number, number, number, number] | null {
  return toFraction([d.x1, d.y1, d.x2, d.y2], frame);
}

/** The plate box a vehicle detection's `attributes` carries (full-frame pixels), or null when none was found. */
export function plateBoxOf(d: Detection, frame: Pick<Frame, 'width' | 'height'>): [number, number, number, number] | null {
  try {
    const box = (JSON.parse(d.attributes || '{}') as { plate_bbox?: [number, number, number, number] }).plate_bbox;
    return Array.isArray(box) && box.length === 4 ? toFraction(box, frame) : null;
  } catch {
    return null;
  }
}

/** `frames`, split into consecutive burst groups (a frame with no `burst_key` is its own single-frame group). Order follows `frames`. */
export function burstGroups(frames: Frame[]): Frame[][] {
  const order: string[] = [];
  const map = new Map<string, Frame[]>();
  for (const f of frames) {
    const key = f.burst_key == null ? `frame-${f.id}` : `burst-${f.burst_key}`;
    const list = map.get(key);
    if (list) list.push(f); else { map.set(key, [f]); order.push(key); }
  }
  return order.map((key) => map.get(key)!);
}

/** The burst's keeper: the flagged pick, or its first frame. */
export function burstPickOf(group: Frame[], facts: Map<number, FrameFacts>): Frame {
  return group.find((f) => facts.get(f.id)?.pick) ?? group[0];
}

export type Verdict = 'good' | 'fair' | 'poor' | 'none';
export const verdictOf = (stars: number): Verdict => (stars >= 4 ? 'good' : stars === 3 ? 'fair' : stars > 0 ? 'poor' : 'none');
