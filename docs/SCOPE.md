# Fileboat – v1 scope

A file explorer for developers. It keeps what Explorer and Finder do well, adds what developers need every day
(git, real search, previews of code, Claude Code), and stays as small and quiet as Tableboat.
Built with Tauri 2 (Rust backend, the system WebView for the UI), on the same foundations as Tableboat.

Platforms: **Windows 10/11 and macOS 11+ (Apple Silicon)**, like Tableboat. Linux later.

Status: `[ ]` not started · `[~]` in progress · `[x]` done and checked by you.

---

## Budget (applies to everything below)

- [ ] Idle with one window open: under 130 MB total (app + WebView), not counting the search index or the
      Claude pane
- [ ] Search index: at most ~60 bytes of memory per indexed entry (1 million files ≈ 60 MB), most of it on disk
- [ ] No CPU use while idle: no polling. Changes arrive from the OS's file events; indexing runs at low
      priority and pauses on battery (setting)
- [ ] Folders of any size open instantly: entries stream in, and every view only draws the rows on screen
- [ ] Nothing slow on the UI thread: listing, sizes, git status, previews and search all run in Rust in the
      background and can be cancelled
- [ ] **No Node.js at runtime.** One Rust executable plus the WebView the system already has
- [ ] RAM and CPU measured and reported at each milestone

## 0. Foundation (what every explorer needs)

- [ ] Window with a custom title bar, tabs (`Ctrl+T` new, `Ctrl+W` close, drag to reorder), restored at start-up
- [ ] Back / Forward / Up (`Alt+←`, `Alt+→`, `Alt+↑`; mouse back/forward buttons), per-tab history
- [ ] Breadcrumb address bar: click a part to go there, click its arrow for sibling folders, click empty space
      (or `Ctrl+L`) to type a path with completion; `~`, `%VAR%`/`$VAR` and `\\server\share` work
