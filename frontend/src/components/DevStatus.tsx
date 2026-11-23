import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { openPath } from '@tauri-apps/plugin-opener';
import { inTauri } from '../lib/api';
import { placeAnchoredMenu } from '../lib/anchoredMenu.mjs';
import {
  DEV_CHIP_TOOLTIP, RELOAD, BUILD_LABEL, RELOAD_UI_LABEL, FOLDER_LABEL,
} from '../lib/devInfo.mjs';
import type { DevInfo, ReloadState } from '../lib/types';

/**
 * The dev chrome in the title bar: a solid DEV chip (identity) and a live-reload
 * chip (state) after the wordmark. The live chip opens the Development popover,
 * anchored exactly like StatusPill (same placement, outside-click and Esc rules).
 * Nothing here renders in normal mode — TitleBar only mounts it when dev info
 * is present, so the DOM is unchanged.
 */
export function DevCluster({ dev, state }: { dev: DevInfo; state: ReloadState }) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLSpanElement>(null);
  const popover = useRef<HTMLDivElement>(null);
  const s = RELOAD[state];

  useLayoutEffect(() => {
    if (!open || !box.current || !popover.current) return;
    const place = () => placeAnchoredMenu(box.current!, popover.current!, 'left');
    place();
    window.addEventListener('resize', place);
    document.addEventListener('scroll', place, true);
    return () => {
      window.removeEventListener('resize', place);
      document.removeEventListener('scroll', place, true);
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const away = (e: PointerEvent) => { if (!box.current?.contains(e.target as Node)) setOpen(false); };
    const esc = (e: KeyboardEvent) => { if (e.key === 'Escape') setOpen(false); };
    window.addEventListener('pointerdown', away);
    window.addEventListener('keydown', esc);
    return () => { window.removeEventListener('pointerdown', away); window.removeEventListener('keydown', esc); };
  }, [open]);

  const openFolder = () => { if (inTauri) void openPath(dev.path).catch(() => undefined); };

  return (
    <span className="devcluster" ref={box}>
      <span className="dev-flag" title={DEV_CHIP_TOOLTIP(dev.home)}>DEV</span>
      <button
        type="button"
        className={`dev-live ${s.cls}`}
        title={s.tooltip(dev.url)}
        aria-expanded={open}
        aria-haspopup="dialog"
        onClick={() => setOpen((v) => !v)}
      >
        <span className="light" />
        <span className="dev-live-text">{s.label}</span>
      </button>
      {open && (
        <div className="popover dev-popover" role="dialog" aria-label="Development" ref={popover}>
          <div className="popover-head">
            <h3>Development</h3>
            <span className={`state ${s.stateCls}`}>{s.state}</span>
          </div>
          <div className="readout">
            <div className="read"><span className="lbl">Build</span><span className="val mono">{BUILD_LABEL(dev.version, dev.commit)}</span></div>
            <div className="read"><span className="lbl">UI source</span><span className="val">{s.ui(dev.url)}</span></div>
            <div className="read"><span className="lbl">Data home</span><span className="val mono strong">{dev.home}</span></div>
          </div>
          <p className="small-print">Started with <span className="mono">--dev</span>. Production Conrod keeps its library in <span className="mono">~/.conrod</span>; this instance never reads or writes it.</p>
          <div className="popover-actions">
            <button type="button" className="ghost small" onClick={() => location.reload()}>{RELOAD_UI_LABEL}</button>
            <button type="button" className="ghost small" onClick={openFolder}>{FOLDER_LABEL}</button>
          </div>
        </div>
      )}
    </span>
  );
}
