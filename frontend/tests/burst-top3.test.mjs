import test from 'node:test';
import assert from 'node:assert/strict';
import {burstTopRanks} from '../src/review.mjs';

const f = (id, extra = {}) => ({id, rating: null, manual_stars: null, rejected: 0, ...extra});

test('top 3 ranks follow manual stars, rejects, rating, and the engine pick wins rank 1', () => {
 const group = [
   f(1, {rating: .9}),
   f(2, {rating: .95, rejected: 1}),
   f(3, {rating: .7}),
   f(4, {rating: .8}),
   f(5, {manual_stars: 2, rating: .1}),
 ];
 const ranks = burstTopRanks(group, x => x.id === 3);
 assert.deepEqual([...ranks].sort((a, b) => a[1] - b[1]), [[3, 1], [5, 2], [1, 3]]);
});

test('without an engine pick the ordering alone decides, and ties prefer the pick', () => {
 const group = [f(1, {rating: .8}), f(2, {rating: .8}), f(3, {rating: .8}), f(4, {rating: .5})];
 const ranks = burstTopRanks(group, x => x.id === 3);
 assert.deepEqual([...ranks].sort((a, b) => a[1] - b[1]), [[3, 1], [1, 2], [2, 3]]);
});

test('small bursts get fewer ranks and nothing is ranked beyond three', () => {
 assert.equal(burstTopRanks([f(1)], () => false).size, 1);
 assert.equal(burstTopRanks([f(1), f(2, {manual_stars: 5}), f(3), f(4)], () => false).has(4), false);
});
