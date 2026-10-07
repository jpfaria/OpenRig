// Responsibility: shows each app screenshot in the app theme that matches the page theme.
import { THEME_CHANGED } from './theme.js';

// A <picture data-themed> holds one <source> with the light shot; its media query decides.
function lightShotMedia() {
  const forced = document.documentElement.dataset.theme;
  if (forced === 'light') return 'all';
  if (forced === 'dark') return 'not all';
  return '(prefers-color-scheme: light)';
}

export function syncShots(root) {
  const media = lightShotMedia();
  root.querySelectorAll('picture[data-themed] source').forEach(s => { s.media = media; });
}

export function wireShotTheme() {
  syncShots(document);
  document.addEventListener(THEME_CHANGED, () => syncShots(document));
}
