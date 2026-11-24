// The shape of the Settings page: tabs, sections, field labels and the small pure
// helpers the page needs (search, dirty state, per-album override counts).
// Setting names are the engine's wire format and must never change here.

export const TABS = [
  { id: 'culling', label: 'Culling', hint: 'What counts as a keeper' },
  { id: 'identification', label: 'Identification', hint: 'Plates, numbers and photo reading' },
  { id: 'workflow', label: 'Workflow', hint: 'Stage order and parallel execution' },
  { id: 'metadata', label: 'Metadata', hint: 'What is written next to your files' },
  { id: 'lightroom', label: 'Lightroom', hint: 'Stars, picks and labels in your files' },
  { id: 'system', label: 'System', hint: 'Models, cache and library data' },
];

/** Default order of the three Identify stages: OCR, then Decision, then Vision. */
export const DEFAULT_IDENTIFY_ORDER = ['ocr', 'decision', 'vision'];

/** One line shown wherever parallel identify is offered: sequential is the default, and why. */
export const PARALLEL_NOTE = 'Parallel can change results and is slower to verify. Sequential OCR, Decision, Vision is the default.';

/** Normalise a configured Identify stage order into a valid 3-stage permutation of ['ocr', 'decision', 'vision']. */
export function normalizeIdentifyOrder(order) {
  const out = [];
  if (Array.isArray(order)) {
    for (const raw of order) {
      const key = String(raw ?? '').trim().toLowerCase().replace(/^identify:/, '');
      if (DEFAULT_IDENTIFY_ORDER.includes(key) && !out.includes(key)) out.push(key);
    }
  }
  for (const def of DEFAULT_IDENTIFY_ORDER) {
    if (!out.includes(def)) out.push(def);
  }
  return out;
}

/** Move `stage` up (`delta = -1`) or down (`delta = 1`) in the Identify stage order. */
export function moveIdentifyStage(order, stage, delta) {
  const list = normalizeIdentifyOrder(order);
  const from = list.indexOf(stage);
  const to = from + delta;
  if (from < 0 || to < 0 || to >= list.length) return list;
  const next = [...list];
  [next[from], next[to]] = [next[to], next[from]];
  return next;
}

