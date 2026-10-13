# Packaging and deployment

Everything needed to go from source to an app someone else can double-click.

## What a build produces

```sh
npm install
npm run app:build
```

| Artifact | Path |
| --- | --- |
| App bundle | `src-tauri/target/release/bundle/macos/OpenCode Go Usage.app` |
| Disk image | `src-tauri/target/release/bundle/dmg/OpenCode Go Usage_0.2.0_aarch64.dmg` |
| Raw binary | `src-tauri/target/release/opencode-go-usage` |

The app is about 5 MB (2.7 MB compressed) because the frontend is ~30 kB of
plain HTML/CSS/JS and the Rust side is statically linked — the only sizeable
passenger is the bundled SQLite used to read OpenCode's local database.

### Prerequisites

- macOS 10.15 or newer
- Xcode Command Line Tools: `xcode-select --install`
- Rust via [rustup](https://rustup.rs)
- Node 20 or newer

The first build compiles ~500 crates plus SQLite from source and takes 4–8
minutes. Later builds reuse that cache and finish in well under a minute.

### Build for both architectures

By default you get the host architecture (Apple Silicon here). For a universal
build:

```sh
rustup target add x86_64-apple-darwin
npm run tauri build -- --target universal-apple-darwin
```

## Versioning

The version lives in three files, and it is easy to forget one:

```sh
npm run bump 0.2.0
```

That rewrites `package.json`, `src-tauri/tauri.conf.json` and
`src-tauri/Cargo.toml`. The bundled `CFBundleShortVersionString` comes from
`tauri.conf.json`; the panel's footer reads `env!("CARGO_PKG_VERSION")` from
`Cargo.toml`.

## Signing

Without any configuration the bundle is signed ad-hoc (`codesign -s -`). That
is enough to run it on the machine that built it and nothing else: Gatekeeper
quarantines the app on any other Mac and reports it as damaged.

### Developer ID (for handing it to other people)

```sh
export APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAMID)"
export APPLE_ID="you@example.com"
export APPLE_PASSWORD="abcd-efgh-ijkl-mnop"   # app-specific password
export APPLE_TEAM_ID="TEAMID"
npm run app:build
```

Tauri signs the app with the hardened runtime, signs the DMG, uploads both to
Apple's notary service and staples the ticket, so the download opens with no
warnings. Verify with:

```sh
codesign -dv --verbose=4 "src-tauri/target/release/bundle/macos/OpenCode Go Usage.app"
spctl -a -vv "src-tauri/target/release/bundle/macos/OpenCode Go Usage.app"
xcrun stapler validate "src-tauri/target/release/bundle/dmg/OpenCode Go Usage_0.2.0_aarch64.dmg"
```

### Installing on another Mac without a Developer ID

Copy the `.app` to `/Applications`, then clear the quarantine flag once:

```sh
xattr -dr com.apple.quarantine "/Applications/OpenCode Go Usage.app"
```

Or right-click the app and choose **Open** the first time.

### Why this app cannot ship on the Mac App Store

The panel is a transparent, vibrancy-backed window, which needs
`macos-private-api` (`app.macOSPrivateApi` in `tauri.conf.json` plus the
`macos-private-api` feature in `Cargo.toml`). That combination is explicitly
rejected by App Review. Making it store-legal means dropping transparency and
window effects in favour of a solid panel:

1. `"macOSPrivateApi": false`, remove `macos-private-api` from the Cargo
   features, remove `"transparent": true` and the `windowEffects` block.
2. Give `.view` a solid background instead of relying on the material.

Notarization, on the other hand, is unaffected — Developer ID builds with the
private API are fine.

## Distributing

The DMG is a drag-to-Applications image with the app icon and an Applications
link, so it needs no explanation. For anything more than a handful of people:

- **GitHub Releases** plus the CI job below.
- **Homebrew cask** in a personal tap:

  ```ruby
  cask "opencode-go-usage" do
    version "0.2.0"
    sha256 "..."
    url "https://github.com/<you>/opencode-go-usage/releases/download/v#{version}/OpenCode.Go.Usage_#{version}_aarch64.dmg"
    name "OpenCode Go Usage"
    desc "Menu bar meter for OpenCode Go plan usage"
    homepage "https://github.com/<you>/opencode-go-usage"
    app "OpenCode Go Usage.app"
  end
  ```

## Continuous integration

```yaml
# .github/workflows/release.yml
name: release
on:
  push:
    tags: ["v*"]

jobs:
  bundle:
    runs-on: macos-14
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: 22
      - uses: dtolnay/rust-toolchain@stable
        with:
          targets: aarch64-apple-darwin,x86_64-apple-darwin
      - run: npm ci
      - uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          APPLE_SIGNING_IDENTITY: ${{ secrets.APPLE_SIGNING_IDENTITY }}
          APPLE_ID: ${{ secrets.APPLE_ID }}
          APPLE_PASSWORD: ${{ secrets.APPLE_PASSWORD }}
          APPLE_TEAM_ID: ${{ secrets.APPLE_TEAM_ID }}
        with:
          tagName: ${{ github.ref_name }}
          releaseName: "OpenCode Go Usage ${{ github.ref_name }}"
          releaseDraft: true
          args: --target universal-apple-darwin
```

The `APPLE_*` secrets are optional: without them the workflow still produces an
unsigned DMG for internal use.

## Installing on your own Mac

1. Drag `OpenCode Go Usage.app` to `/Applications`.
2. Launch it, click the menu bar icon, paste your Go API key.
3. To start it with the Mac: **Settings → Launch at login** inside the app. On
   macOS 13+ this registers the app with ServiceManagement, so System Settings →
   Login Items lists it under the app's own name and icon. Older systems get a
   LaunchAgent at `~/Library/LaunchAgents/OpenCode Go Usage.plist` instead.

   Launch the app from `/Applications` *before* enabling this: the login item
   records the bundle it was registered from, so enabling it while running out of
   a build directory would register that path.

   An item enabled by an earlier version (created as a LaunchAgent) is migrated
   to ServiceManagement on first launch of this one, which is what makes System
   Settings show the app rather than an anonymous background agent.

## Uninstalling

```sh
# turn off "Launch at login" in the app first — it removes the login item.
# On macOS 12 only, the fallback launch agent is a file:
rm -f ~/Library/LaunchAgents/OpenCode\ Go\ Usage.plist
rm -rf "/Applications/OpenCode Go Usage.app"
rm -rf ~/Library/Application\ Support/ai.opencode.go-usage
```

The Application Support path holds the API keys, settings and sample histories.

## What the app needs at runtime

- **Outbound HTTPS** to `opencode.ai` for the usage endpoint, on your poll
  interval.
- **Read access** to `~/.local/share/opencode/opencode.db` (or
  `$XDG_DATA_HOME/opencode/`) for the details view. It is opened read-only.
- **Write access** to the login item store, and (macOS 12 only) to
  `~/Library/LaunchAgents/`, if you switch on *Launch at login*.
- It never asks for Screen Recording, Accessibility, or any other permission,
  and it does not phone anywhere except `opencode.ai`.

## Release build settings

`src-tauri/Cargo.toml` tunes the release profile for a small binary:

```toml
[profile.release]
codegen-units = 1
lto = true
opt-level = "s"
panic = "abort"
strip = true
```

`panic = "abort"` is safe here: the app has no unwinding-sensitive state, and a
crash is a crash either way.

## Troubleshooting

| Symptom | Cause and fix |
| --- | --- |
| "OpenCode Go Usage is damaged and can't be opened" | Quarantine on an unsigned build. `xattr -dr com.apple.quarantine` the app, or sign it properly. |
| `failed to read dist/index.html` | The frontend was not built. `npm run build`, or let `npm run app:build` do it. |
| No icon in the menu bar | `src-tauri/icons/tray.png` was not generated. Run `npm run icons`. |
| Details view says it found no database | You are not running OpenCode from a standard location, or it has never recorded a request. Set `XDG_DATA_HOME` if you keep your data elsewhere. |
| `cargo: command not found` in `tauri dev` | Rust is not on `PATH` for the shell that launched the app. `source ~/.cargo/env`. |
| Tauri CLI version mismatch | `npm ci` — the CLI is pinned in `package.json`. |
