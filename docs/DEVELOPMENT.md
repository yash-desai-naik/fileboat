# Developing Fileboat

All commands run from the project folder in a normal terminal (Rust's `cargo` must be on the PATH;
if a build says `cargo ... program not found`, open a new terminal). The same commands work on
Windows (PowerShell or Git Bash) and macOS.

## Run while developing

```sh
npx tauri dev
```

- The first start compiles the Rust side (a few minutes); later starts are much quicker.
- Changes under `src/` (React, CSS) show in the open window on save.
- Changes under `src-tauri/src/` recompile and restart the app.
- Dev mode uses the same settings as the installed app. To keep them apart, set
  `FILEBOAT_DATA_DIR` to another folder first (settings, state and the cache all go there).
- Developer tools: on Windows, run with WebView2's remote debugging and open
  `http://127.0.0.1:9222` in Edge or Chrome:

  ```powershell
  $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9222"; npx tauri dev
  ```

  `node scripts/cdp.mjs steps.json` drives that window (evaluate, click, type, screenshot); set
  `CDP_PORT` if you used another port. On macOS, right-click in a development build and choose
  **Inspect Element**, or press ⌥⌘I.

## Code layout

Rust, `src-tauri/src/`:

- `lib.rs`: the window-free core `fileboat_core`, which a future `fb` command line will share:
  - `paths`: the data folder (`%APPDATA%\Fileboat`, `~/Library/Application Support/Fileboat`) and
    the cache folder (`%LOCALAPPDATA%\Fileboat`, `~/Library/Caches/Fileboat`);
  - `prefs`: `settings.json`, with validation and the help text Claude will use;
  - `themes`: the theme ids from `src/themes/themes.json`, and the contrast tests.
- `main.rs` with `settings` (the app's settings store and commands), `window` (window size and
  place), `desktop` (Fileboat's folders, opening things with the system) and `update` (updates).

UI, `src/` (React 19, no state library: small stores with `useSyncExternalStore`):

- `main.tsx` loads the settings and applies the theme before the first paint; `App.tsx` lays out
  the window.
- `shell/`: title bar, icons, splitter, dialogs, About, the update notice.
- `palette/`: the command bar. `lib/commands.ts` is every command with its shortcuts;
  `lib/shortcuts.ts` runs them from the keyboard.
- `settings/`: the settings page and theme picker. `lib/settings.ts` is the settings store.
- `themes/`: `themes.json` (the palettes) and `index.ts` (turns a theme into CSS variables).
- CSS: `styles.css` (tokens and base), `controls.css`, `shell.css`, `palette.css`, `update.css`,
  and `type.css` (the type hierarchy, loaded last). Components use the theme's variables only.

## Checks

```sh
npm run typecheck                    # TypeScript
(cd src-tauri && cargo test --lib)   # Rust: settings, paths, themes and their contrast rules
```

CI (`.github/workflows/ci.yml`) runs these and builds the app on Windows and macOS for every push
and pull request.

## Build the installer

```sh
npx tauri build
```

- Windows: `src-tauri\target\release\bundle\nsis\Fileboat_<version>_x64-setup.exe`. It installs for
  the current Windows user only (no admin prompt) into `%LOCALAPPDATA%\Fileboat`.
- macOS: `src-tauri/target/release/bundle/dmg/Fileboat_<version>_aarch64.dmg`, ad-hoc signed
  (`"signingIdentity": "-"`), not notarized yet.
- The app alone, without an installer: `npx tauri build --no-bundle`
  (`src-tauri\target\release\fileboat.exe`).
- If Windows packaging fails with "Access is denied (os error 5)" while setting up NSIS, see the
  same section in Tableboat's DEVELOPMENT.md: the antivirus blocks a rename; finish it by hand.

### macOS window chrome

`src-tauri/tauri.macos.conf.json` overrides the window for macOS: native decorations with
`titleBarStyle: "Overlay"`, so the system's traffic lights sit over the custom 40 px title bar
(`src/shell/TitleBar.tsx` adds left padding on macOS and hides the Windows-style caption buttons).

## Release a new version

Releases are built by GitHub Actions (`.github/workflows/release.yml`), so every download is built
from the source. One tag builds both platforms.

1. Pick the version ([Semantic Versioning](https://semver.org)).
2. Set it in all three files: `"version"` in `package.json` and `src-tauri/tauri.conf.json`, and
   `version` in `src-tauri/Cargo.toml` (then run `cargo build` once so `Cargo.lock` follows).
3. In `CHANGELOG.md`, rename `## [Unreleased]` to `## [x.y.z] - YYYY-MM-DD`, add a fresh empty
   `## [Unreleased]` above it, and add the version links at the bottom. Write entries for users.
4. Commit, then tag and push:

   ```sh
   git commit -am "chore(release): x.y.z"
   git tag -a vx.y.z -m "Fileboat x.y.z"
   git push origin master vx.y.z
   ```

5. The Release workflow checks the version matches the tag in all three files, builds on
   `windows-latest` and `macos-latest`, and creates a **draft** release with the installer, the DMG,
   the updater archives and signatures, `SHA256SUMS.txt`, notes from the changelog plus
   `.github/release-footer.md`, and `latest.json`, the update feed.
6. Read the draft under Releases, then press **Publish release**. Installed copies only see
   published releases.

Installed copies read the feed from `releases/latest/download/latest.json`, which only works while
the repository is public (or the feed is hosted elsewhere and `endpoints` in
`src-tauri/tauri.conf.json` points there).

### The updater key

- Every update must be signed with Fileboat's updater key; installed copies check it against the
  public key in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`).
- Private key: `~/.tauri/fileboat-updater.key` on the maintainer's PC (no password), and the GitHub
  repository secret `TAURI_SIGNING_PRIVATE_KEY` (its contents) for the Release workflow.
- **Keep a backup of the key** (e.g. in a password manager). Without it, no new version can be signed
  for existing installs.
- Local builds don't need it. To make a signed one locally, as the workflow does:

  ```sh
  export TAURI_SIGNING_PRIVATE_KEY="$(cat "$HOME/.tauri/fileboat-updater.key")"
  npx tauri build --config src-tauri/tauri.release.conf.json
  ```

- `FILEBOAT_UPDATE_URL` points a running copy at another feed (for testing updates).

## Measure memory and CPU

On Windows:

```powershell
powershell -ExecutionPolicy Bypass -File scripts\measure.ps1 -Exe src-tauri\target\release\fileboat.exe
```

| When | Build | Private memory | Idle CPU (30 s) |
|---|---|---|---|
| M0 (2026-11-16) | release, no folder open, Windows 11 | 115 MB across 7 processes (WebView2 GPU 55, browser 29, renderer 16, app 5) | 0.06 % |