/** Fields in page order. `kind` picks the control; `album` marks a per-album override. */
export const FIELDS = {
  // ── culling ──
  blur_strictness: {
    label: 'Blur strictness', kind: 'segmented', album: true,
    hint: 'How much softness to accept before a photo is rated soft or culled. Moderate suits long-lens and heat-haze shots; strict flags any softness.',
    options: [{ id: 'critical', name: 'Critical' }, { id: 'strict', name: 'Strict' }, { id: 'moderate', name: 'Moderate' }, { id: 'lenient', name: 'Lenient' }],
  },
  burst_gap: {
    label: 'Burst gap', kind: 'number', album: true, unit: 's',
    hint: 'Frames taken closer together than this belong to one pass.',
  },
  sharp_at: {
    label: 'Sharp at', kind: 'number',
    hint: 'A subject scoring at or above this counts as sharp.',
  },
  blurred_below: {
    label: 'Blurred below', kind: 'number',
    hint: 'A subject scoring below this is treated as blurred.',
  },
  auto_reject_below_stars: {
    label: 'Auto-reject below (stars)', kind: 'number',
    hint: '0 never rejects on its own.',
  },
  cull_blurred: {
    label: 'Cull blurred frames', kind: 'toggle',
    hint: 'A red rating, never a deletion.',
  },
  cull_exposure: {
    label: 'Cull blown or crushed frames', kind: 'toggle',
    hint: 'Frames blown out or crushed to black beyond what a RAW can recover get a red rating, never a deletion. Night, stage and high-key shoots are exempt.',
  },
  scan_profile: {
    label: 'Default shoot type', kind: 'profile', album: true,
    hint: 'The import sheet starts on this. Each album can choose another.',
  },
  include_cars: { label: 'Cars', kind: 'toggle' },
  include_bikes: { label: 'Bikes', kind: 'toggle' },
  include_trucks: { label: 'Trucks', kind: 'toggle' },
  include_people: {
    label: 'People', kind: 'toggle',
    hint: 'Find people for culling, faces and crowds.',
  },
  detect_conf: {
    label: 'Detection confidence', kind: 'number',
    hint: 'Lower finds more vehicles, and more false ones.',
  },
  min_box_fraction: {
    label: 'Smallest subject', kind: 'number',
    hint: 'As a fraction of the frame. Smaller values find more distant subjects.',
  },
  max_vehicles_per_frame: {
    label: 'Vehicles per frame', kind: 'number',
    hint: 'Ignore extra subjects beyond this in one frame.',
  },

  // ── identification ──
  moments_backend: {
    label: 'Judge moments with', kind: 'segmented',
    hint: 'The vision model names tags it is sure of but has no real probability, so its readings are marked unverified. The decision model answers calibrated probabilities: on this PC or through OpenRouter, as chosen under AI models below.',
    options: [{ id: 'vlm', name: 'Vision model' }, { id: 'decider', name: 'Decision model' }],
  },
  moments_threshold: {
    label: 'Moment confidence', kind: 'number', album: true,
    hint: 'A moment must be this likely (0.5 to 1) before it is shown as sure or can lift a frame. Below it, down to 0.5, it shows as maybe and never changes ranking.',
  },
  moments_transport: {
    label: 'Where it runs', kind: 'segmented',
    hint: 'Local uses this PC’s graphics card, with no account or internet. OpenCode sends each question to JEV, its decision model, in the cloud, charged per use to your OpenCode credits. OpenRouter sends them to a smaller decision model.',
    options: [{ id: 'auto', name: 'Auto' }, { id: 'in_process', name: 'Local (this PC)' }, { id: 'opencode', name: 'OpenCode (JEV)' }, { id: 'openrouter', name: 'OpenRouter' }],
  },
  moments_openrouter_model: { label: 'OpenRouter model', kind: 'text', hint: 'Model id used for the decision model through OpenRouter, e.g. perplexity/pplx-decider-flash-v1.3:free.' },
  moments_openrouter_key: { label: 'OpenRouter key', kind: 'text', hint: 'Shared with an OpenRouter vision service if you add one. Stored in your settings file; never written to logs.' },
  moments_opencode_model: {
    label: 'JEV model', kind: 'segmented',
    hint: 'JEV is OpenCode’s decision model and reads a photo as well as text. Paid is charged to your Console credits ($0.042 per million input tokens; output free). Free costs nothing but OpenCode only allows it after you turn on free endpoints that train on request data; the paid model works straight away.',
    options: [{ id: 'jev-1.13-free', name: 'JEV Free' }, { id: 'jev-1.13', name: 'JEV Paid' }],
  },
  moments_opencode_key: { label: 'OpenCode key', kind: 'text', hint: 'The OpenCode Console key, charged per use to your pay-as-you-go credits. Do not paste an OpenCode Go coding-subscription key. Leave blank to reuse the key of an OpenCode vision service. Stored in your settings file; never written to logs.' },
  read_plates: {
    label: 'Read licence plates', kind: 'toggle', album: true,
    hint: 'OCR registration plates.',
  },
  read_numbers: {
    label: 'Read race numbers', kind: 'toggle', album: true,
    hint: 'Competition numbers on cars and bikes.',
  },
  read_text: {
    label: 'Read other text', kind: 'toggle',
    hint: 'Signage, sponsor boards and anything else in frame.',
  },
  read_action: {
    label: 'Read action', kind: 'toggle', album: true,
    hint: 'Dust, tyre smoke, airborne, flames and contact, from the vision model.',
  },
  use_known_vehicles: {
    label: 'Use known vehicles', kind: 'toggle',
    hint: 'Fill blanks from cars you have met before.',
  },
  normalise_names: {
    label: 'Tidy make and model names', kind: 'toggle',
    hint: 'Spelling and capitalisation are normalised.',
  },
  entry_list: {
    label: 'Entry list', kind: 'text',
    hint: 'Optional CSV or JSON (number, team, drivers, make, model, sponsors). Team and drivers are filled from the car number instead of guessed.',
  },
  vlm_strategy: {
    label: 'Load balancing', kind: 'segmented',
    hint: 'How work is spread across the enabled providers.',
    options: [{ id: 'least_busy', name: 'Least busy' }, { id: 'round_robin', name: 'Round robin' }, { id: 'random', name: 'Random' }],
  },
  moments_strategy: {
    label: 'How work is shared', kind: 'segmented',
    hint: 'How moment questions are spread across the enabled services. Least busy uses the shortest queue; round robin takes turns.',
    options: [{ id: 'least_busy', name: 'Least busy' }, { id: 'round_robin', name: 'Round robin' }, { id: 'random', name: 'Random' }],
  },
  moments_vlm_fallback: {
    label: 'Use the car reader if no moment service answers', kind: 'toggle',
    hint: 'When every moment service fails, let the car reader judge the frame. Its readings stay unverified.',
  },
  use_vlm: {
    label: 'Use a vision model', kind: 'toggle',
    hint: 'Ask a vision model for make, model, colour and team.',
  },
  vlm_reasoning: {
    label: 'Extended thinking', kind: 'toggle',
    hint: 'Some models think step by step before answering. Off keeps car reading fast and cheap. Not all services support this.',
  },
  use_decider: {
    label: 'Use the decision model', kind: 'toggle',
    hint: "Runs the Perplexity decision model to answer yes/no questions about a frame. Off, or not enough video memory: every scan behaves exactly as without it.",
  },
  use_quality_score: {
    label: 'Score sharpness/quality', kind: 'toggle',
    hint: 'Scores each subject for technical quality and uses it only to break a tie between equally sharp frames. Opt-in: it needs its own small model download.',
  },
  decider_quant: {
    label: 'Precision', kind: 'segmented',
    hint: 'Auto picks the largest that fits your free video memory: int8 is closest to the reference, int4 needs about 2 GB less.',
    options: [{ id: 'auto', name: 'Auto' }, { id: 'int8', name: 'int8' }, { id: 'int4', name: 'int4' }],
  },
  vlm_timeout: { label: 'Timeout', kind: 'number', unit: 's', hint: 'Give up on one request after this long.' },
  vlm_max_retries: { label: 'Retries', kind: 'number', hint: 'How many times to retry a failed request.' },
  vlm_input_edge: {
    label: 'Longest image edge', kind: 'number', unit: 'px',
    hint: 'Images are resized so their longest edge fits this before upload.',
  },

  // ── workflow ──
  identify_parallel: {
    label: 'Run OCR, Decider and Vision at the same time', kind: 'toggle',
    hint: 'Runs CPU OCR, the decision model and the vision model concurrently when your hardware allows it. On a single GPU that fits only one local model at a time, CPU OCR still runs in parallel while Decider and Vision take turns. Parallel can change results and is slower to verify.',
  },
  identify_order: {
    label: 'Identify stage order', kind: 'workflow',
    hint: 'Order in which OCR, Decider and Vision run sequentially (and the GPU handoff order when a card runs local models serially).',
  },

  // ── metadata ──
  write_rating: { label: 'Write star rating', kind: 'toggle', hint: 'An XMP rating that Lightroom reads.' },
  write_label: { label: 'Write colour label', kind: 'toggle' },
  write_caption: { label: 'Write caption', kind: 'toggle' },
  write_plate_keyword: { label: 'Write plate as a keyword', kind: 'toggle' },
  write_sidecar_for_raw: { label: 'Write sidecars next to RAW files', kind: 'toggle' },
  keyword_prefix: { label: 'Keyword prefix', kind: 'text', hint: 'Prepended to every keyword Conrod writes.' },
  overwrite_rating: { label: 'Overwrite existing stars', kind: 'toggle', hint: 'Off keeps the rating already in the file.' },
  overwrite_label: { label: 'Overwrite existing colour labels', kind: 'toggle' },
  overwrite_caption: { label: 'Overwrite existing captions', kind: 'toggle' },
  close_to_tray: { label: 'Keep running in the tray when closed', kind: 'toggle' },

  // ── applied automatically ──
  auto_apply: {
    label: 'Apply stars and picks automatically', kind: 'toggle',
    hint: 'When an album finishes, write its stars and its suggested picks to the files. A pick flag cannot live in an XMP file, so the pick is written as a colour label (Blue by default) that Lightroom reads on Metadata → Read Metadata from File, or when the photos are imported.',
  },
  pick_label: {
    label: 'Colour for suggested picks', kind: 'segmented',
    hint: 'Lightroom shows this colour on the frames Conrod suggests keeping. None leaves the colour label alone.',
    options: [
      { id: 'None', name: 'None' }, { id: 'Red', name: 'Red' }, { id: 'Yellow', name: 'Yellow' },
      { id: 'Green', name: 'Green' }, { id: 'Blue', name: 'Blue' }, { id: 'Purple', name: 'Purple' },
    ],
  },

  // ── lightroom ──
  lightroom_bridge: {
    label: 'Install the Conrod Bridge plug-in', kind: 'toggle',
    hint: 'Copies the plug-in into Lightroom Classic and keeps it current. Optional: Conrod reads and writes the files without it.',
  },
};

