// The bar only gets its background once the hero has scrolled past.
export function wireNav() {
  const nav = document.getElementById('nav');
  if (!nav) return;
  addEventListener('scroll', () => nav.classList.toggle('solid', scrollY > 60), { passive: true });
}
