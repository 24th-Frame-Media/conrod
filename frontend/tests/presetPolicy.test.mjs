import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import policies from '../src/lib/presetPolicies.mjs';
import {
  DETECT_KEYS, READ_KEYS, MOMENT_KEYS, applyOverrides, defaultSummary, facetsFromSummary, isOn, needsLabel, nextOverrides, noteList, overridesOf,
} from '../src/lib/presetPolicy.mjs';

const rust = readFileSync(new URL('../../crates/conrod-core/src/profile.rs', import.meta.url), 'utf8');
const types = readFileSync(new URL('../src/lib/types.ts', import.meta.url), 'utf8');
const rustNames = [...rust.matchAll(/^\s+"([a-z-]+)" => ShootPreset::\w+,\r?$/gm)].map((m) => m[1]);
// `auto` is a meta-type ("let Conrod decide"), not one of the presets
// `ShootPreset::ALL` iterates: the picker and the generated summary skip it.
const enginePresets = rustNames.filter((name) => name !== 'auto');
const group = types.slice(types.indexOf('export const PROFILE_GROUPS'), types.indexOf('export const MIXED_PROFILE'));
const tsIds = [...group.matchAll(/id: '([a-z-]+)'/g)].map((m) => m[1]);

test('the type picker lists exactly the presets the engine knows, and the engine summary covers each', () => {
  assert.ok(rustNames.length >= 28, 'parsed the Rust preset names');
  assert.deepEqual([...tsIds].sort(), [...enginePresets].sort());
  assert.deepEqual(Object.keys(policies).sort(), [...enginePresets].sort());
});

test('every summary lists every detector and reader the engine can switch', () => {
  for (const [id, s] of Object.entries(policies)) {
    assert.deepEqual(s.detects.map((r) => r.key), DETECT_KEYS, id);
    assert.deepEqual(s.reads.map((r) => r.key), READ_KEYS, id);
    assert.equal(s.preset, id);
    for (const r of [...s.detects, ...s.reads]) assert.ok(r.label, `${id} ${r.key}`);
  }
});

test('race numbers are on for competition and off where they would be noise', () => {
  for (const id of ['motorsport', 'motorsport-burnouts', 'motorsport-track', 'motorsport-rally', 'sport-cycling', 'sport-action', 'mix']) {
    assert.equal(isOn(policies[id], 'numbers'), true, id);
  }
  for (const id of ['motorsport-meet', 'auto-dealer', 'aviation', 'portrait', 'wedding', 'concert', 'event-shows', 'landscape', 'property-listing']) {
    assert.equal(isOn(policies[id], 'numbers'), false, id);
  }
});

test('cars are an option everywhere and on by default only where they are the subject', () => {
  for (const [id, s] of Object.entries(policies)) {
    const row = s.detects.find((r) => r.key === 'vehicles');
    assert.equal(row.blocked, false, `${id}: vehicles is never blocked, always available`);
  }
  for (const id of ['motorsport', 'motorsport-meet', 'auto-dealer', 'mix', 'motorsport-rally']) assert.equal(isOn(policies[id], 'vehicles'), true, id);
  for (const id of ['event-shows', 'portrait', 'wedding', 'concert', 'event-parties', 'landscape']) assert.equal(isOn(policies[id], 'vehicles'), false, id);
});

test('switching a detector or reader works both ways on every type', () => {
  for (const [id, s] of Object.entries(policies)) {
    for (const row of [...s.detects, ...s.reads]) {
      for (const on of [true, false]) {
        // A reader needs its detector: switch that on too.
        const base = row.requires ? { [row.requires]: true } : {};
        const applied = applyOverrides(s, { ...base, [row.key]: on });
        assert.equal(isOn(applied, row.key), on, `${id} ${row.key} ${on}`);
        assert.equal([...applied.detects, ...applied.reads].find((r) => r.key === row.key).overridden, true);
      }
    }
  }
});

