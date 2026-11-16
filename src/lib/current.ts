import { useSyncExternalStore } from "react";
import type { Entry } from "./folder";

/**
 * What the active tab is on, for the preview pane: one selected item, several, or none (then
 * the folder itself). The active folder pane keeps it current, including when the selected file
 * changes on disk (a new `entry` with another size or date), so the preview refreshes.
 */
export interface Current {
  folder: string;
  /** The single selected item, or null. */
  entry: Entry | null;
  /** How many are selected. */
  count: number;
}

let current: Current | null = null;
const listeners = new Set<() => void>();

function same(a: Current | null, b: Current | null): boolean {
  if (!a || !b) return a === b;
  const ea = a.entry;
  const eb = b.entry;
  return (
    a.folder === b.folder &&
    a.count === b.count &&
    (ea === eb || (!!ea && !!eb && ea.name === eb.name && ea.size === eb.size && ea.modified === eb.modified && ea.kind === eb.kind))
  );
}

export function setCurrent(next: Current | null) {
  if (same(current, next)) return;
  current = next;
  for (const l of listeners) l();
}

export function getCurrent(): Current | null {
  return current;
}

export function useCurrent(): Current | null {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => {
        listeners.delete(l);
      };
    },
    () => current,
  );
}

// "Show Diff" from a menu: the preview opens that file on its Diff tab.
let diffFor: string | null = null;
const diffListeners = new Set<() => void>();

export function requestDiff(path: string) {
  diffFor = path;
  for (const l of diffListeners) l();
}

/** Whether a diff was asked for `path` (asking clears it). */
export function takeDiffRequest(path: string): boolean {
  if (!diffFor || diffFor.toLowerCase() !== path.toLowerCase()) return false;
  diffFor = null;
  return true;
}

export function onDiffRequest(l: () => void): () => void {
  diffListeners.add(l);
  return () => {
    diffListeners.delete(l);
  };
}
