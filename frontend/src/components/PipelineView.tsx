import { useState } from 'react';
import { bottleneck, formatRate, orderStages, percent, readyQueue, stageStatus, waitingText, workerCounts, workersOf } from '../lib/scan.mjs';
import type { PipelineStage, PipelineWorker } from '../lib/types';
import { Icon } from './basics';

const n = (v: number) => v.toLocaleString();

function Dots({ stage, workers }: { stage: PipelineStage; workers: PipelineWorker[] }) {
  const mine = workersOf(workers, stage.id);
  const c = workerCounts(stage, workers);
  const lanes: ('busy' | 'waiting' | 'idle')[] = mine.length
    ? mine.map((w) => w.state)
    : [...Array(c.busy).fill('busy'), ...Array(c.idle).fill('idle')];
  const shown = lanes.slice(0, 16);
  return (
    <span className="wdots" aria-hidden="true">
      {shown.map((state, i) => <i key={i} className={state} />)}
      {lanes.length > shown.length && <small>+{lanes.length - shown.length}</small>}
    </span>
  );
}

function Node({ stage, status, jam, queued, open, workers, running, onToggle }: {
  stage: PipelineStage; status: ReturnType<typeof stageStatus>; jam: boolean; queued: number; open: boolean; workers: PipelineWorker[]; running: boolean; onToggle: () => void;
}) {
  const off = status === 'off';
  const c = workerCounts(stage, workers);
  const working = c.busy + c.waiting;
  return (
    <button type="button" className={`pipe-node ${status}${jam ? ' jam' : ''}${open ? ' open' : ''}`} aria-expanded={open} disabled={off}
      title={off ? `${stage.label} is not used for this shoot type` : `Show the workers on ${stage.label}`} onClick={onToggle}>
      <span className="pn-head">
        <span className="pn-state" aria-label={status === 'done' ? 'Finished' : status === 'active' ? 'Running' : off ? 'Not used' : 'Waiting to start'}>
          {status === 'done' ? <Icon name="check" size={13} /> : <i />}
        </span>
        <b>{stage.label}</b>
      </span>
      {jam && (
        <span className="pn-jam" role="img" aria-label="Bottleneck: this step is holding up the pipeline" title="Bottleneck: the step most frames are waiting on">
          <Icon name="hourglass" size={13} />
        </span>
      )}
      <span className="pn-model" title={stage.model}>{off ? 'Not used for this shoot' : stage.model || '…'}</span>
      {!off && <span className="tag pn-device">{stage.device || '…'}</span>}
      {!off && (
        <>
          <span className="pn-workers">
            <Dots stage={stage} workers={workers} />
            <span className="mono" title={`${working} working of ${stage.workers_total}`}>{status === 'done' ? `${stage.workers_total} ${stage.workers_total === 1 ? 'worker' : 'workers'}` : `${working}/${stage.workers_total}`}</span>
          </span>
          {status === 'done' ? (
            <span className="pn-stats"><span>Finished</span></span>
          ) : (
            <span className="pn-stats">
              <span title="Finished upstream and waiting for this stage"><b className="mono">{n(queued)}</b> queued</span>
              <span className="mono pn-rate">{(running && formatRate(stage.rate_per_s)) || '–'}</span>
            </span>
          )}
          <span className="bar pn-bar"><span className="fill" style={{ width: `${percent(stage.done, stage.total)}%` }} /></span>
          <span className="pn-done mono">{n(stage.done)} / {n(stage.total)}</span>
        </>
      )}
    </button>
  );
}

/** One row per worker on the stage: what it holds, which step, how long, and what it waits for. */
function Lanes({ stage, workers, all }: { stage: PipelineStage; workers: PipelineWorker[]; all: boolean }) {
  const here = workersOf(workers, stage.id);
  const rows = all ? [...workers].sort((a, b) => Number(b.stage === stage.id) - Number(a.stage === stage.id) || a.stage.localeCompare(b.stage) || a.worker_index - b.worker_index) : here;
  if (!rows.length) return <p className="muted lanes-empty">No worker is on {stage.label} right now.</p>;
  return (
    <table className="lanes">
      <thead><tr><th>Worker</th>{all && <th>Stage</th>}<th>State</th><th>File</th><th>Step</th><th className="num">In step</th></tr></thead>
      <tbody>
        {rows.map((w) => (
          <tr key={`${w.stage}:${w.worker_index}:${all ? 'a' : 'h'}`} className={`lane ${w.state}`}>
            <td><span className="lane-dot" /> <span className="mono">{w.worker_index + 1}</span></td>
            {all && <td className="muted">{w.stage}</td>}
            <td className="lane-state">{w.state === 'busy' ? 'working' : waitingText(w)}</td>
            <td className="mono lane-file">{w.file ?? '–'}</td>
            <td className="mono">{w.step ?? '–'}</td>
            <td className="num mono">{w.state === 'idle' ? '–' : `${Math.round(w.ms_in_step).toLocaleString()} ms`}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** The flowchart: stage nodes left to right with connectors; click a node for its worker lanes. */
export function PipelineView({ stages, workers, running = true }: { stages: PipelineStage[]; workers: PipelineWorker[]; running?: boolean }) {
  const [openId, setOpenId] = useState<string | null>(null);
  const [all, setAll] = useState(false);
  const ordered = orderStages(stages);
  const jam = running ? bottleneck(ordered, workers) : null;
  const open = ordered.find((s) => s.id === openId);
  const elsewhere = open ? workers.filter((w) => w.stage !== open.id).length : 0;
  return (
    <section className={`pipe${running ? '' : ' still'}`} aria-label="Pipeline">
      <div className="pipe-row">
        {ordered.map((stage, i) => {
          const status = stageStatus(stage);
          const prev = ordered[i - 1];
          return (
            <div className="pipe-cell" key={stage.id}>
              {prev && <span className={`pipe-link ${stageStatus(prev) === 'done' ? 'done' : stageStatus(prev) === 'active' ? 'flow' : ''}`} aria-hidden="true" />}
              <Node stage={stage} status={status} jam={jam === stage.id} queued={readyQueue(ordered, i)} open={openId === stage.id} workers={workers} running={running}
                onToggle={() => setOpenId((id) => (id === stage.id ? null : stage.id))} />
            </div>
          );
        })}
      </div>
      {open && (
        <div className="pipe-lanes">
          <div className="pipe-lanes-head">
            <b>{open.label}</b><span className="muted">{open.model} · {open.device}</span>
            <span className="spacer" />
            {elsewhere > 0 && <button className="ghost small" onClick={() => setAll((v) => !v)}>{all ? 'Only this stage' : `Show all ${workers.length} workers`}</button>}
            <button className="ghost small" onClick={() => setOpenId(null)}>Close</button>
          </div>
          <Lanes stage={open} workers={workers} all={all} />
        </div>
      )}
    </section>
  );
}
