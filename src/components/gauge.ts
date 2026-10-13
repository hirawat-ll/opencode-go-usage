/**
 * The triple ring gauge: one arc per usage window, concentric.
 *
 * Structure encodes meaning: the time horizon grows outward (5-hour inside,
 * month outside), so the same shape is readable at any glance. Each arc is
 * coloured by its own severity, and the centre reads out the window that is
 * closest to its limit.
 */
import { clampPercent } from "../format";
import type { Severity, Usage } from "../types";
import { WINDOW_ORDER, severityOf } from "../types";

const SIZE = 190;
const C = SIZE / 2;

/** Inner = 5-hour, middle = week, outer = month. */
export const RING_RADIUS: Record<string, number> = {
  rolling: 36,
  weekly: 59,
  monthly: 82,
};

const STROKE = 10;
const SWEEP_START = -135;
const SWEEP_END = 135;

function pointOn(r: number, deg: number): [number, number] {
  const rad = (deg * Math.PI) / 180;
  return [C + r * Math.sin(rad), C - r * Math.cos(rad)];
}

function arcPath(r: number, fromDeg: number, toDeg: number): string {
  const [x1, y1] = pointOn(r, fromDeg);
  const [x2, y2] = pointOn(r, toDeg);
  const largeArc = Math.abs(toDeg - fromDeg) > 180 ? 1 : 0;
  return `M ${x1.toFixed(2)} ${y1.toFixed(2)} A ${r} ${r} 0 ${largeArc} 1 ${x2.toFixed(2)} ${y2.toFixed(2)}`;
}

/** Dash offset for a ring at the given percent (value arcs use pathLength="100"). */
export function ringOffsetFor(percent: number): number {
  return 100 - clampPercent(percent);
}

/** Rain: value arcs use pathLength=100 so percent maps straight to dash units. */
function ring(radius: number, percent: number, severity: Severity): string {
  const path = arcPath(radius, SWEEP_START, SWEEP_END);
  const offset = ringOffsetFor(percent);
  return `
    <path class="ring-track" d="${path}" stroke-width="${STROKE}" />
    <path
      class="ring-value sev-${severity}"
      d="${path}"
      stroke-width="${STROKE}"
      pathLength="100"
      stroke-dasharray="100 100"
      style="stroke-dashoffset:${offset.toFixed(2)}"
    />`;
}

function ticks(): string {
  const out: string[] = [];
  for (let i = 0; i <= 4; i++) {
    const deg = SWEEP_START + (i / 4) * (SWEEP_END - SWEEP_START);
    const [x1, y1] = pointOn(90, deg);
    const [x2, y2] = pointOn(93, deg);
    out.push(
      `<line x1="${x1.toFixed(1)}" y1="${y1.toFixed(1)}" x2="${x2.toFixed(1)}" y2="${y2.toFixed(1)}" />`,
    );
  }
  return `<g class="ticks">${out.join("")}</g>`;
}

/**
 * Renders the gauge only — the centre readout lives in HTML next to it so it
 * can use the normal type scale.
 */
export function gaugeSvg(usage: Usage): string {
  const rings = WINDOW_ORDER.map((key) =>
    ring(RING_RADIUS[key], usage[key].percent, severityOf(usage[key].percent)),
  ).join("");
  return `
    <svg class="gauge" viewBox="0 0 ${SIZE} ${SIZE}" role="img" aria-label="Usage by window">
      ${ticks()}
      ${rings}
    </svg>`;
}

/**
 * Tiny three-circle swatch used in the legend. The row's own ring is drawn in
 * its severity colour, the others stay faint — the legend teaches the mapping.
 */
const SWATCH_SIZE = 28;
const SW_C = SWATCH_SIZE / 2;
const SW_RADIUS: Record<string, number> = { rolling: 4.4, weekly: 8, monthly: 11.4 };

function swatchPointOn(r: number, deg: number): [number, number] {
  const rad = (deg * Math.PI) / 180;
  return [SW_C + r * Math.sin(rad), SW_C - r * Math.cos(rad)];
}

function swatchArc(r: number): string {
  const [x1, y1] = swatchPointOn(r, SWEEP_START);
  const [x2, y2] = swatchPointOn(r, SWEEP_END);
  return `M ${x1.toFixed(2)} ${y1.toFixed(2)} A ${r} ${r} 0 1 1 ${x2.toFixed(2)} ${y2.toFixed(2)}`;
}

export function swatchSvg(active: string, severity: Severity, activePercent: number): string {
  const rings = WINDOW_ORDER.map((key) => {
    const path = swatchArc(SW_RADIUS[key]);
    if (key !== active) {
      return `<path class="sw-track" d="${path}" />`;
    }
    const p = Math.min(100, Math.max(3, clampPercent(activePercent)));
    return `<path class="sw-value sev-${severity}" d="${path}" pathLength="100" stroke-dasharray="100 100" style="stroke-dashoffset:${(100 - p).toFixed(1)}" />`;
  }).join("");
  return `<svg class="swatch" viewBox="0 0 ${SWATCH_SIZE} ${SWATCH_SIZE}" aria-hidden="true">${rings}</svg>`;
}
