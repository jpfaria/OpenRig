import { GEAR_READY } from './state.js';
import { wireTheme } from './theme.js';
import { wireReveal } from './reveal.js';
import { loadSections } from './sections.js';
import { loadRelease } from './release.js';
import { loadGearStats } from './gear.js';
import { wireCaptureDemo } from './capture-demo.js';
import { detectLang, applyLang, reapplyLang, wireLangSwitch } from './i18n.js';

wireTheme();
document.addEventListener(GEAR_READY, reapplyLang);
loadSections().then(() => {
  wireReveal(document);
  wireLangSwitch(document);
  wireCaptureDemo(document);
  loadRelease().then(() => applyLang(detectLang()));
  loadGearStats();
});
