import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import policies from '../src/lib/presetPolicies.mjs';
import { applyOverrides, defaultSummary, isOn } from '../src/lib/presetPolicy.mjs';
import {
  PEOPLE_PRESETS, WIZARD_GROUPS, WIZARD_PRESETS, answersFromProfile, buildProfile, listWords, orderSelected, prefilledReads,
  profileDefaults, readerApplies, shouldAutoOpenWizard, summaryLines,
} from '../src/lib/wizard.mjs';

const rust = readFileSync(new URL('../../crates/conrod-core/src/profile.rs', import.meta.url), 'utf8');
const rustNames = [...rust.matchAll(/^\s+"([a-z-]+)" => ShootPreset::\w+,\r?$/gm)].map((m) => m[1]);

test('every shoot type the wizard offers is one the engine knows, once', () => {
  assert.equal(new Set(WIZARD_PRESETS).size, WIZARD_PRESETS.length);
  for (const id of WIZARD_PRESETS) assert.ok(rustNames.includes(id), id);
  for (const id of PEOPLE_PRESETS) assert.ok(WIZARD_PRESETS.includes(id), id);
  assert.deepEqual(WIZARD_GROUPS.map((g) => g.title), ['Motorsport', 'People', 'Sport', 'Other']);
});

test('the wizard opens by itself on a fresh install only', () => {
  const fresh = { profile: {}, albums: 0, whatsNewSeen: null };
  assert.equal(shouldAutoOpenWizard(fresh), true);
  assert.equal(shouldAutoOpenWizard({ ...fresh, profile: undefined }), true);
  // Finished, or skipped on purpose: never again by itself.
  assert.equal(shouldAutoOpenWizard({ ...fresh, profile: { done: true } }), false);
  // An existing library or a version already seen is an upgrade, not a first run.
  assert.equal(shouldAutoOpenWizard({ ...fresh, albums: 3 }), false);
  assert.equal(shouldAutoOpenWizard({ ...fresh, whatsNewSeen: '0.9.0' }), false);
});

test('a skipped wizard changes nothing but remembers it was skipped', () => {
  assert.deepEqual(buildProfile({ selected: [] }), { done: true });
});

test('the first type chosen is the default, the rest follow as chosen', () => {
  const p = buildProfile({ selected: ['wedding', 'motorsport-track', 'wedding', 'not-a-type'] });
  assert.equal(p.default_preset, 'wedding');
  assert.deepEqual(p.shoots, ['wedding', 'motorsport-track']);
  assert.deepEqual(orderSelected(['a', 'b', 'c'], 'c'), ['c', 'a', 'b']);
  assert.deepEqual(orderSelected(['a', 'b'], 'zz'), ['a', 'b']);
  assert.deepEqual(orderSelected(['a', 'b'], null), ['a', 'b']);
});

test('who is in the pictures sets faces and people for that motorsport type only', () => {
  const selected = ['motorsport-track', 'motorsport-rally', 'motorsport-burnouts', 'wedding'];
  const p = buildProfile({ selected, people: { 'motorsport-track': 'faces', 'motorsport-rally': 'none', 'motorsport-burnouts': 'around', wedding: 'faces' } });
  assert.deepEqual(p.presets['motorsport-track'], { faces: true });
  assert.deepEqual(p.presets['motorsport-rally'], { people: false });
  // The type's own default: nothing stored. A wedding is never asked.
  assert.equal(p.presets['motorsport-burnouts'], undefined);
  assert.equal(p.presets.wedding, undefined);
});

test('faces at motorsport are off until asked for, and people stay on', () => {
  for (const id of ['motorsport-track', 'motorsport-rally', 'motorsport-burnouts']) {
    assert.equal(isOn(policies[id], 'people'), true, id);
    assert.equal(isOn(policies[id], 'faces'), false, id);
  }
  assert.equal(isOn(applyOverrides(defaultSummary('motorsport-track'), { faces: true }), 'names'), true);
});

test('readers start from what the chosen types read, and only changed ones are saved', () => {
  const selected = ['motorsport-track', 'wedding'];
  const on = prefilledReads(selected);
  assert.equal(on.numbers, true);
  assert.equal(on.plates, true);
  // Names are on for a wedding, and for track days only once faces are on.
  assert.equal(on.names, true);
  // Nothing touched: nothing saved beyond the done mark and the types.
  assert.equal(buildProfile({ selected, reads: {} }).presets, undefined);
  // Plates off: only where plates are read at all (a wedding has none to turn off).
  const p = buildProfile({ selected, reads: { plates: false } });
  assert.deepEqual(p.presets['motorsport-track'], { plates: false });
  assert.equal(p.presets.wedding, undefined);
  assert.deepEqual(readerApplies(selected, 'plates'), ['motorsport-track']);
  // A reader switched on where the type has a car to read.
  const q = buildProfile({ selected: ['motorsport'], reads: { plates: true } });
  assert.deepEqual(q.presets.motorsport, { plates: true });
  // Names off at a wedding.
  const r = buildProfile({ selected, reads: { names: false } });
  assert.deepEqual(r.presets.wedding, { names: false });
});

