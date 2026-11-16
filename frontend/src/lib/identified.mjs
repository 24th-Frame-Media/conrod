/* The identifications list: what a detection was read as, in plain words. Pure, so node --test can import it. */

/** Parse the `attributes` column (a JSON string) without throwing; missing or bad JSON is an empty object. */
export function attributesOf(raw) {
  if (raw && typeof raw === 'object') return raw;
  if (typeof raw !== 'string' || raw.trim() === '') return {};
  try {
    const value = JSON.parse(raw);
    return value && typeof value === 'object' && !Array.isArray(value) ? value : {};
  } catch {
    return {};
  }
}

const clean = (v) => (typeof v === 'string' && v.trim() !== '' ? v.trim() : null);

/** The fields worth showing for one detection, strongest first, each already a string or null. */
export function readOf(row) {
  const attrs = attributesOf(row?.attributes);
  const number = clean(row?.number) ?? clean(attrs.race_number);
  const plate = clean(row?.plate) ?? clean(attrs.plate);
  const make = clean(attrs.make) ?? clean(attrs.group_make);
  const model = clean(attrs.model) ?? clean(attrs.group_model);
  const vehicle = [make, model].filter(Boolean).join(' ') || null;
  const confidence = Math.max(
    Number(row?.number_conf) || 0,
    Number(row?.plate_conf) || 0,
    Number(attrs.vlm_conf) || 0,
  );
  return {
    number,
    plate,
    vehicle,
    team: clean(attrs.team),
    driver: clean(attrs.driver),
    colour: clean(attrs.colour),
    confidence,
    reviewed: Number(row?.reviewed) === 1,
    rejected: Number(row?.rejected) === 1,
    person: clean(attrs.person_name) ?? clean(attrs.name),
  };
}

/** "No. 23 · MNT911 · Manthey · Dick Johnson" — the one line the list shows under the frame name. */
export function summaryOf(row) {
  const r = readOf(row);
  const parts = [
    r.number ? `No. ${r.number}` : null,
    r.plate,
    r.vehicle,
    r.team,
    r.driver,
    r.person,
  ].filter(Boolean);
  return parts.length ? parts.join(' · ') : 'Nothing readable';
}

/** Counts for the header: how much is done, and how much still needs a look. */
export function countsOf(rows) {
  const list = (Array.isArray(rows) ? rows : []).filter(Boolean);
  return {
    total: list.length,
    reviewed: list.filter((r) => Number(r.reviewed) === 1).length,
    unreviewed: list.filter((r) => Number(r.reviewed) !== 1).length,
    rejected: list.filter((r) => Number(r.rejected) === 1).length,
  };
}
