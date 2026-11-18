import { useEffect, useRef, useState } from 'react';
import { Icon, Portal, useBackdropClose } from './basics';
import { call } from '../lib/api';
import { formatAperture, formatShutter, type ShootStats } from '../lib/shot.mjs';
import {
  afParts, bucketRows, cameraRows, exposureChips, exposureTiles, lensRows, qualityParts, shootMiniStats,
  shootTiles, type BucketRow,
} from '../lib/shotStats.mjs';

type Props = {
  visible: boolean;
  jobId: number;
  jobLabel: string;
  onClose: () => void;
  /** The first read backfills EXIF into the album's frames, so Review reloads its shot fields. */
  onStatsLoaded?: () => void;
};

/** The top values of one exposure bucket: a label, a count and a thin share bar. */
function Distribution({ title, rows }: { title: string; rows: BucketRow[] }) {
  return (
    <div className="shoot-stats-dist">
      <h4>{title}</h4>
      {rows.map((row) => (
        <div className="shoot-stats-bucket" key={row.value}>
          <div className="shoot-stats-row-head">
            <span className="shoot-stats-name"><span className="text">{row.label}</span></span>
            <span className="shoot-stats-n">{row.countLabel}</span>
          </div>
          <div className="shoot-stats-bar"><i style={{ width: `${row.bar ?? 0}%` }} /></div>
        </div>
      ))}
    </div>
  );
}

