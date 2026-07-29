const nav = document.getElementById('nav');
addEventListener('scroll', () => nav.classList.toggle('solid', scrollY > 60), { passive: true });

const io = new IntersectionObserver(es => es.forEach(e => {
  if (e.isIntersecting) { e.target.classList.add('in'); io.unobserve(e.target); }
}), { threshold: .15 });
function wireReveal(root) {
  root.querySelectorAll('.rv').forEach((el, i) => { el.style.transitionDelay = (i % 3) * 70 + 'ms'; io.observe(el); });
}

// The page is one file per section under sections/; index.html only carries the
// shell (nav, hero, footer) and the placeholders these get injected into.
async function loadSections() {
  await Promise.all([...document.querySelectorAll('[data-include]')].map(async slot => {
    try {
      // Revalidate: a cached partial paired with a fresh index.html renders stale copy.
      const r = await fetch(slot.dataset.include, { cache: 'no-cache' });
      if (r.ok) slot.innerHTML = await r.text();
    } catch (e) { /* a missing partial leaves an empty slot rather than killing the page */ }
  }));
}

const SUPPORTED_LANGS = ['en', 'pt-BR', 'es-ES'];
// The hero states the catalog size, so it has to come from the same live source as
// the stats grid rather than a number baked into the copy that goes stale.
let gearTotal = null;
let currentLang = null;
function detectLang() {
  const saved = localStorage.getItem('openrig-lang');
  if (saved && SUPPORTED_LANGS.includes(saved)) return saved;
  const nl = navigator.language || 'en';
  if (nl.startsWith('pt')) return 'pt-BR';
  if (nl.startsWith('es')) return 'es-ES';
  return 'en';
}

// Version, download links and changelog are never hardcoded: pulled live from the
// GitHub Releases API so the site always matches whatever was last published.
let release = { version: '', downloadUrl: 'https://github.com/jpfaria/OpenRig/releases', changelogUrl: 'https://github.com/jpfaria/OpenRig/blob/develop/CHANGELOG.md', betaVersion: '', betaUrl: '', totalDownloads: 0 };

function coreVersion(tag) { return (tag || '').replace(/^v/, '').split('-')[0].split('.').map(Number); }
function isNewerCore(a, b) {
  for (let i = 0; i < 3; i++) { const x = a[i] || 0, y = b[i] || 0; if (x !== y) return x > y; }
  return false;
}

async function loadRelease() {
  try {
    const r = await fetch('https://api.github.com/repos/jpfaria/OpenRig/releases');
    if (r.ok) {
      const list = await r.json();
      const stable = list.find(rel => !rel.prerelease && !rel.draft);
      const beta = list.find(rel => rel.prerelease && !rel.draft);
      release.totalDownloads = list.reduce((sum, rel) => sum + (rel.assets || []).reduce((s, a) => s + (a.download_count || 0), 0), 0);
      if (stable) {
        const asset = (stable.assets || []).find(a => /macos/i.test(a.name));
        release.version = (stable.tag_name || '').replace(/^v/, '');
        release.downloadUrl = asset ? asset.browser_download_url : release.downloadUrl;
        release.changelogUrl = stable.html_url || release.changelogUrl;
      }
      // Only surface the beta link when it's genuinely ahead of stable — a stale
      // pre-release older than what's already shipped stays hidden.
      if (beta && stable && isNewerCore(coreVersion(beta.tag_name), coreVersion(stable.tag_name))) {
        const betaAsset = (beta.assets || []).find(a => /macos/i.test(a.name));
        if (betaAsset) {
          release.betaVersion = (beta.tag_name || '').replace(/^v/, '');
          release.betaUrl = betaAsset.browser_download_url;
        }
      }
    }
  } catch (e) { /* offline or rate-limited: keep the GitHub releases page as fallback */ }
  const btn = document.getElementById('dl-download-btn');
  if (btn && release.downloadUrl) btn.href = release.downloadUrl;
  const cl = document.getElementById('dl-changelog-link');
  if (cl) cl.href = release.changelogUrl;
  // Small counts read as unimpressive rather than reassuring — only show once it's a real number.
  const DOWNLOADS_DISPLAY_THRESHOLD = 1000;
  const dlCount = document.getElementById('dl-downloads');
  if (dlCount && release.totalDownloads >= DOWNLOADS_DISPLAY_THRESHOLD) dlCount.style.display = '';
  const betaWrap = document.getElementById('dl-beta');
  const betaLink = document.getElementById('dl-beta-link');
  if (betaWrap && betaLink && release.betaUrl) {
    betaLink.href = release.betaUrl;
    betaWrap.style.display = '';
  }
}

