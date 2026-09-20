import { memo, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react';
import { asset } from '../lib/api';
import { filename, frameStars, verdictOf, type FrameFacts } from '../lib/review';
import type { Frame } from '../lib/types';
import { StarPill } from './Stars';

const GAP = 10, MIN_W = 190, BODY_H = 64, PAD = 18;

type CardProps = {
  checked?: boolean; onToggle?: (id: number) => void;
  frame: Frame; facts: FrameFacts | undefined; selected: boolean;
  left: number; top: number; width: number; height: number;
  onSelect: (id: number) => void; onOpen: () => void; onReject: (id: number, rejected: boolean) => void;
};

const Card = memo(function Card({ frame, facts, selected, left, top, width, height, onSelect, onOpen, onReject, checked, onToggle }: CardProps) {
  const verdict = verdictOf(frameStars(frame));
  const cls = ['card', selected && 'selected current', frame.rejected && 'rejected', facts?.cull && 'culled', verdict !== 'none' && `rated-${verdict}`]
    .filter(Boolean).join(' ');
  const swatch = facts?.colourCss && CSS.supports('color', facts.colourCss) ? facts.colourCss : undefined;
  return (
    <div className={cls} style={{ left, top, width, height }} role="option" aria-selected={selected}
      onClick={() => onSelect(frame.id)} onDoubleClick={onOpen}>
      <div className="frame-box">
        {onToggle && <input style={{position: 'absolute', left: 8, top: 8, zIndex: 4}} type="checkbox" aria-label={`Select ${filename(frame.path)}`} checked={checked} onClick={e => e.stopPropagation()} onChange={() => onToggle(frame.id)} />}
        <img className="thumb" loading="lazy" decoding="async" draggable={false} src={asset(frame.thumb_path)} alt={filename(frame.path)} />
        <StarPill frame={frame} />
        {facts?.panned && <span className="focus panning" title="Subject sharp against a blurred background. Kept, never auto-culled">panned</span>}
        {facts?.pick && <span className="focus keeper" title="The sharpest frame of this pass">keeper</span>}
        <button className="icon-btn" aria-label={frame.rejected ? 'Put back' : 'Reject'} title={frame.rejected ? 'Put back (X)' : 'Reject (X)'}
          onClick={(e) => { e.stopPropagation(); onReject(frame.id, !frame.rejected); }}>{frame.rejected ? '↺' : '✕'}</button>
        {facts?.cull && <div className="culled-note">{facts.cull}</div>}
        {frame.status !== 'done' && <span className="stack-count">{frame.status}</span>}
      </div>
      <div className="body">
        <div className="cap" title={frame.path}>{filename(frame.path)}</div>
        <div className="chips">
          {facts?.plate && <span className="fact plate">{facts.plate}</span>}
          {facts?.number && <span className="fact number">#{facts.number}</span>}
          {facts?.vehicle && <span className="tag">{facts.vehicle}</span>}
          {facts?.colour && <span className="tag"><i className="swatch" style={{ background: swatch }} />{facts.colour}</span>}
          {facts?.team && <span className="fact team">{facts.team}</span>}
        </div>
      </div>
    </div>
  );
});

type Props = {
  checked?: Set<number>; onToggle?: (id: number) => void;
  frames: Frame[]; facts: Map<number, FrameFacts>; selected: number | null; empty: ReactNode;
  onSelect: (id: number) => void; onOpen: () => void; onReject: (id: number, rejected: boolean) => void;
};

/** Windowed grid: only the rows in view are mounted, so a 5 000-frame album scrolls like a 50-frame one. */
export function PhotoGrid({ frames, facts, selected, empty, onSelect, onOpen, onReject, checked, onToggle }: Props) {
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
  const height = Math.round((width * 2) / 3) + BODY_H + 2;
  const rowH = height + GAP;
  const rows = Math.ceil(frames.length / cols);
  const first = Math.max(0, Math.floor((top - PAD) / rowH) - 1);
  const last = Math.min(rows, Math.ceil((top + size.h) / rowH) + 1);

  // Keep the selected card in view when the selection moves, not when the list refreshes.
  const layout = useRef({ frames, cols, rowH });
  useEffect(() => { layout.current = { frames, cols, rowH }; });
  useEffect(() => {
    const { frames: list, cols: c, rowH: h } = layout.current;
    const i = list.findIndex((f) => f.id === selected);
    const el = wrap.current;
    if (i < 0 || !el) return;
    const y = Math.floor(i / c) * h;
    const behavior = matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth';
    if (y < el.scrollTop) el.scrollTo({ top: y, behavior });
    else if (y + h + PAD * 2 > el.scrollTop + el.clientHeight) el.scrollTo({ top: y + h + PAD * 2 - el.clientHeight, behavior });
  }, [selected]);

  return (
    <div className="grid-wrap" ref={wrap} onScroll={(e) => setTop(e.currentTarget.scrollTop)}>
      <div className="grid-inner" ref={inner} role="listbox" aria-label="Photographs" style={{ height: Math.max(0, rows * rowH - GAP) }}>
        {frames.slice(first * cols, last * cols).map((f, n) => {
          const i = first * cols + n;
          return (
            <Card checked={checked?.has(f.id)} onToggle={onToggle} key={f.id} frame={f} facts={facts.get(f.id)} selected={selected === f.id}
              left={(i % cols) * (width + GAP)} top={Math.floor(i / cols) * rowH} width={width} height={height}
              onSelect={onSelect} onOpen={onOpen} onReject={onReject} />
          );
        })}
      </div>
      {!frames.length && empty}
    </div>
  );
}
