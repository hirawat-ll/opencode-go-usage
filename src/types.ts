/** Shapes shared with the Rust side. Keep in sync with src-tauri/src/state.rs */

export type WindowKey = "rolling" | "weekly" | "monthly";

export interface UsageWindow {
  /** "ok" | "rate-limited" */
  status: string;
  /** 0-100 */
  percent: number;
  /** ISO 8601 timestamp */
  resetsAt: string;
}

export interface Usage {
  rolling: UsageWindow;
  weekly: UsageWindow;
  monthly: UsageWindow;
  /** Only present once the endpoint exposes the wallet balance. */
  balance?: Balance | null;
}

/** Credit balance, as proposed in anomalyco/opencode#44189. */
export interface Balance {
  usd: number;
  currency: string;
  asOf?: string | null;
}

export type ErrorKind =
  | "unconfigured"
  | "unauthorized"
  | "no_subscription"
  | "network"
  | "http"
  | "parse";
export interface UsageError {
  kind: ErrorKind;
  message: string;
  /** HTTP status when there was a response. */
  status?: number;
}

/** The two preferences the panel owns. */
export interface Settings {
  pollSeconds: number;
  showPercentInMenuBar: boolean;
}

/** One Go API key the app meters. Secrets never cross the IPC boundary. */
export interface WorkspaceInfo {
  id: string;
  label: string;
  active: boolean;
  /** False when no key is stored yet. */
  configured: boolean;
  /** Masked tail of the key, e.g. "…a91f". */
  preview: string;
}

export interface Sample {
  /** unix seconds */
  t: number;
  /** highest percent across the three windows at that moment */
  peak: number;
}

export interface AppState {
  settings: Settings;
  workspaces: WorkspaceInfo[];
  activeKeyId: string;
  usage: Usage | null;
  error: UsageError | null;
  /** unix seconds of the last successful fetch of the active workspace */
  fetchedAt: number | null;
  /** true when the newest attempt failed but older data is still on screen */
  stale: boolean;
  /** true while a request is in flight */
  loading: boolean;
  /** samples of the active workspace */
  history: Sample[];
  version: string;
  /** Token detail is local and independent of the API key. */
  local: LocalSource;
}

export interface LocalSource {
  available: boolean;
  database: string;
}

/** The workspace the panel is showing. */
export function activeWorkspace(state: AppState): WorkspaceInfo | undefined {
  return state.workspaces.find((workspace) => workspace.id === state.activeKeyId);
}

export const WINDOW_LABEL: Record<WindowKey, string> = {
  rolling: "5-hour",
  weekly: "Week",
  monthly: "Month",
};

/** Chronological, shortest horizon first. */
export const WINDOW_ORDER: WindowKey[] = ["rolling", "weekly", "monthly"];

export function peakPercent(usage: Usage | null): number {
  if (!usage) return 0;
  return Math.max(
    usage.rolling.percent,
    usage.weekly.percent,
    usage.monthly.percent,
  );
}

export function bindingWindow(usage: Usage | null): WindowKey | null {
  if (!usage) return null;
  let best: WindowKey = "rolling";
  for (const key of WINDOW_ORDER) {
    if (usage[key].percent > usage[best].percent) best = key;
  }
  return best;
}

export type Severity = "ok" | "warn" | "over";

export function severityOf(percent: number): Severity {
  if (percent >= 90) return "over";
  if (percent >= 70) return "warn";
  return "ok";
}

/** Providers whose models are the ones this meter is for. */
export const GO_PROVIDERS = ["opencode-go", "opencode"];

/** True when a model row came from outside the Go plan. */
export function isForeignProvider(provider: string): boolean {
  return !GO_PROVIDERS.includes(provider);
}

/* ------------------------------------------------- token detail (local) */

export type DetailRange = "24h" | "7d" | "30d";

export const DETAIL_RANGES: DetailRange[] = ["24h", "7d", "30d"];

/** Which providers a detail read counts. */
export type DetailFilter = "all" | "go";

export interface DetailTotals {
  input: number;
  output: number;
  reasoning: number;
  cacheRead: number;
  cacheWrite: number;
  /** everything the models processed, cache reads included */
  tokens: number;
  requests: number;
  cost: number;
}

export interface DetailModel {
  provider: string;
  model: string;
  input: number;
  output: number;
  reasoning: number;
  cacheRead: number;
  tokens: number;
  requests: number;
  cost: number;
}

export interface DetailBucket {
  /** epoch-ms string for hours, local date for days */
  key: string;
  /** set for hourly buckets */
  start: number | null;
  tokens: number;
  requests: number;
  cost: number;
}

export interface Detail {
  range: DetailRange;
  /** Which providers this read counted. */
  filter: DetailFilter;
  available: boolean;
  message: string | null;
  database: string;
  totals: DetailTotals;
  models: DetailModel[];
  bucketUnit: "hour" | "day";
  buckets: DetailBucket[];
}
