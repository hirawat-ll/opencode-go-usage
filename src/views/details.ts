import { bar, foot, textButton } from "../components/chrome";
import { bucketLabel, escapeHtml, formatCost, formatPercent, formatRequests, formatTokens } from "../format";
import { icons } from "../icons";
import type { Detail, DetailFilter, DetailModel, DetailRange } from "../types";
import { DETAIL_RANGES, isForeignProvider } from "../types";

export type DetailState =
  | { status: "loading"; range: DetailRange; filter: DetailFilter }
  | { status: "ready"; range: DetailRange; filter: DetailFilter; detail: Detail }
  | { status: "error"; range: DetailRange; filter: DetailFilter; message: string };

/** What the details view needs from the outside world. */
export interface DetailSource {
  usageDetail(range: DetailRange, goOnly: boolean): Promise<Detail>;
}

/** Reads one range. Rendering is the caller's business. */
export async function loadDetail(
  source: DetailSource,
  range: DetailRange,
  filter: DetailFilter = "all",
): Promise<DetailState> {
  try {
    return { status: "ready", range, filter, detail: await source.usageDetail(range, filter === "go") };
  } catch (error) {
    return { status: "error", range, filter, message: String(error) };
  }
}

const RANGE_LABEL: Record<DetailRange, string> = {
  "24h": "24 h",
  "7d": "7 d",
  "30d": "30 d",
};

function sourceTag(state: DetailState): string {
  const tag =
    state.status === "ready" && state.detail.available
      ? `from ${state.detail.database} · local usage`
      : "local usage on this Mac";
  return `<span class="version" id="details-source">${escapeHtml(tag)}</span>`;
}

export function detailsHtml(state: DetailState): string {
  return `
    <div class="view view-details">
      ${bar({
        title: "Usage details",
        back: true,
        actions: [{ action: "reload-details", icon: icons.refresh, title: "Reload" }],
      })}

      <div class="segmented" role="radiogroup" aria-label="Range">
        ${DETAIL_RANGES.map(
          (range) => `
          <button
            class="segment ${state.range === range ? "on" : ""}"
            data-action="set-range"
            data-value="${range}"
            role="radio"
            aria-checked="${state.range === range}"
          >${RANGE_LABEL[range]}</button>`,
        ).join("")}
      </div>

      <div class="filter-row">
        <button
          class="chip-btn ${state.filter === "go" ? "on" : ""}"
          data-action="toggle-detail-filter"
          aria-pressed="${state.filter === "go"}"
        >${state.filter === "go" ? `${icons.check} Go models only` : "All providers"}</button>
      </div>

      <div id="details-body">${bodyHtml(state)}</div>

      ${foot({ left: sourceTag(state), right: textButton("quit", "Quit") })}
    </div>`;
}

function bodyHtml(state: DetailState): string {
  if (state.status === "loading") {
    return `<p class="note pulse" style="padding:24px 0 34px">Reading OpenCode's database…</p>`;
  }
  if (state.status === "error") {
    return `
      <div class="empty">
        <p class="empty-title">No token detail</p>
        <p class="empty-body">${escapeHtml(state.message)}</p>
        <button class="ghost-btn" data-action="reload-details">Try again</button>
      </div>`;
  }

  const { detail } = state;
  if (!detail.available) {
    return `
      <div class="empty">
        <p class="empty-title">No token detail</p>
        <p class="empty-body">${escapeHtml(detail.message ?? "Nothing to show.")}</p>
        <button class="ghost-btn" data-action="reload-details">Try again</button>
      </div>`;
  }

  return `
    ${totalsBlock(detail)}
    ${chartBlock(detail)}
    ${modelsBlock(detail)}`;
}

