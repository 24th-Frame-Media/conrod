import { burstTopRanks, filename, filterFrames, flawWords, focusSharp, frameFlaw, frameStars, frameExcluded, subjectExcluded } from '../review.mjs';
import { decisionOf, groupKeyOf, keeperIds } from './stacks.mjs';
import { tasteOrder } from './taste.mjs';
import { sureTags } from './moments.mjs';
import type { AttributeKey, Attributes, Detection, Facet, Frame, Review, Sort, View } from './types';

export { filename, frameStars, frameExcluded, decisionOf, keeperIds };

/** What a card, facet list and search need to know about one frame, read once per review load. */
export type FrameFacts = {
  dets: Detection[]; plates: string[]; numbers: string[];
  /** Make and model, team, and subject classes (animals and the like) seen in the frame. */
  makes: string[]; teams: string[]; subjects: string[];
  /** The shoot's moments the judge was sure of (see `moments.mjs`), for the facet list. */
  moments: string[];
  /** The people (face clusters, by `group_key`) seen in the frame. */
  people: string[];
  plate: string | null; number: string | null; vehicle: string | null;
  colour: string | null; colourCss: string | null; team: string | null;
  panned: boolean; pick: boolean; cull: string | null; cullDetail: string | null;
  /** A frame the detector found no vehicle in that the camera's sharp focus area keeps. */
  keepNote: string | null;
  /** One plain sentence: the main car and why this frame was kept or set aside. */
  why: string | null;
  text: string;
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
  const flaw = frameFlaw(frame);
  if (flaw) return `${flaw}${frame.rating != null ? ` · rating ${frame.rating.toFixed(2)}` : ''}`;
  const culled = dets.find((d) => d.cull_reason);
  if (!culled) return null;
  const bits = [culled.cull_reason as string];
  if (culled.rating != null) bits.push(`rating ${culled.rating.toFixed(2)} (${culled.rating_verdict ?? 'unrated'})`);
  if (culled.sharp_end) bits.push(`sharp end: ${culled.sharp_end}`);
  if (culled.quality != null) bits.push(`quality ${culled.quality.toFixed(2)}`);
  return bits.join(' · ');
}

/** The detection the frame is about: a vehicle, the burst pick first, then the sharpest. */
function mainDetection(dets: Detection[]): Detection | null {
  if (!dets.length) return null;
  const vehicles = dets.filter((d) => (d.region_type ?? 'vehicle') === 'vehicle' && !d.bystander);
  const pool = vehicles.length ? vehicles : dets.filter((d) => !d.bystander);
  const candidates = pool.length ? pool : dets;
  return candidates.find((d) => d.burst_pick) ?? [...candidates].sort((a, b) => (b.rating ?? b.sharpness) - (a.rating ?? a.sharpness))[0];
}

/** "the #32 car" / "the Porsche 911" / "the car", from what was read. */
function subjectName(d: Detection): string {
  const attrs = parseAttributes(d);
  const number = attrs.race_number ?? d.number;
  if (number) return `the #${number} car`;
  const make = [attrs.make, attrs.model].filter(Boolean).join(' ');
  if (make) return `the ${make}`;
  const plate = attrs.plate ?? d.plate;
  if (plate) return `the car with plate ${plate}`;
  if (d.region_type === 'face') return 'the face';
  return (d.region_type ?? 'vehicle') === 'vehicle' ? 'the car' : 'the subject';
}

/** One plain sentence for a culled subject's stored reason; never says "culled". */
function cullWords(reason: string, name: string, d: Detection, frame: Frame): string {
  const r = reason.trim().toLowerCase();
  if (r === 'eyes closed') return 'The eyes are closed.';
  if (r === 'too blurred' || r.endsWith(' soft') || /^\d star/.test(r)) {
    const soft = (d.rating ?? d.sharpness) < 0.606;
    if (soft && focusSharp(frame)) return `Soft: focus landed elsewhere, not on ${name}.`;
    return `Soft: ${name} is out of focus.`;
  }
  if (r.includes('edge') || r.includes('cut off')) return `${name[0].toUpperCase()}${name.slice(1)} is cut off at the frame edge.`;
  return `${name[0].toUpperCase()}${name.slice(1)}: ${reason.trim()}.`;
}

/**
 * The inspector's one plain line: which car the frame is about, and why it was kept or set
 * aside. Read from fields the engine already stores (`burst_pick`, `rating_verdict`,
 * `cull_reason`, `sharpness`, the frame's AF reading); a sharp main car is never called culled.
 */
