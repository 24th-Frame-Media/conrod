import test from 'node:test';
import assert from 'node:assert/strict';
import {facetsFor, tabsWithData, validTab, FACET_KINDS} from '../src/lib/facets.mjs';

test('rodeo, equestrian and portraits never offer numbers or plates', () => {
  for (const id of ['sport-rodeo', 'sport-equestrian', 'portrait', 'portrait-group', 'portrait-couple', 'wedding', 'event', 'event-shows', 'event-parties', 'concert', 'portrait-pets', 'wildlife-birds']) {
    const tabs = facetsFor(id);
    assert.ok(!tabs.includes('number') && !tabs.includes('plate'), id);
  }
  assert.deepEqual(facetsFor('sport-rodeo'), ['person', 'subject']);
  assert.deepEqual(facetsFor('portrait'), ['person']);
  assert.deepEqual(facetsFor('portrait-pets'), ['subject', 'person']);
});

test('motorsport keeps numbers, plates only for burnouts, meets and dealers add makes', () => {
  assert.deepEqual(facetsFor('motorsport'), ['number', 'person']);
  assert.deepEqual(facetsFor('motorsport-rally'), ['number', 'person']);
  assert.deepEqual(facetsFor('motorsport-burnouts'), ['number', 'plate', 'person']);
  assert.deepEqual(facetsFor('motorsport-meet'), ['plate', 'make', 'person']);
  assert.deepEqual(facetsFor('auto-dealer'), ['plate', 'make', 'person']);
  assert.deepEqual(facetsFor('sport-cycling'), ['number', 'person']);
  assert.deepEqual(facetsFor('aviation'), ['plate', 'make']);
});

test('property, landscape and astro have no facet tabs', () => {
  for (const id of ['property-listing', 'property-report', 'landscape', 'astro']) assert.deepEqual(facetsFor(id), []);
});

test('mix and unknown ids fall back to every tab that can have data', () => {
  for (const id of ['mix', 'future-preset', '', undefined, null, 'constructor', '__proto__']) {
    const tabs = facetsFor(id);
    assert.deepEqual(tabs, ['number', 'plate', 'make', 'team', 'person']);
    assert.ok(tabs.every((k) => FACET_KINDS.includes(k)));
  }
});

test('facetsFor hands out a copy, so callers cannot change the table', () => {
  facetsFor('portrait').push('number');
  assert.deepEqual(facetsFor('portrait'), ['person']);
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
