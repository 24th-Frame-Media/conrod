import { useState, useEffect } from 'react';
import { call } from '../lib/api';
import { Confirm } from './basics';
import { useRun } from '../state/hooks';

export function Maintenance() {
  const run = useRun();
  const [cache, setCache] = useState<{ total: { files: number; bytes: number } } | null>(null);
  const [health, setHealth] = useState<{ name: string; ready: boolean; detail?: string }[]>([]);
  const [clear, setClear] = useState<string | null>(null);

  useEffect(() => {
    void call<{ total: { files: number; bytes: number } }>('cache_info').then(setCache).catch(() => {});
  }, []);

  return (
    <section className="setting-group maintenance-group">
      <h3>System & Maintenance</h3>

      <div className="maintenance-sections">
        {/* Setup Diagnostics Card */}
        <div className="maintenance-card">
          <div className="maintenance-card-header">
            <div>
              <strong>Setup & Model Diagnostics</strong>
              <p className="hint">Verify that AI detector models, OpenCV runtime, and vision hosts are available.</p>
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
                  <span className={`health-dot ${h.ready ? 'dot-ready' : 'dot-missing'}`} />
                  <span className="health-name">{h.name}</span>
                  <span className="health-detail muted small">{h.detail ?? (h.ready ? 'Ready' : 'Not installed')}</span>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Cache & Storage Card */}
        <div className="maintenance-card">
          <div className="maintenance-card-header">
            <div>
              <strong>Cache & Storage</strong>
              <p className="hint">
                {cache
                  ? `${cache.total.files.toLocaleString()} cached thumbnails and previews · ${(cache.total.bytes / (1024 * 1024)).toFixed(1)} MB`
                  : 'Survey thumbnail, preview, and temporary inspection caches.'}
              </p>
            </div>
            <div className="maintenance-btn-row">
              <button
                type="button"
                onClick={() => void run(async () => setCache(await call('cache_info')))}
              >
                Refresh usage
              </button>
              <button
                type="button"
                onClick={() => setClear('orphaned')}
              >
                Clear unused cache
              </button>
              <button
                type="button"
                onClick={() => setClear('previews')}
              >
                Clear previews
              </button>
            </div>
          </div>
        </div>

        {/* Library Reset / Danger Zone */}
        <div className="maintenance-card danger-card">
          <div className="maintenance-card-header">
            <div>
              <strong className="danger-text">Reset Library Data</strong>
              <p className="hint">Removes all scanned albums, detections, and ratings. Your original RAW and JPEG photos are never modified or deleted.</p>
            </div>
            <button
              type="button"
              className="danger"
              onClick={() => setClear('all')}
            >
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
              ? 'All albums, keeper stacks, and review markers will be removed. Known vehicles, learned focus models, and original photo files will remain untouched.'
              : 'Cached previews and temporary files will be purged. High-resolution previews are re-created automatically whenever you inspect photos.'}
          </p>
        </Confirm>
      )}
    </section>
  );
}
