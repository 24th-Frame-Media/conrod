import { useCallback, useEffect, useMemo, useState } from 'react';
import {
  orderEntries, readSeen, shouldShowWhatsNew, splitEntries, writeSeen, type WhatsNewEntry,
} from '../lib/whatsNew.mjs';
import rawEntries from '../lib/whatsNew.json';
import { inTauri, mocked } from '../lib/api';
import { useAppVersion } from './hooks';

const ENTRIES = rawEntries as WhatsNewEntry[];

export type WhatsNew = {
  /** True while the sheet should be on screen: once per new version, or any time Settings asks for it. */
  open: boolean;
  /** Released since the version last seen, up to this build, newest first (at least the newest). */
  fresh: WhatsNewEntry[];
  /** Everything older, for the collapsed list. */
  earlier: WhatsNewEntry[];
  close: () => void;
  openManually: () => void;
  /** The first-run wizard is taking this build's place: count the release as seen so the sheet stays quiet. */
  markSeen: () => void;
};

/**
 * "What's new": opens itself once per version bump (the version last seen lives in localStorage),
 * and any time from Settings. Outside Tauri (the browser mock, `?mock=1`) there is no real app
 * version to compare against, so the newest release stands in for it, which keeps the show-once
 * behaviour testable without a build.
 */
export function useWhatsNew(): WhatsNew {
  const tauriVersion = useAppVersion();
  const current = tauriVersion ?? (!inTauri && mocked ? orderEntries(ENTRIES)[0]?.version ?? null : null);

  const [seen, setSeen] = useState<string | null>(() => readSeen(localStorage));
  const [openedManually, setOpenedManually] = useState(false);
  // QA only: `?mock=1&whatsnew=0` keeps the sheet out of the way for screenshots.
  // Set by `markSeen` before the app version is known; the mark is written as soon as it is.
  const [muted, setMuted] = useState(() => Boolean(
    mocked && new URLSearchParams(location.search).get('whatsnew') === '0',
  ));

  const autoOpen = useMemo(() => shouldShowWhatsNew({ current, lastSeen: seen, entries: ENTRIES }), [current, seen]);
  const { fresh, earlier } = useMemo(() => splitEntries({ entries: ENTRIES, current, lastSeen: seen }), [current, seen]);

  const close = useCallback(() => {
    setOpenedManually(false);
    if (current) setSeen(writeSeen(localStorage, current));
  }, [current]);
  const openManually = useCallback(() => setOpenedManually(true), []);
  const markSeen = useCallback(() => setMuted(true), []);
  useEffect(() => {
    if (muted && current) setSeen(writeSeen(localStorage, current));
  }, [muted, current]);

  return { open: openedManually || (autoOpen && !muted), fresh, earlier, close, openManually, markSeen };
}
