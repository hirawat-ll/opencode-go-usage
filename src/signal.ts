/**
 * The one place that decides what the panel is saying.
 *
 * Both the first render and every in-place update read from here; they used to
 * work the severity rules out separately, which is how two code paths start
 * disagreeing about the same numbers.
 */
import type { AppState, ErrorKind, Severity, Usage, WindowKey } from "./types";
import { WINDOW_ORDER, activeWorkspace, bindingWindow, peakPercent, severityOf } from "./types";

export interface Signal {
  usage: Usage | null;
  /** Window closest to its limit, if there is data. */
  binding: WindowKey | null;
  /** Its percent, 0-100. */
  peak: number;
  /** Severity of the peak, escalated when the endpoint reports a spent window. */
  severity: Severity;
  /** Windows the endpoint reports as no longer ok. */
  blocked: WindowKey[];
  /** Severity of the last failed attempt, or null when the last one worked. */
  errorSeverity: Severity | null;
  /** What the header dot should show. */
  dot: Severity | null;
  hasData: boolean;
  /** True when the active workspace has a key stored. */
  configured: boolean;
}

/** A rejected key is fatal; a bad connection is a warning. */
export function errorSeverity(kind: ErrorKind): Severity {
  return kind === "unauthorized" || kind === "no_subscription" ? "over" : "warn";
}

export function blockedWindows(usage: Usage): WindowKey[] {
  return WINDOW_ORDER.filter((key) => usage[key].status !== "ok");
}

export function signalOf(state: AppState): Signal {
  const { usage } = state;
  const blocked = usage ? blockedWindows(usage) : [];
  const peak = peakPercent(usage);
  const severity: Severity = blocked.length > 0 ? "over" : severityOf(peak);
  const failed = state.error ? errorSeverity(state.error.kind) : null;

  return {
    usage,
    binding: bindingWindow(usage),
    peak,
    severity,
    blocked,
    errorSeverity: failed,
    dot: failed ?? (usage ? severity : null),
    hasData: usage !== null,
    configured: activeWorkspace(state)?.configured ?? false,
  };
}
