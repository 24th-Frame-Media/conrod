import assert from 'node:assert/strict';
import test from 'node:test';
import { hiddenCopies, jobState, queueLabel } from '../src/lib/format.ts';

test('hiddenCopies says nothing when an album hides no copies', () => {
  assert.equal(hiddenCopies(undefined), '');
  assert.equal(hiddenCopies(0), '');
});

test('hiddenCopies counts the hidden copies in the album line, singular for one', () => {
  assert.equal(hiddenCopies(1), ' · 1 copy hidden');
  assert.equal(hiddenCopies(2437), ' · 2,437 copies hidden');
});

test('the queue label counts from one and says the ordinal the way a line is read', () => {
  assert.equal(queueLabel(0), 'Queued · 1st in line', 'the next album to run');
  assert.equal(queueLabel(1), 'Queued · 2nd in line');
  assert.equal(queueLabel(2), 'Queued · 3rd in line');
  assert.equal(queueLabel(3), 'Queued · 4th in line');
  assert.equal(queueLabel(20), 'Queued · 21st in line');
});

test('the queue label keeps the teen endings, which are all "th"', () => {
  assert.equal(queueLabel(10), 'Queued · 11th in line');
  assert.equal(queueLabel(11), 'Queued · 12th in line');
  assert.equal(queueLabel(12), 'Queued · 13th in line');
  assert.equal(queueLabel(110), 'Queued · 111th in line');
});

test('a queued album that a restart left waiting says it has not started', () => {
  const job = { id: 1, label: 'Bathurst', root: 'D:\\Photos', status: 'queued', total: 40, done: 0, created_at: 0 };
  assert.equal(jobState(job), 'Not started');
  assert.equal(jobState({ ...job, status: 'stopped' }), 'Stopped at 0%');
});
