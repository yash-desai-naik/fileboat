# Changelog

All notable changes to Fileboat are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **A Refresh button** beside Up in the toolbar (or `F5`): reads the folder again.
- **`fileboat` in a terminal,** as VS Code's `code`: `fileboat .` opens the folder you're in,
  `fileboat D:\x` or `fileboat notes.md` work too, and the terminal is free at once. Install it
  from the command bar (**Shell Command: Install the 'fileboat' Command in PATH**), then open a new
  terminal; it works in PowerShell, cmd and Git Bash. **Remove the 'fileboat' Command from PATH**
  takes it away again (do that before uninstalling Fileboat). On Windows the launchers go in
  `%LOCALAPPDATA%\Fileboat\bin`, added to your user PATH; on macOS in `/usr/local/bin`.

### Changed

- **List view uses the whole width,** as Explorer's List does: names flow down a column and on into
  the next, and a big folder scrolls sideways (the wheel scrolls it too; `←` `→` move a column).
  A folder that fits on screen is spread over the width, so long names show in full. Columns are
  at least as wide as Details' Name column.

### Fixed

- **A long path no longer fills the whole address bar:** a strip at its end always stays free to
  click for typing a path (as does `Ctrl+L`).

## [0.5.0] - 2026-11-18

### Added

- **Live theme preview in the command bar,** as in VS Code: arrowing or pointing at a Theme command
  shows its theme at once; `Enter` keeps it, `Esc` puts yours back. Light, Dark, Toggle and Next
  Theme preview the theme they would switch to.

## [0.4.0] - 2026-11-17

### Added

- **Go to File** (`Ctrl+P`), as in VS Code: any file in the index by name. `daily_logs.ts:32` (or
  `:32:5`) opens it at that line in your editor (VS Code, Cursor, Windsurf, Zed, Sublime Text,
  Notepad++, JetBrains IDEs); `Ctrl+Enter` shows it here with the preview at that line.
- **Go to Symbol:** `daily_logs.ts@load` lists the file's functions, classes, types and methods
  (or a Markdown file's headings) and jumps to one. `Ctrl+Shift+O` lists the selected file's;
  `Ctrl+G` goes to a line in it.

## [0.3.0] - 2026-11-17

### Added

- **Zoom with the wheel:** `Ctrl`+wheel (or a pinch) zooms the whole window. `Alt`+wheel zooms just
  the section under the pointer (the sidebar, the files, the preview or the Claude pane), and each
  keeps its level; Reset Section Zoom in the command bar puts them back.

## [0.2.0] - 2026-11-17

### Added

- **Favourites, by drag and drop:** drop folders on Favourites to pin them (between two, or on the
  heading); drag a favourite up or down to reorder it (or Move Up / Move Down on its menu).
- **Right-click a favourite or home folder** for Open in New Tab, Show in Explorer, Copy Path,
  Change Icon…, and Add to or Remove from Favourites.

### Changed

- Favourites and home folders show their folder's icon, as in the list: they follow the icon theme
  and Change Icon…. Home, Desktop, Documents, Downloads, Pictures, Music and Videos share one colour
  and each have a mark, in the sidebar and in lists alike.

## [0.1.0] - 2026-11-16

The first release. Made and tested on Windows 11. The macOS build is made by the same workflow but
hasn't been tried on a Mac yet, and a few things are Windows-only for now: your system's own icons,
undoing a move to the Trash, Open With, and files on the clipboard shared with Finder.

### Added

- **Files in use:** when a file can't be deleted, renamed, copied or moved because a program has it
  open, Fileboat says which program (on Windows), and which file inside a folder.
- **Awkward names** that WSL, git or a Mac can make and Windows' own tools trip over (a trailing dot
  or space, device names like `con.txt`) open, preview, copy, rename and delete like any other.
- **A folder deleted while open** takes its tab to the nearest folder above it that still exists.
- **Huge repositories:** where `git status` takes over 10 seconds, it stops running on every change;
  "status paused" in the status bar runs it when you ask.
- **Cloud files** (OneDrive, iCloud) are still never downloaded by browsing; their preview offers
  Download to preview.
- **Icons:** Fileboat's own, drawn for developers: a shape per kind with its mark and colour,
  language colours, and special files and folders (`package.json`, `Cargo.toml`, `.git`,
  `node_modules`, `src`, `docs`…). Or Minimal, your system's own icons (Windows), or any **VS Code
  icon theme** (Material Icon Theme, vscode-icons…): put its folder in the icon themes folder and
  choose it in Settings ▸ Appearance.
- **Change Icon…** on any file or folder: a colour and an icon (one of Fileboat's, any icon of the
  current VS Code theme, or your own SVG, PNG or ICO) for that item, everything with its name, or
  every file with its extension. "Back to the theme's icon" undoes it.
