import { invoke } from "@tauri-apps/api/core";
import { pasteInto } from "../lib/clipboard";
import { addFavourite, placeFavourites, removeFavourite } from "../lib/favourites";
import { childPath, type Entry } from "../lib/folder";
import { paneActions } from "../lib/pane";
import { FILE_MANAGER, IS_MAC, fmtShortcut } from "../lib/platform";
import { getSettings, updateSettings, type GroupBy, type SortBy } from "../lib/settings";
import { hasOwnView, makeDefaultView, resetFolderView, setFolderView, viewFor } from "../lib/folderViews";
import { newTab } from "../lib/tabs";
import { openInEditor, openTerminal, type Tools } from "../lib/tools";
import type { MenuItem } from "../shell/Menu";
import { showToast } from "../shell/Toast";
import { queueMentions } from "../lib/claude";
import { uiActions } from "../lib/ui";
import { askConfirm } from "../shell/Confirm";
import { requestDiff } from "../lib/current";
import type { RepoInfo } from "../lib/git";

/** Everything a context menu needs to know about where it was opened. */
export interface MenuContext {
  /** The open folder. */
  folder: string;
  /** The items the menu acts on (the selection); empty for a click on empty space. */
  entries: Entry[];
  tools: Tools | null;
  /** Open an entry as a double-click does. */
  open: (entry: Entry) => void;
  /** Make a new file from a template (name and content), then select it. */
  newFromTemplate: (name: string, content: string) => void;
  /** The git repository the folder is in, if any. */
  repo: RepoInfo | null;
}

const run = (command: string, args: Record<string, unknown>) => () =>
  void invoke(command, args).catch((err) => showToast(String(err), "error"));

/** A folder's own name ("C:\\" for a drive). */
function folderName(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  return trimmed.slice(Math.max(trimmed.lastIndexOf("\\"), trimmed.lastIndexOf("/")) + 1) || path;
}

// ---------- the search index ----------

/** Index This Folder / Leave Out of the Index, for one folder. */
function indexItems(folder: string): MenuItem[] {
  const s = getSettings();
  const lower = (p: string) => p.replace(/[\\/]+$/, "").toLowerCase();
  const under = (p: string, base: string) => lower(p) === lower(base) || lower(p).startsWith(lower(base) + (base.includes("\\") ? "\\" : "/"));
  const excluded = s.indexExcludes.some((e) => lower(e) === lower(folder));
  return [
    excluded
      ? {
          label: "Put Back in the Index",
          onSelect: () => updateSettings({ indexExcludes: s.indexExcludes.filter((e) => lower(e) !== lower(folder)) }),
        }
      : {
          label: "Leave Out of the Index",
          onSelect: () => {
            updateSettings({ indexExcludes: [...s.indexExcludes, folder] });
            showToast("Left out of the index");
          },
        },
    {
      label: "Index This Folder",
      onSelect: async () => {
        // The folders indexed now (the home folder when none were chosen).
        const roots = s.indexRoots.length ? s.indexRoots : ((await invoke<{ roots: string[] }>("index_status").catch(() => null))?.roots ?? []);
        // Outside every root: a new root; inside one (left out by a rule): indexed anyway.
        if (roots.some((r) => under(folder, r))) updateSettings({ indexIncludes: [...s.indexIncludes, folder] });
        else updateSettings({ indexRoots: [...roots, folder] });
        showToast("Indexing it now");
      },
    },
  ];
}

// ---------- git ----------

const STAGEABLE = new Set(["modified", "added", "deleted", "renamed", "untracked", "conflicted", "changed"]);
const DISCARDABLE = new Set(["modified", "deleted", "renamed", "untracked", "changed"]);
const DIFFABLE = new Set(["modified", "added", "renamed", "conflicted"]);

/** Open the selection's page on GitHub / GitLab, or copy its permalink. */
function webPage(path: string, permalink: boolean) {
  void invoke<string>("git_web_url", { path, permalink }).then(
    (url) =>
      permalink
        ? void navigator.clipboard.writeText(url).then(() => showToast("Copied the permalink"))
        : void invoke("open_url", { url }).catch((err) => showToast(String(err), "error")),
    (err) => showToast(String(err), "error"),
  );
}