/** The album's shot statistics: hero tiles, then gear, exposure and subject quality. */
export function ShootStatsModal({ visible, jobId, jobLabel, onClose, onStatsLoaded }: Props) {
  const [stats, setStats] = useState<ShootStats | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const backdrop = useBackdropClose(onClose);
  // The parent passes a fresh callback each render; keeping it in a ref means the fetch below runs
  // when the dialog opens (or the album changes), not on every parent render.
  const loaded = useRef(onStatsLoaded);
  loaded.current = onStatsLoaded;

  // Read only while the dialog is open; a closed or reopened dialog drops the stale response
  // instead of painting it over the next album's numbers.
  useEffect(() => {
    if (!visible) return;
    let live = true;
    setStats(null);
    setError(null);
    setLoading(true);
    call<ShootStats>('shoot_stats', { jobId })
      .then((next) => {
        if (!live) return;
        setStats(next);
        // Only a first read (which backfills lens/ISO/size) changes the frames payload; a cached
        // read must not make Review reload 15 MB for nothing.
        if (next.backfilled > 0) loaded.current?.();
      })
      .catch((e) => {
        if (live) setError(String(e));
      })
      .finally(() => {
        if (live) setLoading(false);
      });
    return () => {
      live = false;
    };
  }, [visible, jobId]);

  if (!visible) return null;

  const tiles = stats ? shootTiles(stats) : [];
  const cameras = stats ? cameraRows(stats) : [];
  const lenses = stats ? lensRows(stats) : { rows: [], more: 0 };
  const exposure = stats ? exposureTiles(stats) : [];
  const fStops = stats ? bucketRows(stats.f_numbers, formatAperture, stats.frames) : [];
  const shutters = stats ? bucketRows(stats.shutters, formatShutter, stats.frames) : [];
  const chips = stats ? exposureChips(stats) : [];
  const quality = stats ? qualityParts(stats) : [];
  const af = stats ? afParts(stats) : null;
  const mini = stats ? shootMiniStats(stats) : [];
  const hasGear = cameras.length > 0 || lenses.rows.length > 0;
  const hasExposure = exposure.length > 0 || fStops.length > 0 || shutters.length > 0 || chips.length > 0;
  const hasQuality = quality.length > 0 || mini.length > 0 || (af?.items.length ?? 0) > 0;
  const hasData = tiles.length > 0 || hasGear || hasExposure || hasQuality;

  return (
    <Portal>
    <div className="import-overlay" {...backdrop}>
      <div className="import-dialog shoot-stats-dialog">
        <button className="ghost icon import-close" aria-label="Close" onClick={onClose}>
          <Icon name="x" />
        </button>
        <h2>Shoot statistics</h2>
        <p className="lede">{jobLabel ? `For “${jobLabel}”` : 'This shoot'}</p>

        {loading && <p className="muted shoot-stats-note">Reading shot data…</p>}
        {error != null && (
          <p className="error-text shoot-stats-error" title={error}>
            Shot statistics could not be read.
          </p>
        )}

        {stats != null && hasData && (
          <div className="shoot-stats-body">
            {tiles.length > 0 && (
              <section className="shoot-stats-section">
                <div className="shoot-stats-tiles">
                  {tiles.map((tile) => (
                    <div className="shoot-stats-tile" key={tile.key}>
                      <span className="shoot-stats-label">{tile.label}</span>
                      <span className="shoot-stats-value">{tile.value}</span>
                      {tile.sub != null && <span className="shoot-stats-sub">{tile.sub}</span>}
                    </div>
                  ))}
                </div>
              </section>
            )}

            {hasGear && (
              <section className="shoot-stats-section">
                <h3>Gear</h3>
                <div className="shoot-stats-list">
                  {cameras.map((camera) => (
                    <div className="shoot-stats-row" key={camera.name}>
                      <span className="shoot-stats-name" title={camera.name}>
                        <span className="text">{camera.name}</span>
                      </span>
                      <span className="shoot-stats-n">{camera.countLabel}</span>
                    </div>
                  ))}
                </div>
                {lenses.rows.length > 0 && (
                  <div className="shoot-stats-list">
                    <span className="shoot-stats-subhead">Lenses</span>
                    {lenses.rows.map((lens) => (
                      <div className="shoot-stats-bar-row" key={lens.name}>
                        <div className="shoot-stats-row-head">
                          <span className="shoot-stats-name" title={lens.name}>
                            <span className="text">{lens.name}</span>
                            {lens.mostUsed && <span className="shoot-stats-pill">Most used</span>}
                          </span>
                          <span className="shoot-stats-n">{lens.countLabel}</span>
                        </div>
                        <div className="shoot-stats-bar"><i style={{ width: `${lens.share ?? 0}%` }} /></div>
                      </div>
                    ))}
                    {lenses.more > 0 && <p className="muted shoot-stats-more">{lenses.more} more</p>}
                  </div>
                )}
              </section>
            )}

            {hasExposure && (
              <section className="shoot-stats-section">
                <h3>Exposure</h3>
                {exposure.length > 0 && (
                  <div className="shoot-stats-tiles shoot-stats-tiles-exposure">
                    {exposure.map((tile) => (
                      <div className="shoot-stats-tile" key={tile.key}>
                        <span className="shoot-stats-label">{tile.label}</span>
                        <span className="shoot-stats-value">{tile.value}</span>
                      </div>
                    ))}
                  </div>
                )}
                {(fStops.length > 0 || shutters.length > 0) && (
                  <div className="shoot-stats-dists">
                    {fStops.length > 0 && <Distribution title="f-stops" rows={fStops} />}
                    {shutters.length > 0 && <Distribution title="Shutters" rows={shutters} />}
                  </div>
                )}
                {chips.length > 0 && (
                  <div className="shoot-stats-chips">
                    {chips.map((chip) => <span className="shoot-stats-chip" key={chip}>{chip}</span>)}
                  </div>
                )}
              </section>
            )}

            {hasQuality && (
              <section className="shoot-stats-section">
                <h3>Quality</h3>
                {quality.length > 0 && (
                  <>
                    <div className="shoot-stats-stack" role="img" aria-label="Subject sharpness">
                      {quality.map((segment) => (
                        <i
                          key={segment.key}
                          className={`tone-${segment.tone}`}
                          style={{ flexGrow: segment.percent ?? 0 }}
                          title={`${segment.label}: ${segment.countLabel} (${segment.percent}%)`}
                        />
                      ))}
                    </div>
                    <div className="shoot-stats-legend">
                      {quality.map((segment) => (
                        <span key={segment.key}>
                          <span className={`dot tone-${segment.tone}`} />
                          {segment.label} <b>{segment.countLabel}</b>
                          <span className="shoot-stats-pct">{segment.percent}%</span>
                        </span>
                      ))}
                    </div>
                  </>
                )}
                {mini.length > 0 && (
                  <div className="shoot-stats-mini">
                    {mini.map((item) => (
                      <div key={item.key}>
                        <b>{item.value}</b>
                        <span>{item.label}</span>
                      </div>
                    ))}
                  </div>
                )}
                {af != null && af.items.length > 0 && (
                  <div className="shoot-stats-chips">
                    {af.items.map((item) => (
                      <span className="shoot-stats-chip" key={item.key}>{item.countLabel} {item.label}</span>
                    ))}
                  </div>
                )}
              </section>
            )}
          </div>
        )}

        {stats != null && !hasData && !loading && (
          <p className="muted shoot-stats-note">No shot data in this shoot yet.</p>
        )}
        {stats != null && stats.backfilled > 0 && (
          <p className="muted shoot-stats-note">
            Read shot data for {stats.backfilled.toLocaleString()} frames.
          </p>
        )}
      </div>
    </div>
    </Portal>
  );
}
