import test from 'node:test';
import assert from 'node:assert/strict';
import { applyLine } from '../src/lib/apply.mjs';

test('the review note says what arrives at album finish, and names the pick colour', () => {
  assert.equal(
    applyLine({ autoApply: true, status: 'scanning', pickLabel: 'Blue' }),
    'Stars and Blue picks are applied when the scan finishes.',
  );
  assert.equal(
    applyLine({ autoApply: true, status: 'done', pickLabel: 'Blue' }),
    'Stars and Blue picks applied to the files.',
  );
  assert.equal(
    applyLine({ autoApply: true, status: 'done_errors', pickLabel: 'Purple' }),
    'Stars and Purple picks are applied when the frames that failed are read.',
  );
  assert.equal(
    applyLine({ autoApply: true, status: 'scanning', pickLabel: 'None' }),
    'Stars are applied when the scan finishes.',
    'no pick colour means stars only',
  );
  assert.equal(applyLine({ autoApply: true, status: 'done' }), 'Stars applied to the files.');
});

test('turning the switch off says so instead of promising an apply', () => {
  assert.equal(
    applyLine({ autoApply: false, status: 'done', pickLabel: 'Blue' }),
    'Stars and picks are not applied automatically.',
  );
});
