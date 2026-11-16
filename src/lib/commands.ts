import { getSettings, updateSettings, type Density, type GroupBy, type Settings, type SortBy, type Theme, type ViewMode } from "./settings";
import { pickTheme, THEMES, themesFor, type ThemeData } from "../themes";
import { OS_NAME } from "./platform";
import { shownMode } from "./themeMode";
import { uiActions } from "./ui";
import { checkForUpdates } from "./update";
import { addFavourite, removeFavourite } from "./favourites";
import { hasOwnView, makeDefaultView, resetFolderView, setFolderView, viewFor } from "./folderViews";
import { paneActions, undoLast } from "./pane";
import {
  activateTabAt,
  activeTab,
  closeOtherTabs,
  closeTab,
  cycleTab,
  duplicateTab,
  goBack,
  goForward,
  goUp,
  newTab,
  reopenClosedTab,
} from "./tabs";

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
  const here = activeTab().path;
  const v = viewFor(here, s);
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

  const viewMode = (value: ViewMode, title: string, key: string): Command => ({
    id: `view.mode.${value}`,
    category: "View",
    title,
    keys: [key],
    checked: v.viewMode === value,
    run: () => setFolderView(activeTab().path, { viewMode: value }),
  });

  const sortBy = (value: SortBy, title: string): Command => ({
    id: `sort.${value}`,
    category: "Sort",
    title: `By ${title}`,
    checked: v.sortBy === value,
    run: () => setFolderView(activeTab().path, { sortBy: value }),
  });

  const groupBy = (value: GroupBy, title: string): Command => ({
    id: `group.${value}`,
    category: "Group",
    title,
    checked: v.groupBy === value,
    run: () => setFolderView(activeTab().path, { groupBy: value }),
  });

  return [
    groupBy("none", "None"),
    groupBy("type", "By Type"),
    groupBy("modified", "By Date Modified"),
    groupBy("size", "By Size"),
    groupBy("letter", "By First Letter"),
    {
      id: "commandBar.commands",
      category: "Help",
      title: "Show All Commands",
      keys: ["Ctrl+Shift+P", "F1"],
      run: () => uiActions.openCommandBar(">"),
    },
    { id: "commandBar.help", category: "Help", title: "Command Bar Modes", run: () => uiActions.openCommandBar("?") },
    { id: "settings.open", category: "Preferences", title: "Open Settings", keys: ["Ctrl+,"], run: () => uiActions.openSettings() },
    { id: "help.about", category: "Help", title: "About Fileboat (version)", run: () => uiActions.openAbout() },
    { id: "help.updates", category: "Help", title: "Check for Updates…", run: () => void checkForUpdates() },
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
    { id: "go.back", category: "Go", title: "Back", keys: ["Alt+Left", "Backspace", "Ctrl+["], run: goBack },
    { id: "go.forward", category: "Go", title: "Forward", keys: ["Alt+Right", "Ctrl+]"], run: goForward },
    { id: "go.up", category: "Go", title: "Up to the Parent Folder", keys: ["Alt+Up", "Ctrl+Up"], run: () => goUp() },
    { id: "go.address", category: "Go", title: "Type a Path…", keys: ["Ctrl+L", "Alt+D"], run: () => uiActions.focusAddress() },
    { id: "edit.undo", category: "Edit", title: "Undo", keys: ["Ctrl+Z"], run: () => void undoLast() },
    { id: "file.newFolder", category: "File", title: "New Folder", keys: ["Ctrl+Shift+N"], run: () => paneActions.newFolder() },
    { id: "file.newFile", category: "File", title: "New File", keys: ["Ctrl+Alt+N"], run: () => paneActions.newFile() },
    {
      id: "file.delete",
      category: "File",
      title: "Delete (to the Recycle Bin / Trash)",
      keys: ["Delete", "Ctrl+Backspace"],
      run: () => paneActions.delete(false),
    },
    {
      id: "file.deletePermanently",
      category: "File",
      title: "Delete Permanently…",
      keys: ["Shift+Delete", "Ctrl+Alt+Backspace"],
      run: () => paneActions.delete(true),
    },
    { id: "edit.copy", category: "Edit", title: "Copy", keys: ["Ctrl+C"], run: () => paneActions.copy() },
    { id: "edit.cut", category: "Edit", title: "Cut", keys: ["Ctrl+X"], run: () => paneActions.cut() },
    { id: "edit.paste", category: "Edit", title: "Paste", keys: ["Ctrl+V"], run: () => paneActions.paste() },
    { id: "edit.copyPath", category: "Edit", title: "Copy Path", keys: ["Ctrl+Shift+C"], run: () => paneActions.copyPath() },
    { id: "open.editor", category: "Open", title: "In Editor", keys: ["Ctrl+Shift+E"], run: () => paneActions.openInEditor() },
    { id: "open.terminal", category: "Open", title: "Terminal Here", keys: ["Ctrl+`"], run: () => paneActions.terminalHere() },
    { id: "file.contextMenu", category: "File", title: "Show Context Menu", keys: ["Shift+F10", "ContextMenu"], run: () => paneActions.contextMenu() },
    { id: "file.properties", category: "File", title: "Properties", keys: ["Alt+Enter"], run: () => paneActions.properties() },
    { id: "file.duplicate", category: "File", title: "Duplicate", run: () => paneActions.duplicate() },
    { id: "file.rename", category: "File", title: "Rename", keys: ["F2"], run: () => paneActions.rename() },
    { id: "view.refresh", category: "View", title: "Refresh", keys: ["F5", "Ctrl+R"], run: () => paneActions.refresh() },
    { id: "favourites.add", category: "Favourites", title: "Add This Folder", keys: ["Ctrl+D"], run: () => addFavourite() },
    { id: "favourites.remove", category: "Favourites", title: "Remove This Folder", run: () => removeFavourite(activeTab().path) },
    { id: "tab.new", category: "Tabs", title: "New Tab", keys: ["Ctrl+T"], run: () => newTab() },
    { id: "tab.close", category: "Tabs", title: "Close Tab", keys: ["Ctrl+W", "Ctrl+F4"], run: () => closeTab() },
    { id: "tab.closeOthers", category: "Tabs", title: "Close Other Tabs", run: () => closeOtherTabs() },
    { id: "tab.reopen", category: "Tabs", title: "Reopen Closed Tab", keys: ["Ctrl+Shift+T"], run: reopenClosedTab },
    { id: "tab.duplicate", category: "Tabs", title: "Duplicate Tab", run: duplicateTab },
    { id: "tab.next", category: "Tabs", title: "Next Tab", keys: ["Ctrl+Tab", "Ctrl+PageDown"], run: () => cycleTab(1) },
    { id: "tab.previous", category: "Tabs", title: "Previous Tab", keys: ["Ctrl+Shift+Tab", "Ctrl+PageUp"], run: () => cycleTab(-1) },
    ...[1, 2, 3, 4, 5, 6, 7, 8, 9].map(
      (n): Command => ({
        id: `tab.goto${n}`,
        category: "Tabs",
        title: n === 9 ? "Go to Last Tab" : `Go to Tab ${n}`,
        keys: [`Ctrl+${n}`],
        hidden: true,
        run: () => activateTabAt(n),
      }),
    ),
    viewMode("columns", "Columns", "Ctrl+Shift+1"),
    viewMode("list", "List", "Ctrl+Shift+2"),
    viewMode("details", "Details", "Ctrl+Shift+3"),
    {
      id: "view.hidden",
      category: "View",
      title: "Show Hidden Files",
      keys: ["Ctrl+Shift+."],
      checked: s.showHidden,
      run: () => updateSettings({ showHidden: !getSettings().showHidden }),
    },
    sortBy("name", "Name"),
    sortBy("modified", "Date Modified"),
    sortBy("created", "Date Created"),
    sortBy("type", "Type"),
    sortBy("size", "Size"),
    {
      id: "sort.descending",
      category: "Sort",
      title: "Descending",
      checked: v.sortDescending,
      run: () => {
        const path = activeTab().path;
        setFolderView(path, { sortDescending: !viewFor(path).sortDescending });
      },
    },
    {
      id: "view.reset",
      category: "View",
      title: "Reset This Folder to the Default View",
      hidden: !hasOwnView(here),
      run: () => resetFolderView(activeTab().path),
    },
    {
      id: "view.makeDefault",
      category: "View",
      title: "Use This Folder's View as the Default",
      run: () => makeDefaultView(activeTab().path),
    },
    {
      id: "sort.foldersFirst",
      category: "Sort",
      title: "Folders First",
      checked: s.foldersFirst,
      run: () => updateSettings({ foldersFirst: !getSettings().foldersFirst }),
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
