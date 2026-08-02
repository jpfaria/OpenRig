// Everything the page displays that isn't copy: filled in by release.js and gear.js,
// read back by i18n.js when it interpolates {version}, {downloads}, {gear}.
export const release = {
  version: '',
  downloadUrl: 'https://github.com/jpfaria/OpenRig/releases',
  changelogUrl: 'https://github.com/jpfaria/OpenRig/blob/develop/CHANGELOG.md',
  betaVersion: '',
  betaUrl: '',
  totalDownloads: 0,
};

// null until both catalog sources answer — the copy keeps its markup fallback until then.
export const gear = { total: null };

// Fired once the catalog totals land, so the language pass can run again with {gear} resolved.
export const GEAR_READY = 'openrig:gear-ready';

export function fmtNum(n) { return n.toLocaleString('en-US'); }
export function setNum(id, val) {
  if (val === undefined || val === null) return;
  const el = document.getElementById(id);
  if (el) el.textContent = fmtNum(val);
}
