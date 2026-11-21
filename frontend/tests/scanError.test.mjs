import assert from 'node:assert/strict';
import test from 'node:test';
import { classifyScanError, scanErrorKind, scanFailureFor } from '../src/lib/scanError.mjs';

test('disk-full errors are recognised before anything else', () => {
  assert.equal(scanErrorKind('There is not enough space on the disk. (os error 112)'), 'disk_full');
  assert.equal(scanErrorKind('ENOSPC: no space left on device'), 'disk_full');
  const words = classifyScanError('There is not enough space on the disk.', { label: 'Bathurst' });
  assert.equal(words.title, 'The disk is full');
  assert.match(words.message, /Clear some space.*press Resume/);
});

test('a network path or network wording is NAS unreachable', () => {
  assert.equal(scanErrorKind('The system cannot find the path specified.', '\\\\NAS\\photos'), 'nas_unreachable');
  assert.equal(scanErrorKind('connection timed out'), 'nas_unreachable');
  const words = classifyScanError('connection refused', { root: '\\\\NAS\\photos', label: 'Rally' });
  assert.equal(words.title, 'The network drive is offline');
  assert.match(words.message, /Rally/);
});

test('a local missing folder is folder missing, not a model failure', () => {
  assert.equal(scanErrorKind('The system cannot find the file specified. (os error 2)', 'D:\\Photos\\2026'), 'folder_missing');
  const words = classifyScanError('os error 2', { root: 'D:\\Photos\\Sunday hillclimb' });
  assert.equal(words.title, 'Conrod lost the folder');
  assert.match(words.message, /Sunday hillclimb/);
});

test('model load and install failures get the model line', () => {
  assert.equal(scanErrorKind('could not load the detector: invalid onnx'), 'model_failed');
  assert.equal(scanErrorKind('could not install the models: download failed'), 'model_failed');
  const words = classifyScanError('could not load the detector');
  assert.equal(words.title, 'A model could not be loaded');
});

test('anything else is the safe unknown line, never the raw engine text', () => {
  const words = classifyScanError('some internal thing');
  assert.equal(words.kind, 'unknown');
  assert.equal(words.title, 'The scan did not finish');
  assert.ok(!words.message.includes('some internal thing'));
  assert.equal(words.detail, 'some internal thing');
});

test('a seed failure wins; otherwise only an errored album makes a banner', () => {
  const seed = classifyScanError('boom', { label: 'Seed' });
  assert.equal(scanFailureFor({ seed }), seed);
  assert.equal(scanFailureFor({ job: { id: 1, status: 'done_errors', label: 'A', root: '', error_samples: [] } }), null);
  const failure = scanFailureFor({
    job: { id: 2, status: 'error', label: 'Sunday', root: 'D:\\Photos\\Sunday', error_samples: [] },
    progress: { state: 'error' },
    status: { tasks: [{ id: 9, state: 'failed', error: 'There is not enough space on the disk.' }] },
  });
  assert.equal(failure.kind, 'disk_full');
  assert.equal(failure.jobId, 2);
  assert.equal(failure.label, 'Sunday');
});
