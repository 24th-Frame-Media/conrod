import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
  type ImgHTMLAttributes,
  type PointerEvent,
  type ReactNode,
  type WheelEvent,
} from 'react';
import { clampPan, fitScaleAndSize, focusView, zoomToPoint } from '../lib/zoom.mjs';

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

export type ZoomPanRef = {
  toggleZoom: (clientX?: number, clientY?: number) => void;
  reset: () => void;
  zoom1to1: () => void;
  zoom2to1: () => void;
  zoomIn: () => void;
  zoomOut: () => void;
  /** Zoom so a fractional (0..1) box fills roughly 3/4 of the view. */
  zoomToBox: (box: [number, number, number, number]) => void;
};

type ZoomProps = {
  src?: string;
  alt: string;
  children?: ReactNode;
  className?: string;
  zoomed?: boolean;
  onZoomChange?: (zoomed: boolean) => void;
  focusBox?: [number, number, number, number] | null;
  /**
   * The loaded photo's own shape (width / height), or null while it is unknown. The stage takes the
   * photo's shape, so a tall photo is never cropped to a wide box when it is zoomed.
   */
  onAspect?: (ratio: number | null) => void;
  /**
   * A single click on the photo that was not a drag: `target` is what was pressed (a detection box, or the
   * stage). When set, the owner handles the click-to-zoom toggle itself instead of the default one.
   */
  onSurfaceClick?: (clientX: number, clientY: number, target: HTMLElement | null) => void;
};

