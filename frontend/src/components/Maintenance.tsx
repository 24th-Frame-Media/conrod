import { useState, useEffect } from 'react';
import { call } from '../lib/api';
import { Confirm } from './basics';
import { useRun } from '../state/hooks';

/** Diagnostics, cache and the library reset. The Settings page supplies the heading. */
export function Maintenance() {
  const run = useRun();
  const [cache, setCache] = useState<{ total: { files: number; bytes: number } } | null>(null);
  const [health, setHealth] = useState<{ name: string; ready: boolean; detail?: string }[]>([]);
  const [clear, setClear] = useState<string | null>(null);

  useEffect(() => {
    void call<{ total: { files: number; bytes: number } }>('cache_info').then(setCache).catch(() => {});
  }, []);

  return (
    <div className="maintenance-group">
      <div className="maintenance-sections">
        {/* Setup diagnostics */}
        <div className="maintenance-card">
          <div className="maintenance-card-header">
            <div>
              <strong>Setup and models</strong>
              <p className="hint">Check that the detector, plate reader and vision hosts are available.</p>
            </div>
            <div className="maintenance-btn-row">
              <button
                type="button"
                onClick={() => void run(async () => setHealth(await call('health')))}
              >
                Check setup
              </button>
              <button
                type="button"
                onClick={() => void run(async () => { await call('install_models'); setHealth(await call('health')); })}
              >
                Install missing models
              </button>
            </div>
          </div>

          {health.length > 0 && (
            <div className="health-list">
              {health.map((h) => (
                <div className={`health-item ${h.ready ? 'ready' : 'missing'}`} key={h.name}>
                  <span className={`health-dot ${h.ready ? 'dot-ready' : 'dot-missing'}`} aria-hidden="true" />
                  <span className="health-name">{h.name}</span>
                  <span className="health-detail muted small">{h.detail ?? (h.ready ? 'Ready' : 'Not installed')}</span>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Cache and storage */}
        <div className="maintenance-card">
          <div className="maintenance-card-header">
            <div>
              <strong>Cache and storage</strong>
              <p className="hint">
                {cache
                  ? `${cache.total.files.toLocaleString()} cached thumbnails and previews · ${(cache.total.bytes / (1024 * 1024)).toFixed(1)} MB`
                  : 'See how much space thumbnails, previews and temporary files use.'}
              </p>
            </div>
            <div className="maintenance-btn-row">
              <button
                type="button"
                onClick={() => void run(async () => setCache(await call('cache_info')))}
              >
                Refresh usage
              </button>
              <button type="button" onClick={() => setClear('orphaned')}>
                Clear unused cache
              </button>
              <button type="button" onClick={() => setClear('previews')}>
                Clear previews
              </button>
            </div>
          </div>
        </div>

        {/* Library reset */}
        <div className="maintenance-card danger-card">
          <div className="maintenance-card-header">
            <div>
              <strong className="danger-text">Reset library data</strong>
              <p className="hint">Removes scanned albums, detections and ratings. Your original RAW and JPEG files are never modified or deleted.</p>
            </div>
            <button type="button" className="danger" onClick={() => setClear('all')}>
              Reset library…
            </button>
          </div>
        </div>
      </div>

      {clear && (
        <Confirm
          title={clear === 'all' ? 'Reset the library?' : 'Clear cached files?'}
          action={clear === 'all' ? 'Reset everything' : 'Clear cache'}
          onCancel={() => setClear(null)}
          onConfirm={() => {
            void run(async () => {
              await call(clear === 'all' ? 'reset_all' : 'cache_clear', { [clear]: true });
              setCache(await call('cache_info'));
            });
            setClear(null);
          }}
        >
          <p>
            {clear === 'all'
              ? 'All albums, keeper stacks and review markers will be removed. Known vehicles, learned focus models and original photo files will remain untouched.'
              : 'Cached previews and temporary files will be purged. High-resolution previews are re-created whenever you inspect photos.'}
          </p>
        </Confirm>
      )}
    </div>
  );
}