- **Claude pane** (`Ctrl+Alt+B`): your own Claude Code (your login, no Node.js) in a side pane,
  working in the repository you're browsing, with that project's `CLAUDE.md`, chats and memory. It
  sees your folder, selection, preview and search; `@` adds files and folders, images can be
  pasted; Ask Claude on any file. Fileboat's own tools let it search the index, list folders, read
  git status, show and select files, run any command and change settings; creating, renaming,
  moving or deleting files asks you first and can be undone with `Ctrl+Z`. Earlier chats resume;
  closing the pane stops Claude Code.
- **Search** (`Ctrl+F` here, `Ctrl+Shift+F` everywhere): results as you type, matched letters
  highlighted, the preview following; Enter opens, `Ctrl+Enter` shows it in its folder, `Alt+Enter`
  opens a tab. Words match fuzzily; filters narrow it down: `ext:rs,ts`, `kind:image`,
  `size:>10mb`, `modified:<7d`, `in:src`, `src/main`, `/regex/`.
- **A search index** of names and dates (never contents) of your home folder and the folders you
  add, in milliseconds, kept current as files change and caught up quickly at start-up. It leaves
  out what developers never search, each a rule you can switch off: `node_modules`, `target`,
  `.venv`, `__pycache__`, package and tool caches, `.git`, system folders and more; `vendor`,
  `build`, `dist`, `bin`/`obj` only where a project makes them so. `.gitignore` is honoured;
  cloud files are never downloaded. Settings ▸ Search index, and Search Index on a folder's menu.
- **Search inside files** (`%` in the command bar, `Ctrl+Shift+H`), ripgrep-style, `.gitignore`
  aware, streamed as found; Enter shows the file with the preview at that line.
- **Command bar modes:** Go to a file or folder (recent and favourite folders, then the index), a
  typed path with completion, `@` places, `!` a shell command in this folder.
- **Git, live:** inside a repository every file shows its status (the name in the status colour
  and M, A, D, R, U or ! for a conflict), folders with changes inside show a dot, and files git
  ignores are dimmed, hidden or shown normally (a setting). The status bar shows the branch,
  ahead/behind, a merge or rebase in progress and how many files changed. All of it follows
  changes made anywhere: in Fileboat, an editor or the `git` command line.
- **Git panel** (`Ctrl+Shift+G`, beside Places in the sidebar): changed files as Conflicts, Staged,
  Changes and Untracked, with stage, unstage and discard; commit (`Ctrl+Enter`); switch to or
  create a branch; fetch, pull and push with progress; stash and pop. Everything runs your own
  `git`, so hooks, signing and credentials work as in your terminal.
- **Diffs** of changed files in the preview pane (inline or side by side; unstaged or staged).
- **Git menu** on files and folders: Stage, Unstage, Discard Changes, Show Diff, Add to .gitignore,
  Open on the Web and Copy Permalink (GitHub, GitLab, Bitbucket). **Group by Git status.** In the
  command bar, `#` lists changed files, branches and recent commits.
- **Preview pane** (`Alt+P`): follows the selection and updates when the file changes on disk.
  Code with highlighting for over a hundred languages (line numbers, folding, wrap); the first
  512 KB of big files with Load more; the encoding (UTF-8, UTF-16, legacy code pages) and line
  endings. Markdown rendered safely (no scripts, remote images not loaded), JSON, YAML and TOML as
  a tree, CSV as a table, images with their size, audio and video, PDFs, fonts, zip and tar
  contents, and a hex view for anything else. With nothing selected, the folder: its counts,
  size and README. Open, Open in editor and Copy contents from the pane; the path copies on click.
- **Quick Look** (`Space`): the selected file large over the window; the arrow keys move on.
- **Columns view** (`Ctrl+Shift+1`), as in Finder: the folders on the way down to the open one,
  and a peek into the selected folder. → goes into a folder, ← goes up, a click in any column
  goes there; moving around in columns doesn't fill Back's history.
- **Group by** type, date modified (Today, Yesterday, Earlier this week…), size or first letter.
  Headers show the count and total size; click one to collapse it.
- **Each folder remembers its view**, sort and grouping; the Settings values are the defaults for
  the rest. "Reset to Default View" and "Use as Default View" in the View menu; turn it off for
  one view everywhere.
- **Folder sizes** ("Show folder sizes", or "Calculate Folder Sizes Here" for one folder): worked
  out in the background for the folder in front, shown in the Size column, the selection's total
  and group totals, and sortable. Links and junctions aren't followed and cloud-only folders
  aren't entered, so nothing is downloaded. Sizes are worked out again on F5, when the folder's
  own items change, and after copies and moves into it.
