// The page is one file per section under sections/; index.html only carries the
// shell (nav, hero, footer) and the placeholders these get injected into.
export async function loadSections() {
  await Promise.all([...document.querySelectorAll('[data-include]')].map(async slot => {
    try {
      // Revalidate: a cached partial paired with a fresh index.html renders stale copy.
      const r = await fetch(slot.dataset.include, { cache: 'no-cache' });
      if (r.ok) slot.innerHTML = await r.text();
    } catch (e) { /* a missing partial leaves an empty slot rather than killing the page */ }
  }));
}
