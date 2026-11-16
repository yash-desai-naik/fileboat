import { invoke } from "@tauri-apps/api/core";
import { useSyncExternalStore } from "react";
import { getSettings, updateSettings, useSettings, type GroupBy, type Settings, type SortBy, type ViewMode } from "./settings";

/** How one folder shows. Settings hold the defaults; a folder changed by hand keeps its own. */
export interface FolderView {
  viewMode: ViewMode;
  sortBy: SortBy;
  sortDescending: boolean;
  groupBy: GroupBy;
}

/** How many folders are remembered (the most recently changed). */
const LIMIT = 1000;
const STATE = "views";

/** Folder key → its own view, oldest change first. */
let views = new Map<string, FolderView>();
/**
 * Folders reached by moving around in the Columns view (this run only): they stay in Columns
 * whatever they remember, so the columns don't vanish under the pointer.
 */
const columnsWalk = new Set<string>();
let version = 0;
const listeners = new Set<() => void>();

function emit() {
  version++;
  for (const listener of listeners) listener();
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Matches `samePath`: case and trailing separators don't make a different folder. */
const keyOf = (path: string) => path.replace(/[\\/]+$/, "").toLowerCase();

function defaults(s: Settings): FolderView {
  return { viewMode: s.viewMode, sortBy: s.sortBy, sortDescending: s.sortDescending, groupBy: s.groupBy };
}

const sameView = (a: FolderView, b: FolderView) =>
  a.viewMode === b.viewMode && a.sortBy === b.sortBy && a.sortDescending === b.sortDescending && a.groupBy === b.groupBy;

/** Read the remembered views (at start-up, and when the window comes back: another window may have changed them). */
export async function loadFolderViews() {
  if (saveTimer !== undefined) return; // ours are newer
  try {
    const text = await invoke<string | null>("state_read", { name: STATE });
    const parsed = text ? (JSON.parse(text) as { folders?: [string, FolderView][] }) : {};
    const next = new Map<string, FolderView>();
    for (const [path, view] of parsed.folders ?? []) {
      if (typeof path === "string" && view && typeof view === "object") next.set(keyOf(path), view);
    }
    views = next;
    emit();
  } catch {
    // A damaged file: start afresh (it's rewritten on the next change).
  }
}

let saveTimer: number | undefined;
function save() {
  window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => {
    const contents = JSON.stringify({ folders: [...views] });
    void invoke("state_write", { name: STATE, contents })
      .catch(() => {})
      .finally(() => (saveTimer = undefined));
  }, 300);
}

/** How `path` shows: its own view when it has one (and remembering is on), else the defaults. */
export function viewFor(path: string, s: Settings = getSettings()): FolderView {
  const base = defaults(s);
  if (!s.rememberViews) return base;
  const key = keyOf(path);
  const own = views.get(key);
  const view = own ? { ...base, ...own } : base;
  return columnsWalk.has(key) ? { ...view, viewMode: "columns" } : view;
}

/** `viewFor` that re-renders when the view or the settings change. */
export function useFolderView(path: string): FolderView {
  const s = useSettings();
  useSyncExternalStore(subscribe, () => version);
  return viewFor(path, s);
}

/** Change how `path` shows: for that folder alone, or for every folder when remembering is off. */
export function setFolderView(path: string, patch: Partial<FolderView>) {
  const s = getSettings();
  if (!s.rememberViews) return updateSettings(patch);
  const key = keyOf(path);
  const next = { ...viewFor(path, s), ...patch };
  // Choosing another view ends a walk through Columns.
  if (patch.viewMode && patch.viewMode !== "columns") columnsWalk.clear();
  else columnsWalk.delete(key);
  views.delete(key);
  // Back to the defaults: nothing to remember.
  if (!sameView(next, defaults(s))) views.set(key, next);
  while (views.size > LIMIT) views.delete(views.keys().next().value as string);
  emit();
  save();
}

/** Forget `path`'s own view: it shows as the defaults say. */
export function resetFolderView(path: string) {
  const key = keyOf(path);
  const hadOwn = views.delete(key);
  const walked = columnsWalk.delete(key);
  if (!hadOwn && !walked) return;
  emit();
  if (hadOwn) save();
}

/** Make `path`'s view the default for every folder without one of its own. */
export function makeDefaultView(path: string) {
  const view = viewFor(path);
  views.delete(keyOf(path));
  updateSettings(view);
  emit();
  save();
}

/** Whether `path` has a view of its own. */
export function hasOwnView(path: string): boolean {
  return getSettings().rememberViews && views.has(keyOf(path));
}

/** `path` was reached by moving around in Columns: keep showing it in Columns. */
export function keepColumns(path: string) {
  if (!getSettings().rememberViews || viewFor(path).viewMode === "columns") return;
  columnsWalk.add(keyOf(path));
  emit();
}
