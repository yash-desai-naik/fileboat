# Fileboat

A small, fast file explorer for developers, on **Windows and macOS**: live git status, real search
with an index that knows what not to index, previews of code and everything else, a command bar for
files and folders, and **Claude Code** built in.

Built with [Tauri 2](https://tauri.app) (Rust backend, and the WebView your system already has:
WebView2 on Windows, WebKit on macOS), on the same foundations as its sibling,
[Tableboat](https://github.com/yash-desai-naik/tableboat). No Electron and no Node.js at runtime.

> Status: **0.1.0**, the first release (made and tested on Windows; see the
> [changelog](CHANGELOG.md) for what's not on macOS yet). Browse folders in tabs (Columns, List and Details views,
> grouping, folder sizes, a view remembered per folder), preview any file (code, Markdown, data,
> images, media, PDFs, fonts, archives; Quick Look with Space), use git (live status on every
> file, the branch, a git panel to stage, commit, pull and push, diffs) and work with files: copy,
> move, rename, delete, undo, the clipboard and drag and drop (shared with Explorer), and context
> menus. Search as you type here or everywhere (an index that knows what developers never search),
> search inside files, jump anywhere from the command bar, and ask your own Claude Code about the
> project you're browsing in a side pane. Icons are Fileboat's own, your system's or a VS Code icon
> theme, and any file or folder can have its own. What v1 covers is in [docs/SCOPE.md](docs/SCOPE.md);
> how it's built is in [docs/PLAN.md](docs/PLAN.md).

## What it does

- **Views:** Columns (Finder style), List and Details, with sorting and grouping (type, extension,
  date, size, git status), remembered per folder.
- **Git, live:** status badges on every file and folder, branch and ahead/behind, a git panel to
  stage, commit, pull and push, and diffs in the preview pane.
- **Search:** an in-app index of names, kept current by file system events, that skips what
  developers never want (`node_modules`, `target`, `.venv`, caches…) with rules you can change;
  contents search on demand.
- **Preview pane:** code with highlighting, Markdown, JSON/YAML trees, images, media, PDFs,
  archives, and folder summaries with their README, refreshed as files change.
- **Claude Code pane:** Claude works in the repository you're browsing, sees your selection, and asks
  before it changes anything.
- **Command bar:** go to any file or folder, run commands, search contents, git, places and shell
  commands, all from one box.
- **Context menus** made for developers (copy path in every form, open in editor or terminal, git
  actions, the native Explorer menu one click away), **19 themes**, and **customisable icons**
  (VS Code icon themes work).

## Build from source

### Requirements

- Windows 10 or 11 (WebView2, which Windows 11 already has) with the Microsoft C++ build tools, or
  macOS 11 or later on Apple Silicon with the Xcode Command Line Tools (`xcode-select --install`)
- [Rust](https://rustup.rs) (stable)
- [Node.js](https://nodejs.org) 20+ (only to build the UI)

### Steps

```sh
git clone https://github.com/yash-desai-naik/fileboat.git
cd fileboat
npm install
npx tauri dev      # run while developing
npx tauri build    # the installer (Windows) or DMG (macOS)
```

More in [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md).

## Documentation

- [docs/SCOPE.md](docs/SCOPE.md): what's in v1
- [docs/PLAN.md](docs/PLAN.md): how it's built, milestones
- [docs/THEMES.md](docs/THEMES.md): themes and their contrast rules
- [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md): building, code layout, checks, releases
- [CHANGELOG.md](CHANGELOG.md): what changed in each version

## Credits

Theme palettes are based on Catppuccin, Tokyo Night, GitHub Primer, One Dark/One Light, Rosé Pine,
Gruvbox, Kanagawa, Nord and Dracula, all MIT-licensed. Only their colour values are used, some nudged
for contrast (see [docs/THEMES.md](docs/THEMES.md)).

## Contributing

Issues and pull requests are welcome. Please keep the project's budget in mind: low memory, no idle
CPU, and no Node.js at runtime.

## License

[MIT](LICENSE) © 2026 Yash Desai

Fileboat is an independent project. It is not affiliated with or endorsed by Microsoft, Apple,
Anthropic or any other company whose products it works with or mentions. Product names are
trademarks of their owners.