function totalsBlock(detail: Detail): string {
  const { totals } = detail;
  // Cache reads are the bulk of a coding agent's tokens and the cheapest; the
  // share is what explains a big total against a small plan percentage.
  const inputTokens = totals.input + totals.cacheRead;
  const cacheShare = inputTokens > 0 ? (totals.cacheRead / inputTokens) * 100 : 0;
  const breakdown = [
    `input ${formatTokens(totals.input)}`,
    `output ${formatTokens(totals.output)}`,
    totals.reasoning > 0 ? `thinking ${formatTokens(totals.reasoning)}` : null,
    `cache read ${formatTokens(totals.cacheRead)}${cacheShare > 0 ? ` (${cacheShare.toFixed(1)}%)` : ""}`,
  ]
    .filter(Boolean)
    .join(" · ");

  return `
    <section class="totals">
      <span class="eyebrow">Tokens processed</span>
      <span class="total-value">${formatTokens(totals.tokens)}</span>
      <span class="total-sub">${formatRequests(totals.requests)} · ${formatCost(totals.cost)} estimated</span>
      <span class="total-breakdown">${breakdown}</span>
    </section>`;
}

function chartBlock(detail: Detail): string {
  const { buckets, bucketUnit } = detail;
  if (buckets.length === 0) return "";

  const max = Math.max(...buckets.map((b) => b.tokens), 1);
  const bars = buckets
    .map((bucket, index) => {
      const share = bucket.tokens / max;
      const { long } = bucketLabel(bucket.key, bucket.start, bucketUnit);
      const newest = index === buckets.length - 1;
      const title = `${long} — ${formatTokens(bucket.tokens)} tokens, ${formatRequests(
        bucket.requests,
      )}, ${formatCost(bucket.cost)}`;
      return `<span class="bucket ${newest ? "bucket-now" : ""}" style="height:${Math.max(
        2,
        Math.round(share * 100),
      )}%" title="${escapeHtml(title)}"></span>`;
    })
    .join("");

  const peak = buckets.reduce((best, b) => (b.tokens > best.tokens ? b : best), buckets[0]);
  const peakLabel = bucketLabel(peak.key, peak.start, bucketUnit).long;

  return `
    <section class="activity">
      <div class="hist-head">
        <span class="eyebrow">Activity · ${bucketUnit === "hour" ? "per hour" : "per day"}</span>
        <span class="hist-value">peak ${formatTokens(peak.tokens)} · ${escapeHtml(peakLabel)}</span>
      </div>
      <div class="bars" data-unit="${bucketUnit}">
        ${bars}
      </div>
    </section>`;
}
function modelsBlock(detail: Detail): string {
  const { models, totals } = detail;
  const foreign = models.some((model) => isForeignProvider(model.provider));
  if (models.length === 0) {
    return `
      <section class="models">
        <span class="eyebrow">By model</span>
        <p class="field-help">${
          detail.filter === "go"
            ? "No Go requests recorded in this window."
            : "No requests recorded in this window."
        }</p>
      </section>`;
  }

  const shown = models.slice(0, 6);
  const rest = models.slice(6);
  const restTokens = rest.reduce((total, m) => total + m.tokens, 0);
  const restRequests = rest.reduce((total, m) => total + m.requests, 0);

  return `
    <section class="models">
      <span class="eyebrow">By model${
        detail.filter === "go" ? " · Go only" : foreign ? " · all providers on this Mac" : ""
      }</span>
      ${shown.map((model) => modelRow(model, totals.tokens)).join("")}
      ${
        rest.length > 0
          ? `<p class="field-help more-models">${rest.length} more · ${formatTokens(
              restTokens,
            )} tokens · ${restRequests.toLocaleString()} requests</p>`
          : ""
      }
    </section>`;
}

function modelRow(model: DetailModel, totalTokens: number): string {
  const share = totalTokens > 0 ? (model.tokens / totalTokens) * 100 : 0;
  const name = model.model.replace(/^(opencode-go|opencode)\//, "");
  // Models from other providers are tagged, so a mixed total is never a
  // surprise.
  const tag = isForeignProvider(model.provider)
    ? `<span class="model-tag">${escapeHtml(model.provider)}</span>`
    : "";
  return `
    <div class="model-row">
      <span class="model-rail"><span class="model-rail-fill" style="width:${share.toFixed(
        1,
      )}%"></span></span>
      <span class="model-name" title="${escapeHtml(`${model.provider}/${model.model}`)}">${escapeHtml(
        name,
      )}${tag}</span>
      <span class="model-share">${formatPercent(share)}</span>
      <span class="model-stats">
        <span>${formatTokens(model.tokens)} tok</span>
        <span>${model.requests.toLocaleString()} req</span>
        <span>${formatCost(model.cost)}</span>
      </span>
    </div>`;
}