function whyNote(frame: Frame, dets: Detection[], burstSize: number): string | null {
  if (frame.rejected) return 'You rejected this frame.';
  const flaw = frameFlaw(frame);
  if (flaw) return flawWords(flaw);
  const main = mainDetection(dets);
  const culled = frameExcluded(frame, dets);
  if (culled) {
    const det = dets.find((d) => d.cull_reason && !d.bystander) ?? dets.find((d) => d.cull_reason);
    if (det?.cull_reason) return cullWords(det.cull_reason, subjectName(det), det, frame);
    return main ? `No sharp photo of ${subjectName(main)} in this frame.` : 'Nothing in this frame is sharp enough.';
  }
  if (!main) return dets.length ? 'No car found — kept because focus is sharp' : null;
  const name = subjectName(main);
  const panned = dets.some((d) => d.panning);
  const verdict = main.rating_verdict ?? '';
  const pick = Boolean(main.burst_pick || frame.burst_pick);
  if (pick) return burstSize > 1 ? `Sharp photo of ${name}, best of its burst.` : `Sharp photo of ${name}.`;
  if (verdict === 'good') return `Sharp photo of ${name}.`;
  if (panned) return `Kept: ${name} is blurred against a panning background.`;
  return `Kept: ${name} is sharp enough.`;
}

const GROUP_AGREEMENT_LOW = 0.5;
/** A vehicle detection with neither number nor plate read, or a shaky group match: worth a second look. */
function needsLook(x: FrameFacts | undefined): boolean {
  if (!x) return false;
  return x.dets.some((d) => {
    const isVehicle = (d.region_type ?? 'vehicle') === 'vehicle';
    if (!isVehicle || d.bystander) return false;
    const unread = !d.number && !d.plate;
    const shaky = d.group_agreement != null && d.group_agreement < GROUP_AGREEMENT_LOW;
    return unread || shaky;
  });
}

const NOT_SUBJECT = new Set(['person', 'face', 'eye', 'plate']);

function factsFor(frame: Frame, dets: Detection[], burstSize: number): FrameFacts {
  const attrs = dets.map(parseAttributes);
  const pick = (key: AttributeKey) => first(attrs.map((a) => a[key]));
  const plates = unique(attrs.map((a) => a.plate));
  const numbers = unique(attrs.map((a) => a.race_number));
  const people = unique(dets.filter((d) => d.region_type === 'face' && d.group_key != null).map((d) => String(d.group_key)));
  const vehicle = [pick('make'), pick('model')].filter(Boolean).join(' ') || null;
  const colour = pick('colour');
  const team = pick('team');
  const makes = unique(attrs.map((a) => [a.make, a.model].filter(Boolean).join(' ')));
  const teams = unique(attrs.map((a) => a.team));
  const subjects = unique(dets.filter((d) => !NOT_SUBJECT.has((d.cls || '').toLowerCase()) && d.region_type !== 'face' && d.region_type !== 'eye').map((d) => (d.cls || '').toLowerCase()));
  const active = dets.filter((d) => !d.bystander);
  const keptByFocus = !frame.rejected && focusSharp(frame) && dets.length > 0 && dets.every((d) => subjectExcluded(frame, d));
  return {
    dets, plates, numbers, makes, teams, subjects, people, moments: sureTags(frame.moments), plate: plates[0] ?? null, number: numbers[0] ?? null, vehicle, colour,
    colourCss: first(attrs.map((a) => a.colour_hex)) ?? colour, team,
    panned: dets.some((d) => d.panning), pick: dets.some((d) => Boolean(d.burst_pick)) || Boolean(frame.burst_pick),
    reviewed: active.length > 0 && active.every((d) => Boolean(d.reviewed)),
    cull: frameExcluded(frame, dets) ? (frame.rejected ? 'Rejected · excluded from ML' : 'Culled · excluded from ML') : null,
    cullDetail: cullDetail(frame, dets),
    keepNote: keptByFocus ? 'No car found — kept because focus is sharp' : null,
    why: whyNote(frame, dets, burstSize),
    text: [frame.path, ...plates, ...numbers, vehicle, colour, team, ...attrs.map((a) => a.driver), ...attrs.map((a) => a.country), ...attrs.map((a) => a.person_name)].join(' ').toLowerCase(),
  };
}

export function buildFacts(review: Review): Map<number, FrameFacts> {
  const byFrame = new Map<number, Detection[]>();
  for (const d of review.detections) {
    const list = byFrame.get(d.image_id);
    if (list) list.push(d); else byFrame.set(d.image_id, [d]);
  }
  // How many frames share each burst: "best of its burst" only when there is a burst to be best of.
  const burstSizes = new Map<number, number>();
  for (const f of review.frames) {
    if (f.burst_key == null) continue;
    burstSizes.set(f.burst_key, (burstSizes.get(f.burst_key) ?? 0) + 1);
  }
  return new Map(review.frames.map((f) => [f.id, factsFor(f, byFrame.get(f.id) ?? [], f.burst_key == null ? 1 : burstSizes.get(f.burst_key) ?? 1)]));
}

