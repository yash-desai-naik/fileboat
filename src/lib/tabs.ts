import { invoke } from "@tauri-apps/api/core";
import { useSyncExternalStore } from "react";

/** One tab: the folder it shows and its own Back / Forward history. */
export interface Tab {
  id: string;
  path: string;
  back: string[];
  forward: string[];
  /** A name to select once the folder shows (the folder you came up from, a file you went to). */
  select: string | null;
}

interface TabsState {
  tabs: Tab[];
  activeId: string;
}

let counter = 0;
const newId = () => `t${++counter}`;

function makeTab(path: string, back: string[] = [], forward: string[] = []): Tab {
  return { id: newId(), path, back, forward, select: null };
}

const HOME = "~";
const first = makeTab(HOME);
let state: TabsState = { tabs: [first], activeId: first.id };
/** Recently closed tabs, newest last, for Ctrl+Shift+T. */
const closed: Tab[] = [];
const listeners = new Set<() => void>();

function set(next: TabsState) {
  state = next;
  for (const l of listeners) l();
  scheduleSave();
}

export function getTabs(): TabsState {
  return state;
}

export function useTabs(): TabsState {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => state,
  );
}

export function activeTab(): Tab {
  return state.tabs.find((t) => t.id === state.activeId) ?? state.tabs[0];
}

function updateTab(id: string, patch: Partial<Tab>) {
  set({ ...state, tabs: state.tabs.map((t) => (t.id === id ? { ...t, ...patch } : t)) });
}

/** Paths compare without case on Windows and macOS, and without a trailing separator. */
export function samePath(a: string, b: string): boolean {
  const norm = (p: string) => p.replace(/[\\/]+$/, "").toLowerCase();
  return norm(a) === norm(b);
}

/** The name shown on a tab: the folder's name, or the drive. */
export function tabTitle(path: string): string {
  if (path === HOME) return "Home";
  const trimmed = path.replace(/[\\/]+$/, "");
  if (/^[A-Za-z]:$/.test(trimmed)) return trimmed;
  return trimmed.slice(Math.max(trimmed.lastIndexOf("\\"), trimmed.lastIndexOf("/")) + 1) || path;
}

// ---------- navigation in the active tab ----------

/**
 * Show `path` in the active tab, remembering where it was for Back (unless `replace`: moving
 * around in the Columns view doesn't fill the history).
 */
export function navigate(path: string, select: string | null = null, options: { replace?: boolean } = {}) {
  const tab = activeTab();
  if (samePath(tab.path, path)) {
    if (select) updateTab(tab.id, { select });
    return;
  }
  if (options.replace) updateTab(tab.id, { path, select, forward: [] });
  else updateTab(tab.id, { path, select, back: [...tab.back, tab.path].slice(-100), forward: [] });
}

/** The folder resolved to its real path (e.g. "~" → C:\Users\me): not a step in the history. */
export function settlePath(tabId: string, path: string) {
  const tab = state.tabs.find((t) => t.id === tabId);
  if (tab && tab.path !== path) updateTab(tabId, { path });
}

/** Clear the name to select once it has been selected. */
export function selected(tabId: string) {
  const tab = state.tabs.find((t) => t.id === tabId);
  if (tab?.select) updateTab(tabId, { select: null });
}

export function goBack() {
  const tab = activeTab();
  const to = tab.back[tab.back.length - 1];
  if (to === undefined) return;
  updateTab(tab.id, { path: to, back: tab.back.slice(0, -1), forward: [tab.path, ...tab.forward], select: lastName(tab.path, to) });
}

export function goForward() {
  const tab = activeTab();
  const [to, ...rest] = tab.forward;
  if (to === undefined) return;
  updateTab(tab.id, { path: to, back: [...tab.back, tab.path], forward: rest, select: null });
}

/** The folder above, with the one you were in selected. */
export function goUp(options: { replace?: boolean } = {}) {
  const tab = activeTab();
  const parent = parentOf(tab.path);
  if (parent) navigate(parent, lastName(tab.path, parent), options);
}

/** The parent folder of a path, or null at a drive or the root. */
export function parentOf(path: string): string | null {
  const trimmed = path.replace(/[\\/]+$/, "");
  const i = Math.max(trimmed.lastIndexOf("\\"), trimmed.lastIndexOf("/"));
  if (i < 0) return null;
  const parent = trimmed.slice(0, i);
  // "C:" → "C:\", "" → "/" ; a UNC share root ("\\server\share") has no parent.
  if (/^[A-Za-z]:$/.test(parent)) return parent + "\\";
  if (parent === "") return trimmed.length > 1 ? "/" : null;
  if (/^\\\\[^\\]+$/.test(parent)) return null;
  return parent;
}

/** When going from `from` to an ancestor `to`, the name of the child of `to` on the way. */
function lastName(from: string, to: string): string | null {
  const base = to.replace(/[\\/]+$/, "");
  if (from.length <= base.length || !from.toLowerCase().startsWith(base.toLowerCase())) return null;
  const rest = from.slice(base.length);
  if (!/^[\\/]/.test(rest)) return null;
  return rest.replace(/^[\\/]+/, "").split(/[\\/]/)[0] || null;
}

// ---------- tabs ----------