// Each definition carries its own key, so rows and search results are self-describing.
for (const [key, field] of Object.entries(FIELDS)) field.key = key;

/** Page order. `advanced` sections are folded until opened. */
export const SECTIONS = [
  {
    id: 'culling', tab: 'culling', title: 'Culling',
    hint: 'How soft a frame may be before it is rated soft or culled. These are the controls a photographer actually changes.',
    keys: ['blur_strictness', 'burst_gap', 'sharp_at', 'blurred_below', 'auto_reject_below_stars', 'cull_blurred', 'cull_exposure'],
  },
  {
    id: 'new-scans', tab: 'culling', title: 'New scans',
    hint: 'Where the import sheet starts.',
    keys: ['scan_profile'],
  },
  {
    id: 'detection-tuning', tab: 'culling', title: 'Detection tuning', advanced: true,
    hint: 'Rarely worth changing unless a shoot is being missed or over-detected.',
    keys: ['detect_conf', 'min_box_fraction', 'max_vehicles_per_frame'],
  },
  {
    id: 'ai-models', tab: 'identification', title: 'Photo helpers',
    hint: 'What Conrod finds, checks and reads. Everything runs on this PC unless a row says Cloud.',
     keys: [
      'include_cars', 'include_bikes', 'include_trucks',
      'read_plates', 'read_numbers', 'read_text', 'include_people',
      'use_quality_score', 'use_vlm', 'vlm_reasoning', 'read_action', 'vlm_strategy',
      'use_decider', 'decider_quant', 'moments_transport', 'moments_strategy', 'moments_vlm_fallback',
      'moments_threshold',
      'moments_openrouter_model', 'moments_openrouter_key',
      'moments_opencode_model', 'moments_opencode_key',
    ],
  },
  {
    id: 'known-fill', tab: 'identification', title: 'Known vehicles and names',
    hint: 'Fill in what the model misses, without guessing over a fresh reading.',
    keys: ['use_known_vehicles', 'normalise_names', 'entry_list'],
  },
  {
    id: 'vision-limits', tab: 'identification', title: 'Vision limits', advanced: true,
    hint: 'Only worth changing when a host is slow or rejects large images.',
    keys: ['vlm_timeout', 'vlm_max_retries', 'vlm_input_edge'],
  },
  {
    id: 'identify-workflow', tab: 'workflow', title: 'Identify workflow',
    hint: 'Choose the order OCR, Decider and Vision run in, or run them at the same time.',
    keys: ['identify_parallel', 'identify_order'],
  },
  {
    id: 'metadata-write', tab: 'metadata', title: 'Written on export',
    hint: 'Decisions are written as XMP sidecars, or into the file. Your originals are never modified.',
    keys: ['write_rating', 'write_label', 'write_caption', 'write_plate_keyword', 'write_sidecar_for_raw', 'keyword_prefix'],
  },
  {
    id: 'metadata-overwrite', tab: 'metadata', title: 'Overwrite existing metadata', advanced: true,
    hint: 'Off keeps whatever is already in the file.',
    keys: ['overwrite_rating', 'overwrite_label', 'overwrite_caption'],
  },
  {
    id: 'desktop', tab: 'metadata', title: 'Desktop',
    keys: ['close_to_tray'],
  },
  {
    id: 'auto-apply', tab: 'lightroom', title: 'Write stars, picks and colour labels to your photos',
    hint: 'Conrod writes its stars and its suggested picks into the files you already have, so Lightroom sees them with or without the plug-in. Never over your own stars or labels.',
    keys: ['auto_apply', 'pick_label'],
  },
  {
    id: 'lightroom', tab: 'lightroom', title: 'Optional: Lightroom plug-in',
    hint: 'Adds live flags and labels inside Lightroom without re-reading metadata. Conrod reads and writes the files fine without it.',
    keys: ['lightroom_bridge'],
  },
];

