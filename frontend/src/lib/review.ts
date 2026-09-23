import { filename, filterFrames, frameStars } from '../review.mjs';
import type { AttributeKey, Attributes, Detection, Facet, Frame, Review, Sort, View } from './types';

export { filename, frameStars };

/** What a card, facet list and search need to know about one frame, read once per review load. */
export type FrameFacts = {
  dets: Detection[]; plates: string[]; numbers: string[];
  plate: string | null; number: string | null; vehicle: string | null;
  colour: string | null; colourCss: string | null; team: string | null;
  panned: boolean; pick: boolean; cull: string | null; text: string;
  reviewed: boolean;
};
export type FacetItem = { value: string; count: number; who: string };
export type Filters = { search: string; minStars: number; view: View; sort: Sort; facet: Facet | null };
export const defaultFilters: Filters = { search: '', minStars: 0, view: 'review', sort: 'frame', facet: null };

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

function factsFor(frame: Frame, dets: Detection[]): FrameFacts {
  const attrs = dets.map(parseAttributes);
  const pick = (key: AttributeKey) => first(attrs.map((a) => a[key]));
  const plates = unique(attrs.map((a) => a.plate));
  const numbers = unique(attrs.map((a) => a.race_number));
  const vehicle = [pick('make'), pick('model')].filter(Boolean).join(' ') || null;
  const colour = pick('colour');
  const team = pick('team');
  return {
    dets, plates, numbers, plate: plates[0] ?? null, number: numbers[0] ?? null, vehicle, colour,
    colourCss: first(attrs.map((a) => a.colour_hex)) ?? colour, team,
    panned: dets.some((d) => d.panning), pick: dets.some((d) => d.burst_pick),
    reviewed: dets.length > 0 && dets.every((d) => Boolean(d.reviewed)),
    cull: first(dets.map((d) => d.cull_reason)),
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
  const bucket = { number: new Map<string, FacetItem>(), plate: new Map<string, FacetItem>() };
  const bump = (map: Map<string, FacetItem>, value: string, who: string | null) => {
    const item = map.get(value);
    if (item) { item.count += 1; if (!item.who && who) item.who = who; } else map.set(value, { value, count: 1, who: who ?? '' });
  };
  for (const f of frames) {
    const x = facts.get(f.id);
    if (!x) continue;
    x.numbers.forEach((n) => bump(bucket.number, n, x.vehicle));
    x.plates.forEach((p) => bump(bucket.plate, p, x.vehicle));
  }
  return {
    number: [...bucket.number.values()].sort((a, b) => Number(a.value) - Number(b.value) || a.value.localeCompare(b.value)),
    plate: [...bucket.plate.values()].sort((a, b) => a.value.localeCompare(b.value)),
  };
}

/** Frames that survive the toolbar. */
export function visibleFrames(frames: Frame[], facts: Map<number, FrameFacts>, f: Filters): Frame[] {
  const needle = f.search.trim().toLowerCase();
  const keep = (frame: Frame) => {
    const x = facts.get(frame.id);
    if (needle && !x?.text.includes(needle)) return false;
    if (f.facet && !(f.facet.kind === 'number' ? x?.numbers : x?.plates)?.includes(f.facet.value)) return false;
    if (f.view === 'review') return !frame.rejected && !x?.reviewed;
    if (f.view === 'picks') return !frame.rejected && Boolean(x?.pick);
    return true;
  };
  const reject = f.view === 'rejected' ? 'rejected' : 'all';
  const shown = filterFrames(frames, '', f.minStars, reject).filter(keep);
  return f.sort === 'frame' ? shown : sortFrames(shown, f.sort, facts);
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

/** Detection boxes are pixels of the full frame, or already fractions; both come out as 0..1. */
export function boxOf(d: Detection, frame: Pick<Frame, 'width' | 'height'>): [number, number, number, number] | null {
  const fractional = d.x2 <= 1.0001 && d.y2 <= 1.0001;
  const w = fractional ? 1 : frame.width, h = fractional ? 1 : frame.height;
  if (!w || !h) return null;
  const c = (v: number) => Math.max(0, Math.min(1, v));
  return [c(d.x1 / w), c(d.y1 / h), c(d.x2 / w), c(d.y2 / h)];
}

export type Verdict = 'good' | 'fair' | 'poor' | 'none';
export const verdictOf = (stars: number): Verdict => (stars >= 4 ? 'good' : stars === 3 ? 'fair' : stars > 0 ? 'poor' : 'none');
