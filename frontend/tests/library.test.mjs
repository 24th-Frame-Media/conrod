import assert from 'node:assert/strict';
import test from 'node:test';
import {
  STATUS_OPTIONS, SORT_OPTIONS, plural, pct, shortDate, hiddenCopies, queueLabel,
  albumCount, groupTitle, statusBucket, isAttention, matchesSearch, matchesStatus,
  visibleJobs, groupAlbums, cardView, stripWork, setupLine, errorBanner,
} from '../src/lib/library.mjs';

const done = { id: 1, label: 'Bathurst 1000', root: 'D:\\Photos\\Bathurst', status: 'done', total: 1284, done: 1284, created_at: 1789000000, copies: 3 };
const indexed = { id: 2, label: 'Indexed shoot', root: '', status: 'indexed', total: 612, done: 612, created_at: 1786000000 };
const scanning = { id: 3, label: 'Sunday hillclimb', root: '', status: 'scanning', total: 120, done: 62, created_at: 1789900000 };
const stopped = { id: 4, label: 'Wakefield twilight', root: '', status: 'stopped', total: 340, done: 118, created_at: 1783000000 };
const queued = { id: 5, label: 'Phillip Island', root: '', status: 'queued', total: 612, done: 0, created_at: 1786000000 };
const retry = { id: 6, label: 'Needs a retry', root: '', status: 'done_errors', total: 100, done: 100, errors: 3, created_at: 1785000000 };
const empty = { id: 7, label: 'Fresh import', root: '', status: 'scanning', total: 0, done: 0, created_at: 1789950000 };

