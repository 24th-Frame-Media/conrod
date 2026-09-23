import { useEffect, useState } from 'react';

import { call } from '../lib/api';

import { Confirm } from './basics';

import { useRun } from '../state/hooks';

import { useToast } from '../state/toast';



export function AlbumTools({ jobId, label, refresh, ids, imageIds }: { jobId: number; label: string; refresh: () => Promise<void>; ids: number[]; imageIds: number[] }) {

  const run = useRun(), toast = useToast();

  const [name, setName] = useState(label), [number, setNumber] = useState('');

  const [watch, setWatch] = useState(false);

  const [reset, setReset] = useState<string | null>(null);

  const [summary, setSummary] = useState<{images: Record<string, number>; counts: Record<string, number>} | null>(null);

  useEffect(() => { setName(label); setSummary(null); void run(async () => {

    const w = await call<{active: boolean; jobId: number}>('watch_status'); setWatch(w.active && w.jobId === jobId);

  }); }, [jobId, label, run]);

  const act = (action: string, args: object = {jobId}) => run(async () => { await call(action, args); await refresh(); toast('Done. Background work appears in Activity.'); });

  const reject = (rejected: boolean) => run(async () => {
    for (const imageId of imageIds) await call('mark', {imageId, rejected});
    await refresh();
    toast(`${imageIds.length} photographs ${rejected ? 'rejected' : 'restored'}`);
  });
  return <details className="album-tools"><summary>Album tools</summary><div className="actions-row">

    <input aria-label="Rename album" value={name} onChange={e => setName(e.target.value)} />

    <button onClick={() => void act('rename_job', {jobId, label: name})}>Rename</button>

    <button onClick={() => void act('rescore')}>Rescore</button>

    <button onClick={() => void act('pick_keepers')}>Pick keepers</button>

    <button onClick={() => void act('group')}>Group cars</button>

    <button onClick={() => void run(async () => setSummary(await call('summary', {jobId})))}>Summary</button>

    <label><input type="checkbox" checked={watch} onChange={e => { const active = e.target.checked; void run(async () => { await call('set_watch', {jobId, active}); setWatch(active); }); }} /> Watch for new photos</label>

    <button className="danger" onClick={() => setReset('reset_identifications')}>Reset identification</button>

    <button className="danger" onClick={() => setReset('reset_detections')}>Reset detections</button>

  </div>

  {summary && <p>{summary.images.scanned} scanned · {summary.images.written} written · {summary.images.errors} errors · {summary.counts.numbered} numbered · {summary.counts.plated} plated · {summary.counts.to_review} to review</p>}

  <div className="actions-row"><span>{ids.length} selected subjects</span><input aria-label="Bulk race number" placeholder="Race number" value={number} onChange={e => setNumber(e.target.value)} />

    <button disabled={!ids.length} onClick={() => void act('bulk_edit', {ids, number})}>Set number</button>

    <button disabled={!imageIds.length} onClick={() => void reject(true)}>Reject selected</button>

    <button disabled={!imageIds.length} onClick={() => void reject(false)}>Restore selected</button>

  </div>

  {reset && <Confirm title="Reset this album?" action="Reset" onCancel={() => setReset(null)} onConfirm={() => { void act(reset); setReset(null); }}><p>This removes {reset === 'reset_detections' ? 'detections and cached crops' : 'identification results'} from this album. Your original photographs stay untouched.</p></Confirm>}

  </details>;

}

