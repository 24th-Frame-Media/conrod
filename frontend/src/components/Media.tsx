import { useEffect, useRef, useState, type ImgHTMLAttributes, type PointerEvent, type ReactNode, type WheelEvent } from 'react';

type ImageProps = ImgHTMLAttributes<HTMLImageElement> & { src?: string };

/** Never lets the browser's broken-image glyph flash while an asset is still resolving. */
function ImagePayload({ src, className = '', alt = '', onLoad, onError, ...props }: ImageProps) {
  const [state, setState] = useState<'loading' | 'ready' | 'error'>('loading');
  return (
    <span className={`image-load ${state}${className ? ` ${className}-wrap` : ''}`}>
      {src && (
        <img {...props} className={className} src={src} alt={alt}
          onLoad={(event) => { setState('ready'); onLoad?.(event); }}
          onError={(event) => { setState('error'); onError?.(event); }} />
      )}
      {state === 'loading' && <span className="image-spinner" role="status" aria-label={`Loading ${alt || 'image'}`} />}
      {state === 'error' && <span className="image-unavailable" role="img" aria-label={`${alt || 'Image'} unavailable`}>Image unavailable</span>}
    </span>
  );
}

export function LoadableImage(props: ImageProps) {
  return <ImagePayload key={props.src ?? ''} {...props} />;
}

type ZoomProps = {
  src?: string; alt: string; children?: ReactNode; className?: string; zoomed?: boolean;
};

/** Shared wheel/button zoom and pointer pan for the review viewer and training area. */
export function ZoomPanImage({ src, alt, children, className = '', zoomed }: ZoomProps) {
  const [view, setView] = useState({ scale: 1, x: 0, y: 0 });
  const root = useRef<HTMLDivElement>(null);
  const drag = useRef<{ id: number; x: number; y: number; ox: number; oy: number } | null>(null);
  const clamp = (value: number) => Math.max(1, Math.min(8, value));
  const reset = () => setView({ scale: 1, x: 0, y: 0 });
  const changeScale = (factor: number, clientX?: number, clientY?: number) => {
    setView((previous) => {
      const scale = clamp(previous.scale * factor);
      if (scale === 1) return { scale, x: 0, y: 0 };
      const box = root.current?.getBoundingClientRect();
      if (!box || clientX == null || clientY == null) return { ...previous, scale };
      const x = clientX - box.left - box.width / 2;
      const y = clientY - box.top - box.height / 2;
      const ratio = scale / previous.scale;
      return { scale, x: x - (x - previous.x) * ratio, y: y - (y - previous.y) * ratio };
    });
  };

  useEffect(reset, [src]);
  useEffect(() => { if (zoomed !== undefined) setView(zoomed ? { scale: 2.5, x: 0, y: 0 } : { scale: 1, x: 0, y: 0 }); }, [zoomed]);

  const down = (event: PointerEvent<HTMLDivElement>) => {
    if (view.scale <= 1 || event.button !== 0 || (event.target as HTMLElement).closest('button')) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = { id: event.pointerId, x: event.clientX, y: event.clientY, ox: view.x, oy: view.y };
  };
  const move = (event: PointerEvent<HTMLDivElement>) => {
    const start = drag.current;
    if (!start || start.id !== event.pointerId) return;
    setView((previous) => ({ ...previous, x: start.ox + event.clientX - start.x, y: start.oy + event.clientY - start.y }));
  };
  const up = (event: PointerEvent<HTMLDivElement>) => {
    if (drag.current?.id === event.pointerId) drag.current = null;
  };
  const wheel = (event: WheelEvent<HTMLDivElement>) => {
    event.preventDefault();
    changeScale(Math.exp(-event.deltaY * 0.0025), event.clientX, event.clientY);
  };

  return (
    <div ref={root} className={`zoom-pan ${view.scale > 1 ? 'zoomed' : ''} ${className}`} onWheel={wheel} onPointerDown={down} onPointerMove={move} onPointerUp={up} onPointerCancel={up}
      onDoubleClick={(event) => view.scale > 1 ? reset() : changeScale(2.5, event.clientX, event.clientY)}>
      <div className="zoom-content" style={{ transform: `translate(${view.x}px, ${view.y}px) scale(${view.scale})` }}>
        <LoadableImage src={src} alt={alt} draggable={false} />
        {children}
      </div>
      <div className="zoom-tools" role="group" aria-label="Image zoom">
        <button type="button" title="Zoom out" aria-label="Zoom out" onPointerDown={(e) => e.stopPropagation()} onClick={(e) => { e.stopPropagation(); changeScale(1 / 1.5); }}>−</button>
        <button type="button" title="Reset zoom" onPointerDown={(e) => e.stopPropagation()} onClick={(e) => { e.stopPropagation(); reset(); }}>{Math.round(view.scale * 100)}%</button>
        <button type="button" title="Zoom in" aria-label="Zoom in" onPointerDown={(e) => e.stopPropagation()} onClick={(e) => { e.stopPropagation(); changeScale(1.5); }}>+</button>
      </div>
    </div>
  );
}
