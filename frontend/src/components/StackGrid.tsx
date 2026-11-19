import { memo, useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { asset } from '../lib/api';
import { scrollTopFor, stackLayout, visibleRows, type Stack } from '../lib/stacks.mjs';
import { frameStars } from '../lib/review';
import { lowStarBadge } from '../lib/decisions.mjs';
import type { Frame, MarkValues } from '../lib/types';
import { BurstFilmstrip } from './BurstFilmstrip';
import { LoadableImage } from './Media';
import { KeeperMark, StarPill } from './Stars';

export type StackView = Stack<Frame> & { title: string; subtitle: string; photoWord?: string; ranks?: Map<number, number> };

const PAD = 20, GAP = 18, MIN_W = 230, LAYER_INSET = 8, CAPTION_H = 38;
/** Tall enough for the strip's action row, its composition chips (up to three labels plus the cleanest-background marker). */
const EXPANDED_H = 320;

type CardProps = {
  stack: StackView; selected: boolean; open: boolean;
  /** Frames in the burst when its lead is the engine's unaccepted suggestion. */
  suggestion?: number;
  /** The burst's size and best stars when no frame in it is worth keeping. */
  soft?: { size: number; best: number };
  left: number; top: number; width: number; height: number;
  onToggle: (stack: StackView) => void;
  onPick?: (id: number) => void;
  /** The engine's cull (or this frame's reject), for the stack's overall tone. */
  culled: (frame: Frame) => boolean;
};

const StackCard = memo(function StackCard({ stack, selected, open, suggestion, soft, left, top, width, height, onToggle, onPick, culled }: CardProps) {
  const { lead } = stack;
  const count = stack.frames.length;
  const suggested = suggestion != null && !lead.picked;
  // The card's edge says where the whole stack stands: a keeper and no undecided frame, all culled, or still mixed.
  const live = stack.frames.filter((f) => !culled(f));
  const pool = live.length ? live : stack.frames;
  const best = pool.reduce((top, f) => Math.max(top, frameStars(f)), 0);
  const lowStar = lowStarBadge(best);
  const allCulled = stack.frames.every((f) => f.rejected || culled(f));
  const decided = stack.frames.every((f) => f.picked || f.rejected || culled(f));
  const tone = allCulled ? 'culled' : decided && stack.keepers.length ? 'kept' : 'mixed';
  const toneTitle = tone === 'kept'
    ? 'Every frame in this stack is kept or culled, and it has a keeper'
    : tone === 'culled' ? 'Every frame in this stack is culled'
    : 'Some frames in this stack are still undecided';
  return (
    <article className={`burst-stack${open ? ' open' : ''}${selected ? ' selected' : ''}${suggested ? ' suggested' : ''} tone-${tone}`} role="listitem" aria-current={selected || undefined} title={toneTitle}
      style={{ left, top, width, height }} onClick={() => onToggle(stack)}>
      <div className="burst-layers" style={{ '--layers': Math.min(count, 4) } as CSSProperties}>
        <LoadableImage src={asset(lead.thumb_path)} alt={stack.title} />
        <StarPill frame={lead} />
        {lowStar && <span className="low-stars" title={`No highly rated frame in this burst (best is ${best}★)`}>{lowStar}</span>}
        {count > 1 || stack.total > count ? <span className="stack-count" title={stack.total > count ? `${count} of ${stack.total} frames match the filter` : `${count} frames`}>{stack.total > count ? `${count}/${stack.total}` : count}</span> : null}
        {suggested ? (
          <button type="button" className="suggest"
            aria-label={`Keep ${stack.title}, the suggested keeper of its burst`}
            title={`Suggested keeper: the engine ranks this frame best of ${suggestion ?? count} frames in its burst. Click to keep it.`}
            onClick={(e) => { e.stopPropagation(); onPick?.(lead.id); }}>
            <KeeperMark inline /><span>Suggested</span>
          </button>
        ) : soft ? (
          <span className="no-keeper" title={`All ${soft.size} frames in this burst are soft (best ${soft.best} of 5). Nothing here is worth keeping, so no keeper is proposed.`}>
            No keeper
          </span>
        ) : stack.keepers.length > 0 ? (
          <KeeperMark label={lead.picked ? 'Keeper, picked by you' : 'Keeper, the engine\u2019s best of the burst'} />
        ) : null}
      </div>
      <div className="burst-caption">
        <b>{stack.title}</b>
        <span>{stack.subtitle}</span>
      </div>
    </article>
  );
});

/**
 * Shared renderer for grouped-photo grids (burst, duplicate or vehicle stacks). Windowed: only the rows near the
 * viewport are mounted, so a 3 000-frame shoot costs what a 50-frame one does. The open stack's strip sits under its row.
 */
export function StackGrid({ stacks, selectedKey, selectedId, expandedKey, keeperIds, suggestions, softBursts, culled, onToggle, onSelect, onOpen, onPick, onDecide, onStackAction, ariaLabel, empty }: {
  stacks: StackView[]; selectedKey: string | null; selectedId: number | null; expandedKey: string | null; keeperIds: ReadonlySet<number>;
  suggestions?: Map<number, number>;
  softBursts?: Map<number, { size: number; best: number }>;
  culled: (frame: Frame) => boolean; onToggle: (stack: StackView) => void; onSelect: (id: number) => void; onOpen: () => void; onPick?: (id: number) => void;
  /** Keep or cull one frame from the strip's pill. */
  onDecide?: (id: number, values: MarkValues) => void;
  /** The strip's two bulk buttons: keep the focused frame and cull the rest, or cull the whole stack. */
  onStackAction?: (stack: StackView, mode: 'keep' | 'cull') => void;
  ariaLabel: string; empty: ReactNode;
}) {
  const wrap = useRef<HTMLDivElement>(null);
  const inner = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 900, h: 600 });
  const [top, setTop] = useState(0);

  // A scroll fires many times a frame; keep the latest offset in a ref and commit it once per frame.
  const pendingTop = useRef(0);
  const topFrame = useRef(0);
  const onScroll = (e: React.UIEvent<HTMLDivElement>) => {
    pendingTop.current = e.currentTarget.scrollTop;
    if (topFrame.current) return;
    topFrame.current = requestAnimationFrame(() => {
      topFrame.current = 0;
      setTop(pendingTop.current);
    });
  };
  useEffect(() => () => { if (topFrame.current) cancelAnimationFrame(topFrame.current); }, []);

  useLayoutEffect(() => {
    const measure = () => setSize({ w: inner.current!.clientWidth, h: wrap.current!.clientHeight });
    const observer = new ResizeObserver(measure);
    observer.observe(wrap.current!); observer.observe(inner.current!);
    measure();
    return () => observer.disconnect();
  }, []);

  const cols = Math.max(1, Math.floor((size.w + GAP) / (MIN_W + GAP)));
  const width = (size.w - GAP * (cols - 1)) / cols;
  const cardH = Math.round(((width - LAYER_INSET) * 2) / 3) + 9 + CAPTION_H;
  const expandedIndex = useMemo(() => (expandedKey == null ? -1 : stacks.findIndex((s) => s.key === expandedKey)), [stacks, expandedKey]);
  const layout = useMemo(() => stackLayout({ count: stacks.length, cols, rowH: cardH, gap: GAP, pad: PAD, expandedIndex, expandedH: EXPANDED_H }), [stacks.length, cols, cardH, expandedIndex]);
  const [first, last] = visibleRows(layout, top, size.h);

  // Bring the focused stack into view when the focus moves, not when the list refreshes underneath it.
  const selectedIndex = useMemo(() => (selectedKey == null ? -1 : stacks.findIndex((s) => s.key === selectedKey)), [stacks, selectedKey]);
  const latest = useRef({ layout, selectedIndex, size });
  useEffect(() => { latest.current = { layout, selectedIndex, size }; });
  useEffect(() => {
    const { layout: l, selectedIndex: index, size: s } = latest.current;
    const el = wrap.current;
    if (index < 0 || !el) return;
    const next = scrollTopFor(l, index, el.scrollTop, s.h, PAD);
    if (next != null) el.scrollTo({ top: next, behavior: 'auto' });
  }, [selectedKey, expandedKey]);

  const items = stacks.slice(first * layout.cols, Math.min(stacks.length, last * layout.cols));
  const openStack = expandedIndex >= 0 ? stacks[expandedIndex] : null;
  const showStrip = openStack != null && layout.expandedTop + EXPANDED_H > top - 200 && layout.expandedTop < top + size.h + 200;
  // The open strip's own summary: manual keeps, the stack's best live stars, and whether that clears four stars.
  const openFrames = openStack?.frames ?? [];
  const openLive = openFrames.filter((f) => !culled(f));
  const openBest = (openLive.length ? openLive : openFrames).reduce((top, f) => Math.max(top, frameStars(f)), 0);
  const openLowStar = openStack ? lowStarBadge(openBest) : null;
  const openKept = openFrames.filter((f) => f.picked && !f.rejected).length;

  return (
    <div className="burst-grid" ref={wrap} onScroll={onScroll}>
      <div className="burst-grid-inner" ref={inner} role="list" aria-label={ariaLabel} style={{ height: layout.height }}>
        {items.map((stack, n) => {
          const i = first * layout.cols + n;
          const soft = !stack.keepers.length && stack.key.startsWith('b') && stack.lead.burst_key != null ? softBursts?.get(stack.lead.burst_key) : undefined;
          return <StackCard key={stack.key} stack={stack} selected={stack.key === selectedKey} open={stack.key === expandedKey} suggestion={suggestions?.get(stack.lead.id)} soft={soft}
            left={(i % layout.cols) * (width + GAP)} top={layout.rowTop(Math.floor(i / layout.cols))} width={width} height={cardH} onToggle={onToggle} onPick={onPick} culled={culled} />;
        })}
        {showStrip && openStack && (
          <div className="burst-expanded" style={{ top: layout.expandedTop, height: EXPANDED_H }}>
            <div className="burst-expanded-head">
              <b>{openStack.title} · {openStack.frames.length} {openStack.photoWord ?? 'frames'}{openStack.total > openStack.frames.length ? ` of ${openStack.total}` : ''}</b>
              {!openStack.keepers.length && openStack.lead.burst_key != null && softBursts?.get(openStack.lead.burst_key) && (
                <span className="no-keeper-note">· No keeper: all {softBursts.get(openStack.lead.burst_key)!.size} frames are soft</span>
              )}
              <span className="muted" style={{ marginLeft: 8 }}>· In shooting order · ← → to move, Esc to close</span>
            </div>
            <div className="stack-actions">
              <span className={`stack-kept${openKept ? ' has' : ' none'}`} title={`${openKept} of ${openFrames.length} frames in this stack are kept`}>kept {openKept} / {openFrames.length}</span>
              {openLowStar && <span className="low-stars inline" title={`No highly rated frame in this burst (best is ${openBest}★)`}>{openLowStar}</span>}
              <button type="button" className="ghost small" title="Keep the focused frame and cull every other frame of this stack (B)"
                onClick={() => onStackAction?.(openStack, 'keep')}>Keep this, cull rest</button>
              <button type="button" className="ghost small" title="Cull every frame of this stack (Shift+B)"
                onClick={() => onStackAction?.(openStack, 'cull')}>Cull stack</button>
            </div>
            <BurstFilmstrip
              frames={openStack.frames}
              keeperIds={keeperIds}
              selectedId={selectedId}
              culled={culled}
              rankOf={(frame) => openStack.ranks?.get(frame.id)}
              composition
              onSelect={onSelect}
              onOpen={onOpen}
              onDecide={onDecide}
            />
          </div>
        )}
      </div>
      {!stacks.length && empty}
    </div>
  );
}