/** Git ▸ for the selection, inside a repository. */
function gitItems(ctx: MenuContext, paths: string[]): MenuItem[] {
  if (!ctx.repo) return [];
  const { entries } = ctx;
  const one = entries.length === 1 ? entries[0] : null;
  const fail = (err: unknown) => showToast(String(err), "error");
  const discard = async () => {
    const gone = entries.filter((e) => e.git === "untracked").map((e) => childPath(ctx.folder, e.name));
    const tracked = entries.filter((e) => e.git && DISCARDABLE.has(e.git) && e.git !== "untracked").map((e) => childPath(ctx.folder, e.name));
    const what = entries.length === 1 ? `“${entries[0].name}”` : `${entries.length} items`;
    const ok = await askConfirm({
      title: "Discard changes?",
      body: `The unstaged changes to ${what} will be lost.${gone.length ? " Untracked files go to the Recycle Bin." : ""}`,
      confirm: "Discard",
      danger: true,
    });
    if (!ok) return;
    if (tracked.length) await invoke("git_paths", { op: "discard", paths: tracked }).catch(fail);
    if (gone.length) await invoke("op_trash", { paths: gone }).catch(fail);
  };
  const items: MenuItem[] = [
    { label: "Stage", disabled: !entries.some((e) => e.git && STAGEABLE.has(e.git)), onSelect: () => void invoke("git_paths", { op: "stage", paths }).catch(fail) },
    { label: "Unstage", disabled: !entries.some((e) => e.staged), onSelect: () => void invoke("git_paths", { op: "unstage", paths }).catch(fail) },
    { label: "Discard Changes…", danger: true, disabled: !entries.some((e) => e.git && DISCARDABLE.has(e.git)), onSelect: () => void discard() },
  ];
  if (one && one.kind === "file" && one.git && DIFFABLE.has(one.git)) {
    items.push({
      label: "Show Diff",
      onSelect: () => {
        updateSettings({ showPreview: true });
        requestDiff(paths[0]);
      },
    });
  }
  items.push(
    "separator",
    {
      label: "Add to .gitignore",
      disabled: entries.every((e) => e.git === "ignored"),
      onSelect: () =>
        void invoke<number>("git_ignore", { paths }).then((n) => showToast(n ? `Added ${n} to .gitignore` : "Already in .gitignore"), fail),
    },
    "separator",
    { label: "Open on the Web", onSelect: () => webPage(paths[0], false) },
    { label: "Copy Permalink", onSelect: () => webPage(paths[0], true) },
  );
  return items;
}

// ---------- copy path, in every form a developer needs ----------

function copyText(text: string, what: string) {
  void navigator.clipboard.writeText(text).then(() => showToast(`Copied ${what}`));
}

