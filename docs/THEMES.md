# Fileboat – themes

Fileboat uses Tableboat's theme system: the same 19 themes, the same tokens and the same contrast rules,
checked by a test on every build. Only the two Fileboat themes differ from Tableboat's: they keep
Tableboat's warm neutrals but use the blue of the Fileboat icon as their accent.

## 1. Rules every theme follows

1. **Every pixel follows the theme.** Sidebar, tabs, views, git colours, preview, syntax, diff, dialogs, menus,
   the Claude pane, scrollbars. No colour is written directly in a component; components use the CSS
   variables below.
2. **Readable by rule, not by eye.** Every theme passes the contrast rules in section 4, checked in
   `cargo test`. When an original palette colour is too faint, it is nudged lighter or darker just enough; its
   hue and saturation stay the same, and the change is listed in the theme's `adjusted` entries.
3. **Meaning never changes with the theme.** Red is always danger/deleted, amber changed/warning, green
   added/success.
4. **Cheap.** Switching themes changes CSS variables only: no reload, no extra memory.

## 2. How it works

```
src/themes/themes.json      every theme's seed colours
src/themes/index.ts         loads the JSON, derives the other tokens, writes CSS variables on <html>
src/styles.css              layout tokens (sizes, radii, fonts); the Fileboat themes as a first-paint fallback
src-tauri/src/themes.rs     the theme ids settings accept, and the contrast tests (section 4)
```

- Settings: `theme` (`system` · `light` · `dark`), `lightTheme` and `darkTheme` (theme ids, defaults
  `fileboat-light` and `fileboat-dark`). "Match Windows/macOS" switches between your light and dark theme.
- `<html data-theme="dark" data-color-theme="catppuccin-mocha">`; the variables are set in one `<style>`
  element, so a switch is one style recalculation. The native window background follows the theme.

## 3. Tokens

**Seeds** (in `themes.json`): surfaces `bg` (where files are listed), `chrome` (title bar, toolbars, status
bar), `sidebar`, `elevated` (dialogs, menus, command bar); text `primary`, `secondary`, `hint`; `accent`
(`text`, `fill`, `on`); optional `selection`; `status` (`danger`, `warning`, `success`, `info`); `dangerFill`
and `onDanger`; `syntax` (keyword, type, function, string, number, literal, property, operator, comment).

**Derived** (in `index.ts`, the same formulas for every theme, a theme may override any): hover and active
backgrounds, control fills, raised surfaces, borders, zebra rows, focus ring, accent tint and hover, status
backgrounds, search highlights, editor gutter and brackets, scrollbars, scrim and shadows. Mixes are done in
OKLab, and surface steps keep a minimum lightness distance from their base so near-black themes still show
hover and selection.

Git status colours come from the status tokens: modified = warning, added and untracked = success,
deleted and conflicted = danger, renamed = info, ignored = hint.

## 4. Contrast rules (WCAG 2.2)

| Pair | Minimum | On |
|---|---|---|
| primary text | **7:1** | bg, chrome, sidebar, elevated |
| secondary text, hints | **4.5:1** | bg, chrome, sidebar, elevated |
| accent used as text | 4.5:1 | bg, elevated |
| text on the accent fill, white on the danger fill | 4.5:1 | |
| primary text on selection | 4.5:1 | |
| status colours as text | 4.5:1 | bg, elevated |
| syntax colours | 4.5:1; comments **3.5:1** | bg |
| High Contrast themes | text 15:1; secondary, hints, accent, status and syntax 7:1 | |

## 5. The themes

| Theme | Mode | Pairs with |
|---|---|---|
| **Fileboat Dark** (default) | dark | Fileboat Light |
| **Fileboat Light** (default) | light | Fileboat Dark |
| Catppuccin Mocha / Latte | dark / light | each other |
| Tokyo Night | dark | GitHub Light |
| GitHub Dark / Light | dark / light | each other |
| One Dark / One Light | dark / light | each other |
| Rosé Pine / Rosé Pine Dawn | dark / light | each other |
| Gruvbox Dark / Light | dark / light | each other |
| Kanagawa Wave / Lotus | dark / light | each other |
| Nord | dark | GitHub Light |
| Dracula | dark | GitHub Light |
| High Contrast Dark / Light | dark / light | each other |

The colours each theme needed nudged for contrast are listed in its `adjusted` entries in `themes.json`.

## 6. Credits

Palettes: Catppuccin, Tokyo Night, GitHub Primer, One Dark/One Light (Atom), Rosé Pine, Gruvbox, Kanagawa,
Nord, Dracula, all under the MIT licence. Only colour values are used (no code), adjusted for contrast as
listed in `themes.json`, and each is credited by name in the README and About box.
