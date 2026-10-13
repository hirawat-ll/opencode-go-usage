/**
 * Draws the two pieces of art this app needs, with no image dependencies:
 *
 *   assets/app-icon.png       1024x1024 dock/Finder icon (graphite squircle + gauge)
 *   src-tauri/icons/tray.png  44x44 menu bar template icon (alpha mask only)
 *
 * Both are the same instrument: three concentric arcs, one needle.
 *
 *   node scripts/make-icons.mjs
 */
import { deflateSync } from "node:zlib";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");

/* ------------------------------------------------------------- raster */

function createCanvas(width, height) {
  return { width, height, data: new Float32Array(width * height * 4) };
}

/** src-over compositing of one sample onto the canvas. */
function blend(canvas, x, y, [r, g, b], alpha) {
  if (alpha <= 0) return;
  const i = (y * canvas.width + x) * 4;
  const d = canvas.data;
  const da = d[i + 3];
  const outA = alpha + da * (1 - alpha);
  if (outA <= 0) return;
  d[i] = (r * alpha + d[i] * da * (1 - alpha)) / outA;
  d[i + 1] = (g * alpha + d[i + 1] * da * (1 - alpha)) / outA;
  d[i + 2] = (b * alpha + d[i + 2] * da * (1 - alpha)) / outA;
  d[i + 3] = outA;
}

const SAMPLES = 3; // 3x3 supersampling

/**
 * @param {(x:number, y:number) => ([number[], number][] | null)} shade
 *   returns the layers covering a point, back to front, or null when outside
 */
function render(canvas, shade) {
  const step = 1 / SAMPLES;
  const offset = step / 2;
  const total = SAMPLES * SAMPLES;

  for (let y = 0; y < canvas.height; y++) {
    for (let x = 0; x < canvas.width; x++) {
      let accR = 0;
      let accG = 0;
      let accB = 0;
      let accA = 0;

      for (let sy = 0; sy < SAMPLES; sy++) {
        for (let sx = 0; sx < SAMPLES; sx++) {
          const layers = shade(x + sx * step + offset, y + sy * step + offset);
          if (!layers) continue;

          // Composite this sample's layers, then average samples in
          // premultiplied space so edges get real coverage.
          let sr = 0;
          let sg = 0;
          let sb = 0;
          let sa = 0;
          for (const [rgb, alpha] of layers) {
            const outA = alpha + sa * (1 - alpha);
            if (outA <= 0) continue;
            sr = (rgb[0] * alpha + sr * sa * (1 - alpha)) / outA;
            sg = (rgb[1] * alpha + sg * sa * (1 - alpha)) / outA;
            sb = (rgb[2] * alpha + sb * sa * (1 - alpha)) / outA;
            sa = outA;
          }
          accR += sr * sa;
          accG += sg * sa;
          accB += sb * sa;
          accA += sa;
        }
      }

      if (accA <= 0) continue;
      blend(canvas, x, y, [accR / accA, accG / accA, accB / accA], accA / total);
    }
  }
}

/* ------------------------------------------------------------- shapes */

const hex = (value) => [
  parseInt(value.slice(1, 3), 16),
  parseInt(value.slice(3, 5), 16),
  parseInt(value.slice(5, 7), 16),
];

/** Signed "is inside" for a superellipse (macOS-ish squircle). */
function squircle(x, y, cx, cy, rx, ry, n = 5) {
  const dx = Math.abs((x - cx) / rx);
  const dy = Math.abs((y - cy) / ry);
  return Math.pow(dx, n) + Math.pow(dy, n) <= 1;
}

/** angle in degrees, 0 = up, growing clockwise */
function angleOf(x, y, cx, cy) {
  return (Math.atan2(x - cx, -(y - cy)) * 180) / Math.PI;
}

const norm = (deg) => ((deg % 360) + 360) % 360;

function inArc(x, y, cx, cy, radius, width, from, to) {
  const dx = x - cx;
  const dy = y - cy;
  const dist = Math.hypot(dx, dy);
  const half = width / 2;
  if (dist < radius - half || dist > radius + half) {
    // round caps
    for (const end of [from, to]) {
      const rad = (end * Math.PI) / 180;
      const ex = cx + radius * Math.sin(rad);
      const ey = cy - radius * Math.cos(rad);
      if (Math.hypot(x - ex, y - ey) <= half) return true;
    }
    return false;
  }
  const a = norm(angleOf(x, y, cx, cy));
  const start = norm(from);
  const end = norm(to);
  return start <= end ? a >= start && a <= end : a >= start || a <= end;
}

function inCapsule(x, y, x1, y1, x2, y2, width) {
  const dx = x2 - x1;
  const dy = y2 - y1;
  const len2 = dx * dx + dy * dy;
  let t = len2 === 0 ? 0 : ((x - x1) * dx + (y - y1) * dy) / len2;
  t = Math.max(0, Math.min(1, t));
  const px = x1 + t * dx;
  const py = y1 + t * dy;
  return Math.hypot(x - px, y - py) <= width / 2;
}