function fmtNum(n) { return n.toLocaleString('en-US'); }
function setNum(id, val) { if (val === undefined || val === null) return; const el = document.getElementById(id); if (el) el.textContent = fmtNum(val); }

// Gear counts are never hardcoded either: OpenRig-plugins/resume.json (NAM/LV2/VST3/IR
// plugin counts) and this repo's own resume.json (native block counts — plugins can't
// know about those) are fetched live and merged by block_type, every page load.
async function fetchResumeJson(url) {
  try { const r = await fetch(url); return r.ok ? await r.json() : null; } catch (e) { return null; }
}

async function loadGearStats() {
  const [plugins, native] = await Promise.all([
    fetchResumeJson('https://raw.githubusercontent.com/jpfaria/OpenRig-plugins/main/resume.json'),
    fetchResumeJson('https://raw.githubusercontent.com/jpfaria/OpenRig/main/resume.json'),
  ]);
  // Per-source numbers update independently, but the MERGED totals (gearstats grid,
  // the "pieces of gear" total) only update when both sources loaded — a partial
  // merge would under-report categories the missing source contributes to.
  if (native) setNum('stat-native', native.total_native);
  if (plugins) {
    setNum('stat-nam', plugins.by_backend?.nam);
    setNum('stat-lv2', plugins.by_backend?.lv2);
    setNum('stat-vst3', plugins.by_backend?.vst3);
    setNum('stat-ir', plugins.by_backend?.ir);
    setNum('stat-nam-captures', plugins.captures_by_backend?.nam);
  }
  if (!plugins || !native) return;

  const merged = {};
  new Set([...Object.keys(plugins.by_block_type || {}), ...Object.keys(native.by_block_type || {})])
    .forEach(k => { merged[k] = (plugins.by_block_type?.[k] || 0) + (native.by_block_type?.[k] || 0); });

  gearTotal = plugins.total_plugins + native.total_native;
  setNum('stat-gear', gearTotal);
  setNum('gs-amp', merged.amp);
  setNum('gs-preamp', merged.preamp);
  setNum('gs-cab', merged.cab);
  setNum('gs-gain', merged.gain_pedal);
  setNum('gs-reverb', merged.reverb);
  setNum('gs-delay', merged.delay);
  setNum('gs-mod', merged.mod);
  setNum('gs-dyn', merged.dyn);
  setNum('gs-filter', merged.filter);
  setNum('gs-pitch', merged.pitch);
  setNum('gs-body', merged.body);
  setNum('gs-wah', merged.wah);
  // The count landed after the first language pass; redo it so {gear} resolves.
  if (currentLang) applyLang(currentLang);
}

async function applyLang(lang) {
  // Same reason as the partials: a stale dictionary shows English fallbacks for new keys.
  const r = await fetch(`i18n/${lang}.json`, { cache: 'no-cache' });
  const dict = await r.json();
  document.documentElement.lang = lang;
  currentLang = lang;
  localStorage.setItem('openrig-lang', lang);
  document.querySelectorAll('[data-i18n]').forEach(el => {
    const key = el.getAttribute('data-i18n');
    let val = dict[key];
    if (val === undefined) return;
    // Leave the markup fallback in place until the live count is known.
    if (val.includes('{gear}')) {
      if (gearTotal === null) return;
      val = val.replace('{gear}', fmtNum(gearTotal));
    }
    if (release.version) val = val.replace('{version}', release.version);
    if (release.betaVersion) val = val.replace('{betaVersion}', release.betaVersion);
    if (release.totalDownloads) val = val.replace('{downloads}', fmtNum(release.totalDownloads));
    el.innerHTML = val;
  });
  document.querySelectorAll('[data-lang-switch] button').forEach(b => b.classList.toggle('on', b.dataset.lang === lang));
}
function wireLangSwitch(root) {
  root.querySelectorAll('[data-lang-switch] button').forEach(b => b.addEventListener('click', () => applyLang(b.dataset.lang)));
}

// Sections must be in the DOM before anything that queries them: the reveal
// observer, the language pass and the live-number setters all walk the page.
loadSections().then(() => {
  wireReveal(document);
  wireLangSwitch(document);
  loadRelease().then(() => applyLang(detectLang()));
  loadGearStats();
});