test('the small formatters keep the wording the cover and filter row use', () => {
  assert.equal(plural(1, 'photo'), '1 photo');
  assert.equal(plural(1284, 'photo'), '1,284 photos');
  assert.equal(pct(62, 120), 52);
  assert.equal(pct(0, 0), 0);
  assert.equal(queueLabel(0), 'Queued · 1st in line');
  assert.equal(queueLabel(1), 'Queued · 2nd in line');
  assert.equal(queueLabel(10), 'Queued · 11th in line');
  assert.equal(albumCount(4), '4 albums');
  assert.equal(albumCount(1), '1 album');
  assert.equal(groupTitle('Needs attention', 2), 'Needs attention · 2');
  assert.equal(hiddenCopies(3), ' · 3 copies hidden');
  assert.equal(hiddenCopies(0), '');
  assert.equal(shortDate(0), new Date(0).toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' }));
});

test('the status and sort selects carry their exact labels', () => {
  assert.deepEqual(STATUS_OPTIONS.map((o) => o.label), ['All statuses', 'Ready to review', 'Scanning', 'Queued', 'Stopped', 'Needs a retry']);
  assert.deepEqual(SORT_OPTIONS.map((o) => o.label), ['Newest first', 'Oldest first', 'Name A–Z', 'Most photos']);
});

test('statusBucket names every state, and the engine queue wins', () => {
  assert.equal(statusBucket(done, {}), 'done');
  assert.equal(statusBucket(indexed, {}), 'indexed');
  assert.equal(statusBucket(scanning, {}), 'scanning');
  assert.equal(statusBucket(stopped, {}), 'stopped');
  assert.equal(statusBucket(queued, {}), 'queued');
  assert.equal(statusBucket(retry, {}), 'retry');
  assert.equal(statusBucket(empty, {}), 'empty');
  assert.equal(statusBucket({ ...done, status: 'error' }, {}), 'error');
  assert.equal(statusBucket({ ...done, id: 5 }, { queue: [5] }), 'queued');
});

test('only done and indexed albums are Ready to review', () => {
  assert.equal(isAttention(done, {}), false);
  assert.equal(isAttention(indexed, {}), false);
  for (const job of [scanning, stopped, queued, retry, empty]) assert.equal(isAttention(job, {}), true);
});

test('search matches the album label case-insensitively', () => {
  assert.equal(matchesSearch(done, 'bath'), true);
  assert.equal(matchesSearch(done, 'BATHURST'), true);
  assert.equal(matchesSearch(done, '  Bath  '), true);
  assert.equal(matchesSearch(done, 'zulu'), false);
  assert.equal(matchesSearch(done, ''), true);
});

test('the status select filters both groups by bucket', () => {
  const jobs = [done, indexed, scanning, queued, stopped, retry, empty];
  const ids = (s) => jobs.filter((j) => matchesStatus(j, s, {})).map((j) => j.id);
  assert.deepEqual(ids('all'), [1, 2, 3, 5, 4, 6, 7]);
  assert.deepEqual(ids('ready'), [1, 2]);
  assert.deepEqual(ids('scanning'), [3, 7]);
  assert.deepEqual(ids('queued'), [5]);
  assert.deepEqual(ids('stopped'), [4]);
  assert.deepEqual(ids('retry'), [6]);
});

test('search and status combine into the visible set', () => {
  const jobs = [done, indexed, scanning, queued, stopped];
  assert.deepEqual(visibleJobs(jobs, { search: 'island' }).map((j) => j.id), [5]);
  assert.deepEqual(visibleJobs(jobs, { status: 'ready' }).map((j) => j.id), [1, 2]);
  assert.deepEqual(visibleJobs(jobs, { search: 'bath', status: 'ready' }).map((j) => j.id), [1]);
  assert.deepEqual(visibleJobs(jobs, { search: 'zulu', status: 'all' }), []);
});

test('the Ready to review row obeys every sort value', () => {
  const jobs = [
    { id: 1, label: 'Zulu', status: 'done', total: 10, done: 10, created_at: 1000 },
    { id: 2, label: 'alpha', status: 'done', total: 50, done: 50, created_at: 3000 },
    { id: 3, label: 'Mike', status: 'done', total: 30, done: 30, created_at: 2000 },
  ];
  const ids = (sort) => groupAlbums(jobs, { sort }).ready.map((j) => j.id);
  assert.deepEqual(ids('newest'), [2, 3, 1]);
  assert.deepEqual(ids('oldest'), [1, 3, 2]);
  assert.deepEqual(ids('name'), [2, 3, 1]);
  assert.deepEqual(ids('photos'), [2, 3, 1]);
});

test('Needs attention keeps active → empty → queue order → stopped → errors', () => {
  const jobs = [retry, stopped, { ...queued, id: 9 }, queued, empty, scanning, done];
  const { attention, ready } = groupAlbums(jobs, { activeJob: 3, queue: [5, 9], sort: 'newest' });
  assert.deepEqual(attention.map((j) => j.id), [3, 7, 5, 9, 4, 6]);
  assert.deepEqual(ready.map((j) => j.id), [1]);
});

test('cardView gives every state the same six parts, with one primary action', () => {
  const ready = cardView(done);
  assert.deepEqual(ready.chip, { text: 'Ready to review', tone: 'ready' });
  assert.deepEqual(ready.primary, { label: 'Open review', action: 'open', hint: '' });
  assert.equal(ready.badge, '1,284 photos');
  assert.equal(ready.meta, shortDate(1789000000) + ' · 3 copies hidden');
  assert.equal(ready.placeholder, false);
  assert.equal(ready.bar, false);

  const idx = cardView(indexed);
  assert.deepEqual(idx.chip, { text: 'Indexed — ready to cull', tone: 'ready' });
  assert.equal(idx.primary.label, 'Suggest culls');
  assert.equal(idx.primary.action, 'resume');

  const live = cardView(scanning, { percent: 52, stage: 'Detect', eta: '~2 min left' });
  assert.deepEqual(live.chip, { text: 'Scanning · 52% · Detect · ~2 min left', tone: 'busy' });
  assert.equal(live.primary.label, 'Watch scan');
  assert.equal(live.bar, true);
  assert.equal(cardView(scanning, { percent: 52 }).chip.text, 'Scanning · 52%');

  const q = cardView(queued, { queue: [5], queuePosition: 1 });
  assert.deepEqual(q.chip, { text: 'Queued · 2nd in line', tone: 'warn' });
  assert.equal(q.primary.label, 'Remove from queue');
  assert.equal(q.primary.action, 'unqueue');
  assert.equal(q.meta, 'Starts when the current scan finishes');

  const s = cardView(stopped);
  assert.deepEqual(s.chip, { text: 'Stopped at 35% · 222 frames left', tone: 'warn' });
  assert.equal(s.primary.label, 'Resume scan');
  const s2 = cardView(stopped, { scanningElsewhere: true });
  assert.equal(s2.primary.label, 'Add to queue');
  assert.equal(s2.primary.action, 'resume');
  assert.equal(s2.primary.hint, 'Starts when the current scan finishes.');

  const r = cardView(retry);
  assert.deepEqual(r.chip, { text: '3 photos could not be read', tone: 'bad' });
  assert.equal(r.primary.label, 'Retry failed photos');

  const e = cardView({ ...done, status: 'error' });
  assert.deepEqual(e.chip, { text: 'Folder unreachable', tone: 'bad' });
  assert.equal(e.primary.label, 'Resume scan');
  assert.equal(e.meta, 'Nothing already measured is lost.');

  const n = cardView(empty);
  assert.deepEqual(n.chip, { text: 'No photos yet', tone: 'warn' });
  assert.equal(n.badge, null);
  assert.equal(n.placeholder, true);
  assert.equal(n.primary.label, 'Watch scan');
  assert.equal(n.meta, 'Importing · no photos yet');
});

test('the strip work line covers idle, queue, scanning, elsewhere and error', () => {
  assert.deepEqual(stripWork({}), { text: 'No scan running', tone: 'idle', watch: false });
  assert.equal(stripWork({ queueLabels: ['Phillip Island', 'Wakefield'] }).text, '2 albums waiting to scan · Phillip Island starts next');
  const scan = { label: 'Sunday hillclimb', percent: 52, stage: 'Detect', eta: '~2 min left' };
  assert.deepEqual(stripWork({ scan }), { text: 'Scanning Sunday hillclimb · 52% · Detect · ~2 min left', tone: 'busy', watch: true });
  assert.equal(stripWork({ scan, elsewhere: true }).text, 'A scan is running on Sunday hillclimb. Other albums can be added to the queue.');
  const err = stripWork({ error: { label: 'Wakefield twilight', kind: 'folder_unreachable' }, scan });
  assert.equal(err.tone, 'bad');
  assert.match(err.text, /Reconnect the drive/);
});

test('setup and error banners use plain language', () => {
  assert.equal(setupLine({ needed: true, ready: 1, total: 2 }), 'Setup needed: 1 of 2 models ready');
  assert.equal(setupLine({ needed: false, ready: 2, total: 2 }), null);
  assert.equal(setupLine(null), null);
  assert.match(errorBanner('Wakefield twilight', 'folder_unreachable'), /Nothing already measured is lost/);
  assert.match(errorBanner('X', 'disk_full'), /Clear cache/);
  assert.match(errorBanner('X', 'exiftool_missing'), /focus points/);
  assert.match(errorBanner('X', 'no_gpu'), /CPU/);
});
