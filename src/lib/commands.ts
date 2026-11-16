import { getSettings, updateSettings, type Density, type Settings, type Theme } from "./settings";
import { pickTheme, THEMES, themesFor, type ThemeData } from "../themes";
import { OS_NAME } from "./platform";
import { shownMode } from "./themeMode";
import { uiActions } from "./ui";

/**
 * Every action in the app, with its shortcuts. The command bar (`>`) lists them, the shortcut
 * handler runs them, and later Claude sees the same list.
 */
export interface Command {
  id: string;
  category: string;
  title: string;
  /** Shortcuts; the first one is shown in the command bar. */
  keys?: string[];
  /** For toggles and choices: shows a check mark in the command bar. */
  checked?: boolean;
  /** Shortcut-only commands that would clutter the command bar (e.g. Ctrl+1..9). */
  hidden?: boolean;
  run: () => void;
}

const ZOOM_STEPS = [0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2];

function stepZoom(direction: 1 | -1) {
  const zoom = getSettings().uiZoom;
  const next =
    direction > 0
      ? ZOOM_STEPS.find((z) => z > zoom + 0.001)
      : [...ZOOM_STEPS].reverse().find((z) => z < zoom - 0.001);
  if (next !== undefined) updateSettings({ uiZoom: next });
}

/** The colour theme the window shows now for these settings. */
function shownTheme(s: Settings): ThemeData {
  const mode = shownMode(s.theme);
  return pickTheme(mode, mode === "light" ? s.lightTheme : s.darkTheme);
}

/** Choose a theme for its mode, and switch to that mode if the window shows the other one. */
function chooseColorTheme(t: ThemeData) {
  const patch: Partial<Settings> = t.mode === "light" ? { lightTheme: t.id } : { darkTheme: t.id };
  if (shownMode(getSettings().theme) !== t.mode) patch.theme = t.mode;
  updateSettings(patch);
}

/** The next theme (in themes.json order) of the mode showing now. */
function nextColorTheme() {
  const current = shownTheme(getSettings());
  const list = themesFor(current.mode);
  chooseColorTheme(list[(list.findIndex((t) => t.id === current.id) + 1) % list.length]);
}

const toggle = (key: "showSidebar" | "showPreview") => () => updateSettings({ [key]: !getSettings()[key] });

/** Every command in the app. Rebuilt when settings change so check marks stay current. */
export function buildCommands(): Command[] {
  const s = getSettings();
  const showing = shownTheme(s).id;

  const theme = (value: Theme, title: string): Command => ({
    id: `theme.${value}`,
    category: "Theme",
    title,
    checked: s.theme === value,
    run: () => updateSettings({ theme: value }),
  });

  const density = (value: Density, title: string): Command => ({
    id: `density.${value}`,
    category: "Density",
    title,
    checked: s.density === value,
    run: () => updateSettings({ density: value }),
  });

  return [
    {
      id: "commandBar.commands",
      category: "Help",
      title: "Show All Commands",
      keys: ["Ctrl+Shift+P", "F1"],
      run: () => uiActions.openCommandBar(">"),
    },
    { id: "commandBar.help", category: "Help", title: "Command Bar Modes", run: () => uiActions.openCommandBar("?") },
    {
      id: "view.toggleSidebar",
      category: "View",
      title: "Toggle Sidebar",
      keys: ["Ctrl+B"],
      checked: s.showSidebar,
      run: toggle("showSidebar"),
    },
    {
      id: "view.togglePreview",
      category: "View",
      title: "Toggle Preview Pane",
      keys: ["Alt+P"],
      checked: s.showPreview,
      run: toggle("showPreview"),
    },
    { id: "view.zoomIn", category: "View", title: "Zoom In", keys: ["Ctrl+=", "Ctrl+Shift+="], run: () => stepZoom(1) },
    { id: "view.zoomOut", category: "View", title: "Zoom Out", keys: ["Ctrl+-"], run: () => stepZoom(-1) },
    { id: "view.zoomReset", category: "View", title: "Reset Zoom", keys: ["Ctrl+0"], run: () => updateSettings({ uiZoom: 1 }) },
    theme("system", `Match ${OS_NAME}`),
    theme("light", "Light"),
    theme("dark", "Dark"),
    {
      id: "theme.toggle",
      category: "Theme",
      title: "Toggle Light / Dark",
      run: () => {
        const dark = document.documentElement.dataset.theme === "dark";
        updateSettings({ theme: dark ? "light" : "dark" });
      },
    },
    // The command bar shows "Theme: <title>", so these read "Theme: Catppuccin Mocha" and "Theme: Next Theme".
    { id: "theme.next", category: "Theme", title: "Next Theme", run: nextColorTheme },
    ...THEMES.map(
      (t): Command => ({
        id: `theme.set.${t.id}`,
        category: "Theme",
        title: t.name,
        checked: t.id === showing,
        run: () => chooseColorTheme(t),
      }),
    ),
    density("compact", "Compact"),
    density("default", "Default"),
    density("comfortable", "Comfortable"),
    { id: "developer.reload", category: "Developer", title: "Reload Window", run: () => window.location.reload() },
  ];
}
