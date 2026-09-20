import { useCallback, useEffect, useMemo, useState, type CSSProperties, type MutableRefObject } from 'react';
import { Empty } from '../components/basics';
import { asset, engine } from '../lib/api';
import { boxOf, filename } from '../lib/review';
import { REGIONS, type Job, type Region, type Toaster, type Training } from '../lib/types';
import { usePreview, type Runner } from '../state/hooks';
import type { ReviewModel } from '../state/useReview';

/** The verbs the window-level key handler calls on this screen. */
export type TrainVerbs = { rate: (stars: number) => void; pan: () => void; undo: () => void; zoom: () => void; step: (delta: number) => void };
export const noTrainVerbs: TrainVerbs = { rate: () => {}, pan: () => {}, undo: () => {}, zoom: () => {}, step: () => {} };

const WORDS = ['smeared', 'soft', 'usable', 'sharp', 'pin sharp'];
type Props = {
  rv: ReviewModel; jobs: Job[]; jobId: number | null; run: Runner; toast: Toaster; showBoxes: boolean;
  verbs: MutableRefObject<TrainVerbs>; onPickJob: (id: number) => void; onLibrary: () => void;
};

/** Rate how sharp the subject is, 1 to 5; the ratings fit a small model that takes over the built-in measure. */
export function Train({ rv, jobs, jobId, run, toast, showBoxes, verbs, onPickJob, onLibrary }: Props) {
  const [training, setTraining] = useState<Training | null>(null);
  const [region, setRegion] = useState<Region>('vehicle');
  const [pan, setPan] = useState(false);
  const [zoom, setZoom] = useState(false);
  const { frames, facts, frame, selected, setSelected } = rv;

  useEffect(() => { void run(async () => setTraining(await engine.trainingStatus())); }, [run]);

  const inRegion = useCallback((id: number) => (facts.get(id)?.dets ?? []).filter((d) => (d.region_type || 'vehicle') === region), [facts, region]);
  const queue = useMemo(() => frames.filter((f) => inRegion(f.id).length > 0), [frames, inRegion]);
  useEffect(() => {
    if (queue.length && !queue.some((f) => f.id === selected)) setSelected(queue[0].id);
  }, [queue, selected, setSelected]);

  const subject = frame ? inRegion(frame.id)[0] : undefined;
  const preview = usePreview(subject ? frame?.id ?? null : null, run);
  const box = subject && frame ? boxOf(subject, frame) : null;

  const step = useCallback((delta: number) => {
    const i = queue.findIndex((f) => f.id === selected);
    const next = queue[Math.max(0, Math.min(queue.length - 1, i + delta))];
    if (next) setSelected(next.id);
  }, [queue, selected, setSelected]);

  const rate = useCallback(async (stars: number) => {
    if (!subject) return;
    await run(async () => {
      await engine.trainLabel(subject.id, stars, pan);
      setTraining(await engine.trainingStatus());
      setPan(false);
      step(1);
    });
  }, [subject, pan, run, step]);

  const undo = useCallback(() => void run(async () => { await engine.undoLabel(); setTraining(await engine.trainingStatus()); }), [run]);
  verbs.current = { rate: (n) => void rate(n), pan: () => setPan((p) => !p), undo, zoom: () => setZoom((z) => !z), step };

  const fit = () => void run(async () => { setTraining(await engine.trainModel(region)); toast('Training finished. See the validation below.', { tone: 'ok' }); });
  const forget = () => void run(async () => { setTraining(await engine.forgetModel(region)); toast('Back to the built-in measure.'); });

  if (jobId == null) {
    return <div className="screen"><Empty icon="folder" title="Choose a shoot to train on."><button className="primary" onClick={onLibrary}>Open library</button></Empty></div>;
  }
  return (
    <div className="train">
      <div className={`train-stage${zoom ? ' full' : ''}`}>
        {subject && frame ? (
          <div className="train-frame" style={{ '--ar': frame.width && frame.height ? frame.width / frame.height : 1.5 } as CSSProperties}>
            <img src={asset(preview) ?? asset(frame.thumb_path)} alt={filename(frame.path)} />
            {showBoxes && box && <div className="train-box" style={{ left: `${box[0] * 100}%`, top: `${box[1] * 100}%`, width: `${(box[2] - box[0]) * 100}%`, height: `${(box[3] - box[1]) * 100}%` }} />}
          </div>
        ) : (
          <div className="train-empty">Nothing left to rate in this album for “{region}”. Scan a shoot and come back, or pick another region.</div>
        )}
      </div>
      <aside className="train-side">
        <div className="train-selects">
          <select aria-label="Album" value={jobId} onChange={(e) => onPickJob(Number(e.target.value))}>{jobs.map((j) => <option key={j.id} value={j.id}>{j.label}</option>)}</select>
          <select aria-label="Region" value={region} onChange={(e) => setRegion(e.target.value as Region)}>{REGIONS.map((r) => <option key={r}>{r}</option>)}</select>
        </div>
        <h3>How sharp is the {region}?</h3>
        <p className="muted">Judge the subject, not the picture. A pan with a streaked background and a crisp car is a 5.</p>
        <div className="train-stars">
          {WORDS.map((word, i) => <button key={word} disabled={!subject} onClick={() => void rate(i + 1)}><b>{i + 1}</b><small>{word}</small></button>)}
        </div>
        <div className="train-row">
          <button className={pan ? 'on' : ''} aria-pressed={pan} onClick={() => setPan((p) => !p)}>Pan <kbd>P</kbd></button>
          <button disabled={!subject} onClick={() => void rate(0)}>Can&apos;t tell <kbd>X</kbd></button>
          <button onClick={undo}>Undo <kbd>U</kbd></button>
        </div>
        <p className="muted small">Press 1&ndash;5 to rate and move on. <kbd>Z</kbd> shows the crop at 100%, <kbd>B</kbd> hides the outline.</p>
        <div className="train-progress"><b>{training?.labels ?? 0}</b> rated · {training?.active ? 'learned model active' : 'built-in focus measure'} · {queue.length.toLocaleString()} to go</div>
        <div className="train-fit">
          <button className="primary" onClick={fit}>Learn from my ratings</button>
          <button className="ghost" onClick={forget}>Forget what it learned</button>
          {training?.validation != null && <pre className="log mono">{JSON.stringify(training.validation, null, 2)}</pre>}
        </div>
      </aside>
    </div>
  );
}
