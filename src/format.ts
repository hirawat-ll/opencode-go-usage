/** Small formatting helpers. All output is meant to fit a 340pt panel. */

export function clampPercent(n: number): number {
  return Math.min(100, Math.max(0, n));
}

/** 47.6 -> "48%" for display. */
export function formatPercent(n: number): string {
  return `${Math.round(clampPercent(n))}%`;
}

/** Seconds -> "2h 14m", "3d 4h", "48s". */
export function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds)) return "—";
  const s = Math.max(0, Math.round(seconds));
  const days = Math.floor(s / 86400);
  const hours = Math.floor((s % 86400) / 3600);
  const minutes = Math.floor((s % 3600) / 60);
  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${minutes}m`;
  if (minutes > 0) return `${minutes}m ${String(s % 60).padStart(2, "0")}s`;
  return `${s}s`;
}

/** unix seconds -> "in 3d 4h" */
export function untilFromIso(iso: string, now = Date.now()): string {
  const t = Date.parse(iso);
  if (!Number.isFinite(t)) return "—";
  return `in ${formatDuration((t - now) / 1000)}`;
}

/** unix seconds timestamp -> "12s ago" */
export function agoFrom(t: number | null, now = Date.now()): string {
  if (t === null) return "never";
  const seconds = (now - t * 1000) / 1000;
  if (seconds < 5) return "just now";
  if (seconds < 60) return `${Math.round(seconds)}s ago`;
  if (seconds < 3600) return `${Math.round(seconds / 60)}m ago`;
  return `${Math.round(seconds / 3600)}h ago`;
}

/** "en" -> "en-US" style clock time, e.g. 14:32. */
export function clockTime(iso: string): string {
  const t = Date.parse(iso);
  if (!Number.isFinite(t)) return "—";
  return new Date(t).toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
  });
}

/** 1420000 -> "1.42M". Keeps three significant digits at most. */
export function formatTokens(n: number): string {
  const abs = Math.abs(n);
  if (abs >= 1e9) return `${(n / 1e9).toFixed(abs >= 1e10 ? 1 : 2)}B`;
  if (abs >= 1e6) return `${(n / 1e6).toFixed(abs >= 1e7 ? 1 : 2)}M`;
  if (abs >= 1e3) return `${(n / 1e3).toFixed(abs >= 1e5 ? 0 : 1)}k`;
  return String(Math.round(n));
}

/** 4.1234 -> "$4.12", 1420 -> "$1.42k", 0 -> "$0.00". */
export function formatCost(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return "$0.00";
  if (n < 0.01) return "<$0.01";
  if (n >= 1000) return `$${(n / 1000).toFixed(2)}k`;
  return `$${n.toFixed(2)}`;
}

/** 4.10 -> "$4.10", or "4.10 EUR" for anything else. */
export function formatBalance(balance: { usd: number; currency: string }): string {
  const amount = Math.abs(balance.usd) < 0.005 ? "0.00" : balance.usd.toFixed(2);
  const currency = (balance.currency || "USD").toUpperCase();
  return currency === "USD" ? `$${amount}` : `${amount} ${currency}`;
}

/** 4 -> "4 requests", 1 -> "1 request". */
export function formatRequests(n: number): string {
  return `${n.toLocaleString()} ${n === 1 ? "request" : "requests"}`;
}

/** Short label for a chart bucket: "14" for hours, "13" / "Mon" for days. */
export function bucketLabel(
  key: string,
  start: number | null,
  unit: "hour" | "day",
): { short: string; long: string } {
  if (unit === "hour") {
    const date = new Date(start ?? Number(key));
    const hour = date.getHours();
    return {
      short: String(hour).padStart(2, "0"),
      long: date.toLocaleString(undefined, { hour: "2-digit", minute: "2-digit" }),
    };
  }
  const [year, month, day] = key.split("-").map(Number);
  const date = new Date(year, (month ?? 1) - 1, day ?? 1);
  return {
    short: String(day ?? 0),
    long: date.toLocaleDateString(undefined, {
      weekday: "short",
      day: "numeric",
      month: "short",
    }),
  };
}

export function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}
