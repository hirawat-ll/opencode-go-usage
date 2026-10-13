/**
 * Backend boundary.
 *
 * In the packaged app every call goes through Tauri IPC. When the same page is
 * opened in a plain browser (vite dev server) a small in-memory mock takes over
 * so the interface can be worked on without rebuilding the Rust side.
 */
import type {
  AppState,
  Detail,
  DetailBucket,
  DetailModel,
  DetailRange,
  Settings,
  Usage,
} from "./types";
import { WINDOW_ORDER, isForeignProvider } from "./types";

export interface Backend {
  getState(): Promise<AppState>;
  savePreferences(preferences: Settings): Promise<AppState>;
  /** Adds a workspace and makes it active. An empty key is allowed. */
  addWorkspace(label: string | null, apiKey: string): Promise<AppState>;
  setWorkspaceKey(id: string, apiKey: string): Promise<AppState>;
  renameWorkspace(id: string, label: string): Promise<AppState>;
  selectWorkspace(id: string): Promise<AppState>;
  removeWorkspace(id: string): Promise<AppState>;
  refresh(): Promise<AppState>;
  testKey(apiKey: string): Promise<TestResult>;
  /** Token-level detail read from OpenCode's local database. */
  usageDetail(range: DetailRange, goOnly: boolean): Promise<Detail>;
  /** Whether the app is registered to start at login. */
  launchesAtLogin(): Promise<boolean>;
  setLaunchesAtLogin(enabled: boolean): Promise<boolean>;
  /** A note about the login item, when macOS wants something from the user. */
  loginItemNote?(): string | null;
  quit(): Promise<void>;
  hidePanel(): Promise<void>;
  /** Ask the host window to match the measured content height. */
  resize(height: number): void;
  onState(handler: (state: AppState) => void): Promise<() => void>;
  /** Menu bar menu asked for a specific view. */
  onNavigate(handler: (view: string) => void): Promise<() => void>;
}

export interface TestResult {
  ok: boolean;
  /** Short machine-ish reason: "ok" | "unauthorized" | ... */
  kind: string;
  message: string;
  usage?: Usage;
}

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export const backend: Backend = isTauri ? tauriBackend() : mockBackend();

function tauriBackend(): Backend {
  const invoke = async <T,>(cmd: string, args?: Record<string, unknown>) => {
    const { invoke: call } = await import("@tauri-apps/api/core");
    return call<T>(cmd, args);
  };
  /** Set when macOS wants the user to approve the login item. */
  let loginItemNote: string | null = null;
  return {
    getState: () => invoke<AppState>("get_state"),
    savePreferences: (preferences) =>
      invoke<AppState>("save_preferences", { preferences }),
    addWorkspace: (label, apiKey) => invoke<AppState>("add_workspace", { label, apiKey }),
    setWorkspaceKey: (id, apiKey) => invoke<AppState>("set_workspace_key", { id, apiKey }),
    renameWorkspace: (id, label) => invoke<AppState>("rename_workspace", { id, label }),
    selectWorkspace: (id) => invoke<AppState>("select_workspace", { id }),
    removeWorkspace: (id) => invoke<AppState>("remove_workspace", { id }),
    refresh: () => invoke<AppState>("refresh"),
    testKey: (apiKey) => invoke<TestResult>("test_key", { apiKey }),
    usageDetail: (range, goOnly) => invoke<Detail>("usage_detail", { range, goOnly }),
    launchesAtLogin: async () => {
      const { invoke } = await import("@tauri-apps/api/core");
      const state = await invoke<string>("login_item_status");
      loginItemNote = state === "requires_approval" ? "Approve it in System Settings → Login Items." : null;
      return state === "enabled" || state === "requires_approval";
    },
    setLaunchesAtLogin: async (enabled) => {
      const { invoke } = await import("@tauri-apps/api/core");
      const state = await invoke<string>("set_login_item", { enabled });
      loginItemNote =
        state === "requires_approval"
          ? "Approve it in System Settings → Login Items."
          : state === "unsupported"
            ? "Set this up under System Settings → Login Items."
            : null;
      return state === "enabled" || state === "requires_approval";
    },
    loginItemNote: () => loginItemNote,
    quit: () => invoke<void>("quit"),
    hidePanel: () => invoke<void>("hide_panel"),
    resize: (height) => {
      void invoke<void>("resize_panel", { height }).catch(() => {});
    },
    onState: async (handler) => {
      const { listen } = await import("@tauri-apps/api/event");
      return listen<AppState>("usage://update", (event) => handler(event.payload));
    },
    onNavigate: async (handler) => {
      const { listen } = await import("@tauri-apps/api/event");
      return listen<string>("panel://navigate", (event) => handler(event.payload));
    },
  };
}

