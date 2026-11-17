import assert from 'node:assert/strict';
import test from 'node:test';
import { hiddenCopies } from '../src/lib/format.ts';

test('hiddenCopies says nothing when an album hides no copies', () => {
  assert.equal(hiddenCopies(undefined), '');
  assert.equal(hiddenCopies(0), '');
});

test('hiddenCopies counts the hidden copies in the album line, singular for one', () => {
  assert.equal(hiddenCopies(1), ' · 1 copy hidden');
  assert.equal(hiddenCopies(2437), ' · 2,437 copies hidden');
});
