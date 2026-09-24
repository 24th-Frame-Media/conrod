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
};

type ZoomProps = {
  src?: string;
  alt: string;
  children?: ReactNode;
  className?: string;
  zoomed?: boolean;
  onZoomChange?: (zoomed: boolean) => void;
  focusBox?: [number, number, number, number] | null;
};

function getFitScaleAndSize(containerW: number, containerH: number, naturalW: number, naturalH: number) {
  if (!containerW || !containerH || !naturalW || !naturalH) {
    return { scale1to1: 2.5, imageW: containerW || 800, imageH: containerH || 600 };
  }
  const imgAspect = naturalW / naturalH;
  const contAspect = containerW / containerH;
  let imageW = containerW;
  let imageH = containerH;
  if (imgAspect > contAspect) {
    imageW = containerW;
    imageH = containerW / imgAspect;
  } else {
    imageH = containerH;
    imageW = containerH * imgAspect;
  }
  const scale1to1 = Math.max(1, naturalW / imageW);
  return { scale1to1, imageW, imageH };
}

function clampPan(val: number, scale: number, imageDimension: number, containerDimension: number) {
  const max = Math.max(0, (imageDimension * scale - containerDimension) / 2);
  return Math.max(-max, Math.min(max, val));
}

/** Lightroom-inspired zoom & pan: single-click toggle, true 1:1 resolution, bounded panning, presets. */
export const ZoomPanImage = forwardRef<ZoomPanRef, ZoomProps>(function ZoomPanImage(
  { src, alt, children, className = '', zoomed, onZoomChange, focusBox },
  ref,
) {
  const [view, setView] = useState({ scale: 1, x: 0, y: 0 });
  const [isDragging, setIsDragging] = useState(false);
  const [isWheeling, setIsWheeling] = useState(false);
  const [naturalSize, setNaturalSize] = useState<{ width: number; height: number } | null>(null);

  const root = useRef<HTMLDivElement>(null);
  const wheelTimer = useRef<number | null>(null);
  const drag = useRef<{
    id: number;
    startX: number;
    startY: number;
    ox: number;
    oy: number;
    moved: boolean;
    isBox: boolean;
  } | null>(null);

  const getDimensions = useCallback(() => {
    const box = root.current?.getBoundingClientRect();
    const containerW = box?.width || 800;
    const containerH = box?.height || 600;
    const img = root.current?.querySelector('img');
    const natW = img?.naturalWidth || naturalSize?.width || 0;
    const natH = img?.naturalHeight || naturalSize?.height || 0;
    const { scale1to1, imageW, imageH } = getFitScaleAndSize(containerW, containerH, natW, natH);
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

      let targetX = 0;
      let targetY = 0;
      const box = root.current?.getBoundingClientRect();

      if (box && clientX != null && clientY != null) {
        const clickOffsetX = clientX - (box.left + box.width / 2);
        const clickOffsetY = clientY - (box.top + box.height / 2);
        const imgX = (clickOffsetX - view.x) / view.scale;
        const imgY = (clickOffsetY - view.y) / view.scale;

        if (isClickToCenter || view.scale <= 1.01) {
          targetX = -imgX * scale;
          targetY = -imgY * scale;
        } else {
          targetX = clickOffsetX - imgX * scale;
          targetY = clickOffsetY - imgY * scale;
        }
      } else {
        targetX = (view.x / view.scale) * scale;
        targetY = (view.y / view.scale) * scale;
      }

      const clampedX = clampPan(targetX, scale, imageW, containerW);
      const clampedY = clampPan(targetY, scale, imageH, containerH);
      setView({ scale, x: clampedX, y: clampedY });
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
    }),
    [getDimensions, reset, toggleZoom, view.scale, zoomTo],
  );

  // Sync with external zoomed prop (e.g., Train screen)
  useEffect(() => {
    if (zoomed) {
      const { containerW, containerH, scale1to1, imageW, imageH } = getDimensions();
      if (focusBox) {
        const [x1, y1, x2, y2] = focusBox;
        const cx = (x1 + x2) / 2;
        const cy = (y1 + y2) / 2;
        const bw = Math.max(0.01, x2 - x1);
        const bh = Math.max(0.01, y2 - y1);
        const fitSubjectScale = 0.75 / Math.max(bw, bh);
        const targetScale = Math.max(scale1to1, Math.min(scale1to1 * 2, fitSubjectScale));
        const rawX = -(cx - 0.5) * imageW * targetScale;
        const rawY = -(cy - 0.5) * imageH * targetScale;
        const clampedX = clampPan(rawX, targetScale, imageW, containerW);
        const clampedY = clampPan(rawY, targetScale, imageH, containerH);
        setView({ scale: targetScale, x: clampedX, y: clampedY });
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
    const isBox = Boolean((event.target as HTMLElement).closest('.box'));

    drag.current = {
      id: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      ox: view.x,
      oy: view.y,
      moved: false,
      isBox,
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
      if (cur.isBox) return;
      toggleZoom(event.clientX, event.clientY);
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

  const { scale1to1 } = getDimensions();
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
      <div
        className="zoom-content"
        style={{
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