/* ------------------------------------------------------------------ */
/* Browser mock — sample data so the UI can be iterated on in Chrome. */
/* ------------------------------------------------------------------ */

/** Invented token detail: the same shape the Rust side reads from opencode.db. */
function mockDetail(range: DetailRange): Detail {
  // Blended dollars per token, in the range the real database reports once
  // cache reads dominate.
  const TOKEN_COST = 1.2e-8;
  const plans: Record<DetailRange, { requests: number; tokens: number; buckets: number }> = {
    "24h": { requests: 617, tokens: 4_180_000, buckets: 24 },
    "7d": { requests: 3_140, tokens: 21_400_000, buckets: 7 },
    "30d": { requests: 11_260, tokens: 78_900_000, buckets: 30 },
  };
  const { requests, tokens, buckets: count } = plans[range];
  const hourly = range === "24h";

  const models: DetailModel[] = [
    { provider: "opencode-go", model: "deepseek-v4.1-flash", share: 0.62 },
    { provider: "opencode-go", model: "kimi-k2.7-code", share: 0.21 },
    { provider: "opencode-go", model: "glm-5.3-flash", share: 0.11 },
    { provider: "opencode-go", model: "minimax-m3", share: 0.04 },
    { provider: "anthropic", model: "claude-haiku-5-5", share: 0.02 },
  ].map((m) => ({
    provider: m.provider,
    model: m.model,
    tokens: Math.round(tokens * m.share),
    requests: Math.max(1, Math.round(requests * m.share)),
    cost: tokens * m.share * TOKEN_COST,
    input: Math.round(tokens * m.share * 0.05),
    output: Math.round(tokens * m.share * 0.02),
    reasoning: Math.round(tokens * m.share * 0.005),
    cacheRead: Math.round(tokens * m.share * 0.92),
  }));

  const now = new Date();
  const buckets: DetailBucket[] = [];
  for (let ago = count - 1; ago >= 0; ago--) {
    const wobble = hourly
      ? 0.35 + 0.65 * Math.abs(Math.sin((ago + 3) * 1.7))
      : 0.4 + 0.6 * Math.abs(Math.sin(ago * 0.9));
    const tokensHere = Math.round(((tokens * wobble) / count) * 1.6);
    const requestsHere = Math.round((requests * wobble) / count + 1);
    if (hourly) {
      const date = new Date(now);
      date.setMinutes(0, 0, 0);
      date.setHours(date.getHours() - ago);
      const start = date.getTime();
      buckets.push({
        key: String(start),
        start,
        tokens: tokensHere,
        requests: requestsHere,
        cost: tokensHere * TOKEN_COST,
      });
    } else {
      const date = new Date(now);
      date.setDate(date.getDate() - ago);
      const key = `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(
        date.getDate(),
      ).padStart(2, "0")}`;
      buckets.push({
        key,
        start: null,
        tokens: tokensHere,
        requests: requestsHere,
        cost: tokensHere * TOKEN_COST,
      });
    }
  }

  const sum = (pick: (m: DetailModel) => number) => models.reduce((total, m) => total + pick(m), 0);
  const bucketSum = (pick: (b: DetailBucket) => number) =>
    buckets.reduce((total, b) => total + pick(b), 0);

  return {
    range,
    filter: "all",
    available: true,
    message: null,
    database: "opencode.db",
    totals: {
      input: sum((m) => m.input),
      output: sum((m) => m.output),
      reasoning: sum((m) => m.reasoning),
      cacheRead: sum((m) => m.cacheRead),
      cacheWrite: 0,
      // Taken from the buckets so the chart and the headline agree.
      tokens: bucketSum((b) => b.tokens),
      requests: bucketSum((b) => b.requests),
      cost: bucketSum((b) => b.cost),
    },
    models,
    bucketUnit: hourly ? "hour" : "day",
    buckets,
  };
}

/** Browser-only sentinels so every error state can be previewed. */
function simulatedError(apiKey: string): AppState["error"] {
  const key = apiKey.toLowerCase();
  if (key.includes("401")) {
    return { kind: "unauthorized", message: "HTTP 401" };
  }
  if (key.includes("403")) {
    return { kind: "no_subscription", message: "EntitlementError" };
  }
  if (key.includes("net")) {
    return { kind: "network", message: "No connection to opencode.ai." };
  }
  return null;
}

