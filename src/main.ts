import "./style.css";
import { backend } from "./backend";
import type { AppState, DetailFilter, DetailRange } from "./types";
import { activeWorkspace, peakPercent } from "./types";
import {
  animateHero,
  canPatchDashboard,
  dashboardHtml,
  patchDashboard,
  patchTimers,
} from "./views/dashboard";
import { detailsHtml, loadDetail, type DetailSource, type DetailState } from "./views/details";
import {
  keyInputValue,
  patchLoginItem,
  patchSettings,
  renderTestResult,
  settingsHtml,
  toggleKeyVisibility,
  updateSaveState,
} from "./views/settings";

const root = document.getElementById("app")!;
const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

/** The three screens the panel can show. */
const SCREENS = ["dashboard", "settings", "details"] as const;
type Screen = (typeof SCREENS)[number];

const isScreen = (value: string): value is Screen => (SCREENS as readonly string[]).includes(value);

/** Detail is local data; it does not need the poll cadence. */
const DETAIL_REFRESH_MS = 15_000;

/** Views depend on this, not on the concrete IPC layer. */
const source: DetailSource = backend;

let state: AppState | null = null;
let screen: Screen = "dashboard";
let detail: DetailState = { status: "loading", range: "24h", filter: "all" };
let lastDetailLoad = 0;
let lastPeak = 0;
let animatedOnce = false;
/** Workspace awaiting a remove confirmation, so a key is never lost by a slip. */
let confirmingRemove: string | null = null;
/** Whether macOS starts the app at login; owned by the OS, not by config. */
let launchesAtLogin = false;

/* ------------------------------------------------------------- rendering */

function render(force = false): void {
  if (!state) return;
  const now = Date.now();

  switch (screen) {
    case "settings":
      if (force || !root.querySelector(".view-settings")) {
        root.innerHTML = settingsHtml(state, {
          confirmingRemove,
          launchesAtLogin,
          loginItemNote: backend.loginItemNote?.() ?? null,
        });
        updateSaveState(root);
        void refreshLoginItem();
      } else {
        patchSettings(root, state);
      }
      lastPeak = peakPercent(state.usage);
      return;

    case "details":
      // Detail loads on its own cadence; a poll update must not rebuild the
      // chart under the pointer.
      if (!force && root.querySelector(".view-details")) return;
      root.innerHTML = detailsHtml(detail);
      return;

    case "dashboard": {
      const patched = !force && canPatchDashboard(root, state);
      if (patched) {
        patchDashboard(root, state, now);
      } else {
        root.innerHTML = dashboardHtml(state, now);
      }
      const nextPeak = peakPercent(state.usage);
      // Counting up only makes sense when the gauge was already on screen.
      animateHero(root, patched ? lastPeak : animatedOnce ? nextPeak : 0);
      animatedOnce = true;
      lastPeak = nextPeak;
    }
  }
}

function goTo(next: Screen): void {
  screen = next;
  render(true);
}

/* ----------------------------------------------------------------- detail */

async function refreshDetail(
  range: DetailRange,
  filter: DetailFilter,
  showLoading: boolean,
): Promise<void> {
  lastDetailLoad = Date.now();
  if (showLoading) {
    detail = { status: "loading", range, filter };
    render(true);
  }
  detail = await loadDetail(source, range, filter);
  render(true);
}

/** The login item lives in the OS; ask it rather than assuming. */
async function refreshLoginItem(): Promise<void> {
  try {
    launchesAtLogin = await backend.launchesAtLogin();
  } catch {
    launchesAtLogin = false;
  }
  patchLoginItem(root, launchesAtLogin, backend.loginItemNote?.() ?? null);
}

/* ---------------------------------------------------------------- actions */

async function handle(action: string, el: HTMLElement): Promise<void> {
  if (!state) return;

  switch (action) {
    case "refresh": {
      root.querySelector<HTMLElement>("#refresh-btn")?.classList.add("spinning");
      state = await backend.refresh();
      render();
      break;
    }

    case "settings":
    case "back":
      goTo(action === "back" ? "dashboard" : "settings");
      break;

    case "details":
      goTo("details");
      await refreshDetail(detail.range, detail.filter, false);
      break;

    case "set-range": {
      const range = el.dataset.value as DetailRange | undefined;
      if (range) await refreshDetail(range, detail.filter, true);
      break;
    }

    case "toggle-detail-filter": {
      const next = detail.filter === "go" ? "all" : "go";
      await refreshDetail(detail.range, next, true);
      break;
    }

    case "reload-details":
      await refreshDetail(detail.range, detail.filter, true);
      break;

    case "quit":
      await backend.quit();
      break;

    case "toggle-key":
      toggleKeyVisibility(root);
      break;

    /* ------------------------------------------------------ workspaces */

    case "select-workspace": {
      const id = el.dataset.id;
      if (!id || id === state.activeKeyId) break;
      state = await backend.selectWorkspace(id);
      render(true);
      break;
    }

    case "add-workspace": {
      state = await backend.addWorkspace(null, "");
      render(true);
      root.querySelector<HTMLInputElement>("#api-key")?.focus();
      break;
    }

    case "remove-workspace": {
      const id = el.dataset.id;
      if (!id) break;
      confirmingRemove = id;
      render(true);
      break;
    }

    case "cancel-remove-workspace":
      confirmingRemove = null;
      render(true);
      break;

    case "confirm-remove-workspace": {
      const id = el.dataset.id;
      confirmingRemove = null;
      if (id) state = await backend.removeWorkspace(id);
      render(true);
      break;
    }

    /* ---------------------------------------------------- preferences */

    case "toggle-login-item": {
      launchesAtLogin = await backend.setLaunchesAtLogin(!launchesAtLogin);
      patchLoginItem(root, launchesAtLogin, backend.loginItemNote?.() ?? null);
      break;
    }

    case "set-poll": {
      const seconds = Number(el.dataset.value);
      if (!Number.isFinite(seconds)) break;
      await savePreferences({ pollSeconds: seconds });
      patchSettings(root, state);
      break;
    }

    case "toggle-percent":
      await savePreferences({ showPercentInMenuBar: !state.settings.showPercentInMenuBar });
      patchSettings(root, state);
      break;

    case "save-key": {
      const apiKey = keyInputValue(root);
      const id = state.activeKeyId;
      if (!apiKey || !id) break;
      state = await backend.setWorkspaceKey(id, apiKey);
      // Re-render first so the saved state shows, then report what the
      // endpoint said about the key.
      render(true);
      renderTestResult(root, { kind: "testing" });
      const result = await backend.testKey(apiKey);
      renderTestResult(
        root,
        result.ok
          ? { kind: "ok", message: "Saved. Connected — Go plan active." }
          : { kind: "fail", message: result.message },
      );
      break;
    }
  }
}

