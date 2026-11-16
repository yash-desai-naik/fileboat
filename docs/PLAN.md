# Fileboat – build plan

What to build is in [`SCOPE.md`](SCOPE.md). This file is how and in what order.

Fileboat is Tableboat's sibling: same stack, same budget, same habits (Rust does the work, the WebView only
draws what's on screen, nothing polls). Much of Tableboat's shell is reused as-is (section 3).

---

## 1. Decisions

| Decision | Choice | Why |
|---|---|---|
| Starting point | **Fresh Tauri 2 project with Tableboat's layout**, copying Tableboat's shell code (same author, MIT) | Title bar, themes, settings, command palette, Claude pane, MCP server, updater and release pipeline already work and are tested |
| Runtime | One Rust executable + the system WebView. **No Node.js at runtime** | Same budget as Tableboat |
| Where the data lives | **Rust owns every folder listing** (entries, sort, group, filter); the UI asks for the rows on screen | A 200,000-file folder never sits in the WebView; sort and group are instant and off the UI thread (Tableboat's `ResultStore` pattern) |
| Live updates | OS file events (`ReadDirectoryChangesW`, FSEvents) through one watcher service, debounced | No polling, no idle CPU; one source of truth for views, git status, the index and the preview |
| Git: reading | Repository, HEAD, branch and in-progress state read from `.git` directly; **status and ahead/behind from the user's `git status --porcelain=v2 -z --branch`** (no console window, `--no-optional-locks`), coalesced | Exactly what the user's git says: fsmonitor, the untracked cache, `core.autocrlf`, sparse checkouts, submodules and worktrees behave as in the terminal; no heavy dependency. (Changed in M5 from `gix`, whose status was the plan's main git risk.) Browsing works without `git`; badges then don't show |
| Git: writing | **The user's `git` command** (`stage`, `commit`, `pull`, `push`, `switch`, `stash`…) | Hooks, signing, credential helpers, LFS and config behave exactly as in the terminal |
| Search index | **Our own compact index** (struct-of-arrays + name arena, memory-mapped), built with `ignore`'s parallel walker and kept current by file events | Everything-like speed in ~45–60 bytes per entry; no database engine; rules (`node_modules`, build output…) decided while walking |
| Content search | On demand, ripgrep's crates (`grep-searcher`, `ignore`), streamed | Indexing contents costs gigabytes; ripgrep over one project is fast enough |
| Preview | A custom read-only URI scheme served by Rust (`fbpreview://<token>`) with Range support; text read by a command | Images, video, audio, PDF and fonts load natively in the WebView without opening the whole disk to it |
| Folder view settings | Stored in Fileboat's state folder, keyed by path, never written into the folder | No `.DS_Store` / `desktop.ini` noise in git repositories |
| Claude pane | Tableboat's bridge to the user's installed Claude Code (stream-JSON, permission tool → pane), with **the working directory set to the repository root of the current folder** | Chats, `CLAUDE.md` and memory are the project's own, the same ones the terminal and IDE use |
| Icons | **VS Code icon-theme format** for built-in and user themes; SVGs served by Rust and cached | Users can drop in Material Icon Theme or vscode-icons; one mapping format for everything |
| Context menu | Our own themed menu; on Windows "Show more options" opens the real Explorer menu (`IContextMenu`) | Fast and consistent, without losing 7-Zip, TortoiseGit and other shell extensions |

## 2. Stack

Versions are picked (latest stable) when the project is created in M0, and pinned in the lock files.

| Part | Library | Notes |
|---|---|---|
| App shell | Tauri 2 | Plugins: single-instance, dialog, updater (as Tableboat) |
| Folder listing | `std::fs` + `windows` (`FindFirstFileExW` with `FIND_FIRST_EX_LARGE_FETCH`) / `getattrlistbulk` on macOS | Name, size, dates and attributes in one pass, no extra `stat` per file |
| File events | `notify` + `notify-debouncer-full` | Non-recursive watches for open folders, recursive for index roots |
| Walking | `ignore` | Parallel, `.gitignore` aware, custom filters for the index rules |
| Glob rules | `globset` | Include / exclude rules |
| Fuzzy matching | `frizbee` (as Tableboat) | SIMD; checked against `nucleo` on a 1M-entry index in M6 |
| Git | the `git` CLI (status, writes); `.git` read directly for HEAD and state | One process per status refresh, coalesced and rested |
| Content search | `grep-searcher`, `grep-regex` | Streamed results, cancel token, binary files skipped |
| Trash | `trash` | Windows Recycle Bin (`IFileOperation`), macOS Trash; macOS undo uses the URL `trashItemAtURL` returns |
| Copy / move | `CopyFileExW` (progress callback) on Windows; `clonefile` / `copyfile` on macOS | Native speed, metadata kept, instant clones on APFS |
| Drag out | `drag` (CrabNebula) | Dragging files from Fileboat into other apps; dropping in comes from Tauri's drag-drop events |
| Windows shell | `windows` crate | Icons (`SHGetFileInfoW` / `IShellItemImageFactory`), native context menu, Properties (`SHObjectProperties`), clipboard (`CF_HDROP`, "Preferred DropEffect"), Open with, known folders |
| macOS | `objc2`, `objc2-app-kit`, `objc2-foundation` | `NSWorkspace` icons and Open with, pasteboard, Get Info, Trash URL |
| File type sniffing | `infer`, `encoding_rs`, `chardetng` | Preview picks the viewer by content, not just the extension |
| Archives | `zip`, `tar`, `flate2` | File list in the preview; compress / extract |
| UI framework | React 19, Vite, TypeScript (as Tableboat) | |
| Virtual lists | `@tanstack/react-virtual` | DOM rows (icons, inline rename, drag and drop, accessibility); one per column in Columns view |
| Preview: code | CodeMirror 6, read-only, languages loaded lazily (`@codemirror/language-data`) | Same theme tokens as Tableboat's editor |
| Preview: diff | `@codemirror/merge` | Inline and side by side |
| Preview: Markdown | `marked` + `DOMPurify` | No scripts or remote content; local images go through `fbpreview://` |
| Command bar | `cmdk` (as Tableboat) | Mode providers described in section 6 |

## 3. Reused from Tableboat

Copied, then renamed (`tableboat` → `fileboat`), keeping their tests.

| From Tableboat | Becomes |
|---|---|
| `src/shell/` (TitleBar, Menu, Dialog, Splitter, Kbd, Icons, AboutDialog, UpdateNotice) | Same; TitleBar gains the tab strip |
| `src/themes/`, `themes.json`, token CSS, contrast test, `src-tauri/src/themes.rs` | Same 19 themes; new tokens for git status, groups, drop targets |
| `src/settings/` (SettingsPage, ThemePicker), `prefs.rs`, `settings.rs` | Same mechanics, Fileboat's settings |
| `src/lib/commands.ts`, `keys.ts`, `platform.ts`, `ui.ts`, `persist.ts` | The command registry behind the command bar and shortcuts |
| `src/palette/CommandPalette.tsx` | The command bar (with modes) |
| `src/ai/`, `src-tauri/src/ai/`, `claude.rs`, `mcp.rs` (the `Host` trait) | The Claude pane; database tools replaced by file tools |
| `update.rs`, `.github/workflows/`, release footer, NSIS hooks, macOS config | Same pipeline: tag → draft release for Windows and macOS |
| `paths.rs` (`data_dir`, `write_atomic`, `expand_home`) | Same, with `FILEBOAT_DATA_DIR` |

## 4. Architecture

```
Fileboat.exe
├─ Rust core: fileboat_core (src-tauri/src, no window code; a future `fb` CLI shares it)
│  ├─ fs/        listing, folder models (sort · group · filter · row windows), file kinds, sizes
│  ├─ watch/     one watcher service: subscriptions per folder / root, debounce, overflow → rescan
│  ├─ ops/       job queue: copy · move · rename · new · trash · delete · compress/extract, conflicts, undo log
│  ├─ git/       repo discovery and cache, status (gix), branch info, CLI runner for writes
│  ├─ index/     rules, walker, compact store (mmap), live updates, start-up reconcile, search + filters
│  ├─ grep/      content search
│  ├─ preview/   type detection, text reader (encoding, partial), archive lists, the fbpreview:// scheme
│  ├─ icons/     icon themes (VS Code format), resolution per entry, custom rules, system icons
│  ├─ shell/     open / open with, editors and terminals detection, clipboard, native menu, properties
│  ├─ claude, mcp  Claude Code bridge and Fileboat's MCP tools (from Tableboat)
│  └─ prefs, themes, paths
└─ WebView UI (src)  React
   ├─ shell/     title bar with tabs, toolbar (nav · breadcrumb · search · view switch), status bar, panes
   ├─ sidebar/   favourites, projects, drives, home folders, git panel
   ├─ views/     Columns · List · Details, selection, inline rename, drag and drop, group headers
   ├─ preview/   viewers per type, Quick Look overlay
   ├─ palette/   command bar and its modes
   ├─ menus/     context menus
   ├─ settings/  settings page (incl. indexing rules, icon themes)
   └─ ai/        Claude pane
```

**How a folder is shown**

1. The UI calls `folder_open(path, view)`; Rust lists the folder on a worker thread and streams batches of
   entries over a Tauri `Channel` (the first screen arrives in a few ms, even for huge folders).
2. Rust keeps the folder model: entries, the sort, the groups, the filter, git status and icon ids. The UI
   holds only a row count and asks `folder_rows(id, from, to)` for what's visible (blocks of ~200, cached).
3. The watcher sends changes; Rust updates the model and emits a small diff (`inserted`, `removed`,
   `changed` row ranges); the view keeps the selection and scroll position.
4. Git status and folder sizes arrive later as their own updates; they never block the listing.
5. Closing a tab or leaving a folder drops its model and its watch.

**How the index works**

- Rules are compiled once: name sets (`node_modules`, `__pycache__`…), globs, paths, and *conditional* rules
  checked against siblings while walking (`vendor` only beside `composer.json` / `go.mod`; `bin`/`obj` only
  beside a `*.csproj`; `env` only if it contains `pyvenv.cfg`; `build`/`dist`/`out` only inside a project).
- Store: per entry `parent: u32`, `name: (offset u32, len u16)` into one UTF-8 arena, `kind: u8`,
  `flags: u8`, `size: u64`, `mtime: i64` in parallel arrays, saved as one file and memory-mapped at start.
- Live: events from the recursive root watches insert, remove or update entries; deletions leave tombstones
  that a compaction pass removes when idle.
- Start-up reconcile: on Windows, re-list only folders whose modified time changed since the snapshot (NTFS
  updates a folder's time when entries are added, removed or renamed); on macOS, replay FSEvents since the
  saved event id. A full rebuild only when the snapshot is missing or the rules changed.
- Search: filters first (`ext:`, `kind:`, `size:`, `modified:`, `in:`), then fuzzy over names in parallel,
  ranked with boosts for recent places, favourites, projects and shallow paths; top results only.
- The indexer runs at background priority (`THREAD_MODE_BACKGROUND_BEGIN`, QoS background on macOS) and
  pauses on battery when that setting is on.

**How git stays live**

- Each folder resolves its repository once (cached); a repository gets one status worker.
- Triggers: file events inside the working tree, plus `.git/HEAD`, `.git/index`, `refs/`, `packed-refs`,
  `MERGE_HEAD` / `REBASE_HEAD`. Bursts are coalesced (~150 ms); a newer run cancels an older one.
- Status covers the whole working tree but results are sent only for folders on screen (files, plus the
  "something inside changed" dot for subfolders).
- Writes run `git` with the repository as working directory, stream progress to the git panel, then
  trigger a status refresh.

**How the Claude pane fits**

- Started on demand. Working directory = repository root of the current folder, or the folder itself.
- Navigating into another repository doesn't restart a running chat; the pane shows "Switch to `<repo>`",
  and the next new chat uses the current one.
- MCP tools (in-process, `127.0.0.1`, random token, Origin check, as Tableboat): `get_context`,
  `search_files`, `list_folder`, `git_status`, `get_selection`, `reveal` / `select` / `open_in_app`,
  `list_app_commands`, `run_app_command`, `move` / `rename` / `trash` / `create` (ask first).
- Claude Code's own Read / Glob / Grep are pre-allowed; Edit / Write / Bash and every changing tool ask in
  the pane (Allow / Deny / Always for this chat).

**Where things are stored**

- `%APPDATA%\Fileboat\` (macOS `~/Library/Application Support/Fileboat/`): `settings.json`, `state/` (open tabs
  and windows, favourites, recent places, per-folder view settings with a size limit), `icon-themes/`
- `%LOCALAPPDATA%\Fileboat\index\` (macOS `~/Library/Caches/Fileboat/index/`): the search index, safe to delete
- `FILEBOAT_DATA_DIR` overrides the data folder (tests, portable copy)

## 5. Visual direction

Tableboat's look, arranged like a modern explorer.

- **Title bar (40 px) holds the tabs** (as Windows 11 Explorer and browsers do), with Snap Layouts on Windows
  and traffic lights on macOS. Tab: folder icon, folder name, git branch chip when in a repository.
- **Toolbar (36 px)**: Back, Forward, Up · breadcrumb (repository root shown as a chip with the branch) ·
  search box with scope switch · view switch (Columns / List / Details) · sort/group · preview and Claude toggles.
- **Sidebar (240 px)** a shade darker than the content: Favourites, Projects, Drives, Home, Trash; the git
  panel replaces it with `Ctrl+Shift+G`.
- **Rows**: 24 px (Compact 22, Comfortable 28); names in the UI font, sizes and dates in JetBrains Mono with
  aligned digits, right-aligned sizes; git badge (`M`, `A`, `U`, `D`, `R`, `!`) in the status colour with the
  name tinted; ignored files at reduced contrast that still passes the hint-text rule.
- **Columns view**: 220 px columns by default, a chevron on folders, the selected path stays highlighted in
  every column, a soft divider you can drag.
- **Preview / Claude** on the right (360 px), tabs at the top of that pane when both are open.
- **Status bar (24 px)**: items · selection and size · free space · branch, ahead/behind · indexing progress.
- **Cheap on purpose**: solid surfaces, no blur, no spinners; a 2 px progress bar while a long job runs.

**Keyboard** (macOS uses ⌘ for Ctrl):

| Keys | Action |
|---|---|
| `Ctrl+P` / `Ctrl+Shift+P` | Command bar: go to / commands |
| `Ctrl+L` | Type a path |
| `Ctrl+F` / `Ctrl+Shift+F` | Search here / everywhere |
| `Ctrl+Shift+1/2/3` | Columns / List / Details |
| `Alt+P` (`⌥P`) · `Space` | Preview pane · Quick Look |
| `Ctrl+Alt+B` · `Ctrl+B` · `Ctrl+Shift+G` | Claude pane · sidebar · git panel |
| `` Ctrl+` `` · `Ctrl+Shift+E` | Terminal here · open in editor |
| `F2` · `Ctrl+Shift+N` · `Ctrl+Alt+N` | Rename · new folder · new file |
| `Ctrl+T` / `Ctrl+W` / `Ctrl+Tab` / `Ctrl+1..9` | Tabs |

## 6. The command bar

One `cmdk` box with providers, chosen by the first character (shown as a chip; `Backspace` on an empty box
returns to go-to mode):

| Prefix | Provider | Source |
|---|---|---|
| none | Go to file or folder | recents + favourites + projects, then `index_search` |
| `>` | Commands | the command registry (same list Claude sees) with shortcuts, aware of the selection |
| path-like | Path completion | `path_complete` lists the typed folder |
| `%` | Content search | `grep_start` / streamed results / `grep_cancel` |
| `#` | Git | branches, changed files, recent commits of the current repository |
| `@` | Places | sidebar entries |
| `!` | Shell command | runs in the current folder through the user's shell; output in a panel |
| `?` | Help | lists the modes |

## 7. Milestones

Each milestone ends with: a build you run and try, measured RAM (app + WebView) and idle CPU, and the
relevant `SCOPE.md` items ticked off by you.

| # | Milestone | What you can do at the end |
|---|---|---|
| **M0** | **Shell from Tableboat** – Tauri project (`fileboat_core` lib + app), title bar with tabs, panes, themes and contrast test, settings, command bar with `>` commands, updater, CI on Windows and macOS | Open the app, switch themes, open the command bar. Baseline RAM |
| **M1** | **Browse** – folder models in Rust with streamed listing, watcher service, List and Details views (virtualised), sorting, navigation and history, breadcrumb / address bar with completion, sidebar (favourites, drives, home), open with default app, hidden files, status bar, tabs restored, basic built-in icons | Browse any folder, live, by mouse and keyboard |
| **M2** | **File operations and context menu** – new, rename, copy / cut / paste, drag and drop in and out, trash, delete, undo, job queue with progress and conflicts, clipboard with Explorer / Finder, Open with, editors and terminals detection, copy path variants, Properties, native "Show more options" on Windows, custom menu entries | Use Fileboat instead of Explorer / Finder for daily file work |
| **M3** | **Columns and grouping** – Columns view, group by (type, extension, date, size, git, letter), collapsible groups, per-folder view state, folder sizes | All three views with sort and group |
| **M4** | **Preview pane** – `fbpreview://` scheme, code/text with highlighting, Markdown, JSON/YAML/TOML tree, CSV, images, audio/video, PDF, fonts, archives, hex, folder summary with README, live refresh, Quick Look | See any file without opening it |
| **M5** | **Git** – repository detection, live status badges and folder dots, branch / ahead-behind / in-progress state, git panel (stage, unstage, discard, commit, fetch / pull / push, branches, stash), diff in the preview, git context menu, `#` mode | Everyday git without leaving the explorer |
| **M6** | **Index and search** – rules with developer defaults, indexing settings page, start-up reconcile, search here / everywhere with filters, content search, command bar go-to, path, `%`, `@`, `!` modes | Find any file in milliseconds |
| **M7** | **Claude pane** – Tableboat's bridge, working directory per repository, Fileboat MCP tools, `@` files from the index, drag files into the chat, Ask Claude in the context menu | Ask Claude about the project you're browsing |
| **M8** | **Icons** – full built-in icon theme, VS Code icon themes from `icon-themes/`, System and Minimal themes, Change icon… and folder colours, icon settings | Make Fileboat look the way you like |
| **M9** | **Finish** – installer (per user, no admin) and DMG, edge-case pass (below), final RAM / CPU report, 0.1.0 release | Install it like any app |

## 8. Edge cases built in

Each becomes a test or a manual check in M9.

- **Paths (Windows)**: paths longer than 260 characters (`\\?\` internally), names with trailing dots or spaces,
  reserved names (`CON`, `NUL`), case-only renames (`readme.md` → `README.md`, via a temporary name), UNC
  paths and mapped drives, `\\wsl$`, junctions and symlinks (never followed when sizing or indexing, loops
  detected), files locked by another program (clear message naming the program when Windows knows it).
- **Paths (macOS)**: Unicode normalisation (NFD names on disk match NFC typing), case-insensitive volumes,
  packages (`.app`, `.bundle`) shown as files with "Show package contents", privacy prompts (Desktop,
  Documents, Downloads) and Full Disk Access explained in the UI instead of empty folders.
- **Cloud files**: OneDrive / iCloud placeholders are never downloaded by listing, indexing, sizing or
  previewing (attributes checked first); the preview offers "Download to preview".
- **Big and busy folders**: 1,000,000 entries in one folder; `npm install` creating 100,000 files while being
  watched (watch buffer overflow → that folder rescanned once, not a storm of updates); a file changing
  while it's copied; a folder deleted while it's open (go up to the nearest existing parent).
- **Operations**: copying a folder into itself refused; "Keep both" names like Explorer (`name - Copy (2)`)
  and Finder (`name copy 2`); cut + paste across volumes = copy then delete, only after the copy succeeded;
  undo of a move when the source now exists again asks first; permanent delete always asks.
- **Git**: repositories with submodules and worktrees, bare repositories, sparse checkouts, `core.autocrlf`
  without false "modified", huge monorepos (status capped in time, then "status paused, click to run"),
  `git` missing (browsing still works, writes explain how to install it), repositories on network drives.
- **Preview**: binary files that look like text, UTF-16 with BOM, 2 GB log files (only the start is read),
  SVG with scripts (shown as an image, scripts never run), Markdown with raw HTML (sanitised).
- **Display**: crisp at 100/125/150 % and mixed-DPI monitors; AltGr layouts don't trigger shortcuts;
  right-to-left and very long file names don't break rows.

## 9. Risks

| Risk | Plan |
|---|---|
| WebView alone is ~90 MB, the budget is tight | Measure from M0; lazy-load the preview viewers, CodeMirror and the Claude pane; low-memory mode when minimised |
| Status on huge repositories takes seconds | Runs in the background, a newer run cancels an older one, a busy repository rests between runs; git's own fsmonitor and untracked cache speed it up |
| Watcher buffers overflow under heavy writes | Detect overflow and rescan only the affected folder; coalesce git and index updates |
| Index memory on big disks | Memory-mapped store, home + projects by default, drives opt-in, size and count shown in settings |
| Native Explorer context menu inside a WebView window (COM, shell extensions that crash) | Runs on the UI thread in an STA; behind "Show more options" only; a separate helper process if crashes show up |
| Dragging files out of the WebView | `drag` crate in M2; tested with VS Code, browsers, Explorer, Finder, Slack |
| Icon theme licences | Ship only MIT-licensed icons with credits (as Tableboat does for theme palettes); user themes are loaded, never redistributed |
| Claude Code's stream-JSON and permission formats change | Shared code with Tableboat, fixed once for both |
| Scope keeps growing ("more coming soon") | New ideas go to the "Not in v1" list in `SCOPE.md` until 0.1.0 ships |
