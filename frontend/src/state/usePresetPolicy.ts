import { useEffect, useState } from 'react';
import { engine } from '../lib/api';
import { applyOverrides, defaultSummary, type Overrides, type PolicySummary } from '../lib/presetPolicy.mjs';

/** What Conrod will do for a shoot type with an album's switches. Paints at once from the browser's copy of the
 * defaults, then takes the engine's own answer (the engine is the source of truth). */
export function usePresetPolicy(preset: string, overrides: Overrides): PolicySummary {
  const local = () => applyOverrides(defaultSummary(preset), overrides);
  const [summary, setSummary] = useState<PolicySummary>(local);
  const key = `${preset}|${JSON.stringify(overrides)}`;
  useEffect(() => {
    let live = true;
    setSummary(local());
    engine.presetPolicy(preset, overrides).then((s) => { if (live) setSummary(s); }).catch(() => {});
    return () => { live = false; };
  }, [key]); // eslint-disable-line react-hooks/exhaustive-deps -- `key` covers both inputs
  return summary;
}