/** Settings an album can carry its own value for, in the order the sidebar lists them. */
export const OVERRIDABLE = [
  { key: 'scan_profile', label: 'Shoot type' },
  { key: 'blur_strictness', label: 'Blur strictness' },
  { key: 'burst_gap', label: 'Burst gap' },
  { key: 'read_plates', label: 'Licence plates' },
  { key: 'read_numbers', label: 'Race numbers' },
  { key: 'read_action', label: 'Action effects' },
  { key: 'moments_threshold', label: 'Moment confidence' },
];

/** Whole objects that behave like one field on the page, and the tab they dirty. */
const EXTRA_KEY_TABS = { vlm_services: 'identification', moments_services: 'identification' };

const sectionOf = new Map();
for (const section of SECTIONS) for (const key of section.keys) sectionOf.set(key, section);

export function tabById(id) {
  return TABS.find((tab) => tab.id === id) ?? null;
}

export function keyTab(key) {
  return sectionOf.get(key)?.tab ?? EXTRA_KEY_TABS[key] ?? null;
}

export function fieldsOf(sectionId) {
  const section = SECTIONS.find((s) => s.id === sectionId);
  return (section?.keys ?? []).map((key) => FIELDS[key]).filter(Boolean);
}

function sameValue(a, b) {
  if (a === b) return true;
  if (typeof a !== typeof b) return false;
  if (a && b && typeof a === 'object') {
    try { return JSON.stringify(a) === JSON.stringify(b); } catch { return false; }
  }
  return false;
}