/** The tail of a key, as the panel shows it. */
function previewOf(apiKey: string): string {
  const trimmed = apiKey.trim();
  return trimmed ? `…${trimmed.slice(-4)}` : "";
}

function mockBackend(): Backend {
  const started = Date.now() / 1000;
  const listeners = new Set<(state: AppState) => void>();

  interface Entry {
    id: string;
    label: string;
    apiKey: string;
    history: AppState["history"];
  }

  // Two workspaces so the multi-workspace UI is previewable: one with a key
  // and readings, one still empty to show the "add a key" path.
  const entries: Entry[] = [
    { id: "w1", label: "Personal", apiKey: "oc_preview_key_5f2c", history: [] },
    { id: "w2", label: "Work", apiKey: "", history: [] },
  ];
  const readings = new Map<string, { usage: Usage | null; fetchedAt: number | null }>();
  let activeId = entries[0].id;
  let preferences: Settings = { pollSeconds: 60, showPercentInMenuBar: true };
  let loginItem = false;

  const entry = (id: string) => entries.find((candidate) => candidate.id === id);
  const active = () => entry(activeId) ?? entries[0];

  const makeUsage = (t: number): Usage => {
    const drift = (t - started) / 60;
    const clamp = (n: number) => Math.min(99.4, Math.max(0.4, n));
    const key = active().apiKey.toLowerCase();
    return {
      rolling: {
        status: "ok",
        percent: clamp(38 + Math.sin(drift / 3) * 14 + drift * 0.35),
        resetsAt: new Date(Date.now() + 2.1 * 3600_000).toISOString(),
      },
      weekly: {
        status: "ok",
        percent: clamp(57 + Math.sin(drift / 9) * 6 + drift * 0.1),
        resetsAt: new Date(Date.now() + 3.4 * 86400_000).toISOString(),
      },
      monthly: {
        status: "ok",
        percent: clamp(64 + Math.sin(drift / 20) * 3 + drift * 0.04),
        resetsAt: new Date(Date.now() + 17.2 * 86400_000).toISOString(),
      },
      // A browser-only sentinel: keys containing "balance" preview the
      // balance readout the endpoint may grow (#44189).
      balance: key.includes("balance")
        ? { usd: 4.1, currency: "USD", asOf: new Date().toISOString() }
        : null,
    };
  };

  const snapshot = (): AppState => {
    const current = active();
    const reading = readings.get(current.id);
    const configured = current.apiKey.trim().length > 0;
    return {
      settings: preferences,
      workspaces: entries.map((candidate) => ({
        id: candidate.id,
        label: candidate.label,
        active: candidate.id === activeId,
        configured: candidate.apiKey.trim().length > 0,
        preview: previewOf(candidate.apiKey),
      })),
      activeKeyId: activeId,
      usage: reading?.usage ?? null,
      error: configured ? null : { kind: "unconfigured", message: "No API key yet." },
      fetchedAt: reading?.fetchedAt ?? null,
      stale: false,
      loading: false,
      history: current.history,
      version: "0.2.0",
      local: { available: true, database: "opencode.db" },
    };
  };

  const push = () => {
    const next = snapshot();
    listeners.forEach((listener) => listener(next));
  };

  const run = async (): Promise<void> => {
    const current = active();
    push();
    await new Promise((resolve) => setTimeout(resolve, 420));

    if (!current.apiKey.trim()) {
      readings.delete(current.id);
      push();
      return;
    }

    const simulated = simulatedError(current.apiKey);
    if (simulated) {
      // Keep whatever was there, so the stale path is visible too.
      push();
      return;
    }

    const usage = makeUsage(Date.now() / 1000);
    if (current.apiKey.toLowerCase().includes("limit")) {
      usage.rolling = { ...usage.rolling, status: "rate-limited", percent: 100 };
    }
    readings.set(current.id, { usage, fetchedAt: Math.floor(Date.now() / 1000) });

    const peak = Math.max(...WINDOW_ORDER.map((k) => usage[k].percent));
    current.history = [...current.history, { t: Date.now() / 1000, peak }].slice(-1440);
    push();
  };

  // Six hours of invented history for the first workspace, so the sparkline
  // has a shape to draw on load, and a reading so the panel is not empty
  // before the first manual refresh.
  {
    const now = Date.now() / 1000;
    for (let i = 120; i >= 0; i--) {
      const hour = (i * 180) / 3600;
      const peak =
        30 +
        22 * Math.exp(-Math.pow((hour - 4.2) / 1.1, 2)) +
        6 * Math.exp(-Math.pow((hour - 1.4) / 0.7, 2)) +
        Math.sin(hour * 2.1) * 1.5;
      entries[0].history.push({ t: now - i * 180, peak: Math.min(99, Math.max(1, peak)) });
    }
    readings.set(entries[0].id, { usage: makeUsage(now), fetchedAt: Math.floor(now) });
  }

  return {
    getState: async () => snapshot(),

    savePreferences: async (settings) => {
      preferences = settings;
      push();
      return snapshot();
    },

    addWorkspace: async (label, apiKey) => {
      const id = `w${entries.length + 1}-${Date.now().toString(36)}`;
      entries.push({ id, label: label?.trim() || previewOf(apiKey) || "New workspace", apiKey, history: [] });
      activeId = id;
      push();
      await run();
      return snapshot();
    },

    setWorkspaceKey: async (id, apiKey) => {
      const target = entry(id);
      if (target) target.apiKey = apiKey;
      if (id === activeId) await run();
      else push();
      return snapshot();
    },

    renameWorkspace: async (id, label) => {
      const target = entry(id);
      if (target && label.trim()) target.label = label.trim();
      push();
      return snapshot();
    },

    selectWorkspace: async (id) => {
      if (entry(id)) activeId = id;
      push();
      await run();
      return snapshot();
    },

    removeWorkspace: async (id) => {
      const index = entries.findIndex((candidate) => candidate.id === id);
      if (index >= 0) {
        entries.splice(index, 1);
        readings.delete(id);
      }
      if (activeId === id) activeId = entries[0]?.id ?? "";
      push();
      return snapshot();
    },

    refresh: async () => {
      await run();
      return snapshot();
    },

    testKey: async (apiKey) => {
      await new Promise((resolve) => setTimeout(resolve, 500));
      if (!apiKey.trim()) {
        return { ok: false, kind: "unauthorized", message: "Enter a key first." };
      }
      const simulated = simulatedError(apiKey);
      if (simulated) {
        return { ok: false, kind: simulated.kind, message: simulated.message ?? "Rejected." };
      }
      return { ok: true, kind: "ok", message: "Connected — Go plan active." };
    },

    quit: async () => {},

    usageDetail: async (range, goOnly) => {
      await new Promise((resolve) => setTimeout(resolve, 260));
      // Browser-only sentinel: preview the "no database" state.
      if (active().apiKey.toLowerCase().includes("nodb")) {
        return {
          range,
          filter: goOnly ? "go" : "all",
          available: false,
          message: "No OpenCode database found on this Mac, so there is no local detail to show.",
          database: "",
          totals: {
            input: 0,
            output: 0,
            reasoning: 0,
            cacheRead: 0,
            cacheWrite: 0,
            tokens: 0,
            requests: 0,
            cost: 0,
          },
          models: [],
          bucketUnit: range === "24h" ? "hour" : "day",
          buckets: [],
        };
      }
      const detail = mockDetail(range);
      if (!goOnly) return detail;
      // Same shape, only the Go models and their share of the totals.
      const models = detail.models.filter((model) => !isForeignProvider(model.provider));
      const tokens = models.reduce((total, model) => total + model.tokens, 0);
      const requests = models.reduce((total, model) => total + model.requests, 0);
      const scale = detail.totals.tokens > 0 ? tokens / detail.totals.tokens : 0;
      return {
        ...detail,
        filter: "go",
        models,
        totals: {
          input: Math.round(detail.totals.input * scale),
          output: Math.round(detail.totals.output * scale),
          reasoning: Math.round(detail.totals.reasoning * scale),
          cacheRead: Math.round(detail.totals.cacheRead * scale),
          cacheWrite: 0,
          tokens,
          requests,
          cost: models.reduce((total, model) => total + model.cost, 0),
        },
        buckets: detail.buckets.map((bucket) => ({
          ...bucket,
          tokens: Math.round(bucket.tokens * scale),
          requests: Math.round(bucket.requests * scale),
          cost: bucket.cost * scale,
        })),
      };
    },

    launchesAtLogin: async () => loginItem,
    setLaunchesAtLogin: async (enabled) => {
      await new Promise((resolve) => setTimeout(resolve, 200));
      loginItem = enabled;
      return loginItem;
    },
    loginItemNote: () => null,

    hidePanel: async () => {},
    resize: () => {},
    onState: async (handler) => {
      listeners.add(handler);
      return () => listeners.delete(handler);
    },
    onNavigate: async () => () => {},
  };
}
