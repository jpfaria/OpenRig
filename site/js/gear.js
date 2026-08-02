import { gear, setNum, GEAR_READY } from './state.js';

// Gear counts are never hardcoded either: OpenRig-plugins/resume.json (NAM/LV2/VST3/IR
// plugin counts) and this repo's own resume.json (native block counts — plugins can't
// know about those) are fetched live and merged by block_type, every page load.
const PLUGINS_URL = 'https://raw.githubusercontent.com/jpfaria/OpenRig-plugins/main/resume.json';
const NATIVE_URL = 'https://raw.githubusercontent.com/jpfaria/OpenRig/main/resume.json';

async function fetchResumeJson(url) {
  try { const r = await fetch(url); return r.ok ? await r.json() : null; } catch (e) { return null; }
}

function mergeByBlockType(plugins, native) {
  const merged = {};
  new Set([...Object.keys(plugins.by_block_type || {}), ...Object.keys(native.by_block_type || {})])
    .forEach(k => { merged[k] = (plugins.by_block_type?.[k] || 0) + (native.by_block_type?.[k] || 0); });
  return merged;
}

export async function loadGearStats() {
  const [plugins, native] = await Promise.all([fetchResumeJson(PLUGINS_URL), fetchResumeJson(NATIVE_URL)]);
  // Per-source numbers update independently, but the MERGED totals only update when both
  // sources loaded — a partial merge would under-report categories the missing one feeds.
  if (native) setNum('stat-native', native.total_native);
  if (plugins) {
    setNum('stat-nam', plugins.by_backend?.nam);
    setNum('stat-lv2', plugins.by_backend?.lv2);
    setNum('stat-vst3', plugins.by_backend?.vst3);
    setNum('stat-ir', plugins.by_backend?.ir);
    setNum('stat-nam-captures', plugins.captures_by_backend?.nam);
  }
  if (!plugins || !native) return;

  const merged = mergeByBlockType(plugins, native);
  gear.total = plugins.total_plugins + native.total_native;
  setNum('stat-gear', gear.total);
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
  document.dispatchEvent(new Event(GEAR_READY));
}
