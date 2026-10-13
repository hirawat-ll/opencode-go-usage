/** Small motion helpers, all of them optional under reduced motion. */

export function prefersReducedMotion(): boolean {
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

/**
 * Counts an element from one value to another.
 *
 * @param el element whose text is rewritten
 * @param target final value, read from the element by the caller
 * @param from value to start at (the previous reading)
 * @param format how to render each step
 */
export function countUp(
  el: HTMLElement,
  from: number,
  target: number,
  format: (value: number) => string,
  duration = 520,
): void {
  if (prefersReducedMotion() || Math.abs(target - from) < 0.4) return;
  const start = performance.now();
  const step = (now: number) => {
    const progress = Math.min(1, (now - start) / duration);
    const eased = 1 - Math.pow(1 - progress, 3);
    el.textContent = format(from + (target - from) * eased);
    if (progress < 1) requestAnimationFrame(step);
  };
  requestAnimationFrame(step);
}
