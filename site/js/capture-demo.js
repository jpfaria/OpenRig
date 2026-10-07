// Responsibility: drives the capture-pack demo, where the gain knob picks which .nam file is loaded.
// The knob arc covers 270° of a r=62 circle: 2π·62 = 389.56, three quarters of it = 292.17.
const CIRCUMFERENCE = 389.56;
const SWEEP = 292.17;

export function wireCaptureDemo(root) {
  const demo = root.querySelector('[data-capture-demo]');
  if (!demo) return;
  const input = demo.querySelector('input[type=range]');
  const arc = demo.querySelector('.knob-val');
  const pointer = demo.querySelector('.knob-ptr');
  const readout = demo.querySelector('.knob-read');
  const files = demo.querySelectorAll('li[data-step]');
  const paint = () => {
    const value = Number(input.value);
    const f = (value - Number(input.min)) / (Number(input.max) - Number(input.min));
    arc.setAttribute('stroke-dasharray', `${(SWEEP * f).toFixed(2)} ${CIRCUMFERENCE}`);
    pointer.setAttribute('transform', `rotate(${(-135 + 270 * f).toFixed(2)} 80 80)`);
    readout.textContent = value;
    files.forEach(li => li.classList.toggle('on', Number(li.dataset.step) === value));
  };
  input.addEventListener('input', paint);
  paint();
}
