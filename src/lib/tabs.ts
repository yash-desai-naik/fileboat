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

function makeTab(path: string): Tab {
  return { id: newId(), path, back: [], forward: [], select: null };
}

const first = makeTab("~");
let state: TabsState = { tabs: [first], activeId: first.id };
const listeners = new Set<() => void>();

function set(next: TabsState) {
  state = next;
  for (const l of listeners) l();
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

/** Show `path` in the active tab, remembering where it was for Back. */
export function navigate(path: string, select: string | null = null) {
  const tab = activeTab();
  if (samePath(tab.path, path)) {
    if (select) updateTab(tab.id, { select });
    return;
  }
  updateTab(tab.id, { path, select, back: [...tab.back, tab.path].slice(-100), forward: [] });
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
export function goUp() {
  const tab = activeTab();
  const parent = parentOf(tab.path);
  if (parent) navigate(parent, lastName(tab.path, parent));
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
