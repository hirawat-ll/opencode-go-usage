import { bar, foot, textButton, versionTag } from "../components/chrome";
import { gaugeSvg, swatchSvg, ringOffsetFor } from "../components/gauge";
import { historyPeak, sparklineSvg } from "../components/sparkline";
import { agoFrom, escapeHtml, formatBalance, formatPercent, untilFromIso } from "../format";
import { icons } from "../icons";
import { countUp } from "../motion";
import type { AppState, Severity, Usage, UsageError, WindowKey } from "../types";
import { WINDOW_LABEL, WINDOW_ORDER, severityOf } from "../types";
import { signalOf, type Signal } from "../signal";

/* ------------------------------------------------------------ fragments */

function strip(
  severity: Severity,
  title: string,
  body: string,
  label: string,
  action: string,
): string {
  return `
    <div class="strip sev-${severity}">
      <span class="strip-icon">${icons.alert}</span>
      <div class="strip-text">
        <strong>${title}</strong>
        <span>${body}</span>
      </div>
      <button class="chip-btn" data-action="${action}">${label}</button>
    </div>`;
}

/** Every way a request can fail, phrased so the next step is obvious. */
function errorStrip(error: UsageError): string {
  switch (error.kind) {
    case "unauthorized":
      return strip("over", "Key rejected", "The API key was not accepted.", "Settings", "settings");
    case "no_subscription":
      return strip("over", "No Go subscription", "This key's workspace is not on a Go plan.", "Settings", "settings");
    case "network":
      return strip("warn", "Can't reach opencode.ai", "The request did not complete.", "Retry", "refresh");
    case "http":
      return strip("warn", `Server said ${error.status ?? "no"}`, escapeHtml(error.message), "Retry", "refresh");
    case "parse":
      return strip("warn", "Unexpected response", "The usage endpoint returned something new.", "Retry", "refresh");
    default:
      return "";
  }
}

/** The endpoint's `status` field is the only sign that requests are refused. */
function limitStrip(usage: Usage, blocked: WindowKey[]): string {
  if (blocked.length === 0) return "";
  const names = blocked.map((key) => WINDOW_LABEL[key]).join(" and ");
  const soonest = blocked
    .map((key) => usage[key].resetsAt)
    .filter((iso) => Number.isFinite(Date.parse(iso)))
    .sort((a, b) => Date.parse(a) - Date.parse(b))[0];
  const resume = soonest ? ` Requests resume ${untilFromIso(soonest)}.` : "";
  return strip(
    "over",
    "Limit reached",
    `${names} ${blocked.length > 1 ? "windows are" : "window is"} spent.${resume}`,
    "Check now",
    "refresh",
  );
}

function statusLine(state: AppState): string {
  if (state.loading && !state.usage) return "Checking your key…";
  if (state.loading) return "Checking…";
  if (!state.fetchedAt) return "Waiting for the first check";
  const every = state.settings.pollSeconds;
  const interval = every < 60 ? `${every}s` : `${Math.round(every / 60)}m`;
  return `Updated ${agoFrom(state.fetchedAt)} · every ${interval}`;
}

function header(state: AppState, dot: Severity | null): string {
  return bar({
    title: "OpenCode Go",
    leading: `<span class="dot ${dot ? `sev-${dot}` : ""}" id="status-dot"></span>`,
    actions: [
      { action: "refresh", icon: icons.refresh, title: "Check now", id: "refresh-btn", spinning: state.loading },
      { action: "settings", icon: icons.gear, title: "Settings" },
    ],
  });
}

function panelFooter(state: AppState): string {
  return foot({
    left: versionTag(state.version),
    right: `${textButton("details", "Details ›", { strong: true })}${textButton("quit", "Quit")}`,
  });
}

function legendRow(key: WindowKey, usage: Usage, now: number): string {
  const window = usage[key];
  const severity = severityOf(window.percent);
  return `
    <div class="legend-row">
      ${swatchSvg(key, severity, window.percent)}
      <span class="legend-name">${WINDOW_LABEL[key]}</span>
      <span class="legend-reset" data-resets="${window.resetsAt}">${untilFromIso(window.resetsAt, now)}</span>
      <span class="legend-pct sev-${severity}">${formatPercent(window.percent)}</span>
    </div>`;
}

