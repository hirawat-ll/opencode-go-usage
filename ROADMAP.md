# Roadmap

Where this could go next. Each item says what it is, why it is worth doing, and
what is in the way — so a decision can be made without re-deriving the context.

Priority is roughly top to bottom within each section. Nothing here is
committed; the app works as it is.

## Shipped, for reference

- Plan meters: 5-hour / weekly / monthly windows from
  `GET https://opencode.ai/zen/go/v1/usage`, with stale-while-error behaviour.
- Triple ring gauge panel, severity colouring, observed-peak sparkline.
- **Multiple workspaces**: a list of keys with labels, an active selection, per
  workspace readings and histories, migration from the single-key layout, and
  secrets that never cross IPC.
- Token detail: models, tokens, requests, estimated cost over 24 h / 7 d / 30 d,
  read read-only from OpenCode's SQLite database, with the cache-hit share,
  provider tags on non-Go models, and a **Go-only filter** applied in SQL so the
  chart and the totals always agree.
- Wallet **balance** is parsed and shown whenever the endpoint starts reporting
  one — nothing renders until then.
- **Launch at login**, toggled from Settings (a LaunchAgent the app manages).
- Menu bar title, right-click menu, keyboard shortcuts, polling on a
  configurable interval.
- Packaging: signed-or-ad-hoc `.app` + `.dmg`, version bump script.

## Next

### 1. Notifications at thresholds

One notification when a window crosses ~80%, one when it is spent, and one when
it resets. **The hard part is not spamming**: the state must be remembered per
window per reset period, which means persisting "last notified reset time" per
window per workspace. Needs `tauri-plugin-notification` and a permission prompt.

### 2. Keychain storage for keys

Keys sit in `config.json` with `0600`. The `keyring` crate moves them into the
login keychain and keeps only references on disk — the per-workspace shape makes
this a clean swap of the `apiKey` field. Trade-off: prompts on first read and one
more native dependency.

## Soon

### 3. Per-project and per-agent breakdown

The database records `session_id` (→ project directory and title) and `agent`
(build / plan / general). "Which project burned the plan this week" is
answerable today and is the obvious next cut after per-model. One extra group-by
plus a segmented control in the details view, next to the provider filter.

### 4. Account-wide numbers via the Console usage API

`GET {console}/api/v1/usage/export` returns CSV with provider, model and token
counts for 24 h / 7 d / 30 d — billed, account-wide, unlike the local figures.
It needs a **service-account** key (`oc_sk_…`), which a single-member Go
workspace may not be able to create. Worth testing before building: if a Go
plan cannot mint one, this stays theoretical.

### 10. Longer history

Sample history is capped at six hours. Keeping 30 days of samples would allow
week-over-week comparison and a real trend line, at the cost of a slightly
bigger config file (a sample per minute ≈ 43 k entries ≈ 1.3 MB of JSON).
Better: move samples into a small SQLite table (rusqlite is already a
dependency) and keep `config.json` for settings only.

### 11. Updater

`tauri-plugin-updater` plus a signed manifest in GitHub Releases, so a build
placed on another Mac can update itself. Requires proper Developer ID signing
and notarization first — see [docs/PACKAGING.md](docs/PACKAGING.md).

## Later / exploratory

- **Detachable details window.** A real window with a menu bar, dock icon while
  it is open (activation policy swap), and a wider layout. The panel is
  deliberately one narrow column today; a second window is the honest way to go
  wider, since a tray-anchored panel that grows sideways can fall off the screen
  edge (its x position is clamped only when it is shown).
- **A Go-only summary for key-less users.** Without a key the panel shows an
  empty dial. It could instead show local Go-model usage as a stand-in meter,
  clearly labelled as local. Only worth it if people use the app without a key.
- **`reduceTransparency` fallback.** Honour macOS's accessibility setting by
  swapping the vibrancy material for a solid background.
- **Provider-level filtering of the local data**, e.g. "only count
  `opencode-go`" so the local and plan numbers line up.
- **CSV/JSON export** of the details view.
- **Tray title options**: per-window triple (`5h·w·m`), or the window that resets
  soonest rather than the one closest to its limit.
- **A "since reset" reading** — how much of the current window was consumed
  since its last reset, rather than the raw percentage.

## Explicitly not planned

- **Aggregating usage across workspaces** into one number. Percentages of
  different limits do not add up; it would be a lie dressed as a feature.
- **Reading tokens from the Go API.** It reports plan percentages only. Token
  detail comes from the local database; the Console CSV is the account-wide path
  (item 9).
- **Writing to OpenCode's database**, or to any file outside the app's own
  config directory.
- **Auto-blocking requests** when a limit is near. The app watches, it does not
  gate.
- **Mac App Store distribution** while the panel uses `macos-private-api` for
  transparency; an App Store build would need the solid-panel fallback described
  in [docs/PACKAGING.md](docs/PACKAGING.md).

## Sources

Ideas and constraints above came from:

- [Issue #44189](https://github.com/anomalyco/opencode/issues/44189) — expose
  balance on the usage endpoint (use cases: status-bar widgets, budget alerts).
- [Issue #10448](https://github.com/anomalyco/opencode/issues/10448) —
  `GET /zen/v1/balance`, same motivation.
- [Issue #31084](https://github.com/anomalyco/opencode/issues/31084) — public
  API for Go plan usage.
- [Go docs](https://opencode.ai/docs/console/go) — plan shape, per-model monthly
  limits, the "use balance beyond limits" option.
- [Console usage API](https://opencode.ai/v2/docs/console/usage) — CSV export
  scopes and the service-account requirement.
- The local schema, read from `~/.local/share/opencode/opencode.db` while
  building the details view.
