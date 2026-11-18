import test from 'node:test';
import assert from 'node:assert/strict';
import {
  FIELDS, OVERRIDABLE, SECTIONS, TABS, albumOverrideCounts, dirtyKeys, dirtyTabs, fieldsOf, keyTab, searchFields, tabById,
} from '../src/lib/settings.mjs';

const LEGACY_GROUPS = [
  'detect_conf', 'min_box_fraction', 'max_vehicles_per_frame', 'include_cars', 'include_bikes', 'include_trucks',
  'sharp_at', 'blurred_below', 'blur_strictness', 'auto_reject_below_stars', 'cull_blurred', 'burst_gap',
  'read_plates', 'read_numbers', 'read_text', 'use_vlm', 'vlm_timeout', 'vlm_max_retries', 'vlm_input_edge',
  'use_known_vehicles', 'entry_list', 'normalise_names',
  'write_rating', 'write_label', 'overwrite_rating', 'overwrite_label', 'write_caption', 'overwrite_caption',
  'write_plate_keyword', 'write_sidecar_for_raw', 'keyword_prefix', 'close_to_tray',
];

test('every setting keeps its engine key and appears in exactly one section', () => {
  const seen = new Map();
  for (const section of SECTIONS) {
    for (const key of section.keys) {
      assert.ok(FIELDS[key], `field definition for ${key}`);
      assert.equal(seen.get(key), undefined, `${key} listed twice`);
      seen.set(key, section.id);
    }
  }
  for (const key of [...LEGACY_GROUPS, 'scan_profile', 'vlm_strategy', 'vlm_services']) {
    if (key === 'vlm_services') continue; // a whole list, rendered by the providers section
    assert.ok(seen.has(key), `${key} is still on the page`);
  }
  assert.equal(SECTIONS.length, 12);
  assert.deepEqual(TABS.map((t) => t.label), ['Culling', 'Identification', 'Metadata', 'System']);
});

test('blur strictness offers critical as the strictest stop', () => {
  assert.deepEqual(FIELDS.blur_strictness.options.map((o) => o.id), ['critical', 'strict', 'moderate', 'lenient']);
});

test('sections expose their fields in section order, and every tab has a section except System', () => {
  assert.deepEqual(fieldsOf('culling').slice(0, 3).map((f) => f.key), ['blur_strictness', 'burst_gap', 'sharp_at']);

  assert.deepEqual(fieldsOf('detection-tuning').map((f) => f.key), ['detect_conf', 'min_box_fraction', 'max_vehicles_per_frame']);
  assert.deepEqual(fieldsOf('missing'), []);
  for (const tab of TABS) {
    const count = SECTIONS.filter((s) => s.tab === tab.id).length;
    assert.equal(count > 0, tab.id !== 'system', `${tab.id} sections`);
  }
  assert.equal(tabById('culling').label, 'Culling');
  assert.equal(tabById('nope'), null);
  assert.equal(keyTab('read_plates'), 'identification');
  assert.equal(keyTab('vlm_services'), 'identification');
  assert.equal(keyTab('not_a_setting'), null);
});

test('the per-album settings are exactly the ones the engine overrides', () => {
  const album = Object.entries(FIELDS).filter(([, f]) => f.album).map(([key]) => key);
  assert.deepEqual(album, ['blur_strictness', 'burst_gap', 'scan_profile', 'read_plates', 'read_numbers', 'read_action']);
  assert.deepEqual(OVERRIDABLE.map((o) => o.key).sort(), album.slice().sort());
});

test('search matches labels, keys and hints, ignoring case and padding', () => {
  assert.deepEqual(searchFields('  '), []);
  assert.deepEqual(searchFields('burst').map((m) => m.key), ['burst_gap']);
  assert.equal(searchFields('STRICTNESS')[0].key, 'blur_strictness');
  assert.equal(searchFields('licence plates')[0].key, 'read_plates');
  assert.equal(searchFields('vlm_services').length, 0);
  const plate = searchFields('plate').map((m) => m.key);
  assert.ok(plate.includes('read_plates') && plate.includes('write_plate_keyword'));
  const vision = searchFields('vision');
  assert.ok(vision.some((m) => m.tab === 'identification' && m.section === 'vision-providers'));
  assert.equal(searchFields('zzzz').length, 0);
  assert.equal(searchFields('burst')[0].sectionTitle, 'Culling');
});

test('dirty state is per key, per tab and compares lists deeply', () => {
  const saved = { blur_strictness: 'moderate', burst_gap: 1.5, include_cars: true, vlm_services: [{ id: 'a' }] };
  assert.deepEqual(dirtyKeys(saved, saved), []);
  const draft = { ...saved, blur_strictness: 'strict', burst_gap: 2, include_cars: true };
  assert.deepEqual(dirtyKeys(draft, saved).sort(), ['blur_strictness', 'burst_gap']);
  assert.deepEqual(dirtyTabs(draft, saved), { culling: 2, identification: 0, metadata: 0, system: 0 });
  const services = { ...saved, vlm_services: [{ id: 'a' }, { id: 'b' }] };
  assert.deepEqual(dirtyTabs(services, saved), { culling: 0, identification: 1, metadata: 0, system: 0 });
  assert.deepEqual(dirtyTabs({ ...draft, close_to_tray: true, odd_key: 1 }, saved), { culling: 2, identification: 0, metadata: 1, system: 0 });
  assert.deepEqual(dirtyKeys({}, saved).sort(), Object.keys(saved).sort());
});

test('album override counts only count explicit values that differ from the default', () => {
  const settings = { scan_profile: 'motorsport', blur_strictness: 'moderate', burst_gap: 1.5, read_plates: true, read_numbers: true };
  const jobs = [
    { id: 1, scan_profile: 'portrait-group', settings_json: JSON.stringify({ blur_strictness: 'strict', burst_gap: 1.5 }) },
    { id: 2, settings_json: JSON.stringify({ read_plates: false, read_numbers: true }) },
    { id: 3, burst_gap: 2.5, settings_json: '{broken' },
    { id: 4 },
  ];
  const counts = Object.fromEntries(albumOverrideCounts(jobs, settings).map((o) => [o.key, o.count]));
  assert.equal(counts.scan_profile, 1);
  assert.equal(counts.blur_strictness, 1);
  assert.equal(counts.burst_gap, 1);
  assert.equal(counts.read_plates, 1);
  assert.equal(counts.read_numbers, 0);
  assert.deepEqual(albumOverrideCounts(undefined, settings).map((o) => o.count), [0, 0, 0, 0, 0, 0]);
});