- **File operations:** new folder (`Ctrl+Shift+N`) and file (`Ctrl+Alt+N`), rename in place (`F2`;
  names Windows refuses are explained as you type), duplicate, delete to the Recycle Bin / Trash
  (`Del`) or permanently (`Shift+Del`, asks first).
- **Copy and move** in the background with progress and Cancel. When a name is taken, choose
  Replace, Skip or Keep both (for one or all); replaced files go to the Recycle Bin.
- **Undo** (`Ctrl+Z`) for new items, renames, deletes to the Recycle Bin, copies and moves.
- **Clipboard shared with Explorer:** `Ctrl+C`, `Ctrl+X`, `Ctrl+V` between Fileboat and Explorer
  both ways; `Ctrl+Shift+C` copies paths as text.
- **Drag and drop:** drag files out to Explorer, editors, browsers and chat apps; drop files on a
  folder, a tab, a sidebar place or a breadcrumb part (same drive moves, another drive copies; Ctrl
  copies, Shift moves).
- **Context menus** for files, folders and empty space: open with, open in your editor, terminal
  here, copy path in every form, new from templates (`.gitignore`, `README.md`, `.editorconfig`,
  `.env`), view and sort, Properties, and on Windows "Show More Options" for Explorer's own menu.
- **Open in editor** (`Ctrl+Shift+E`) and **terminal here** (`` Ctrl+` ``), with the editors and
  terminals installed on your computer found automatically.
- **Browse folders** in a Details view (Name, Modified, Type, Size: click a header to sort,
  drag to resize) or a List view. Huge folders open at once: the first rows show in milliseconds
  and only the rows on screen are drawn, so 200,000 files scroll smoothly.
- **Live:** folders update the moment another program creates, changes, renames or deletes
  files. Nothing polls.
- Select with click, Ctrl+click, Shift+click, the arrow keys and Ctrl+A; type a name to jump to
  it; Enter or a double-click opens a folder, or opens a file with its app.
- Natural order (file2 before file10), folders first, newest or biggest first for dates and
  sizes; hidden files shown on request (`Ctrl+Shift+.`).
- **Tabs** in the title bar, each with its own history; `Ctrl+T`, `Ctrl+W`, `Ctrl+Shift+T`,
  `Ctrl+Tab`, `Ctrl+1..9`, middle-click to open a folder in a new tab, drag to reorder. Tabs come
  back next time.
- **Back, Forward and Up** (Alt+arrows, Backspace, the mouse's side buttons); going up selects the
  folder you came from.
- **Address bar:** the path as a breadcrumb whose arrows list the folders beside each part;
  `Ctrl+L` to type a path with completion (`~`, `%VAR%` and `$VAR` work; a file's path opens its
  folder with the file selected).
- **Sidebar:** favourites (`Ctrl+D`), the home folders, and every drive with its free space. An
  unreachable network drive doesn't hold anything up.
- **Status bar:** item count, hidden items, the selection's size, the drive's free space.
- File icons by type, coloured from the theme.
- `fileboat <folder or file>` opens it in a new tab, in the running app if there is one.
- The Fileboat window: a custom title bar (Snap Layouts on Windows, traffic lights on macOS), a
  sidebar and a preview pane you can show, hide and resize. It opens maximized without a white flash.
- **19 colour themes**: Fileboat Light and Dark, Catppuccin Latte and Mocha, Tokyo Night, GitHub
  Light and Dark, One Light and One Dark, Rosé Pine and Rosé Pine Dawn, Gruvbox Light and Dark,
  Kanagawa Wave and Lotus, Nord, Dracula, and High Contrast Light and Dark. Pick a light and a dark
  theme; "Match Windows/macOS" switches between them. Every theme passes contrast rules checked on
  every build.
- **Command bar** (`Ctrl+Shift+P` or `F1`; `⇧⌘P` on macOS): every command with its shortcut, and `?`
  for the modes still to come (go to files, search contents, git, places, shell).
- **Settings** (`Ctrl+,`): searchable; theme cards preview on the window as you point at them; saved
  to `settings.json`, which you can also edit by hand.
- Three densities and zoom (`Ctrl+=`, `Ctrl+-`, `Ctrl+0`).
- **Updates**: Fileboat checks GitHub for a new version at start-up and once a day (you can turn this
  off), and installs it with one click after checking its signature.
- **About** box with the version, build and Fileboat's folders.

[Unreleased]: https://github.com/yash-desai-naik/fileboat/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/yash-desai-naik/fileboat/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/yash-desai-naik/fileboat/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/yash-desai-naik/fileboat/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/yash-desai-naik/fileboat/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/yash-desai-naik/fileboat/releases/tag/v0.1.0
