const nav = document.getElementById('nav');
addEventListener('scroll', () => nav.classList.toggle('solid', scrollY > 60), { passive: true });

const io = new IntersectionObserver(es => es.forEach(e => {
  if (e.isIntersecting) { e.target.classList.add('in'); io.unobserve(e.target); }
}), { threshold: .15 });
document.querySelectorAll('.rv').forEach((el, i) => { el.style.transitionDelay = (i % 3) * 70 + 'ms'; io.observe(el); });

const SUPPORTED_LANGS = ['en', 'pt-BR', 'es-ES'];
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
  const dlCount = document.getElementById('dl-downloads');
  if (dlCount && release.totalDownloads > 0) dlCount.style.display = '';
  const betaWrap = document.getElementById('dl-beta');
  const betaLink = document.getElementById('dl-beta-link');
  if (betaWrap && betaLink && release.betaUrl) {
    betaLink.href = release.betaUrl;
    betaWrap.style.display = '';
  }
}

function fmtNum(n) { return n.toLocaleString('en-US'); }

async function applyLang(lang) {
  const r = await fetch(`i18n/${lang}.json`);
  const dict = await r.json();
  document.documentElement.lang = lang;
  localStorage.setItem('openrig-lang', lang);
  document.querySelectorAll('[data-i18n]').forEach(el => {
    const key = el.getAttribute('data-i18n');
    let val = dict[key];
    if (val === undefined) return;
    if (release.version) val = val.replace('{version}', release.version);
    if (release.betaVersion) val = val.replace('{betaVersion}', release.betaVersion);
    if (release.totalDownloads) val = val.replace('{downloads}', fmtNum(release.totalDownloads));
    el.innerHTML = val;
  });
  document.querySelectorAll('[data-lang-switch] button').forEach(b => b.classList.toggle('on', b.dataset.lang === lang));
}
document.querySelectorAll('[data-lang-switch] button').forEach(b => b.addEventListener('click', () => applyLang(b.dataset.lang)));

loadRelease().then(() => applyLang(detectLang()));
