import { useCallback, useMemo, useState } from 'react';
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

  const autoOpen = useMemo(() => shouldShowWhatsNew({ current, lastSeen: seen, entries: ENTRIES }), [current, seen]);
  const { fresh, earlier } = useMemo(() => splitEntries({ entries: ENTRIES, current, lastSeen: seen }), [current, seen]);

  const close = useCallback(() => {
    setOpenedManually(false);
    if (current) setSeen(writeSeen(localStorage, current));
  }, [current]);
  const openManually = useCallback(() => setOpenedManually(true), []);

  return { open: openedManually || autoOpen, fresh, earlier, close, openManually };
}
