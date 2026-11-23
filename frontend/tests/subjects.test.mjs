import test from 'node:test';
import assert from 'node:assert/strict';
import { applyOverrides, defaultSummary, isOn } from '../src/lib/presetPolicy.mjs';
import { identifyTitle, readsSubjects, subjectKind, subjectWords, detectorPurpose } from '../src/lib/subjects.mjs';
import { etaLines, ingestNodes } from '../src/lib/ingest.mjs';
import { rateLabel } from '../src/lib/identifyFlow.mjs';

const album = (preset) => applyOverrides(defaultSummary(preset));
const stage = (id, done = 0, total = 0) => ({ id, label: id[0].toUpperCase() + id.slice(1), model: '', workers_total: 0, workers_busy: 0, done, total, rate_per_s: 0, eta_s: null });

test('a rodeo looks for the animal and the rider, and never vehicles', () => {
  const rodeo = album('sport-rodeo');
  assert.equal(isOn(rodeo, 'vehicles'), false, 'a rodeo has no car lane');
  assert.equal(isOn(rodeo, 'animals'), true);
  assert.equal(isOn(rodeo, 'people'), true, 'the rider is a person');
  assert.equal(subjectKind(rodeo), 'subjects', 'an animal and its rider share no one word');
  assert.equal(readsSubjects(rodeo), false, 'nothing is read off a bull or a rider');
});

test('the live identification banner names the album\'s subjects, or the moments judge', () => {
  assert.equal(identifyTitle(album('motorsport')), 'Identifying vehicles');
  assert.equal(identifyTitle(album('wildlife-birds')), 'Judging moments', 'nothing is read off a bird crop');
  assert.equal(identifyTitle(album('sport-rodeo')), 'Judging moments', 'a rodeo is judged, not read');
});

test('the subject words follow the album, not a hardcoded car', () => {
  assert.equal(subjectWords(album('motorsport')).many, 'vehicles');
  assert.equal(subjectWords(album('wildlife-birds')).many, 'animals');
  assert.equal(subjectWords(album('event-parties')).many, 'people');
  assert.equal(subjectWords(album('sport-rodeo')).many, 'subjects');
  // An album that turns vehicles off stops reading subjects and stops saying cars.
  const track = applyOverrides(defaultSummary('motorsport-track'), { vehicles: false });
  assert.equal(subjectWords(track).many, 'people');
  assert.equal(readsSubjects(track), false, 'its readers went with the vehicles');
  assert.equal(detectorPurpose(null), "Finds the shoot's subjects");
});

test('the pipeline flowchart words a rodeo without vehicles and without a VISION node', () => {
  const rodeo = album('sport-rodeo');
  const subject = subjectWords(rodeo);
  const reads = readsSubjects(rodeo);
  assert.equal(reads, false);
  const stages = [stage('read', 10, 10), stage('detect', 10, 10), stage('identify', 0, 60)];
  const nodes = ingestNodes({ stages, subject, reads });
  assert.equal(nodes.find((n) => n.id === 'detect').sub, "Finds the shoot's subjects");
  assert.equal(nodes.find((n) => n.id === 'sharpness').sub, 'Checks the main subject is in focus');
  assert.equal(nodes.find((n) => n.id === 'vision'), undefined, 'a rodeo identifies only moments; the DECIDER node is the judge');
  assert.doesNotMatch(JSON.stringify(nodes), /vehicle|car/i, 'no lane claims cars on a rodeo');
  // The same run on a motorsport album says vehicles, and keeps its VISION node.
  const motor = album('motorsport');
  const carNodes = ingestNodes({ stages, subject: subjectWords(motor), reads: readsSubjects(motor) });
  assert.equal(carNodes.find((n) => n.id === 'vision').sub, 'Reading vehicles');
  assert.equal(carNodes.find((n) => n.id === 'detect').sub, 'Vehicles and people detector');
});

test('the rate and the time left use the album\'s own subject', () => {  const subject = subjectWords(album('sport-rodeo'));
  assert.equal(rateLabel(12, 30000, subject), '24 subjects/min');
  assert.equal(rateLabel(1, 8000, subject), '8.0 s/subject');
  const [scan, id] = etaLines({ vision: true, scanDone: false, scanEta: 300, scanStable: true, identify: { state: 'running', done: 0, total: 40 }, subject });
  assert.equal(scan, 'about 5 min left');
  assert.match(id, /\(40 subjects\)$/);
});
