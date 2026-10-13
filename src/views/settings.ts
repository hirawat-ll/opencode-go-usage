import { bar, foot, versionTag } from "../components/chrome";
import { escapeHtml } from "../format";
import { icons } from "../icons";
import type { AppState, WorkspaceInfo } from "../types";
import { activeWorkspace } from "../types";

const POLL_CHOICES = [30, 60, 300, 900];

function pollLabel(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  return `${seconds / 60}m`;
}

export interface SettingsOptions {
  /** Workspace the user asked to remove, waiting for confirmation. */
  confirmingRemove?: string | null;
  /** Whether the app starts at login. Read from the OS, not from config. */
  launchesAtLogin?: boolean;
  /** What macOS wants from the user about the login item, if anything. */
  loginItemNote?: string | null;
}

/**
 * One column: the workspace list, then the key for whichever workspace is
 * active, then the two preferences. Saving is the only action on a key — it
 * checks the key on the way through, so there is no separate "test" button and
 * no second status line to read.
 */
export function settingsHtml(state: AppState, options: SettingsOptions = {}): string {
  const active = activeWorkspace(state);
  const hasKey = active?.configured ?? false;

  return `
    <div class="view view-settings">
      ${bar({ title: "Settings", back: true })}

      <section class="field">
        <label class="field-label">Workspaces</label>
        <div class="rows workspace-rows" id="workspace-list">
          ${state.workspaces
            .map((workspace) => workspaceRow(workspace, options.confirmingRemove ?? null))
            .join("")}
          <button class="row row-button add-workspace" data-action="add-workspace" id="add-workspace">
            <span class="row-label">
              <span class="plus">＋</span> Add workspace
            </span>
          </button>
        </div>
      </section>

      <section class="field">
        <label class="field-label" for="api-key">API key${
          active ? ` · ${escapeHtml(active.label)}` : ""
        }</label>
        <div class="key-row">
          <input
            id="api-key"
            class="key-input"
            type="password"
            spellcheck="false"
            autocapitalize="off"
            autocomplete="off"
            placeholder="${
              hasKey
                ? `Saved as ${escapeHtml(active?.preview ?? "")} — paste a new key to replace it`
                : "Paste your key"
            }"
            value=""
          />
          <button class="icon-btn" data-action="toggle-key" id="toggle-key" title="Show key" aria-label="Show key">${icons.eye}</button>
          <button class="primary-btn compact" data-action="save-key" id="save-key" disabled>Save</button>
        </div>
        <p class="note" id="test-result" role="status">${
          hasKey
            ? ""
            : "Get a key from the OpenCode Console, then Save it here."
        }</p>
      </section>

      <section class="rows">
        <div class="row">
          <span class="row-label" id="poll-label">Check for updates</span>
          <div class="segmented inline" role="radiogroup" aria-labelledby="poll-label">
            ${POLL_CHOICES.map(
              (seconds) => `
              <button
                class="segment ${state.settings.pollSeconds === seconds ? "on" : ""}"
                data-action="set-poll"
                data-value="${seconds}"
                role="radio"
                aria-checked="${state.settings.pollSeconds === seconds}"
              >${pollLabel(seconds)}</button>`,
            ).join("")}
          </div>
        </div>

        <button
          class="row row-button"
          data-action="toggle-percent"
          role="switch"
          aria-checked="${state.settings.showPercentInMenuBar}"
          id="percent-switch"
        >
          <span class="row-label">Show percentage in the menu bar</span>
          <span class="switch ${state.settings.showPercentInMenuBar ? "on" : ""}"></span>
        </button>

        <button
          class="row row-button"
          data-action="toggle-login-item"
          role="switch"
          aria-checked="${options.launchesAtLogin ?? false}"
          id="login-item-switch"
        >
          <span class="row-label">Launch at login</span>
          <span class="switch ${options.launchesAtLogin ? "on" : ""}"></span>
        </button>
      </section>

      <p class="foot-note" id="login-item-note" ${
        options.loginItemNote ? "" : "hidden"
      }>${escapeHtml(options.loginItemNote ?? "")}</p>

      <p class="foot-note">Keys are stored on this Mac and sent only to opencode.ai.</p>

      ${foot({ left: versionTag(state.version) })}
    </div>`;
}

