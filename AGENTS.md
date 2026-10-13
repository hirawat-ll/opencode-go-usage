# AGENTS.md

Working notes for agents (and people) changing this repository. The point of
this file is to save you the time the last contributor already spent: the
commands, the rules that keep the code coherent, how to prove a change works,
and the traps that have already bitten.

Read [README.md](README.md) for what the app is, [DESIGN.md](DESIGN.md) for why
it looks the way it does, [ROADMAP.md](ROADMAP.md) for what is planned (and
explicitly not), and [docs/PACKAGING.md](docs/PACKAGING.md) for shipping.

## Commands

```sh
npm install
npm run app            # tauri dev: hot reload for the panel, rebuilds Rust
npm run dev            # the panel alone in a browser, against the mock backend
npm run app:build      # release .app + .dmg
npm run icons          # regenerate the app icon and menu bar art
npm run bump 0.3.0     # set the version in all three files at once

npx tsc --noEmit       # panel types
cd src-tauri && cargo test --release   # 38 tests, ~1s once built
```

`npm run dev` is the fast loop for anything visual: the mock backend in
`src/backend.ts` stands in for Rust, so panel changes need no Rust build.

## Architecture, and the rules that keep it coherent

| Layer | Files | Rule |
| --- | --- | --- |
| Persisted | `config.rs` | Workspaces, settings, sample history, migration. All writes go through `ConfigStore`. |
| Runtime | `state.rs` | `AppState` composes `ConfigStore` + `Meter`. **Locks and the in-flight guard never leak**: callers use intent-shaped methods (`begin_poll`, `finish_poll`, `select_workspace`). |
| Wire | `ipc.rs` | The shapes the panel sees. **Secrets never cross this boundary** — a workspace is sent as a masked tail (`…a91f`); keys are written, never read back. |
| Sources | `usage.rs`, `localdb.rs` | The Go endpoint, and OpenCode's SQLite database (always opened read-only). |
| Surfaces | `tray.rs`, `panel.rs` | The status item and the popover window. |
| Wiring | `lib.rs` | Commands, the poll loop, setup. Keep it wiring: new logic belongs in a module that can be tested. |

Panel rules:

- **One derivation.** `src/signal.ts` decides severity, the binding window and
  which windows are blocked. Views must not re-derive any of it — that is how
  two code paths start disagreeing about the same numbers.
- **One chrome.** `src/components/chrome.ts` owns the title bar and footer.
  Do not hand-roll `class="bar"` or `class="foot"` inside a view.
- **No framework, no runtime dependencies.** Plain TypeScript, `src/style.css`
  for tokens. Monospace numerals, colour only ever means severity, honour
  `prefers-reduced-motion`, label every control.
- **Filters belong server-side** when they affect an aggregate: the details
  filter is SQL, because the chart is aggregated in SQL and a client-side filter
  would leave the chart and the headline counting different things.

Cross-cutting invariants:

- Percentages from different workspaces are **never** combined — limits are per
  workspace. The tray and the panel always describe the *active* workspace.
- `GO_PROVIDERS` exists in both `localdb.rs` and `src/types.ts`; a Rust test
  reads the TypeScript file and fails if the two lists drift apart.
- Parsing is lenient: a missing field inside a usage window is tolerated, but a
  response with no windows at all is a `parse` error rather than fake zeros.

## Verification: what "done" means here

Evidence before assertions. A change is not finished until you have run the
relevant check and seen it pass.

- **Rust**: `cargo test --release`. The `localdb` tests run against the real
  database when this machine has one and assert the invariants that catch silent
  breakage: `buckets == models == headline`, exactly 24/7/30 buckets, and the Go
  filter removing exactly the non-Go tokens.
- **Panel**: `npx tsc --noEmit`, then drive the mock in a browser at 344 px wide.
  The mock has sentinels so every state is reachable without a key: a workspace
  whose key contains `401`, `403`, `net`, `limit`, `nodb` or `balance` previews
  that state, and two workspaces are seeded (one configured, one not).
- **Login item**: `sfltool dumpbtm | grep -A6 <identifier>`. A LaunchAgent shows
  as `legacy agent (0x10008)` with `Developer Name: (null)` and no icon;
  ServiceManagement shows `Type: app (0x2)` with `URL:` pointing at the app
  bundle. Also check `ls ~/Library/LaunchAgents`.
- **The bundled app**: Tauri compresses embedded assets, so `strings` on the
  binary will **not** find panel text. Check `dist/assets/*.js` for the string
  and compare timestamps — `dist` is written before the binary, so a newer binary
  means the bundle embedded that panel build.
- **Relaunching the app**: `pkill -x opencode-go-usage`. Never
  `pkill -f "...opencode-go-usage"` — the pattern matches the shell running the
  command and kills it mid-sequence (this has happened; the build had completed,
  so only the relaunch was lost).

## Traps that have already cost time

1. **Bucket alignment.** The hourly grid must start on an hour boundary and the
   daily grid on a local date, or the `LEFT JOIN` misses and every bar reads zero
   *while the totals still look correct*. Assert `buckets == totals`.
2. **rusqlite parameter counts.** Binding more parameters than the statement uses
   is an error, so a query containing only `?1` cannot be handed `[a, b]`.
3. **CSS class collisions.** `.bar` is the window header; chart buckets are
   `.bucket`. Adding a class to a view can repaint an unrelated element.
4. **A malformed `balance` must not fail the response.** Strip it before
   deserializing `Usage`, then attach it leniently — a cosmetic endpoint change
   should never surface as "Unexpected response".
5. **Imprecise test assertions.** `json.contains("apiKey")` failed a migration
   test because workspaces legitimately carry `apiKey`; parse the JSON and assert
   where the field lives. Likewise, "a failure records no sample" must compare
   against the count *before* the failure.
6. **The mock must stay internally consistent.** A key-less workspace with seeded
   readings hid the first-run empty state during development.
7. **Login items.** A LaunchAgent is filed as an anonymous background agent
   (no icon, "Unknown Developer") — that is why `login_item.rs` uses
   `SMAppService`. When migrating an old item, register first and remove the old
   plist only on success, so a refusal cannot silently lose the user's item.
8. **Enable "Launch at login" only from `/Applications`.** The record captures
   the bundle path it was registered from.
9. **The version lives in three files.** Use `npm run bump`, never edit one.
10. **`tauri icon` regenerates mobile and Windows art.** Those directories are
    gitignored; `npm run icons` brings them back if ever needed.

## Releasing

```sh
npm run bump 0.3.0
npm run app:build
gh release create v0.3.0 "src-tauri/target/release/bundle/dmg/OpenCode Go Usage_0.3.0_aarch64.dmg" \
  --title "OpenCode Go Usage 0.3.0" --notes-file /tmp/notes.md
```

GitHub sanitises spaces in asset names (`OpenCode.Go.Usage_…dmg`), so check the
uploaded name before quoting it anywhere, and include the SHA-256 in the notes.
Builds are ad-hoc signed: the notes must carry the `xattr -dr
com.apple.quarantine` step until the project has a Developer ID.

## Do not

- **Write to OpenCode's database.** Read-only, always.
- **Combine percentages across workspaces**, or present an average of limits.
- **Invent token data**, or extrapolate a partial bucket to look complete.
- **Expect Mac App Store eligibility** while `macos-private-api` is enabled for
  the transparent panel.
- **Add a UI framework or a runtime npm dependency** without a strong reason;
  the panel's smallness is a feature.
