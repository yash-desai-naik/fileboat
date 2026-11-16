import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useSyncExternalStore } from "react";

/** Mirrors `Hit` in src-tauri/src/index/query.rs. */
export interface Hit {
  path: string;
  name: string;
  folder: boolean;
  size: number;
  modified: number;
  /** Characters of the name that matched. */
  indices: number[];
}

export interface SearchResult {
  hits: Hit[];
  total: number;
  unsupported: string[];
}

/** Mirrors `Status` in src-tauri/src/indexer.rs. */
export interface IndexStatus {
  phase: "off" | "loading" | "building" | "reconciling" | "ready" | "paused";
  entries: number;
  progress: number;
  bytes: number;
  roots: string[];
  error: string | null;
}

export type Scope = "here" | "everywhere";

/** A search in the active tab: what's typed, where, and what came back. */
export interface SearchState {
  open: boolean;
  query: string;
  scope: Scope;
  /** The folder "here" means (the tab's, when the search started). */
  folder: string;
  result: SearchResult | null;
  error: string | null;
  busy: boolean;
  /** Bumped to put the keyboard in the box. */
  focus: number;
}

const CLOSED: SearchState = { open: false, query: "", scope: "here", folder: "", result: null, error: null, busy: false, focus: 0 };

/** Searches by tab id. */
const states = new Map<string, SearchState>();
const listeners = new Set<() => void>();
let version = 0;

function emit() {
  version++;
  for (const l of listeners) l();
}

function subscribe(l: () => void) {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
}

export function getSearch(tabId: string): SearchState {
  return states.get(tabId) ?? CLOSED;
}

export function useSearch(tabId: string): SearchState {
  useSyncExternalStore(subscribe, () => version);
  return getSearch(tabId);
}

function set(tabId: string, patch: Partial<SearchState>) {
  states.set(tabId, { ...getSearch(tabId), ...patch });
  emit();
}

let timer: number | undefined;
let ticket = 0;

function run(tabId: string) {
  window.clearTimeout(timer);
  const s = getSearch(tabId);
  if (!s.query.trim()) return set(tabId, { result: null, error: null, busy: false });
  set(tabId, { busy: true });
  timer = window.setTimeout(() => {
    const mine = ++ticket;
    const scope = s.scope === "here" ? s.folder : null;
    invoke<SearchResult>("index_search", { query: s.query, scope, limit: 500 }).then(
      (result) => mine === ticket && set(tabId, { result, error: null, busy: false }),
      (err) => mine === ticket && set(tabId, { error: String(err), busy: false }),
    );
  }, 90);
}

/** Open the search box in a tab (Ctrl+F here, Ctrl+Shift+F everywhere). */
export function openSearch(tabId: string, folder: string, scope: Scope) {
  const s = getSearch(tabId);
  set(tabId, { open: true, scope, folder: s.open ? s.folder : folder, focus: s.focus + 1 });
  if (s.query) run(tabId);
}

export function setQuery(tabId: string, query: string) {
  set(tabId, { query });
  run(tabId);
}

export function setScope(tabId: string, scope: Scope) {
  set(tabId, { scope });
  run(tabId);
}

export function closeSearch(tabId: string) {
  states.delete(tabId);
  emit();
}

// ---------- the index's state, for the status bar and settings ----------

let indexStatus: IndexStatus | null = null;
const statusListeners = new Set<() => void>();
let watching = false;

function watchStatus() {
  if (watching) return;
  watching = true;
  const take = (s: IndexStatus) => {
    indexStatus = s;
    for (const l of statusListeners) l();
  };
  void invoke<IndexStatus>("index_status").then(take);
  void listen<IndexStatus>("index-status", (e) => take(e.payload));
}

export function useIndexStatus(): IndexStatus | null {
  watchStatus();
  return useSyncExternalStore(
    (l) => {
      statusListeners.add(l);
      return () => {
        statusListeners.delete(l);
      };
    },
    () => indexStatus,
  );
}
