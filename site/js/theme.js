// Responsibility: switches the page between the light and dark theme and remembers the choice.
const STORAGE_KEY = 'openrig-theme';

// Fired after the toggle flips the theme, so theme-dependent images can follow.
export const THEME_CHANGED = 'openrig:theme-changed';

function currentTheme() {
  const set = document.documentElement.dataset.theme;
  if (set) return set;
  return matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

export function wireTheme() {
  document.querySelectorAll('[data-theme-toggle]').forEach(btn => btn.addEventListener('click', () => {
    const next = currentTheme() === 'dark' ? 'light' : 'dark';
    document.documentElement.dataset.theme = next;
    try { localStorage.setItem(STORAGE_KEY, next); } catch (e) { /* private mode: the choice lasts this visit */ }
    document.dispatchEvent(new Event(THEME_CHANGED));
  }));
}
