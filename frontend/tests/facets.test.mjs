import test from 'node:test';
import assert from 'node:assert/strict';
import {facetsFor as facetsWithMoments, tabsWithData, validTab, FACET_KINDS} from '../src/lib/facets.mjs';

/** The people-and-vehicle tabs; moments are covered on their own below. */
const facetsFor = (id, overrides) => facetsWithMoments(id, overrides).filter((k) => k !== 'moment');

test('rodeo, equestrian and portraits never offer numbers or plates', () => {
  for (const id of ['sport-rodeo', 'sport-equestrian', 'portrait', 'portrait-group', 'portrait-couple', 'wedding', 'event', 'event-shows', 'event-parties', 'concert', 'portrait-pets', 'wildlife-birds']) {
    const tabs = facetsFor(id);
    assert.ok(!tabs.includes('number') && !tabs.includes('plate'), id);
  }
  assert.deepEqual(facetsFor('sport-rodeo'), ['person', 'subject']);
  assert.deepEqual(facetsFor('wedding'), ['person']);
  assert.deepEqual(facetsFor('portrait-pets'), ['person', 'subject']);
});

test('tabs follow what each competition and vehicle type reads', () => {
  assert.deepEqual(facetsFor('motorsport'), ['number', 'make', 'team', 'person']);
  assert.deepEqual(facetsFor('motorsport-rally'), ['number', 'plate', 'make', 'team', 'person']);
  assert.deepEqual(facetsFor('motorsport-burnouts'), ['number', 'plate', 'make', 'person']);
  assert.deepEqual(facetsFor('motorsport-meet'), ['plate', 'make', 'person']);
  assert.deepEqual(facetsFor('auto-dealer'), ['plate', 'make']);
  assert.deepEqual(facetsFor('sport-cycling'), ['number', 'person']);
  assert.deepEqual(facetsFor('aviation'), ['plate', 'make', 'team']);
});

test('an album switch changes the tabs', () => {
  assert.deepEqual(facetsFor('wedding', { vehicles: true, plates: true }), ['plate', 'person']);
  assert.deepEqual(facetsFor('motorsport-meet', { vehicles: false }), ['person']);
  assert.deepEqual(facetsFor('motorsport', { numbers: false }), ['make', 'team', 'person']);
});

test('property, landscape and astro have no facet tabs', () => {
  for (const id of ['property-listing', 'property-report', 'landscape', 'astro']) assert.deepEqual(facetsFor(id), []);
});

test('mix and unknown ids read everything, so every tab can have data', () => {
  for (const id of ['mix', 'future-preset', '', undefined, null, 'constructor', '__proto__']) {
    const tabs = facetsFor(id);
    assert.deepEqual(tabs, ['number', 'plate', 'make', 'team', 'person', 'subject']);
    assert.ok(tabs.every((k) => FACET_KINDS.includes(k)));
  }
});

test('facetsFor hands out a copy, so callers cannot change the table', () => {
  facetsFor('portrait').push('number');
  assert.deepEqual(facetsFor('portrait'), ['person', 'subject']);
});

test('tabs with no entries are hidden even when the preset allows them', () => {
  const facets = {number: [], plate: [{value: 'A'}], make: [], team: [], person: [{value: '1'}], subject: [{value: 'horse'}]};
  assert.deepEqual(tabsWithData('motorsport-burnouts', facets), ['plate', 'person']);
  assert.deepEqual(tabsWithData('sport-rodeo', facets), ['person', 'subject']);
  assert.deepEqual(tabsWithData('portrait', {}), []);
  assert.deepEqual(tabsWithData('landscape', facets), []);
});

test('the selected tab stays valid when tabs change', () => {
  assert.equal(validTab('plate', ['number', 'plate']), 'plate');
  assert.equal(validTab('plate', ['number', 'person']), 'number');
  assert.equal(validTab(null, ['person']), 'person');
  assert.equal(validTab('number', []), null);
});

test('moments get a tab wherever the shoot has moments to judge, and switching them off removes it', () => {
  for (const id of ['wedding', 'motorsport-track', 'sport-rodeo', 'portrait', 'astro', 'mix']) assert.ok(facetsWithMoments(id).includes('moment'), id);
  for (const id of ['property-listing', 'property-report', 'auto-dealer']) assert.ok(!facetsWithMoments(id).includes('moment'), id);
  assert.ok(!facetsWithMoments('wedding', { moments_detect: false }).includes('moment'));
  assert.equal(facetsWithMoments('wedding').at(-1), 'moment');
});
