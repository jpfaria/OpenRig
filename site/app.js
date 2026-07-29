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

// Version, download link and changelog are never hardcoded: pulled live from the
// GitHub Releases API so the site always matches whatever was last published.
let release = { version: '', downloadUrl: 'https://github.com/jpfaria/OpenRig/releases', changelogUrl: 'https://github.com/jpfaria/OpenRig/blob/develop/CHANGELOG.md' };
async function loadRelease() {
  try {
    const r = await fetch('https://api.github.com/repos/jpfaria/OpenRig/releases/latest');
    if (r.ok) {
      const data = await r.json();
      const asset = (data.assets || []).find(a => /macos/i.test(a.name));
      release = {
        version: (data.tag_name || '').replace(/^v/, ''),
        downloadUrl: asset ? asset.browser_download_url : release.downloadUrl,
        changelogUrl: data.html_url || release.changelogUrl,
      };
    }
  } catch (e) { /* offline or rate-limited: keep the GitHub releases page as fallback */ }
  const btn = document.getElementById('dl-download-btn');
  if (btn && release.downloadUrl) btn.href = release.downloadUrl;
  const cl = document.getElementById('dl-changelog-link');
  if (cl) cl.href = release.changelogUrl;
}

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
    el.innerHTML = val;
  });
  document.querySelectorAll('#lang-switch button').forEach(b => b.classList.toggle('on', b.dataset.lang === lang));
}
document.querySelectorAll('#lang-switch button').forEach(b => b.addEventListener('click', () => applyLang(b.dataset.lang)));

loadRelease().then(() => applyLang(detectLang()));
