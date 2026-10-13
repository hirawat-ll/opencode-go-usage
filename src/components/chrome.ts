/**
 * Window chrome shared by every view: the title bar, the buttons in it, and
 * the footer. Views compose these instead of rebuilding the same markup, so
 * the three screens cannot drift apart.
 */
import { escapeHtml } from "../format";
import { icons } from "../icons";

export interface BarAction {
  action: string;
  icon: string;
  title: string;
  id?: string;
  spinning?: boolean;
}

export interface BarOptions {
  title: string;
  /** Show the back chevron. */
  back?: boolean;
  /** Markup rendered between the back button and the title, e.g. a status dot. */
  leading?: string;
  actions?: BarAction[];
}

export function iconButton(options: BarAction): string {
  const id = options.id ? ` id="${options.id}"` : "";
  const spinning = options.spinning ? " spinning" : "";
  return `<button class="icon-btn${spinning}"${id} data-action="${options.action}" title="${escapeHtml(
    options.title,
  )}" aria-label="${escapeHtml(options.title)}">${options.icon}</button>`;
}

export function textButton(
  action: string,
  label: string,
  options: { strong?: boolean; id?: string } = {},
): string {
  const id = options.id ? ` id="${options.id}"` : "";
  return `<button class="text-btn${options.strong ? " strong" : ""}"${id} data-action="${action}">${escapeHtml(
    label,
  )}</button>`;
}

export function versionTag(version: string): string {
  return `<span class="version">v${escapeHtml(version)}</span>`;
}

export function bar(options: BarOptions): string {
  const back = options.back
    ? `<button class="icon-btn" data-action="back" title="Back" aria-label="Back">${icons.back}</button>`
    : "";
  const actions = options.actions?.length
    ? `<div class="bar-right">${options.actions.map(iconButton).join("")}</div>`
    : "";
  return `
    <header class="bar">
      <div class="bar-left">
        ${back}
        ${options.leading ?? ""}
        <span class="app-name">${escapeHtml(options.title)}</span>
      </div>
      ${actions}
    </header>`;
}

export interface FootOptions {
  /** Left slot: usually the version tag. */
  left?: string;
  /** Right slot: usually text buttons. */
  right?: string;
}

export function foot(options: FootOptions = {}): string {
  const right = options.right ? `<div class="foot-actions">${options.right}</div>` : "";
  return `
    <footer class="foot">
      ${options.left ?? ""}
      ${right}
    </footer>`;
}
