import { useCallback, useEffect, useState } from 'react';
import { mocked } from '../lib/api';
import type { Settings } from '../lib/types';
import { profileOf, shouldAutoOpenWizard } from '../lib/wizard.mjs';
import { readSeen } from '../lib/whatsNew.mjs';

export type Wizard = {
  /** The first-run check has run: what's new may open now if the wizard did not take its place. */
  decided: boolean;
  open: boolean;
  close: () => void;
  openManually: () => void;
};

/**
 * The setup wizard opens by itself once, on a fresh install: no finished profile, no albums and no What's New
 * seen. It takes What's New's place there (`markSeen`), so the two never stack. Any other start leaves What's
 * New as it was. Settings reopens it any time. `?mock=1&wizard=1` forces it in the browser mock.
 */
export function useWizard(args: { ready: boolean; settings: Settings; albums: number; markSeen: () => void }): Wizard {
  const { ready, settings, albums, markSeen } = args;
  const [decided, setDecided] = useState(false);
  const [open, setOpen] = useState(false);
  useEffect(() => {
    if (!ready || decided) return;
    const forced = mocked && new URLSearchParams(location.search).get('wizard') === '1';
    if (forced) {
      setOpen(true);
    } else if (shouldAutoOpenWizard({ profile: profileOf(settings), albums, whatsNewSeen: readSeen(localStorage) })) {
      setOpen(true);
      markSeen();
    }
    setDecided(true);
  }, [ready]); // eslint-disable-line react-hooks/exhaustive-deps -- decided once, from the first load
  const close = useCallback(() => setOpen(false), []);
  const openManually = useCallback(() => setOpen(true), []);
  return { decided, open, close, openManually };
}