/** A new tab after the active one (the home folder unless a path is given). */
export function newTab(path: string = HOME, activate = true, select: string | null = null) {
  const tab = { ...makeTab(path), select };
  const at = state.tabs.findIndex((t) => t.id === state.activeId) + 1;
  const tabs = [...state.tabs.slice(0, at), tab, ...state.tabs.slice(at)];
  set({ tabs, activeId: activate ? tab.id : state.activeId });
}

export function duplicateTab() {
  const tab = activeTab();
  newTab(tab.path);
  const copy = activeTab();
  updateTab(copy.id, { back: [...tab.back], forward: [...tab.forward] });
}

/** Close a tab (the active one by default). The last tab goes back home instead. */
export function closeTab(id: string = state.activeId) {
  const i = state.tabs.findIndex((t) => t.id === id);
  if (i < 0) return;
  closed.push(state.tabs[i]);
  if (closed.length > 20) closed.shift();
  if (state.tabs.length === 1) {
    const home = makeTab(HOME);
    set({ tabs: [home], activeId: home.id });
    return;
  }
  const tabs = state.tabs.filter((t) => t.id !== id);
  // Closing the active tab activates its right neighbour (or the new last one).
  const activeId = id === state.activeId ? tabs[Math.min(i, tabs.length - 1)].id : state.activeId;
  set({ tabs, activeId });
}

export function closeOtherTabs(id: string = state.activeId) {
  closed.push(...state.tabs.filter((t) => t.id !== id));
  set({ tabs: state.tabs.filter((t) => t.id === id), activeId: id });
}

/** Ctrl+Shift+T: the most recently closed tab, with its history. */
export function reopenClosedTab() {
  const tab = closed.pop();
  if (!tab) return;
  const copy = makeTab(tab.path, tab.back, tab.forward);
  const at = state.tabs.findIndex((t) => t.id === state.activeId) + 1;
  set({ tabs: [...state.tabs.slice(0, at), copy, ...state.tabs.slice(at)], activeId: copy.id });
}

export function activateTab(id: string) {
  if (id !== state.activeId && state.tabs.some((t) => t.id === id)) set({ ...state, activeId: id });
}

/** Ctrl+1..8 go to that tab; Ctrl+9 to the last, as in browsers. */
export function activateTabAt(n: number) {
  const tab = n === 9 ? state.tabs[state.tabs.length - 1] : state.tabs[n - 1];
  if (tab) activateTab(tab.id);
}

export function cycleTab(step: 1 | -1) {
  const i = state.tabs.findIndex((t) => t.id === state.activeId);
  const next = state.tabs[(i + step + state.tabs.length) % state.tabs.length];
  activateTab(next.id);
}

/** Drag to reorder: put tab `id` at position `to`. */
export function moveTab(id: string, to: number) {
  const from = state.tabs.findIndex((t) => t.id === id);
  if (from < 0 || from === to) return;
  const tabs = [...state.tabs];
  const [tab] = tabs.splice(from, 1);
  tabs.splice(Math.max(0, Math.min(tabs.length, to)), 0, tab);
  set({ ...state, tabs });
}

// ---------- kept between runs (state/tabs.json) ----------

interface SavedTabs {
  version: 1;
  active: number;
  tabs: { path: string; back: string[]; forward: string[] }[];
}

let saveTimer: number | undefined;
let restoring = true;

function scheduleSave() {
  if (restoring) return;
  window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => void saveTabs(), 500);
}

export async function saveTabs() {
  window.clearTimeout(saveTimer);
  const saved: SavedTabs = {
    version: 1,
    active: Math.max(0, state.tabs.findIndex((t) => t.id === state.activeId)),
    tabs: state.tabs.map((t) => ({ path: t.path, back: t.back.slice(-30), forward: t.forward.slice(0, 30) })),
  };
  await invoke("state_write", { name: "tabs", contents: JSON.stringify(saved) }).catch((err) =>
    console.error("Could not save tabs", err),
  );
}

/** Restore the tabs of the last run (before the first render). Missing folders are dealt with by
 *  their tab when it opens; a broken file just means one home tab. */
export async function restoreTabs() {
  try {
    const text = await invoke<string | null>("state_read", { name: "tabs" });
    const saved = text ? (JSON.parse(text) as SavedTabs) : null;
    if (saved?.version === 1 && Array.isArray(saved.tabs) && saved.tabs.length) {
      const tabs = saved.tabs
        .filter((t) => typeof t.path === "string" && t.path)
        .map((t) => makeTab(t.path, Array.isArray(t.back) ? t.back : [], Array.isArray(t.forward) ? t.forward : []));
      if (tabs.length) state = { tabs, activeId: (tabs[saved.active] ?? tabs[0]).id };
    }
  } catch (err) {
    console.error("Could not restore tabs", err);
  } finally {
    restoring = false;
  }
}

/**
 * A folder or file given from outside (the command line, a second launch): open it in a new tab;
 * a file opens its folder with the file selected.
 */
export async function openFromOutside(path: string) {
  const r = await invoke<{ path: string; kind: string; parent: string | null; name: string | null }>("path_resolve", {
    input: path,
  });
  if (r.kind === "folder") newTab(r.path);
  else if (r.kind === "file" && r.parent) newTab(r.parent, true, r.name);
}
