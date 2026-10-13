/**
 * Observed-peak sparkline: the highest of the three windows as sampled over
 * the last few hours. This is history the API never gives you — it is built
 * from our own polls, so it starts flat and fills in as the app runs.
 */
import { clampPercent } from "../format";
import type { Sample, Severity } from "../types";

const W = 312;
const H = 46;
const PAD = 5;

export function sparklineSvg(samples: Sample[], severity: Severity): string {
  const usable = samples.slice(-360);
  // The dashed hairline at the top is the limit; the bottom edge is zero.
  const limit = `<line class="spark-limit" x1="0" y1="${PAD}" x2="${W}" y2="${PAD}" />`;

  if (usable.length < 2) {
    return `<svg class="spark sev-${severity}" viewBox="0 0 ${W} ${H}" aria-hidden="true">
      ${limit}
    </svg>`;
  }

  const t0 = usable[0].t;
  const t1 = usable[usable.length - 1].t;
  const span = Math.max(1, t1 - t0);

  const x = (t: number) => ((t - t0) / span) * W;
  const y = (peak: number) =>
    H - PAD - (clampPercent(peak) / 100) * (H - PAD * 2);

  const points = usable.map((s) => [x(s.t), y(s.peak)] as const);
  const line = points
    .map(([px, py], i) => `${i === 0 ? "M" : "L"} ${px.toFixed(1)} ${py.toFixed(1)}`)
    .join(" ");
  const area = `${line} L ${W} ${H} L 0 ${H} Z`;
  const [lastX, lastY] = points[points.length - 1];

  return `
    <svg class="spark sev-${severity}" viewBox="0 0 ${W} ${H}" aria-hidden="true">
      <defs>
        <linearGradient id="spark-fill" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" class="spark-stop-a" />
          <stop offset="100%" class="spark-stop-b" />
        </linearGradient>
      </defs>
      ${limit}
      <path class="spark-area" d="${area}" />
      <path class="spark-line" d="${line}" />
      <circle class="spark-dot" cx="${lastX.toFixed(1)}" cy="${lastY.toFixed(1)}" r="2.4" />
    </svg>`;
}

/** Highest sample in the drawn range, for the header readout. */
export function historyPeak(samples: Sample[]): number {
  return samples.slice(-360).reduce((max, s) => Math.max(max, s.peak), 0);
}