/** The sparkline's headline: how close the busiest window came. */
function historyReadout(history: AppState["history"]): string {
  if (history.length < 2) return "collecting samples…";
  return `peak ${formatPercent(historyPeak(history))}`;
}

function heroReadout(signal: Signal): string {
  if (!signal.usage) return "";
  return `
    <span class="hero-num sev-${signal.severity}"><span id="hero-digits" data-value="${signal.peak.toFixed(
      1,
    )}">${Math.round(signal.peak)}</span><span class="hero-pct">%</span></span>
    <span class="hero-label" id="hero-label">${signal.binding ? WINDOW_LABEL[signal.binding] : ""}</span>`;
}

/**
 * Workspace switcher. Hidden for the common single-workspace case, so nothing
 * is added to the panel for people who do not need it.
 */
function workspaceSwitcher(state: AppState): string {
  if (state.workspaces.length < 2) return "";
  const shown = state.workspaces.slice(0, 3);
  const overflow = state.workspaces.length - shown.length;
  return `
    <div class="segmented workspace-switch" role="radiogroup" aria-label="Workspace">
      ${shown
        .map(
          (workspace) => `
        <button
          class="segment ${workspace.active ? "on" : ""}"
          data-action="select-workspace"
          data-id="${workspace.id}"
          role="radio"
          aria-checked="${workspace.active}"
          title="${escapeHtml(workspace.label)}"
        >${escapeHtml(workspace.label)}</button>`,
        )
        .join("")}
      ${
        overflow > 0
          ? `<button class="segment" data-action="settings" title="All workspaces">+${overflow}</button>`
          : ""
      }
    </div>`;
}

/** Wallet balance, once the endpoint exposes one. */
function balanceLine(usage: Usage): string {
  const balance = usage.balance;
  if (!balance || !Number.isFinite(balance.usd)) return "";
  return `<p class="balance-line">Credit balance <strong>${formatBalance(balance)}</strong></p>`;
}

function zeroUsage(): Usage {
  const window = { status: "ok", percent: 0, resetsAt: "" };
  return { rolling: window, weekly: window, monthly: window };
}

/* ---------------------------------------------------------------- view */

/** True when the DOM on screen can be updated in place. */
export function canPatchDashboard(root: HTMLElement, state: AppState): boolean {
  const hasShell = root.querySelector(".view-dashboard .gauge-wrap") !== null;
  return hasShell === (state.usage !== null);
}

export function dashboardHtml(state: AppState, now: number): string {
  const signal = signalOf(state);
  const usage = signal.usage;

  if (!usage) {
    // Plan usage and local token detail are different sources. Say so here so
    // "no usage yet" next to a populated Details view is not a contradiction.
    const localHint = state.local.available
      ? `<p class="empty-hint">Token detail from this Mac is ready now — open <strong>Details ›</strong> below.</p>`
      : "";
    return `
      <div class="view view-dashboard">
        ${header(state, signal.dot)}
        <div class="statusline" id="status-line">${statusLine(state)}</div>
        <div id="strip-slot">${state.error && state.error.kind !== "unconfigured" ? errorStrip(state.error) : ""}</div>
        ${workspaceSwitcher(state)}
        <div class="empty">
          <div class="empty-gauge">${gaugeSvg(zeroUsage())}</div>
          <p class="empty-title">No plan usage yet</p>
          <p class="empty-body">${
            signal.configured
              ? "Waiting for the first reading from this workspace."
              : "Add your OpenCode Go API key to meter the 5-hour, weekly and monthly limits."
          }</p>
          <button class="primary-btn" data-action="settings">${
            signal.configured ? "Open settings" : "Add API key"
          }</button>
          ${localHint}
        </div>
        ${panelFooter(state)}
      </div>`;
  }

  return `
    <div class="view view-dashboard">
      ${header(state, signal.dot)}
      <div class="statusline" id="status-line">${statusLine(state)}</div>
      <div id="strip-slot">${stripSlot(state, signal)}</div>
      ${workspaceSwitcher(state)}
      <section class="hero">
        <div class="gauge-wrap">
          ${gaugeSvg(usage)}
          <div class="hero-readout">${heroReadout(signal)}</div>
        </div>
      </section>
      ${balanceLine(usage)}
      <section class="legend" id="legend-slot">
        ${WINDOW_ORDER.map((key) => legendRow(key, usage, now)).join("")}
      </section>
      <section class="history">
        <div class="hist-head">
          <span class="eyebrow">Last 6 hours</span>
          <span class="hist-value" id="hist-value">${historyReadout(state.history)}</span>
        </div>
        <div id="spark-slot">${sparklineSvg(state.history, signal.severity)}</div>
      </section>
      ${panelFooter(state)}
    </div>`;
}