test('names need faces: unchecking or checking them where faces are off does nothing', () => {
  const p = buildProfile({ selected: ['motorsport-track'], people: { 'motorsport-track': 'around' }, reads: { names: true } });
  assert.equal(p.presets, undefined);
  const q = buildProfile({ selected: ['motorsport-track'], people: { 'motorsport-track': 'faces' }, reads: { names: false } });
  assert.deepEqual(q.presets['motorsport-track'], { faces: true, names: false });
});

test('field and court sport read jersey numbers off the people', () => {
  assert.equal(isOn(policies['sport-field'], 'numbers'), true);
  assert.equal(isOn(policies['sport-court'], 'numbers'), true);
  const row = policies['sport-field'].reads.find((r) => r.key === 'numbers');
  assert.equal(row.label, 'Jersey numbers');
  assert.equal(row.requires, 'people');
  // Unchecking numbers applies to the sport types that read them.
  const p = buildProfile({ selected: ['sport-field', 'wedding'], reads: { numbers: false } });
  assert.deepEqual(p.presets['sport-field'], { numbers: false });
  assert.equal(p.presets.wedding, undefined);
});

test('moments lifting frames is on unless turned off, for every type at once', () => {
  assert.equal(buildProfile({ selected: ['wedding'] }).all, undefined);
  assert.deepEqual(buildProfile({ selected: ['wedding'], momentsRank: false }).all, { moments_rank: false });
});

test('blur follows the types unless a level is chosen', () => {
  assert.equal(buildProfile({ selected: ['wedding'], blur: null }).blur, undefined);
  assert.equal(buildProfile({ selected: ['wedding'], blur: 'strict' }).blur, 'strict');
  assert.equal(buildProfile({ selected: ['wedding'], blur: 'extreme' }).blur, undefined);
});

test('a type\'s own switch wins over the one for every type', () => {
  const profile = { all: { faces: false, moments_rank: false }, presets: { wedding: { faces: true } } };
  assert.deepEqual(profileDefaults(profile, 'wedding'), { faces: true, moments_rank: false });
  assert.deepEqual(profileDefaults(profile, 'portrait'), { faces: false, moments_rank: false });
});

test('the album override beats the profile default beats the type policy', () => {
  const base = defaultSummary('motorsport-track');
  const faces = (s) => s.detects.find((r) => r.key === 'faces');
  assert.equal(faces(applyOverrides(base, {}, {})).on, false);
  const mine = applyOverrides(base, {}, { faces: true });
  assert.equal(faces(mine).on, true);
  // Clearing the album's switch goes back to the profile's default, so that is the row's default.
  assert.equal(faces(mine).default, true);
  assert.equal(faces(mine).overridden, false);
  // The album's own override wins both ways.
  assert.equal(faces(applyOverrides(base, { faces: false }, { faces: true })).on, false);
  assert.equal(faces(applyOverrides(defaultSummary('portrait'), { faces: true }, { faces: false })).on, true);
});

test('the profile round trips through the wizard', () => {
  const answers = { selected: ['motorsport-track', 'sport-field', 'wedding'], people: { 'motorsport-track': 'faces' }, reads: { plates: false }, blur: 'strict', momentsRank: false };
  const profile = buildProfile(answers);
  const back = answersFromProfile(profile);
  assert.deepEqual(back.selected, answers.selected);
  assert.equal(back.defaultId, 'motorsport-track');
  assert.equal(back.people['motorsport-track'], 'faces');
  assert.equal(back.reads.plates, false);
  assert.equal(back.blur, 'strict');
  assert.equal(back.momentsRank, false);
  assert.deepEqual(buildProfile({ ...back, selected: orderSelected(back.selected, back.defaultId) }), profile);
  // No profile, no answers.
  assert.deepEqual(answersFromProfile(undefined).selected, []);
});

test('the last screen says what Conrod will do for each type', () => {
  const profile = buildProfile({ selected: ['motorsport-track', 'wedding'], people: { 'motorsport-track': 'faces' }, blur: 'strict' });
  const [track, wedding] = summaryLines(profile);
  assert.equal(track.title, 'Track days');
  assert.ok(track.looks.includes('faces & eyes') && track.looks.includes('people'));
  assert.ok(track.reads.includes('race numbers'));
  assert.equal(track.blur, 'strict');
  assert.equal(wedding.title, 'Weddings');
  assert.deepEqual(wedding.reads, ['names (from faces)']);
  assert.match(wedding.moments, /lets them lift frames/);
  const off = summaryLines(buildProfile({ selected: ['wedding'], momentsRank: false }))[0];
  assert.match(off.moments, /never lifts frames/);
  assert.deepEqual(summaryLines(undefined), []);
  assert.equal(listWords(['a', 'b', 'c']), 'a, b and c');
  assert.equal(listWords(['a']), 'a');
  assert.equal(listWords([]), '');
});
