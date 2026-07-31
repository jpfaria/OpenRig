import { release, gear, fmtNum } from './state.js';

const SUPPORTED_LANGS = ['en', 'pt-BR', 'es-ES'];
// Each language is split by page area under i18n/<lang>/ instead of one long file.
const PARTS = ['shell', 'gear', 'features', 'end'];
const STORAGE_KEY = 'openrig-lang';
let currentLang = null;

export function detectLang() {
  const saved = localStorage.getItem(STORAGE_KEY);
  if (saved && SUPPORTED_LANGS.includes(saved)) return saved;
  const nl = navigator.language || 'en';
  if (nl.startsWith('pt')) return 'pt-BR';
  if (nl.startsWith('es')) return 'es-ES';
  return 'en';
}

function interpolate(val) {
  // Leave the markup fallback in place until the live count is known.
  if (val.includes('{gear}')) {
    if (gear.total === null) return null;
    val = val.replace('{gear}', fmtNum(gear.total));
  }
  if (release.version) val = val.replace('{version}', release.version);
  if (release.betaVersion) val = val.replace('{betaVersion}', release.betaVersion);
  if (release.totalDownloads) val = val.replace('{downloads}', fmtNum(release.totalDownloads));
  return val;
}

async function loadDict(lang) {
  // Same reason as the partials: a stale dictionary shows English fallbacks for new keys.
  const parts = await Promise.all(PARTS.map(async p => {
    const r = await fetch(`i18n/${lang}/${p}.json`, { cache: 'no-cache' });
    return r.ok ? r.json() : {};
  }));
  return Object.assign({}, ...parts);
}

export async function applyLang(lang) {
  const dict = await loadDict(lang);
  document.documentElement.lang = lang;
  currentLang = lang;
  localStorage.setItem(STORAGE_KEY, lang);
  document.querySelectorAll('[data-i18n]').forEach(el => {
    const raw = dict[el.getAttribute('data-i18n')];
    if (raw === undefined) return;
    const val = interpolate(raw);
    if (val !== null) el.innerHTML = val;
  });
  document.querySelectorAll('[data-lang-switch] button').forEach(b => b.classList.toggle('on', b.dataset.lang === lang));
}

export function reapplyLang() { if (currentLang) applyLang(currentLang); }

export function wireLangSwitch(root) {
  root.querySelectorAll('[data-lang-switch] button').forEach(b => b.addEventListener('click', () => applyLang(b.dataset.lang)));
}
