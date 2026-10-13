/** Inline stroke icons so the panel never depends on a font glyph. */

const base = (path: string, extra = "") =>
  `<svg class="icon" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" ${extra}>${path}</svg>`;

export const icons = {
  refresh: base(
    `<path d="M13.5 8a5.5 5.5 0 1 1-1.7-3.97" /><path d="M13.6 2.6v3.1h-3.1" />`,
  ),
  gear: base(
    `<circle cx="8" cy="8" r="2.1" /><path d="M8 1.6v1.6M8 12.8v1.6M1.6 8h1.6M12.8 8h1.6M3.5 3.5l1.1 1.1M11.4 11.4l1.1 1.1M12.5 3.5l-1.1 1.1M4.6 11.4l-1.1 1.1" />`,
  ),
  back: base(`<path d="M9.8 3.5 5.3 8l4.5 4.5" />`),
  chevronRight: base(`<path d="M6.3 3.6 10.7 8l-4.4 4.4" />`),
  check: base(`<path d="M3.2 8.4 6.4 11.6 12.8 4.8" />`),
  alert: base(`<path d="M8 5.4v3.4" /><circle cx="8" cy="11.4" r=".6" fill="currentColor" stroke="none" />`),
  eye: base(
    `<path d="M1.9 8S4.3 4.4 8 4.4 14.1 8 14.1 8 11.7 11.6 8 11.6 1.9 8 1.9 8Z" /><circle cx="8" cy="8" r="1.3" />`,
  ),
  eyeOff: base(
    `<path d="M6.4 4.7A6.6 6.6 0 0 1 8 4.4c3.7 0 6.1 3.6 6.1 3.6a12 12 0 0 1-2.2 2.5M4.1 5.6A12 12 0 0 0 1.9 8S4.3 11.6 8 11.6c.7 0 1.3-.1 1.9-.3" /><path d="M2.6 2.6l10.8 10.8" />`,
  ),
  close: base(`<path d="M4.2 4.2 11.8 11.8M11.8 4.2 4.2 11.8" />`),
};
