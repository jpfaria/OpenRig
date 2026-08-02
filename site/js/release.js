import { release } from './state.js';

// Version, download links and changelog are never hardcoded: pulled live from the
// GitHub Releases API so the site always matches whatever was last published.
const API = 'https://api.github.com/repos/jpfaria/OpenRig/releases';
// Small counts read as unimpressive rather than reassuring — only show once it's a real number.
const DOWNLOADS_DISPLAY_THRESHOLD = 1000;

function coreVersion(tag) { return (tag || '').replace(/^v/, '').split('-')[0].split('.').map(Number); }
function isNewerCore(a, b) {
  for (let i = 0; i < 3; i++) { const x = a[i] || 0, y = b[i] || 0; if (x !== y) return x > y; }
  return false;
}

function readReleases(list) {
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

function paint() {
  const btn = document.getElementById('dl-download-btn');
  if (btn && release.downloadUrl) btn.href = release.downloadUrl;
  const cl = document.getElementById('dl-changelog-link');
  if (cl) cl.href = release.changelogUrl;
  const dlCount = document.getElementById('dl-downloads');
  if (dlCount && release.totalDownloads >= DOWNLOADS_DISPLAY_THRESHOLD) dlCount.style.display = '';
  const betaWrap = document.getElementById('dl-beta');
  const betaLink = document.getElementById('dl-beta-link');
  if (betaWrap && betaLink && release.betaUrl) {
    betaLink.href = release.betaUrl;
    betaWrap.style.display = '';
  }
}

export async function loadRelease() {
  try {
    const r = await fetch(API);
    if (r.ok) readReleases(await r.json());
  } catch (e) { /* offline or rate-limited: keep the GitHub releases page as fallback */ }
  paint();
}