export function buildFacets(frames: Frame[], facts: Map<number, FrameFacts>): Record<Facet['kind'], FacetItem[]> {
  const bucket = { number: new Map<string, FacetItem>(), plate: new Map<string, FacetItem>(), person: new Map<string, FacetItem>(), make: new Map<string, FacetItem>(), team: new Map<string, FacetItem>(), subject: new Map<string, FacetItem>(), moment: new Map<string, FacetItem>() };
  // One number can belong to several cars (categories share numbers), so a
  // number's row says how many distinct cars it covers rather than naming one.
  const vehiclesByNumber = new Map<string, Set<string>>();
  const bump = (map: Map<string, FacetItem>, value: string, who: string | null) => {
    const item = map.get(value);
    if (item) { item.count += 1; if (!item.who && who) item.who = who; } else map.set(value, { value, count: 1, who: who ?? '' });
  };
  for (const f of frames) {
    const x = facts.get(f.id);
    if (!x) continue;
    x.numbers.forEach((n) => {
      bump(bucket.number, n, x.vehicle);
      if (x.vehicle) {
        const cars = vehiclesByNumber.get(n) ?? new Set<string>();
        cars.add(x.vehicle);
        vehiclesByNumber.set(n, cars);
      }
    });
    x.plates.forEach((p) => bump(bucket.plate, p, x.vehicle));
    x.makes.forEach((m) => bump(bucket.make, m, null));
    x.teams.forEach((t) => bump(bucket.team, t, null));
    x.subjects.forEach((s) => bump(bucket.subject, s, null));
    x.moments.forEach((m) => bump(bucket.moment, m, null));
    for (const key of x.people) {
      const face = x.dets.find((d) => d.region_type === 'face' && String(d.group_key) === key);
      bump(bucket.person, key, face?.known_person_match?.name ?? null);
      const item = bucket.person.get(key);
      if (item && !item.thumb) item.thumb = face?.crop_path ?? null;
    }
  }
  for (const [number, cars] of vehiclesByNumber) {
    const item = bucket.number.get(number);
    if (item && cars.size > 1) {
      const names = [...cars].sort();
      item.who = `${names[0]} +${names.length - 1}`;
    }
  }
  return {
    number: [...bucket.number.values()].sort((a, b) => Number(a.value) - Number(b.value) || a.value.localeCompare(b.value)),
    plate: [...bucket.plate.values()].sort((a, b) => a.value.localeCompare(b.value)),
    person: [...bucket.person.values()].sort((a, b) => b.count - a.count || Number(a.value) - Number(b.value)),
    make: [...bucket.make.values()].sort((a, b) => b.count - a.count || a.value.localeCompare(b.value)),
    team: [...bucket.team.values()].sort((a, b) => b.count - a.count || a.value.localeCompare(b.value)),
    subject: [...bucket.subject.values()].sort((a, b) => b.count - a.count || a.value.localeCompare(b.value)),
    moment: [...bucket.moment.values()].sort((a, b) => b.count - a.count || a.value.localeCompare(b.value)),
  };
}

function facetValues(x: FrameFacts | undefined, kind: Facet['kind']): string[] | undefined {
  if (!x) return undefined;
  return { number: x.numbers, plate: x.plates, make: x.makes, team: x.teams, person: x.people, subject: x.subjects, moment: x.moments }[kind];
}

/**
 * Frames that survive the toolbar, in display order. This is the one filtered model: stacks, the filmstrip,
 * counts and keyboard steps are all built from this list, so none of them can show a frame the filter hides.
 * Keepers are worked out on the whole album first (a filter must not change which frame is a burst's keeper).
 * Rejected frames keep their place, so marking never reshuffles the list under the selection.
 */
export function visibleFrames(frames: Frame[], facts: Map<number, FrameFacts>, f: Filters, keepers: Set<number> = keeperIds(frames, facts)): Frame[] {
  const needle = f.search.trim().toLowerCase();
  const top3 = f.view === 'top3' ? burstRankMap(frames, keepers) : null;
  const keep = (frame: Frame) => {
    const x = facts.get(frame.id);
    if (needle && !x?.text.includes(needle)) return false;
    if (f.facet && !facetValues(x, f.facet.kind)?.includes(f.facet.value)) return false;
    if (f.view === 'rejected') return decisionOf(frame, x) === 'rejected';
    if (f.view === 'undecided') return decisionOf(frame, x) === 'undecided';
    if (f.view === 'keepers') return keepers.has(frame.id);
    if (f.view === 'review') return !x?.cull && !x?.reviewed;
    if (f.view === 'top3') return Boolean(top3?.has(frame.id));
    if (f.view === 'needsLook') return !x?.cull && needsLook(x);
    return true;
  };
  const shown = filterFrames(frames, '', f.minStars, 'all').filter(keep);
  return f.sort === 'frame' ? shown.sort(byName) : sortFrames(shown, f.sort, keepers);
}

