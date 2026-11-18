import { useEffect, useState } from 'react';
import { Icon, Portal, useBackdropClose } from './basics';
import { Distribution } from './ShootStatsModal';
import { call } from '../lib/api';
import { formatAperture, formatShutter } from '../lib/shot.mjs';
import {
  afParts, bucketRows, exposureChips, exposureTiles, qualityParts, shootMiniStats,
} from '../lib/shotStats.mjs';
import { completeNote, globalCameraRows, globalLensRows, globalTiles } from '../lib/globalStats.mjs';
import { plural } from '../lib/format';
import type { GlobalStats } from '../lib/types';

type Props = {
  visible: boolean;
  onClose: () => void;
};

/**
 * The whole library's statistics: totals and hit rate, then the gear, exposure and subject
 * quality gathered from the albums whose shot data is complete. Gear and exposure cover fewer
 * albums than the totals on purpose: a partial album would skew every share and histogram.
 */
export function GlobalStatsModal({ visible, onClose }: Props) {
  const [stats, setStats] = useState<GlobalStats | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const backdrop = useBackdropClose(onClose);

  // Read only while the dialog is open; a closed or reopened dialog drops the stale response
  // instead of painting it over the next read's numbers.
  useEffect(() => {
    if (!visible) return;
    let live = true;
    setStats(null);
    setError(null);
    setLoading(true);
    call<GlobalStats>('global_stats')
      .then((next) => {
        if (live) setStats(next);
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
  }, [visible]);

  if (!visible) return null;

  const empty = stats != null && !(stats.frames > 0);
  const tiles = stats ? globalTiles(stats) : [];
  const complete = stats ? Math.max(0, stats.complete_albums) : 0;
  const hasComplete = complete > 0;
  const cameras = stats && hasComplete ? globalCameraRows(stats) : [];
  const lenses = stats && hasComplete ? globalLensRows(stats) : { rows: [], more: 0 };
  const exposure = stats && hasComplete ? exposureTiles(stats) : [];
  const fStops = stats && hasComplete ? bucketRows(stats.f_numbers, formatAperture, stats.complete_frames) : [];
  const shutters = stats && hasComplete ? bucketRows(stats.shutters, formatShutter, stats.complete_frames) : [];
  const chips = stats && hasComplete ? exposureChips(stats) : [];
  const note = stats ? completeNote(stats) : null;
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
        <h2>Library statistics</h2>
        <p className="lede">{stats != null ? `Across ${plural(stats.albums, 'album')}` : 'Every album in this library'}</p>

        {loading && <p className="muted shoot-stats-note">Reading library data…</p>}
        {error != null && (
          <p className="error-text shoot-stats-error" title={error}>
            Library statistics could not be read.
          </p>
        )}

        {stats != null && !empty && hasData && (
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

            {hasComplete && note != null && (
              <p className="muted shoot-stats-note">{note}</p>
            )}
            {!hasComplete && (
              <p className="muted shoot-stats-note">No album has complete shot data yet.</p>
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

        {empty && !loading && (
          <p className="muted shoot-stats-note">No library statistics yet.</p>
        )}
      </div>
    </div>
    </Portal>
  );
}
