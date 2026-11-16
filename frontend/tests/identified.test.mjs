import test from 'node:test';
import assert from 'node:assert/strict';
import { attributesOf, countsOf, readOf, summaryOf } from '../src/lib/identified.mjs';

const row = {
  number: '23',
  plate: 'MNT911',
  number_conf: 0.9,
  plate_conf: 0.8,
  reviewed: 0,
  attributes: JSON.stringify({ make: 'Porsche', model: '911 GT3 R', team: 'Manthey', driver: 'Dick Johnson', vlm_conf: 0.95 }),
};

test('attributes survive bad JSON and missing values', () => {
  assert.deepEqual(attributesOf(''), {});
  assert.deepEqual(attributesOf('{not json'), {});
  assert.deepEqual(attributesOf(null), {});
  assert.deepEqual(attributesOf({ a: 1 }), { a: 1 });
  assert.equal(attributesOf('[1,2]').length, undefined);
});

test('the read fields come from the row first, then the attributes', () => {
  const r = readOf(row);
  assert.equal(r.number, '23');
  assert.equal(r.plate, 'MNT911');
  assert.equal(r.vehicle, 'Porsche 911 GT3 R');
  assert.equal(r.team, 'Manthey');
  assert.equal(r.driver, 'Dick Johnson');
  assert.equal(r.confidence, 0.95);
  assert.equal(r.reviewed, false);
  assert.equal(readOf({ attributes: '{}' }).vehicle, null);
});

test('the one-line summary skips what is not there', () => {
  assert.equal(summaryOf(row), 'No. 23 · MNT911 · Porsche 911 GT3 R · Manthey · Dick Johnson');
  assert.equal(summaryOf({ attributes: '{}' }), 'Nothing readable');
  assert.equal(summaryOf({ plate: 'ABC123', attributes: '{}' }), 'ABC123');
});

test('counts say how much still needs a look', () => {
  const c = countsOf([row, { reviewed: 1 }, { reviewed: 1, rejected: 1 }, null]);
  assert.equal(c.total, 3, 'a missing row is not a row');
  assert.equal(c.reviewed, 2);
  assert.equal(c.unreviewed, 1);
  assert.equal(c.rejected, 1);
});
