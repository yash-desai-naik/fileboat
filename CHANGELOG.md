# Changelog

All notable changes to Fileboat are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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