function stripSlot(state: AppState, signal: Signal): string {
  const error = state.error ? errorStrip(state.error) : "";
  const limit = signal.usage ? limitStrip(signal.usage, signal.blocked) : "";
  return `${error}${limit}`;
}

/* ------------------------------------------------------------- patches */

/**
 * In-place update. The gauge element is deliberately left alone so its arcs
 * keep transitioning; only their dash offsets and colours are written.
 */
export function patchDashboard(root: HTMLElement, state: AppState, now: number): void {
  const signal = signalOf(state);
  const usage = signal.usage;
  if (!usage) return;

  setText(root, "#status-line", statusLine(state));

  const dot = root.querySelector<HTMLElement>("#status-dot");
  if (dot) dot.className = `dot ${signal.dot ? `sev-${signal.dot}` : ""}`;

  const refresh = root.querySelector<HTMLElement>("#refresh-btn");
  if (refresh) refresh.classList.toggle("spinning", state.loading);

  const strips = root.querySelector<HTMLElement>("#strip-slot");
  if (strips) {
    const next = stripSlot(state, signal);
    if (strips.innerHTML.trim() !== next.trim()) strips.innerHTML = next;
  }

  const rings = root.querySelectorAll<SVGPathElement>(".view-dashboard .ring-value");
  WINDOW_ORDER.forEach((key, index) => {
    const ring = rings[index];
    if (!ring) return;
    ring.setAttribute("class", `ring-value sev-${severityOf(usage[key].percent)}`);
    ring.style.strokeDashoffset = ringOffsetFor(usage[key].percent).toFixed(2);
  });

  const digits = root.querySelector<HTMLElement>("#hero-digits");
  if (digits) digits.dataset.value = signal.peak.toFixed(1);
  const hero = root.querySelector<HTMLElement>(".hero-num");
  if (hero) hero.className = `hero-num sev-${signal.severity}`;
  setText(root, "#hero-label", signal.binding ? WINDOW_LABEL[signal.binding] : "");

  const legend = root.querySelector<HTMLElement>("#legend-slot");
  if (legend) legend.innerHTML = WINDOW_ORDER.map((key) => legendRow(key, usage, now)).join("");

  setText(root, "#hist-value", historyReadout(state.history));
  const spark = root.querySelector<HTMLElement>("#spark-slot");
  if (spark) spark.innerHTML = sparklineSvg(state.history, signal.severity);
}

/** Second-by-second patch: only clock-dependent text, so nothing else moves. */
export function patchTimers(root: HTMLElement, state: AppState, now: number): void {
  setText(root, "#status-line", statusLine(state));
  root.querySelectorAll<HTMLElement>("[data-resets]").forEach((el) => {
    const iso = el.dataset.resets ?? "";
    if (!iso) return;
    const text = untilFromIso(iso, now);
    if (el.textContent !== text) {
      el.textContent = text;
      el.title = `Resets ${text}`;
    }
  });
}

/** Count the hero number up from its previous reading. */
export function animateHero(root: HTMLElement, from: number): void {
  const el = root.querySelector<HTMLElement>("#hero-digits");
  if (!el) return;
  const target = Number(el.dataset.value ?? "0");
  if (!Number.isFinite(target)) return;
  countUp(el, from, target, (value) => String(Math.round(value)));
}

function setText(root: HTMLElement, selector: string, text: string): void {
  const el = root.querySelector<HTMLElement>(selector);
  if (el && el.textContent !== text) el.textContent = text;
}
