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
  const [scale, setScale] = useState(1);
  const [offset, setOffset] = useState({ x: 0, y: 0 });
  const drag = useRef<{ id: number; x: number; y: number; ox: number; oy: number } | null>(null);
  const clamp = (value: number) => Math.max(1, Math.min(8, value));
  const reset = () => { setScale(1); setOffset({ x: 0, y: 0 }); };
  const changeScale = (next: number) => {
    const value = clamp(next);
    setScale(value);
    if (value === 1) setOffset({ x: 0, y: 0 });
  };

  useEffect(reset, [src]);
  useEffect(() => { if (zoomed !== undefined) changeScale(zoomed ? 2 : 1); }, [zoomed]);

  const down = (event: PointerEvent<HTMLDivElement>) => {
    if (scale <= 1 || event.button !== 0) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = { id: event.pointerId, x: event.clientX, y: event.clientY, ox: offset.x, oy: offset.y };
  };
  const move = (event: PointerEvent<HTMLDivElement>) => {
    const start = drag.current;
    if (!start || start.id !== event.pointerId) return;
    setOffset({ x: start.ox + event.clientX - start.x, y: start.oy + event.clientY - start.y });
  };
  const up = (event: PointerEvent<HTMLDivElement>) => {
    if (drag.current?.id === event.pointerId) drag.current = null;
  };
  const wheel = (event: WheelEvent<HTMLDivElement>) => {
    event.preventDefault();
    changeScale(scale * (event.deltaY < 0 ? 1.2 : 1 / 1.2));
  };

  return (
    <div className={`zoom-pan ${scale > 1 ? 'zoomed' : ''} ${className}`} onWheel={wheel} onPointerDown={down} onPointerMove={move} onPointerUp={up} onPointerCancel={up} onDoubleClick={reset}>
      <div className="zoom-content" style={{ transform: `translate(${offset.x}px, ${offset.y}px) scale(${scale})` }}>
        <LoadableImage src={src} alt={alt} draggable={false} />
        {children}
      </div>
      <div className="zoom-tools" role="group" aria-label="Image zoom">
        <button type="button" title="Zoom out" aria-label="Zoom out" onClick={(e) => { e.stopPropagation(); changeScale(scale / 1.35); }}>−</button>
        <button type="button" title="Reset zoom" onClick={(e) => { e.stopPropagation(); reset(); }}>{Math.round(scale * 100)}%</button>
        <button type="button" title="Zoom in" aria-label="Zoom in" onClick={(e) => { e.stopPropagation(); changeScale(scale * 1.35); }}>+</button>
      </div>
    </div>
  );
}