function workspaceRow(workspace: WorkspaceInfo, confirming: string | null): string {
  if (confirming === workspace.id) {
    return `
      <div class="row workspace-row confirming" data-id="${workspace.id}">
        <span class="row-label">Remove “${escapeHtml(workspace.label)}”?</span>
        <span class="confirm-actions">
          <button class="chip-btn" data-action="confirm-remove-workspace" data-id="${workspace.id}">Remove</button>
          <button class="chip-btn muted" data-action="cancel-remove-workspace">Keep</button>
        </span>
      </div>`;
  }

  return `
    <div class="row workspace-row ${workspace.active ? "active" : ""}" data-id="${workspace.id}">
      <button
        class="ws-radio ${workspace.active ? "on" : ""}"
        data-action="select-workspace"
        data-id="${workspace.id}"
        role="radio"
        aria-checked="${workspace.active}"
        title="Use this workspace"
        aria-label="Use ${escapeHtml(workspace.label)}"
      ></button>
      <input
        class="ws-label"
        data-rename-id="${workspace.id}"
        value="${escapeHtml(workspace.label)}"
        aria-label="Workspace name"
        spellcheck="false"
      />
      <span class="ws-preview ${workspace.configured ? "" : "missing"}">${
        workspace.configured ? escapeHtml(workspace.preview) : "no key"
      }</span>
      <button
        class="icon-btn ws-remove"
        data-action="remove-workspace"
        data-id="${workspace.id}"
        title="Remove workspace"
        aria-label="Remove ${escapeHtml(workspace.label)}"
      >${icons.close}</button>
    </div>`;
}

/** Update the parts of settings that can change from outside this view. */
export function patchSettings(root: HTMLElement, state: AppState): void {
  const list = root.querySelector<HTMLElement>("#workspace-list");
  // The list is cheap but has inputs in it, so it is only rebuilt when the
  // shape really changed.
  const shape = state.workspaces.map((w) => `${w.id}:${w.active}:${w.label}`).join("|");
  if (list && list.dataset.shape !== shape) {
    list.dataset.shape = shape;
    const confirming = list.querySelector(".confirming")?.getAttribute("data-id") ?? null;
    list.innerHTML =
      state.workspaces.map((workspace) => workspaceRow(workspace, confirming)).join("") +
      `<button class="row row-button add-workspace" data-action="add-workspace" id="add-workspace">
         <span class="row-label"><span class="plus">＋</span> Add workspace</span>
       </button>`;
  }

  root.querySelectorAll<HTMLElement>(".segment").forEach((el) => {
    const on = Number(el.dataset.value) === state.settings.pollSeconds;
    el.classList.toggle("on", on);
    el.setAttribute("aria-checked", String(on));
  });

  const sw = root.querySelector<HTMLElement>("#percent-switch");
  if (sw) {
    sw.setAttribute("aria-checked", String(state.settings.showPercentInMenuBar));
    sw.querySelector(".switch")?.classList.toggle("on", state.settings.showPercentInMenuBar);
  }

  const label = root.querySelector<HTMLElement>(".field-label[for='api-key']");
  const active = activeWorkspace(state);
  if (label && active) label.textContent = `API key · ${active.label}`;
}

/** The login item is the OS's state, so it is patched from the OS's answer. */
export function patchLoginItem(root: HTMLElement, enabled: boolean, note: string | null): void {
  const row = root.querySelector<HTMLElement>("#login-item-switch");
  if (row) {
    row.setAttribute("aria-checked", String(enabled));
    row.querySelector(".switch")?.classList.toggle("on", enabled);
  }
  const noteEl = root.querySelector<HTMLElement>("#login-item-note");
  if (noteEl) {
    noteEl.textContent = note ?? "";
    noteEl.hidden = !note;
  }
}

/** Save stays out of reach until there is a non-empty key to send. */
export function updateSaveState(root: HTMLElement): void {
  const input = root.querySelector<HTMLInputElement>("#api-key");
  const button = root.querySelector<HTMLButtonElement>("#save-key");
  if (!input || !button) return;
  button.disabled = input.value.trim().length === 0;
}

export function keyInputValue(root: HTMLElement): string {
  return root.querySelector<HTMLInputElement>("#api-key")?.value.trim() ?? "";
}

export type TestState =
  | { kind: "idle" }
  | { kind: "testing" }
  | { kind: "ok"; message: string }
  | { kind: "fail"; message: string };

export function renderTestResult(root: HTMLElement, state: TestState): void {
  const el = root.querySelector<HTMLElement>("#test-result");
  const save = root.querySelector<HTMLButtonElement>("#save-key");
  if (!el) return;
  if (save && state.kind === "testing") save.disabled = true;
  if (state.kind === "idle") {
    el.className = "note";
    el.innerHTML = "";
    return;
  }
  if (state.kind === "testing") {
    el.className = "note";
    el.innerHTML = `<span class="pulse">Checking the key…</span>`;
    return;
  }
  el.className = `note sev-${state.kind === "ok" ? "ok" : "over"}`;
  el.innerHTML = `<span class="note-icon">${
    state.kind === "ok" ? icons.check : icons.alert
  }</span>${escapeHtml(state.message)}`;
}

export function toggleKeyVisibility(root: HTMLElement): void {
  const input = root.querySelector<HTMLInputElement>("#api-key");
  const btn = root.querySelector<HTMLElement>("#toggle-key");
  if (!input || !btn) return;
  const hidden = input.type === "password";
  input.type = hidden ? "text" : "password";
  btn.innerHTML = hidden ? icons.eyeOff : icons.eye;
  btn.title = hidden ? "Hide key" : "Show key";
}