const uncertainty = (f: Frame) => (f.manual_stars != null ? 10 : Math.abs(frameStars(f) - 3));
const byName = (a: Frame, b: Frame) => filename(a.path).localeCompare(filename(b.path), undefined, { numeric: true }) || a.id - b.id;

function sortFrames(frames: Frame[], sort: Sort, keepers: Set<number>): Frame[] {
  const list = [...frames];
  if (sort === 'review') return list.sort((a, b) => uncertainty(a) - uncertainty(b));
  if (sort === 'best') return list.sort((a, b) => frameStars(b) - frameStars(a) || byName(a, b));
  if (sort === 'worst') return list.sort((a, b) => frameStars(a) - frameStars(b) || byName(a, b));
  if (sort === 'pick') return list.sort((a, b) => Number(keepers.has(b.id)) - Number(keepers.has(a.id)) || frameStars(b) - frameStars(a) || byName(a, b));
  if (sort === 'taste') return tasteOrder(list);
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

/** Vehicles as groups of frames (identity, visual group or burst while still identifying), each limited to `visible` frames. */
export function vehicleGroups(review: Review, visible: Set<number>, identifying: boolean): { key: string; frames: Frame[]; total: number; title: string; subtitle: string }[] {
  const byId = new Map(review.frames.map((f) => [f.id, f]));
  const groups = new Map<string, Detection[]>();
  for (const detection of review.detections) {
    if ((detection.region_type ?? 'vehicle') !== 'vehicle') continue;
    const attrs = parseAttributes(detection);
    const identity = attrs.plate || (attrs.race_number && `${attrs.race_number}-${attrs.make ?? ''}-${attrs.model ?? ''}`);
    const frame = byId.get(detection.image_id);
    const key = detection.group_key != null ? `group-${detection.group_key}`
      : identity ? `identity-${identity}`
      : identifying && frame?.burst_key != null ? `processing-burst-${frame.burst_key}`
      : `single-${detection.id}`;
    const list = groups.get(key); if (list) list.push(detection); else groups.set(key, [detection]);
  }
  const out: { key: string; frames: Frame[]; total: number; title: string; subtitle: string }[] = [];
  for (const [key, detections] of groups) {
    const all = [...new Set(detections.map((d) => d.image_id))].map((id) => byId.get(id)).filter((f): f is Frame => Boolean(f));
    const frames = all.filter((f) => visible.has(f.id));
    if (!frames.length) continue;
    const attrs = parseAttributes(detections[0]);
    const processing = identifying && key.startsWith('processing-');
    const title = processing ? 'Being identified' : [attrs.make, attrs.model].filter(Boolean).join(' ') || (attrs.plate || (attrs.race_number && `#${attrs.race_number}`) || 'Unidentified vehicle');
    const subtitle = processing
      ? `Analysing ${all.length} photos`
      : `${all.length} shot${all.length === 1 ? '' : 's'}${attrs.plate ? ` · ${attrs.plate}` : ''}${attrs.race_number ? ` · #${attrs.race_number}` : ''}${attrs.team ? ` · ${attrs.team}` : ''}`;
    out.push({ key, frames, total: all.length, title, subtitle });
  }
  return out;
}

/** Candidate rank 1..3 per frame id for one burst; the keeper is always rank 1. Frames beyond 3 are absent. */
function burstRanksOf(group: Frame[], keepers: Set<number>): Map<number, number> {
  return burstTopRanks(group, (f: Frame) => keepers.has(f.id));
}

/** Candidate ranks for every burst in `frames`, keyed by frame id. */
export function burstRankMap(frames: Frame[], keepers: Set<number>): Map<number, number> {
  const groups = new Map<string, Frame[]>();
  for (const f of frames) {
    const key = groupKeyOf(f);
    const list = groups.get(key);
    if (list) list.push(f); else groups.set(key, [f]);
  }
  const ranks = new Map<number, number>();
  for (const group of groups.values()) for (const [id, rank] of burstRanksOf(group, keepers)) ranks.set(id, rank);
  return ranks;
}

export type Verdict = 'good' | 'fair' | 'poor' | 'none';
export const verdictOf = (stars: number): Verdict => (stars >= 4 ? 'good' : stars === 3 ? 'fair' : stars > 0 ? 'poor' : 'none');