/** Every key whose draft value differs from the saved one. */
export function dirtyKeys(draft, saved) {
  const keys = new Set([...Object.keys(draft ?? {}), ...Object.keys(saved ?? {})]);
  return [...keys].filter((key) => !sameValue(draft?.[key], saved?.[key]));
}

/** Unsaved-change counts per tab, so each tab can show its own dot. */
export function dirtyTabs(draft, saved) {
  const counts = Object.fromEntries(TABS.map((tab) => [tab.id, 0]));
  for (const key of dirtyKeys(draft, saved)) {
    const tab = keyTab(key);
    if (tab) counts[tab] += 1;
  }
  return counts;
}

/** Search every field by name, key, hint and the section it lives in. */
export function searchFields(query) {
  const needle = String(query ?? '').trim().toLowerCase();
  if (!needle) return [];
  const out = [];
  for (const section of SECTIONS) {
    const tab = tabById(section.tab);
    const where = [section.title, section.hint, tab?.label].filter(Boolean).join(' ').toLowerCase();
    for (const key of section.keys) {
      const field = FIELDS[key];
      if (!field) continue;
      const haystack = [key, field.label, field.hint, where].filter(Boolean).join(' ').toLowerCase();
      if (haystack.includes(needle)) {
        out.push({ key, label: field.label, tab: section.tab, section: section.id, sectionTitle: section.title });
      }
    }
  }
  return out;
}

function ownAlbumValue(job, key) {
  let own;
  try { own = job?.settings_json ? JSON.parse(job.settings_json)[key] : undefined; } catch { own = undefined; }
  if (own !== undefined) return own;
  if (job && Object.prototype.hasOwnProperty.call(job, key)) return job[key];
  return undefined;
}

/** How many albums carry their own value for each overridable default. */
export function albumOverrideCounts(jobs, settings) {
  const list = Array.isArray(jobs) ? jobs : [];
  return OVERRIDABLE.map(({ key, label }) => {
    let count = 0;
    for (const job of list) {
      const own = ownAlbumValue(job, key);
      if (own === undefined) continue;
      if (!sameValue(own, settings?.[key])) count += 1;
    }
    return { key, label, count };
  });
}