test('a reader is blocked while its detector is off', () => {
  const s = applyOverrides(policies['motorsport-track'], { vehicles: false });
  for (const key of ['numbers', 'plates', 'make_model', 'sponsors']) {
    const row = s.reads.find((r) => r.key === key);
    assert.equal(row.on, false, key);
    assert.equal(row.blocked, true, key);
    assert.equal(needsLabel(s, row), 'needs Cars & bikes');
  }
  const wedding = applyOverrides(policies.wedding, { faces: false });
  assert.equal(isOn(wedding, 'names'), false);
  assert.equal(needsLabel(wedding, wedding.reads.find((r) => r.key === 'names')), 'needs Faces & eyes');
  // Switching the detector back restores what the album had.
  const back = applyOverrides(policies['motorsport-track'], { vehicles: true, numbers: false });
  assert.equal(isOn(back, 'numbers'), false);
  assert.equal(isOn(back, 'plates'), true);
});

test('a switch back at the default stops being an override', () => {
  const s = policies['event-shows'];
  const vehicles = s.detects[0];
  const on = nextOverrides({}, vehicles, true);
  assert.deepEqual(on, { vehicles: true });
  assert.deepEqual(nextOverrides(on, vehicles, false), {});
  assert.deepEqual(nextOverrides({ numbers: false }, vehicles, true), { numbers: false, vehicles: true });
  // Input is never changed.
  const kept = { vehicles: true };
  nextOverrides(kept, vehicles, false);
  assert.deepEqual(kept, { vehicles: true });
});

test('stored overrides are read leniently', () => {
  assert.deepEqual(overridesOf({ vehicles: true, faces: 'yes', bogus: false, numbers: false }), { vehicles: true, numbers: false });
  for (const bad of [null, undefined, 3, 'x', [], [true]]) assert.deepEqual(overridesOf(bad), {});
});

test('the notes say what is not a switch, in plain words', () => {
  assert.deepEqual(noteList(policies['motorsport-track']), ['Panning blur is forgiven', 'Blur check: critical']);
  assert.ok(noteList(policies.landscape).includes('Horizon and verticals checked'));
  assert.ok(noteList(policies.concert).includes('Dark frames are fine'));
  assert.ok(noteList(policies['property-report']).includes('Keeps nearly everything'));
  assert.ok(noteList(policies.portrait).includes('Soft backgrounds are not forgiven'));
});

test('the sidebar tabs follow what the shoot reads', () => {
  assert.deepEqual(facetsFromSummary(policies.landscape), ['moment']);
  assert.deepEqual(facetsFromSummary(policies['property-listing']), []);
  assert.deepEqual(facetsFromSummary(applyOverrides(policies.portrait, { vehicles: true, numbers: true })), ['number', 'person', 'subject', 'moment']);
  assert.equal(defaultSummary('nonsense').preset, 'mix');
  assert.equal(defaultSummary('Motorsport ').preset, 'motorsport');
});

test('the moment switches are on where the type has moments, and ranking needs detecting', () => {
  for (const [id, s] of Object.entries(policies)) {
    assert.deepEqual([s.moments.detect.key, s.moments.rank.key], MOMENT_KEYS, id);
    const has = s.moments.vocabulary.length > 0;
    assert.equal(s.moments.detect.default, has, id);
    assert.equal(s.moments.rank.default, has, id);
    for (const m of s.moments.vocabulary) assert.ok(m.label && m.weight > 0 && m.weight <= 1, `${id} ${m.key}`);
  }
  const off = applyOverrides(policies.wedding, { moments_detect: false });
  assert.equal(isOn(off, 'moments_detect'), false);
  assert.equal(isOn(off, 'moments_rank'), false);
  assert.equal(off.moments.rank.blocked, true);
  assert.equal(needsLabel(off, off.moments.rank), 'needs Spot moments');
  const rankOff = applyOverrides(policies.wedding, { moments_rank: false });
  assert.equal(isOn(rankOff, 'moments_detect'), true);
  assert.equal(isOn(rankOff, 'moments_rank'), false);
  // Wedding moments are the wedding's, a track day's are the track's.
  const keys = (id) => policies[id].moments.vocabulary.map((m) => m.key);
  assert.ok(keys('wedding').includes('first_kiss') && !keys('wedding').includes('tyre_smoke'));
  assert.ok(keys('motorsport-track').includes('tyre_smoke') && !keys('motorsport-track').includes('first_kiss'));
});
