import { useState } from 'react';
import { formatDate, type WhatsNewEntry } from '../lib/whatsNew.mjs';
import { Icon, Portal, useBackdropClose, type IconName } from './basics';

type Props = {
  visible: boolean;
  fresh: WhatsNewEntry[];
  earlier: WhatsNewEntry[];
  onClose: () => void;
};

const SECTION_ICON: Record<string, IconName> = { New: 'plus', Improved: 'sliders', Fixed: 'check' };

function EntryBlock({ entry }: { entry: WhatsNewEntry }) {
  const date = formatDate(entry.date);
  return (
    <article className="whatsnew-entry">
      <header className="whatsnew-entry-head">
        <h3>v{entry.version}</h3>
        {date && <span className="muted small">{date}</span>}
      </header>
      {entry.highlights && <p className="lede whatsnew-highlights">{entry.highlights}</p>}
      {entry.sections.map((section) => (
        <div className={`whatsnew-section whatsnew-${section.title.toLowerCase()}`} key={section.title}>
          <h4><Icon name={SECTION_ICON[section.title] ?? 'check'} size={13} />{section.title}</h4>
          <ul>{section.items.map((item) => <li key={item}>{item}</li>)}</ul>
        </div>
      ))}
    </article>
  );
}

/** Newest release first, since the version last opened: Settings → "What's new" reopens it any time. */
export function WhatsNewSheet({ visible, fresh, earlier, onClose }: Props) {
  const [showEarlier, setShowEarlier] = useState(false);
  const backdrop = useBackdropClose(onClose);
  if (!visible) return null;
  return (
    <Portal>
      <div className="import-overlay" {...backdrop}>
        <div className="import-dialog whatsnew-sheet">
          <button type="button" className="ghost icon import-close" aria-label="Close" onClick={onClose}><Icon name="x" /></button>
          <h2>What's new</h2>
          <p className="lede">The highlights since you last opened Conrod.</p>

          <div className="whatsnew-list">
            {fresh.map((entry) => <EntryBlock key={entry.version} entry={entry} />)}
          </div>

          {earlier.length > 0 && (
            <details className="whatsnew-earlier" open={showEarlier} onToggle={(e) => setShowEarlier(e.currentTarget.open)}>
              <summary>Earlier releases ({earlier.length})</summary>
              <div className="whatsnew-list">
                {earlier.map((entry) => <EntryBlock key={entry.version} entry={entry} />)}
              </div>
            </details>
          )}

          <div className="modal-actions" style={{ display: 'flex', justifyContent: 'flex-end', gap: 10, marginTop: 18 }}>
            <button type="button" className="primary" onClick={onClose}>Got it</button>
          </div>
        </div>
      </div>
    </Portal>
  );
}
