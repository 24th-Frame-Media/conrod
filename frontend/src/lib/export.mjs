/* The export sheet's numbers. Pure, so node --test can import it. */

/** What the album holds, in the words the export sheet shows above the write. */
export function exportSummary(frames, keeperIds) {
  const list = Array.isArray(frames) ? frames : [];
  const keepers = keeperIds instanceof Set ? keeperIds : new Set(keeperIds ?? []);
  let keeper = 0;
  let rejected = 0;
  let undecided = 0;
  for (const f of list) {
    if (keepers.has(f.id)) keeper += 1;
    else if (f.rejected) rejected += 1;
    else undecided += 1;
  }
  return { total: list.length, keepers: keeper, rejected, undecided };
}

/**
 * The line above the buttons: what actually lands in the files. Stars and labels follow the settings;
 * the sheet only promises what the writer always does.
 */
export function exportLine(summary) {
  const n = (v) => (v ?? 0).toLocaleString('en-US');
  return `${n(summary.total)} photos · ${n(summary.keepers)} keepers · ${n(summary.rejected)} rejected · ${n(summary.undecided)} undecided`;
}
