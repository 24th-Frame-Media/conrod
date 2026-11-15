import { memo, useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { asset } from '../lib/api';
import { scrollTopFor, stackLayout, visibleRows, type Stack } from '../lib/stacks.mjs';
import type { Frame } from '../lib/types';
import { BurstFilmstrip } from './BurstFilmstrip';
import { LoadableImage } from './Media';
import { KeeperMark, StarPill } from './Stars';

export type StackView = Stack<Frame> & { title: string; subtitle: string; photoWord?: string; ranks?: Map<number, number> };

const PAD = 20, GAP = 18, MIN_W = 230, LAYER_INSET = 8, CAPTION_H = 38, EXPANDED_H = 250;

type CardProps = { stack: StackView; selected: boolean; open: boolean; left: number; top: number; width: number; height: number; onToggle: (stack: StackView) => void };

const StackCard = memo(function StackCard({ stack, selected, open, left, top, width, height, onToggle }: CardProps) {
  const { lead } = stack;
  const count = stack.frames.length;
  return (
    <article className={`burst-stack${open ? ' open' : ''}${selected ? ' selected' : ''}`} role="listitem" aria-current={selected || undefined}
      style={{ left, top, width, height }} onClick={() => onToggle(stack)}>
      <div className="burst-layers" style={{ '--layers': Math.min(count, 4) } as CSSProperties}>
        <LoadableImage src={asset(lead.thumb_path)} alt={stack.title} />
        <StarPill frame={lead} />
        {count > 1 || stack.total > count ? <span className="stack-count" title={stack.total > count ? `${count} of ${stack.total} frames match the filter` : `${count} frames`}>{stack.total > count ? `${count}/${stack.total}` : count}</span> : null}
        {stack.keepers.length > 0 && <KeeperMark label={lead.picked ? 'Keeper, picked by you' : 'Keeper, the engine\u2019s best of the burst'} />}
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
export function StackGrid({ stacks, selectedKey, selectedId, expandedKey, keeperIds, culled, onToggle, onSelect, onOpen, ariaLabel, empty }: {
  stacks: StackView[]; selectedKey: string | null; selectedId: number | null; expandedKey: string | null; keeperIds: ReadonlySet<number>;
  culled: (frame: Frame) => boolean; onToggle: (stack: StackView) => void; onSelect: (id: number) => void; onOpen: () => void;
  ariaLabel: string; empty: ReactNode;
}) {
  const wrap = useRef<HTMLDivElement>(null);
  const inner = useRef<HTMLDivElement>(null);
  const [size, setSize] = useState({ w: 900, h: 600 });
  const [top, setTop] = useState(0);

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

  return (
    <div className="burst-grid" ref={wrap} onScroll={(e) => setTop(e.currentTarget.scrollTop)}>
      <div className="burst-grid-inner" ref={inner} role="list" aria-label={ariaLabel} style={{ height: layout.height }}>
        {items.map((stack, n) => {
          const i = first * layout.cols + n;
          return <StackCard key={stack.key} stack={stack} selected={stack.key === selectedKey} open={stack.key === expandedKey}
            left={(i % layout.cols) * (width + GAP)} top={layout.rowTop(Math.floor(i / layout.cols))} width={width} height={cardH} onToggle={onToggle} />;
        })}
        {showStrip && openStack && (
          <div className="burst-expanded" style={{ top: layout.expandedTop, height: EXPANDED_H }}>
            <div className="burst-expanded-head">
              <b>{openStack.title} · {openStack.frames.length} {openStack.photoWord ?? 'frames'}{openStack.total > openStack.frames.length ? ` of ${openStack.total}` : ''}</b>
              <span className="muted" style={{ marginLeft: 8 }}>· In shooting order · ← → to move, Esc to close</span>
            </div>
            <BurstFilmstrip
              frames={openStack.frames}
              keeperIds={keeperIds}
              selectedId={selectedId}
              culled={culled}
              rankOf={(frame) => openStack.ranks?.get(frame.id)}
              onSelect={onSelect}
              onOpen={onOpen}
            />
          </div>
        )}
      </div>
      {!stacks.length && empty}
    </div>
  );
}