/** Persists the preferences, keeping the rest of the settings as they are. */
async function savePreferences(changes: Partial<AppState["settings"]>): Promise<void> {
  if (!state) return;
  state = await backend.savePreferences({ ...state.settings, ...changes });
}

/** Renames are committed on blur or Enter, not on every keystroke. */
async function commitRename(input: HTMLInputElement): Promise<void> {
  const id = input.dataset.renameId;
  const label = input.value.trim();
  if (!id || !state) return;
  const current = state.workspaces.find((workspace) => workspace.id === id);
  if (!label || label === current?.label) {
    input.value = current?.label ?? label;
    return;
  }
  state = await backend.renameWorkspace(id, label);
  render();
}

/* ----------------------------------------------------------------- events */

root.addEventListener("click", (event) => {
  const target = (event.target as HTMLElement).closest<HTMLElement>("[data-action]");
  if (!target) return;
  event.preventDefault();
  void handle(target.dataset.action ?? "", target);
});

// Saving is only offered when the field holds a key.
root.addEventListener("input", (event) => {
  if (!state) return;
  if ((event.target as HTMLElement)?.id === "api-key") {
    updateSaveState(root);
  }
});

// Workspace names commit when the field is left, not on every keystroke.
root.addEventListener(
  "blur",
  (event) => {
    const target = event.target as HTMLInputElement;
    if (target?.classList?.contains("ws-label")) void commitRename(target);
  },
  true,
);

root.addEventListener("keydown", (event) => {
  const target = event.target as HTMLInputElement;
  if (event.key === "Enter" && target?.classList?.contains("ws-label")) {
    event.preventDefault();
    target.blur();
  }
});

document.addEventListener("keydown", (event) => {
  const inField = (event.target as HTMLElement)?.tagName === "INPUT";
  const chord = event.metaKey || event.ctrlKey;

  if (event.key === "Escape") {
    event.preventDefault();
    void backend.hidePanel();
    return;
  }
  if (event.key === "Enter" && inField) {
    event.preventDefault();
    void handle("save-key", root);
    return;
  }
  if (chord && event.key === "r") {
    event.preventDefault();
    if (screen === "details") void refreshDetail(detail.range, detail.filter, false);
    else void handle("refresh", root);
    return;
  }
  if (chord && event.key === ",") {
    event.preventDefault();
    void handle("settings", root);
    return;
  }
  if (chord && event.key === "d") {
    event.preventDefault();
    void handle(screen === "details" ? "back" : "details", root);
  }
});

/* ------------------------------------------------------------------- boot */

/**
 * Keeps the native panel exactly as tall as its content: the error strip and
 * the empty state change the height, and a fixed window would either clip or
 * leave dead space.
 */
function watchHeight(): void {
  let lastSent = 0;
  const measure = () => {
    const view = root.querySelector<HTMLElement>(".view");
    if (!view) return;
    const height = Math.ceil(view.getBoundingClientRect().height);
    if (height > 0 && Math.abs(height - lastSent) >= 1) {
      lastSent = height;
      backend.resize(height);
    }
  };
  new ResizeObserver(measure).observe(root);
  window.setTimeout(measure, 60);
}

async function boot(): Promise<void> {
  document.body.classList.toggle("mock", !isTauri);

  state = await backend.getState();
  if (!activeWorkspace(state)?.configured) screen = "settings";
  render(true);
  watchHeight();

  await backend.onState((next) => {
    state = next;
    render();
  });

  await backend.onNavigate((next) => {
    if (isScreen(next)) goTo(next);
  });

  window.setInterval(() => {
    if (!state) return;
    if (screen === "dashboard") patchTimers(root, state, Date.now());
    if (screen === "details" && Date.now() - lastDetailLoad > DETAIL_REFRESH_MS) {
      void refreshDetail(detail.range, detail.filter, false);
    }
  }, 1000);

  // Opening the panel should show current numbers, not whatever the last
  // timer tick happened to catch.
  window.addEventListener("focus", () => {
    if (!state) return;
    const age = state.fetchedAt ? Date.now() - state.fetchedAt * 1000 : Infinity;
    if (age > 30_000) void handle("refresh", root);
  });
}

void boot().catch((error: unknown) => {
  root.innerHTML = `<div class="view"><p class="empty-body">Could not start: ${String(
    error,
  )}</p></div>`;
});