const pointAt = (cx, cy, r, deg) => {
  const rad = (deg * Math.PI) / 180;
  return [cx + r * Math.sin(rad), cy - r * Math.cos(rad)];
};

/* -------------------------------------------------------------- icons */

const SWEEP = [-135, 135];

function drawAppIcon() {
  const S = 1024;
  const canvas = createCanvas(S, S);
  const c = S / 2;
  const inset = 40;
  const radius = S / 2 - inset;

  const rings = [
    { r: 150, pct: 0.62, alpha: 0.9 },
    { r: 235, pct: 0.42, alpha: 0.5 },
    { r: 320, pct: 0.86, alpha: 0.28 },
  ];
  const stroke = 46;

  render(canvas, (x, y) => {
    const layers = [];

    if (squircle(x, y, c, c, radius, radius)) {
      // graphite face, lit from the top
      const t = (y - inset) / (S - 2 * inset);
      const top = hex("#2B303A");
      const bottom = hex("#0D0F13");
      const rgb = top.map((ch, i) => Math.round(ch + (bottom[i] - ch) * t));
      layers.push([rgb, 1]);
    } else {
      return null;
    }

    for (const { r, pct, alpha } of rings) {
      const to = SWEEP[0] + (SWEEP[1] - SWEEP[0]) * pct;
      if (inArc(x, y, c, c, r, stroke, SWEEP[0], to)) {
        layers.push([hex("#F4F6F8"), alpha]);
      }
    }

    // The needle: the single accent, at the outer arc's end.
    const outer = rings[2];
    const angle = SWEEP[0] + (SWEEP[1] - SWEEP[0]) * outer.pct;
    const [ax, ay] = pointAt(c, c, 62, angle);
    const [bx, by] = pointAt(c, c, outer.r - stroke, angle);
    if (inCapsule(x, y, ax, ay, bx, by, 26)) {
      layers.push([hex("#4FC08D"), 1]);
    }
    if (Math.hypot(x - c, y - c) <= 30) {
      layers.push([hex("#F4F6F8"), 1]);
    }
    return layers;
  });

  return { canvas, path: join(ROOT, "assets", "app-icon.png") };
}

/**
 * Menu bar art. Template images are drawn from the alpha channel alone, so
 * only coverage matters; RGB stays black.
 */
function drawTrayIcon() {
  const S = 44;
  const canvas = createCanvas(S, S);
  const c = S / 2;
  const radius = 15;
  const stroke = 3.2;

  render(canvas, (x, y) => {
    const layers = [];
    if (inArc(x, y, c, c, radius, stroke, SWEEP[0], SWEEP[1])) {
      layers.push([[0, 0, 0], 0.42]);
    }
    const to = SWEEP[0] + (SWEEP[1] - SWEEP[0]) * 0.68;
    if (inArc(x, y, c, c, radius, stroke, SWEEP[0], to)) {
      layers.push([[0, 0, 0], 1]);
    }
    const angle = SWEEP[0] + (SWEEP[1] - SWEEP[0]) * 0.68;
    const [ax, ay] = pointAt(c, c, 3, angle);
    const [bx, by] = pointAt(c, c, radius - stroke, angle);
    if (inCapsule(x, y, ax, ay, bx, by, 2.4)) {
      layers.push([[0, 0, 0], 1]);
    }
    if (Math.hypot(x - c, y - c) <= 2.4) {
      layers.push([[0, 0, 0], 1]);
    }
    return layers;
  });

  return { canvas, path: join(ROOT, "src-tauri", "icons", "tray.png") };
}

/* --------------------------------------------------------------- png */

const CRC_TABLE = (() => {
  const table = new Int32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c;
  }
  return table;
})();

function crc32(buffer) {
  let c = 0xffffffff;
  for (const byte of buffer) c = CRC_TABLE[(c ^ byte) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([length, body, crc]);
}

function encodePng(canvas) {
  const { width, height, data } = canvas;
  const stride = width * 4;
  const raw = Buffer.alloc((stride + 1) * height);
  for (let y = 0; y < height; y++) {
    raw[y * (stride + 1)] = 0; // no filter
    for (let x = 0; x < width; x++) {
      const i = (y * width + x) * 4;
      const o = y * (stride + 1) + 1 + x * 4;
      const alpha = data[i + 3];
      // Template art is black; app art keeps its colour but un-premultiplies.
      raw[o] = Math.round(Math.max(0, Math.min(255, data[i])));
      raw[o + 1] = Math.round(Math.max(0, Math.min(255, data[i + 1])));
      raw[o + 2] = Math.round(Math.max(0, Math.min(255, data[i + 2])));
      raw[o + 3] = Math.round(Math.max(0, Math.min(255, alpha * 255)));
    }
  }

  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

for (const { canvas, path } of [drawAppIcon(), drawTrayIcon()]) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, encodePng(canvas));
  console.log(`wrote ${path.replace(`${ROOT}/`, "")} (${canvas.width}x${canvas.height})`);
}