function pathForms(paths: string[]): MenuItem[] {
  const join = (f: (p: string) => string) => paths.map(f).join("\n");
  const name = (p: string) => p.slice(Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/")) + 1);
  const stem = (p: string) => {
    const n = name(p);
    const dot = n.lastIndexOf(".");
    return dot > 0 ? n.slice(0, dot) : n;
  };
  const url = (p: string) => "file:///" + encodeURI(p.replace(/\\/g, "/").replace(/^\/+/, "")).replace(/#/g, "%23").replace(/\?/g, "%3F");
  const label = paths.length > 1 ? `${paths.length} paths` : "the path";
  return [
    { label: "Full Path", hint: fmtShortcut("Ctrl+Shift+C"), onSelect: () => copyText(join((p) => p), label) },
    { label: "With Forward Slashes", hint: "C:/a/b", onSelect: () => copyText(join((p) => p.replace(/\\/g, "/")), label) },
    ...(IS_MAC ? [] : [{ label: "With Backslashes", hint: "C:\\a\\b", onSelect: () => copyText(join((p) => p.replace(/\//g, "\\")), label) }]),
    { label: "As a File URL", hint: "file:///", onSelect: () => copyText(join(url), paths.length > 1 ? "the URLs" : "the URL") },
    "separator",
    { label: "Name", onSelect: () => copyText(join(name), paths.length > 1 ? "the names" : "the name") },
    { label: "Name Without Extension", onSelect: () => copyText(join(stem), paths.length > 1 ? "the names" : "the name") },
  ];
}

// ---------- open with editors and terminals ----------

function editorItems(ctx: MenuContext, paths: string[]): MenuItem[] {
  const editors = ctx.tools?.editors ?? [];
  if (!editors.length) return [];
  const chosen = editors.find((e) => e.id === getSettings().editor) ?? editors[0];
  const others = editors.filter((e) => e.id !== chosen.id);
  return [
    { label: `Open in ${chosen.name}`, hint: fmtShortcut("Ctrl+Shift+E"), onSelect: () => openInEditor(paths, chosen.id) },
    ...(others.length
      ? [{ label: "Open In", submenu: others.map((e): MenuItem => ({ label: e.name, onSelect: () => openInEditor(paths, e.id) })) }]
      : []),
  ];
}

function terminalItems(ctx: MenuContext, dir: string): MenuItem[] {
  const terminals = ctx.tools?.terminals ?? [];
  if (!terminals.length) return [];
  const chosen = terminals.find((t) => t.id === getSettings().terminal) ?? terminals[0];
  const others = terminals.filter((t) => t.id !== chosen.id);
  return [
    { label: `Terminal Here (${chosen.name})`, hint: fmtShortcut("Ctrl+`"), onSelect: () => openTerminal(dir, chosen.id) },
    ...(others.length
      ? [{ label: "Terminal Here In", submenu: others.map((t): MenuItem => ({ label: t.name, onSelect: () => openTerminal(dir, t.id) })) }]
      : []),
  ];
}

// ---------- templates for New ▸ ----------

const TEMPLATES: [string, (folder: string) => string][] = [
  [
    ".gitignore",
    () => "# Dependencies and build output\nnode_modules/\ndist/\nbuild/\ntarget/\n\n# Local settings and secrets\n.env\n.env.*\n!.env.example\n\n# Logs and OS files\n*.log\n.DS_Store\nThumbs.db\n",
  ],
  ["README.md", (folder) => `# ${folder.slice(Math.max(folder.lastIndexOf("\\"), folder.lastIndexOf("/")) + 1)}\n\n`],
  [
    ".editorconfig",
    () =>
      "root = true\n\n[*]\ncharset = utf-8\nend_of_line = lf\ninsert_final_newline = true\nindent_style = space\nindent_size = 2\ntrim_trailing_whitespace = true\n\n[*.md]\ntrim_trailing_whitespace = false\n",
  ],
  [".env", () => ""],
];

// ---------- the menus ----------

/** Right-click on one or more items. */
export function itemMenu(ctx: MenuContext): MenuItem[] {
  const { entries, folder } = ctx;
  const paths = entries.map((e) => childPath(folder, e.name));
  const one = entries.length === 1 ? entries[0] : null;
  const folders = entries.filter((e) => e.kind === "folder");
  const onlyFolders = folders.length === entries.length;
  const items: MenuItem[] = [
    { label: "Open", hint: "Enter", onSelect: () => entries.slice(0, 20).forEach(ctx.open) },
  ];
  if (folders.length) {
    items.push({
      label: folders.length > 1 ? "Open in New Tabs" : "Open in New Tab",
      hint: "Middle-click",
      onSelect: () => folders.forEach((f) => newTab(childPath(folder, f.name), false)),
    });
  }
  if (one && one.kind === "file" && !IS_MAC) items.push({ label: "Open With…", onSelect: run("open_with_dialog", { path: paths[0] }) });
  items.push("separator", ...editorItems(ctx, paths), ...terminalItems(ctx, one?.kind === "folder" ? paths[0] : folder));
  items.push(
    "separator",
    { label: "Cut", hint: fmtShortcut("Ctrl+X"), onSelect: () => paneActions.cut() },
    { label: "Copy", hint: fmtShortcut("Ctrl+C"), onSelect: () => paneActions.copy() },
  );
  if (one?.kind === "folder") {
    items.push({
      label: "Paste Into Folder",
      onSelect: () => void pasteInto(paths[0]),
    });
  }
  items.push({ label: "Copy Path", submenu: pathForms(paths) });
  items.push({
    label: "Ask Claude",
    hint: fmtShortcut("Ctrl+Alt+B"),
    onSelect: () => {
      queueMentions(ctx.entries.map((e) => ({ path: childPath(ctx.folder, e.name), folder: e.kind === "folder" })));
      uiActions.showClaude();
    },
  });
  const git = gitItems(ctx, paths);
  if (git.length) items.push({ label: "Git", submenu: git });
  items.push(
    "separator",
    { label: "Duplicate", onSelect: () => paneActions.duplicate() },
    { label: "Rename", hint: "F2", disabled: !one, onSelect: () => paneActions.rename() },
    { label: "Delete", hint: IS_MAC ? "⌘⌫" : "Del", onSelect: () => paneActions.delete(false) },
    { label: "Delete Permanently…", hint: IS_MAC ? "⌘⌥⌫" : "Shift+Del", danger: true, onSelect: () => paneActions.delete(true) },
    "separator",
  );
  if (onlyFolders) items.push({ label: "Add to Favourites", onSelect: () => paths.forEach((p) => addFavourite(p)) });
  if (onlyFolders && paths.length === 1) items.push({ label: "Search Index", submenu: indexItems(paths[0]) });
  if (one) items.push({ label: "Change Icon…", onSelect: () => uiActions.changeIcon({ path: paths[0], name: one.name, ext: one.ext, folder: one.kind === "folder" }) });
  items.push({ label: `Show in ${FILE_MANAGER}`, onSelect: run("reveal_in_file_manager", { path: paths[0] }) });
  items.push({ label: IS_MAC ? "Get Info" : "Properties", hint: IS_MAC ? undefined : "Alt+Enter", onSelect: run("show_properties", { path: paths[0] }) });
  // Windows: Explorer's own menu, with what shell extensions add (7-Zip, TortoiseGit, Send to…).
  if (!IS_MAC) items.push("separator", { label: "Show More Options", hint: "Shift: more", onSelect: run("native_menu", { paths, folder }) });
  return items;
}

/** Right-click on empty space: the open folder itself. */
export function folderMenu(ctx: MenuContext): MenuItem[] {
  const s = getSettings();
  const { folder } = ctx;
  const v = viewFor(folder, s);
  const group = (value: GroupBy, label: string): MenuItem => ({
    label,
    checked: v.groupBy === value,
    onSelect: () => setFolderView(folder, { groupBy: value }),
  });
  const sort = (value: SortBy, label: string): MenuItem => ({
    label,
    checked: v.sortBy === value,
    onSelect: () => setFolderView(folder, { sortBy: value }),
  });
  // Only when this folder has a view of its own.
  const ownView: MenuItem[] = hasOwnView(folder)
    ? [
        "separator",
        { label: "Reset to Default View", onSelect: () => resetFolderView(folder) },
        { label: "Use as Default View", onSelect: () => makeDefaultView(folder) },
      ]
    : [];
  return [
    {
      label: "New",
      submenu: [
        { label: "Folder", hint: fmtShortcut("Ctrl+Shift+N"), onSelect: () => paneActions.newFolder() },
        { label: "File", hint: fmtShortcut("Ctrl+Alt+N"), onSelect: () => paneActions.newFile() },
        "separator",
        ...TEMPLATES.map(([name, content]): MenuItem => ({ label: name, onSelect: () => ctx.newFromTemplate(name, content(folder)) })),
      ],
    },
    { label: "Paste", hint: fmtShortcut("Ctrl+V"), onSelect: () => paneActions.paste() },
    "separator",
    ...editorItems(ctx, [folder]),
    ...terminalItems(ctx, folder),
    { label: "Copy Path", submenu: pathForms([folder]) },
    { label: "Search Index", submenu: indexItems(folder) },
    { label: "Change Icon…", onSelect: () => uiActions.changeIcon({ path: folder, name: folderName(folder), ext: "", folder: true }) },
    ...(ctx.repo
      ? [
          {
            label: "Git",
            submenu: [
              { label: "Open Git Panel", hint: fmtShortcut("Ctrl+Shift+G"), onSelect: () => updateSettings({ showSidebar: true, sidebarView: "git" }) },
              "separator",
              { label: "Open on the Web", onSelect: () => webPage(folder, false) },
              { label: "Copy Permalink", onSelect: () => webPage(folder, true) },
            ] as MenuItem[],
          } as MenuItem,
        ]
      : []),
    "separator",
    {
      label: "View",
      submenu: [
        { label: "Columns", hint: fmtShortcut("Ctrl+Shift+1"), checked: v.viewMode === "columns", onSelect: () => setFolderView(folder, { viewMode: "columns" }) },
        { label: "List", hint: fmtShortcut("Ctrl+Shift+2"), checked: v.viewMode === "list", onSelect: () => setFolderView(folder, { viewMode: "list" }) },
        { label: "Details", hint: fmtShortcut("Ctrl+Shift+3"), checked: v.viewMode === "details", onSelect: () => setFolderView(folder, { viewMode: "details" }) },
        "separator",
        { label: "Calculate Folder Sizes", onSelect: () => paneActions.calculateSizes() },
        ...ownView,
      ],
    },
    {
      label: "Sort By",
      submenu: [
        sort("name", "Name"),
        sort("modified", "Date Modified"),
        sort("created", "Date Created"),
        sort("type", "Type"),
        sort("size", "Size"),
        "separator",
        { label: "Descending", checked: v.sortDescending, onSelect: () => setFolderView(folder, { sortDescending: !v.sortDescending }) },
        { label: "Folders First", checked: s.foldersFirst, onSelect: () => updateSettings({ foldersFirst: !s.foldersFirst }) },
      ],
    },
    {
      label: "Group By",
      submenu: [
        group("none", "None"),
        "separator",
        group("type", "Type"),
        group("modified", "Date Modified"),
        group("size", "Size"),
        group("letter", "First Letter"),
        group("git", "Git Status"),
      ],
    },
    {
      label: "Show Hidden Files",
      hint: fmtShortcut("Ctrl+Shift+."),
      checked: s.showHidden,
      onSelect: () => updateSettings({ showHidden: !s.showHidden }),
    },
    { label: "Refresh", hint: "F5", onSelect: () => paneActions.refresh() },
    "separator",
    { label: "Add to Favourites", hint: fmtShortcut("Ctrl+D"), onSelect: () => addFavourite(folder) },
    { label: `Open in ${FILE_MANAGER}`, onSelect: run("open_path", { path: folder }) },
    { label: IS_MAC ? "Get Info" : "Properties", onSelect: run("show_properties", { path: folder }) },
    ...(IS_MAC ? [] : (["separator", { label: "Show More Options", onSelect: run("native_menu", { paths: [], folder }) }] as MenuItem[])),
  ];
}

// ---------- the sidebar's places ----------

/** Right-click on a favourite or a home folder in the sidebar. */
export function placeMenu(path: string, name: string, favourite: { index: number; count: number } | null): MenuItem[] {
  const items: MenuItem[] = [
    { label: "Open in New Tab", onSelect: () => newTab(path, true) },
    { label: `Show in ${FILE_MANAGER}`, onSelect: run("reveal_in_file_manager", { path }) },
    { label: "Copy Path", submenu: pathForms([path]) },
    "separator",
    { label: "Change Icon…", onSelect: () => uiActions.changeIcon({ path, name, ext: "", folder: true }) },
  ];
  if (favourite) {
    const { index, count } = favourite;
    items.push(
      "separator",
      { label: "Move Up", disabled: index === 0, onSelect: () => placeFavourites([path], index - 1) },
      { label: "Move Down", disabled: index === count - 1, onSelect: () => placeFavourites([path], index + 2) },
      { label: "Remove from Favourites", onSelect: () => removeFavourite(path) },
    );
  } else {
    items.push("separator", { label: "Add to Favourites", onSelect: () => addFavourite(path) });
  }
  return items;
}
