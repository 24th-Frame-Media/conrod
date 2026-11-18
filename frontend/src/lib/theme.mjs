/** The one theme switch: a `data-theme` attribute on <html>, remembered per machine. */

export const THEME_KEY = 'conrod-theme';

export function asTheme(value) {
  return value === 'light' ? 'light' : 'dark';
}

/** The saved theme, or the attribute already on the document (the dev mock may set it from the URL). */
export function storedTheme() {
  try {
    const saved = localStorage.getItem(THEME_KEY);
    if (saved) return asTheme(saved);
  } catch { /* storage can be unavailable */ }
  return asTheme(document.documentElement.dataset.theme);
}

/** Apply and remember a theme. Returns the theme actually set. */
export function applyTheme(value) {
  const theme = asTheme(value);
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = theme;
  try { localStorage.setItem(THEME_KEY, theme); } catch { /* not persisted */ }
  return theme;
}
