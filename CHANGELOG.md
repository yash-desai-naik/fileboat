# Changelog

All notable changes to Fileboat are listed here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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