- [ ] Sidebar: Favourites (pin any folder), Projects (git repositories you've opened, newest first), Drives and
      volumes with free space, Home folders (Desktop, Downloads, Documents…), Recycle Bin / Trash
- [ ] Select with click, `Shift`, `Ctrl`/`⌘`, rubber-band and keyboard; type-ahead jumps to a name
- [ ] New file, new folder (`Ctrl+Shift+N`), inline rename (`F2` / `Enter` on macOS) that selects the name
      without its extension
- [ ] Copy, cut, paste (`Ctrl+C/X/V`), also between windows and with other apps' clipboards (Explorer, Finder)
- [ ] Copying and moving run in the background with progress, pause and cancel; name conflicts ask
      Replace / Skip / Keep both, with "do this for all"
- [ ] Drag and drop inside Fileboat and to/from other apps (VS Code, the browser, Explorer, Finder)
- [ ] Delete to Recycle Bin / Trash (`Del`); delete permanently (`Shift+Del`) asks first
- [ ] Undo the last operations of this session (`Ctrl+Z`): rename, move, copy, new, delete-to-trash
- [ ] Open with the default app (`Enter` / double-click), Open with…, run executables and scripts
- [ ] Show / hide hidden and system files; show / hide extensions
- [ ] Changes made by other programs appear immediately (file system events, no refresh needed)
- [ ] Status bar: item count, selection count and size, free space, git branch, indexing progress
- [ ] Several windows; a second launch with a path (`fileboat D:\x`) opens it in the running app
- [ ] `fileboat` on the PATH from the command bar (install / remove): `fileboat .` in a terminal
      opens that folder, without holding the terminal
- [ ] Settings page (searchable) saved to `settings.json`, editable by hand
- [ ] Updates built in (signed, verified, one click), About box

## 1. Views

Every tab has a view mode, remembered per folder (with a default for new folders).

- [ ] **Columns** (macOS Finder style): parent → child columns that scroll sideways; selecting a folder opens
      the next column; arrow keys move across columns; each column resizable; the preview can be the last column
- [ ] **List**: names with icons and git badges in columns that flow down and across (as in Explorer),
      scrolled sideways; a folder that fits on screen is spread over the width
- [ ] **Details**: a table with resizable, reorderable columns you choose: Name, Git, Size, Modified, Created,
      Accessed, Type (description), Extension, Attributes/permissions, Path (in search results)
- [ ] Folder sizes in Details (computed in the background on request or by setting, cached, updated by events)
- [ ] Switch views from the toolbar, command bar and shortcuts (`Ctrl+Shift+1/2/3`; `⌘1/2/3` on macOS)
- [ ] Density (compact / default / comfortable) and zoom, shared with the rest of the UI

## 2. Sorting and grouping

- [ ] Sort by any column, ascending or descending; click a header in Details, or the Sort menu in every view
- [ ] Natural order (`file2` before `file10`), folders first (setting: first / mixed / last), dotfiles together
- [ ] Group by: none, Type/kind, Extension, Date modified (Today, Yesterday, Earlier this week, Last week, …),
      Size (Empty, Tiny, Small, Medium, Large, Huge), Git status (Changed, Untracked, Ignored, Clean),
      first letter
- [ ] Groups collapse and expand; group headers show counts and total size
- [ ] Sort and group are remembered per folder (or one setting for all folders)

## 3. Git, live

- [ ] Any folder inside a git repository knows it: the repository root, branch, ahead/behind, and whether
      a merge, rebase or cherry-pick is in progress, shown in the breadcrumb and the status bar
- [ ] Every file shows its status (modified, added, deleted, renamed, untracked, ignored, conflicted) as a
      coloured badge and name colour; folders show a dot when anything inside changed
- [ ] Ignored files dimmed (setting: dim / hide / show normally)
- [ ] Live: status updates within a moment of any change, made in Fileboat, an editor or the `git` command
      line (watches the working tree, `.git/HEAD`, `.git/index` and refs)
- [ ] Large repositories stay fast: status runs in the background, cancels when you leave, and is cached
- [ ] **Git panel** (sidebar view, `Ctrl+Shift+G`): changed files grouped as Staged / Changes / Untracked /
      Conflicts; stage, unstage, discard (asks first); commit message and Commit; Fetch, Pull, Push with
      progress; switch and create branches; stash and pop
- [ ] Diff of a changed file in the preview pane (inline or side by side), and for staged changes
- [ ] Context menu git actions on files and folders (section 7)
- [ ] Uses your installed `git` for anything that writes (so hooks, signing, credentials and config all
      behave as in your terminal); Fileboat reads the repository itself for speed

## 4. Search with in-app indexing

- [ ] **Search box** in the toolbar (`Ctrl+F`): searches the current folder and everything below it as you
      type; results in the current view with a Path column
- [ ] **Everywhere** (`Ctrl+Shift+F` or the scope switch): searches the whole index in milliseconds
- [ ] Fuzzy matching with matched letters highlighted; exact and regex modes
- [ ] Filters you can type or pick: `ext:rs,ts`, `kind:folder|file|image|code|archive…`, `size:>10mb`,
      `modified:<7d`, `git:modified|untracked|ignored`, `in:src`, `name:` vs whole path
- [ ] **File contents** search (ripgrep-style, `.gitignore`-aware) in the current folder, streamed with line
      previews; opens the file in the preview at that line
- [ ] Index names and metadata only (not contents) of the folders you choose; kept up to date from file
      system events; reconciled quickly at start-up
- [ ] **Sensible defaults: what a developer never wants indexed**, each one a rule you can switch off:
  - [ ] Dependency and build folders: `node_modules`, `bower_components`, `jspm_packages`, `vendor` (only
        inside PHP/Go projects), `target` (Cargo/Maven), `build`, `dist`, `out` (only inside projects),
        `.next`, `.nuxt`, `.svelte-kit`, `.astro`, `.turbo`, `.parcel-cache`, `.vite`, `.angular`,
        `bin`/`obj` (.NET projects), `Pods`, `DerivedData`, `.gradle`, `.dart_tool`, `.pub-cache`,
        `.terraform`, `.serverless`, `coverage`, `.nyc_output`
  - [ ] Environments and caches: `.venv`, `venv`, `env` (when it holds a Python env), `__pycache__`,
        `.tox`, `.nox`, `.mypy_cache`, `.pytest_cache`, `.ruff_cache`, `.ipynb_checkpoints`, `.cache`,
        `.eslintcache`, `.stack-work`, `.cargo/registry`, `.rustup`, `.npm`, `.yarn/cache`, `.pnpm-store`,
        `.m2`, `.nuget/packages`, `go/pkg/mod`
  - [ ] Version control internals: `.git` (objects, refs), `.svn`, `.hg`; `.git` files are still browsable
  - [ ] System and OS: `C:\Windows`, `Program Files*`, `ProgramData`, `$Recycle.Bin`,
        `System Volume Information`, `AppData\Local\Temp`, `AppData\Local\Packages`, page/hiber files;
        macOS `/System`, `/Library`, `/private`, `/Volumes` of network drives, `~/Library/Caches`,
        `~/Library/Containers`, `.Trashes`, `.Spotlight-V100`, `.fseventsd`, `.DS_Store`
  - [ ] Network drives, removable drives and cloud placeholders (OneDrive / iCloud files not on disk) are
        off by default and never downloaded by indexing
  - [ ] Respect `.gitignore` / `.ignore` / `.fbignore` inside projects (setting, on by default; ignored
        build output is still found when you browse or search that folder directly)
- [ ] **Indexing settings**: roots to index (default: your home folder and project folders; drives on
      request), exclude and include rules (folder names, globs, regex, paths), "include anyway" overrides,
      hidden files yes/no, max depth, per-root on/off, pause / resume, rebuild, index size and entry count
- [ ] Right-click a folder → Exclude from index / Include in index
- [ ] Results open, reveal in their folder (`Ctrl+Enter`) or open in a new tab

## 5. Preview pane

- [ ] Toggle on the right (`Alt+P`; `⌥P` on macOS, since `⇧⌘P` opens the command bar), resizable; also Quick Look as an overlay with `Space`
- [ ] Live: follows the selection, and refreshes when the previewed file changes on disk
- [ ] Code and text: syntax highlighting for common languages, line numbers, word wrap toggle, big files
      shown partially (first part, with "load more"), encoding and line ending detection
- [ ] Markdown rendered (with code highlighting, safely: no scripts), toggle to source
- [ ] JSON / YAML / TOML as a collapsible tree and as text; CSV / TSV as a small table
- [ ] Images (PNG, JPEG, GIF, WebP, AVIF, SVG, ICO, BMP) with size and dimensions; zoom and fit
- [ ] Audio and video players; PDF viewer
- [ ] Fonts (sample text), archives (zip, tar, tar.gz: the file list), binary files (hex)
- [ ] Changed files under git: a Diff tab beside the content
- [ ] Folders: item count, total size, git summary and its README rendered
- [ ] Info strip: size, dates, permissions/attributes, full path (click to copy)
- [ ] Open in editor, Open with default app, Copy contents (text) from the pane

## 6. Claude Code pane

- [ ] Toggle on the right (`Ctrl+Alt+B` like VS Code's secondary side bar, and from the command bar);
      shares the right side with the preview (tabs or stacked, setting)
- [ ] Runs your installed Claude Code in streaming-JSON mode, as Tableboat does: no Node.js, your own login
- [ ] **Works in the folder you're in:** Claude Code's working directory is the repository root of the
      current folder (or the folder itself outside a repository), so its project, `CLAUDE.md`, chats and
      memory are the same ones your terminal and IDE use
- [ ] Knows what you have open: the current folder, the selection, the previewed file, the view and search
- [ ] `@` in the chat input adds files and folders (fuzzy, from the index); drag files from the views onto
      the chat; attach images
- [ ] Fileboat's own MCP tools: search the index, list folders with metadata, git status, read the selection,
      select / reveal / open in the app, run any command-bar command
- [ ] Anything that changes files asks first (Allow / Deny / Always in the pane), through Claude Code's
      permission prompt routed to the pane, as in Tableboat
- [ ] Streaming replies, tool calls shown compactly, stop, new chat, resume earlier chats of that project
- [ ] The Claude process starts only when the pane opens and stops when it closes

## 7. Context menu (files and folders)

- [ ] Fileboat's own menu, themed and keyboard-friendly, for files, folders, several items, empty space,
      sidebar entries and tabs
- [ ] Open, Open in new tab, Open in new window, Open with ▸ (apps registered for the type, plus "Choose…")
- [ ] **Open in editor** ▸ (detected: VS Code, Cursor, Zed, JetBrains IDEs, Sublime, Notepad++, Vim in
      terminal; default editor in settings)
- [ ] **Open terminal here** ▸ (detected: Windows Terminal, PowerShell, Git Bash, WSL; Terminal, iTerm2,
      Ghostty, Warp on macOS)
- [ ] Copy path ▸ absolute, relative to repo root, relative to current folder, POSIX style (`/`), Windows
      style (`\`), as `file://` URL, name only, name without extension
- [ ] Cut, Copy, Paste, Duplicate, Rename, Delete, New ▸ (folder, file, and templates: `.gitignore`,
      `README.md`, `.editorconfig`, `.env`)
- [ ] Git ▸ (inside a repository): Stage, Unstage, Discard changes, Show diff, Add to `.gitignore`,
      Open on GitHub/GitLab (from the remote URL), Copy permalink
- [ ] Compress to zip, Extract here
- [ ] Ask Claude about this / Add to Claude chat
- [ ] Add to Favourites, Exclude from / Include in index, Change icon…
- [ ] Reveal in Explorer / Finder, Properties (the system's properties dialog on Windows, Get Info on macOS)
- [ ] **Show more options** on Windows: the native Explorer menu (shell extensions such as 7-Zip, Git, antivirus)
- [ ] Your own entries: commands from settings (`"contextMenu.custom"`), e.g. "Run npm install here",
      with conditions (only on folders containing `package.json`)

## 8. Command bar – `Ctrl+Shift+P` / `Ctrl+P`

VS Code's Quick Open and Command Palette, made for files. One box; the first character picks the mode,
and `?` lists the modes.

- [ ] *(nothing)*: **go to** a file or folder: recent places, favourites and projects first, then the index
      (fuzzy); `Enter` opens or goes there, `Ctrl+Enter` reveals in its folder, `Alt+Enter` opens in a new tab
- [ ] `>`: **commands**: every action with its shortcut (views, sort, group, panes, theme, git, file
      operations), acting on the current selection ("Copy relative path of 3 items")
- [ ] A path (`C:\`, `D:\pro…`, `/Users/…`, `~/`, `..`, `\\server`): **path mode** with completion of each part
- [ ] `%`: **search file contents** in the current folder (results with line previews)
- [ ] `#`: **git**: switch branch, changed files, recent commits of the current repository
- [ ] `@`: **sidebar places**: favourites, projects, drives
- [ ] `!`: **run a shell command** in the current folder, output shown in a panel (asks the first time)
- [ ] Remembers recently used items and commands, and ranks them first

## 9. Themes

- [ ] Tableboat's 19 themes and its token system (light and dark choice, "Match Windows/macOS", live preview
      on hover, contrast rules checked in CI)
- [ ] Every colour from the theme: views, git colours, preview, syntax, diff, Claude pane, menus
- [ ] Git status colours, selection and hover meet the same contrast rules in every theme

## 10. File and folder icons

- [ ] A built-in icon theme with icons for common languages, config files (`package.json`, `Cargo.toml`,
      `Dockerfile`, `.env`, `tsconfig.json`…) and special folders (`src`, `test`, `docs`, `.github`,
      `node_modules`…), open/closed folder variants
- [ ] Choose the icon theme in Settings → Appearance; "Minimal" (plain file/folder) and "System" (the OS's
      own icons, including app icons for `.exe` / `.app`) are always available
- [ ] **Use VS Code icon themes**: drop a VS Code icon theme folder (e.g. Material Icon Theme, vscode-icons)
      into Fileboat's `icon-themes` folder and pick it; same mapping format (file names, extensions, folder
      names, light/dark variants)
- [ ] **Customise**: right-click → Change icon… for this file name, this extension, this folder name or just
      this folder; pick from the theme's icons or any SVG/PNG; give folders a colour; rules saved in settings
      (`"icons.associations"`) so they can be shared
- [ ] Icons are drawn from a sprite/cache, so long folders scroll smoothly

---

## Not in v1 (later)

Staged file operations with a preview and an undo journal (Tableboat's pending edits for files) · bulk rename
(regex, with preview) · split view / dual pane · built-in terminal pane · `fb` command line · tags and colour
labels · compare folders · duplicate finder · disk usage map · workspaces (sets of tabs) · archives browsed as
folders · SFTP/SSH and cloud locations · open SQLite files in Tableboat · Linux · Windows USN journal / MFT
index for whole drives (needs an elevated helper) · plugins.