/** Lightroom-inspired zoom & pan: single-click toggle, true 1:1 resolution, bounded panning, presets. */
export const ZoomPanImage = forwardRef<ZoomPanRef, ZoomProps>(function ZoomPanImage(
  { src, alt, children, className = '', zoomed, onZoomChange, focusBox, onAspect, onSurfaceClick },
  ref,
) {
  const [view, setView] = useState({ scale: 1, x: 0, y: 0 });
  const [isDragging, setIsDragging] = useState(false);
  const [isWheeling, setIsWheeling] = useState(false);
  const [naturalSize, setNaturalSize] = useState<{ width: number; height: number } | null>(null);

  const root = useRef<HTMLDivElement>(null);
  const wheelTimer = useRef<number | null>(null);
  useEffect(() => () => { if (wheelTimer.current != null) window.clearTimeout(wheelTimer.current); }, []);
  const drag = useRef<{
    id: number;
    startX: number;
    startY: number;
    ox: number;
    oy: number;
    moved: boolean;
    /** What the press landed on, before pointer capture retargets the events. */
    target: HTMLElement | null;
  } | null>(null);

  /** The owner's callback, kept in a ref so a new function every render never restarts the load. */
  const aspectRef = useRef(onAspect);
  aspectRef.current = onAspect;

  // The stage changes size when the window moves or the photo's shape arrives: measure it again.
  const [, setBoxTick] = useState(0);
  useEffect(() => {
    const el = root.current;
    if (!el || typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(() => setBoxTick((tick) => tick + 1));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const getDimensions = useCallback(() => {
    const box = root.current?.getBoundingClientRect();
    const containerW = box?.width || 800;
    const containerH = box?.height || 600;
    const img = root.current?.querySelector('img');
    const natW = img?.naturalWidth || naturalSize?.width || 0;
    const natH = img?.naturalHeight || naturalSize?.height || 0;
    const { scale1to1, imageW, imageH } = fitScaleAndSize(containerW, containerH, natW, natH);
    return { containerW, containerH, scale1to1, imageW, imageH };
  }, [naturalSize]);

  const reset = useCallback(() => {
    setView({ scale: 1, x: 0, y: 0 });
    onZoomChange?.(false);
  }, [onZoomChange]);

  const zoomTo = useCallback(
    (targetScale: number, clientX?: number, clientY?: number, isClickToCenter = false) => {
      const { containerW, containerH, scale1to1, imageW, imageH } = getDimensions();
      const maxScale = Math.max(16, scale1to1 * 4);
      const scale = Math.max(1, Math.min(maxScale, targetScale));

      if (scale <= 1.01) {
        reset();
        return;
      }

      const box = root.current?.getBoundingClientRect();
      const offsetX = box && clientX != null ? clientX - (box.left + box.width / 2) : null;
      const offsetY = box && clientY != null ? clientY - (box.top + box.height / 2) : null;

      setView(zoomToPoint({
        offsetX,
        offsetY,
        view,
        scale,
        imageW,
        imageH,
        containerW,
        containerH,
        centre: isClickToCenter,
      }));
      onZoomChange?.(true);
    },
    [getDimensions, onZoomChange, reset, view],
  );

  const toggleZoom = useCallback(
    (clientX?: number, clientY?: number) => {
      if (view.scale > 1.05) {
        reset();
      } else {
        const { scale1to1 } = getDimensions();
        zoomTo(scale1to1, clientX, clientY, true);
      }
    },
    [getDimensions, reset, view.scale, zoomTo],
  );

  useImperativeHandle(
    ref,
    () => ({
      toggleZoom: (cx, cy) => toggleZoom(cx, cy),
      reset,
      zoom1to1: () => {
        const { scale1to1 } = getDimensions();
        zoomTo(scale1to1);
      },
      zoom2to1: () => {
        const { scale1to1 } = getDimensions();
        zoomTo(scale1to1 * 2);
      },
      zoomIn: () => zoomTo(view.scale * 1.3),
      zoomOut: () => zoomTo(view.scale / 1.3),
      zoomToBox: (box) => {
        const { containerW, containerH, scale1to1, imageW, imageH } = getDimensions();
        setView(focusView(box, containerW, containerH, imageW, imageH, scale1to1));
        onZoomChange?.(true);
      },
    }),
    [getDimensions, onZoomChange, reset, toggleZoom, view.scale, zoomTo],
  );

  // Sync with the external zoomed prop
  useEffect(() => {
    if (zoomed) {
      const { containerW, containerH, scale1to1, imageW, imageH } = getDimensions();
      if (focusBox) {
        setView(focusView(focusBox, containerW, containerH, imageW, imageH, scale1to1));
      } else {
        setView({ scale: scale1to1, x: 0, y: 0 });
      }
    } else {
      reset();
    }
  }, [src, zoomed, focusBox, getDimensions, reset]);

  const down = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    if ((event.target as HTMLElement).closest('.zoom-tools, button, select, input, a')) return;

    drag.current = {
      id: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      ox: view.x,
      oy: view.y,
      moved: false,
      target: event.target as HTMLElement,
    };
    event.currentTarget.setPointerCapture(event.pointerId);
  };

  const move = (event: PointerEvent<HTMLDivElement>) => {
    const cur = drag.current;
    if (!cur || cur.id !== event.pointerId) return;
    const dx = event.clientX - cur.startX;
    const dy = event.clientY - cur.startY;

    if (!cur.moved && Math.hypot(dx, dy) >= 5) {
      cur.moved = true;
      setIsDragging(true);
    }

    if (cur.moved && view.scale > 1.01) {
      const { containerW, containerH, imageW, imageH } = getDimensions();
      const rawX = cur.ox + dx;
      const rawY = cur.oy + dy;
      const nextX = clampPan(rawX, view.scale, imageW, containerW);
      const nextY = clampPan(rawY, view.scale, imageH, containerH);
      setView((previous) => ({ ...previous, x: nextX, y: nextY }));
    }
  };

  const up = (event: PointerEvent<HTMLDivElement>) => {
    const cur = drag.current;
    if (!cur || cur.id !== event.pointerId) return;
    setIsDragging(false);
    drag.current = null;
    try {
      event.currentTarget.releasePointerCapture(event.pointerId);
    } catch {}

    if (!cur.moved) {
      // A press that lands on a detection box is a click on the photo too; only a drag is excluded.
      if (onSurfaceClick) onSurfaceClick(event.clientX, event.clientY, cur.target);
      else toggleZoom(event.clientX, event.clientY);
    }
  };

  const wheel = (event: WheelEvent<HTMLDivElement>) => {
    event.preventDefault();
    setIsWheeling(true);
    if (wheelTimer.current != null) window.clearTimeout(wheelTimer.current);
    wheelTimer.current = window.setTimeout(() => setIsWheeling(false), 90);
    const factor = Math.exp(-event.deltaY * 0.0025);
    zoomTo(view.scale * factor, event.clientX, event.clientY);
  };

  const { scale1to1, imageW, imageH } = getDimensions();
  const isFit = view.scale <= 1.02;
  const is1to1 = !isFit && Math.abs(view.scale - scale1to1) / scale1to1 < 0.05;
  const is2to1 = !isFit && Math.abs(view.scale - scale1to1 * 2) / (scale1to1 * 2) < 0.05;
  const displayPercent = isFit ? 'Fit' : `${Math.round((view.scale / scale1to1) * 100)}%`;

  return (
    <div
      ref={root}
      className={`zoom-pan ${view.scale > 1.02 ? 'zoomed' : ''} ${isDragging ? 'dragging' : ''} ${className}`}
      onWheel={wheel}
      onPointerDown={down}
      onPointerMove={move}
      onPointerUp={up}
      onPointerCancel={up}
    >
      {/* The photo's own box, centred in the stage: the whole frame is visible at rest, and the
          detection boxes on top of it sit where they belong, on the photo and not on the stage. */}
      <div
        className="zoom-content"
        style={{
          width: imageW,
          height: imageH,
          marginLeft: -imageW / 2,
          marginTop: -imageH / 2,
          transform: `translate(${view.x}px, ${view.y}px) scale(${view.scale})`,
          transition: isDragging || isWheeling ? 'none' : 'transform 0.16s cubic-bezier(0.2, 0, 0.2, 1)',
        }}
      >
        <LoadableImage
          src={src}
          alt={alt}
          draggable={false}
          onLoad={(e) => {
            const img = e.currentTarget;
            if (img.naturalWidth && img.naturalHeight) {
              setNaturalSize({ width: img.naturalWidth, height: img.naturalHeight });
              aspectRef.current?.(img.naturalWidth / img.naturalHeight);
            }
          }}
        />
        {children}
      </div>

      <div className="zoom-tools" role="group" aria-label="Image zoom controls">
        <div className="zoom-presets">
          <button
            type="button"
            className={isFit ? 'active' : ''}
            title="Fit to view (Space / Z)"
            onPointerDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              reset();
            }}
          >
            Fit
          </button>
          <button
            type="button"
            className={is1to1 ? 'active' : ''}
            title="100% 1:1 Native Resolution (Space / Z)"
            onPointerDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              zoomTo(scale1to1);
            }}
          >
            1:1
          </button>
          <button
            type="button"
            className={is2to1 ? 'active' : ''}
            title="200% 2:1 Critical Focus Check"
            onPointerDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              zoomTo(scale1to1 * 2);
            }}
          >
            2:1
          </button>
        </div>

        <div className="zoom-divider" />

        <div className="zoom-stepper">
          <button
            type="button"
            title="Zoom out (−)"
            aria-label="Zoom out"
            onPointerDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              zoomTo(view.scale / 1.3);
            }}
          >
            −
          </button>
          <button
            type="button"
            className="zoom-percent-btn"
            title="Current zoom relative to 100% native resolution (Click to toggle Fit / 1:1)"
            onPointerDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              if (isFit) zoomTo(scale1to1);
              else reset();
            }}
          >
            {displayPercent}
          </button>
          <button
            type="button"
            title="Zoom in (+)"
            aria-label="Zoom in"
            onPointerDown={(e) => e.stopPropagation()}
            onClick={(e) => {
              e.stopPropagation();
              zoomTo(view.scale * 1.3);
            }}
          >
            +
          </button>
        </div>
      </div>
    </div>
  );
});
