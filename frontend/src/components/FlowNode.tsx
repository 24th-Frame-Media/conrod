import { useState } from 'react';
import { Icon } from './basics';

/** One flowchart node from the ONE surface model (surface.mjs / §6.8). Plain
 *  stages stay filter buttons; the three identify sub-nodes carry a status
 *  word, pause/skip/run-again controls, a greyed reason + shortcut and (on the
 *  DECISION node) the type suggestion. Everything here reads from the node the
 *  snapshot produced — nothing invents a status or a consequence. */
export type FlowControl = {
  action: string; args: { jobId: number | null; stage: string; paused?: boolean; skip?: boolean; full?: boolean };
  available: boolean; label: string; consequence?: string;
};

type Props = {
  node: any;
  first: boolean;
  filter: string | null;
  onFilter?: (f: string | null) => void;
  onStageControl?: (
    action: 'stage_pause' | 'stage_skip' | 'stage_rerun',
    stage: string,
    extra?: { paused?: boolean; skip?: boolean; full?: boolean },
  ) => void;
  onAcceptTypeSuggestion?: (type: string) => void;
  onOpenShootSettings?: () => void;
};

export function FlowNode({ node, first, filter, onFilter, onStageControl, onAcceptTypeSuggestion, onOpenShootSettings }: Props) {
  const [confirmSkip, setConfirmSkip] = useState(false);
  const subNode = node.key != null;
  const showControls = subNode && !node.off && Boolean(onStageControl);

  const inner = (
    <>
      <span className="id-node-top">
        <span className="id-node-num mono">{node.number}</span>
        <span className={`flow-word tone-${node.tone}`}>{node.wordLabel ?? node.state}</span>
        {node.honestCheck && <Icon name="check" size={12} />}
        {node.word === 'failed' && <Icon name="x" size={12} />}
        {node.word === 'running' && <i className="id-node-dot" aria-hidden="true" />}
      </span>
      <b>{node.label}</b>
      <small className="ing-sub" title={node.title || node.sub}>{node.sub}</small>
      {node.meta && <small className="ing-meta" title={node.meta}>{node.meta}</small>}
    </>
  );

  const cls = `id-node ${node.state}${node.off ? ' off' : ''} flow-node`;

  // A greyed step (§6.3): a plain reason and a shortcut to where it is enabled.
  // It is never a filter button, never shows workers, never renders controls.
  const offBlock = node.off ? (
    <div className="flow-off" role="note">
      <span className="flow-off-reason">{node.offReason}</span>
      {onOpenShootSettings && (
        <button type="button" className="flow-shortcut" onClick={onOpenShootSettings}>
          {node.offShortcut?.label ?? 'Open Manage shoot'}
        </button>
      )}
    </div>
  ) : null;

  const controlBlock = showControls ? (
    <div className="flow-controls" role="group" aria-label={`${node.label} controls`}>
      {(['pause', 'skip', 'rerun'] as const).map((kind) => {
        const c: FlowControl = node.controls[kind];
        if (!c) return null;
        return (
          <button
            key={kind} type="button" className={`flow-ctl flow-ctl-${kind}`}
            disabled={!c.available} title={kind === 'skip' ? c.consequence : `${c.label} ${node.label}`}
            onClick={() => {
              if (kind === 'skip') { setConfirmSkip(true); return; }
              const extra = c.args.paused !== undefined ? { paused: c.args.paused } : undefined;
              onStageControl?.(c.action as 'stage_pause' | 'stage_skip' | 'stage_rerun', c.args.stage, extra);
            }}
          >{c.label}</button>
        );
      })}
    </div>
  ) : null;

  // Skip shows its PLAIN CONSEQUENCE (from the snapshot) before confirming (§4).
  const skipControl: FlowControl | undefined = node.controls?.skip;
  const confirmBlock = confirmSkip && skipControl ? (
    <div className="flow-confirm" role="alertdialog" aria-label={`Skip ${node.label}?`}>
      <span className="flow-confirm-line">{skipControl.consequence}</span>
      <div className="flow-confirm-actions">
        <button type="button" className="ghost small" onClick={() => setConfirmSkip(false)}>Cancel</button>
        <button type="button" className="primary small" onClick={() => {
          setConfirmSkip(false);
          onStageControl?.(skipControl.action as 'stage_pause' | 'stage_skip' | 'stage_rerun', skipControl.args.stage);
        }}>Skip this step</button>
      </div>
    </div>
  ) : null;

  const suggestion = node.suggestion;
  const suggestionBlock = suggestion?.shown ? (
    <div className="flow-suggestion" role="note" title={suggestion.note}>
      <b>Suggested: {suggestion.typeLabel}</b>
      <span className="flow-suggestion-conf">{suggestion.confidencePct}% confident · suggestion only</span>
      {onAcceptTypeSuggestion && suggestion.canAccept && (
        <button type="button" className="flow-suggestion-accept" onClick={() => onAcceptTypeSuggestion(suggestion.type)}>
          Accept type
        </button>
      )}
      <small>Only vision reads still to come change.</small>
    </div>
  ) : null;

  const body = <>{inner}{offBlock}{controlBlock}{confirmBlock}{suggestionBlock}</>;

  // Plain nodes keep the click-to-filter behaviour; sub-nodes are a div because
  // they contain real buttons (no nested interactive elements).
  const clickable = Boolean(node.filter) && !node.off && !subNode && Boolean(onFilter);
  const box = clickable ? (
    <button type="button" className={`${cls} ing-node${filter === node.filter ? ' sel' : ''}`} aria-pressed={filter === node.filter}
      title={filter === node.filter ? `Showing ${node.label} workers. Click to show all.` : `Show only the ${node.label} workers`}
      onClick={() => onFilter?.(filter === node.filter ? null : node.filter)}>
      {body}
    </button>
  ) : <div className={cls}>{body}</div>;

  return (
    <li className="id-flow-cell" key={node.id}>
      {!first && <span className={`id-link${node.state === 'active' ? ' flow' : node.state === 'done' ? ' done' : ''}`} aria-hidden="true" />}
      {box}
    </li>
  );
}
