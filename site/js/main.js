import { GEAR_READY } from './state.js';
import { wireNav } from './nav.js';
import { wireReveal } from './reveal.js';
import { loadSections } from './sections.js';
import { loadRelease } from './release.js';
import { loadGearStats } from './gear.js';
import { detectLang, applyLang, reapplyLang, wireLangSwitch } from './i18n.js';

wireNav();
// The counts land after the first language pass; redo it so {gear} resolves.
document.addEventListener(GEAR_READY, reapplyLang);

// Sections must be in the DOM before anything that queries them: the reveal
// observer, the language pass and the live-number setters all walk the page.
loadSections().then(() => {
  wireReveal(document);
  wireLangSwitch(document);
  loadRelease().then(() => applyLang(detectLang()));
  loadGearStats();
});
