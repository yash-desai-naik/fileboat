import { invoke } from "@tauri-apps/api/core";
import { useSyncExternalStore } from "react";
import { showToast } from "../shell/Toast";
import { startTransfer } from "./jobs";

/**
 * Copy, cut and paste of files (src-tauri/src/clipboard.rs). Shared with Explorer on Windows.
 * The paths currently cut are kept here so the views can show them faded.
 */

interface Contents {
  paths: string[];
  cut: boolean;
}

let cutPaths = new Set<string>();
const listeners = new Set<() => void>();
const key = (p: string) => p.toLowerCase();

function setCut(paths: string[]) {
  cutPaths = new Set(paths.map(key));
  for (const l of listeners) l();
}

/** The paths waiting to be moved (lower-case), for fading them in the views. */
export function useCutPaths(): Set<string> {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => cutPaths,
  );
}

export function isCut(cut: Set<string>, path: string): boolean {
  return cut.size > 0 && cut.has(key(path));
}

const plural = (n: number) => (n === 1 ? "1 item" : `${n} items`);

export async function copyFiles(paths: string[]) {
  if (!paths.length) return;
  try {
    await invoke("clipboard_set", { paths, cut: false });
    setCut([]);
    showToast(`Copied ${plural(paths.length)}: paste with Ctrl+V`);
  } catch (err) {
    showToast(String(err), "error");
  }
}

export async function cutFiles(paths: string[]) {
  if (!paths.length) return;
  try {
    await invoke("clipboard_set", { paths, cut: true });
    setCut(paths);
  } catch (err) {
    showToast(String(err), "error");
  }
}

/** Paste the clipboard's files into `dir`: a copy, or a move for a cut (then the clipboard empties). */
export async function pasteInto(dir: string) {
  const c = await invoke<Contents>("clipboard_get").catch(() => ({ paths: [], cut: false }));
  if (!c.paths.length) {
    showToast("There are no files on the clipboard.");
    return;
  }
  const id = await startTransfer(c.paths, dir, c.cut ? "move" : "copy");
  if (id && c.cut) {
    // The files have moved: nothing left to paste, nothing faded.
    void invoke("clipboard_clear_cut");
    setCut([]);
  }
}

/** The clipboard may have changed in another program: forget cut marks that no longer apply. */
window.addEventListener("focus", () => {
  if (!cutPaths.size) return;
  void invoke<Contents>("clipboard_get").then((c) => {
    if (!c.cut || c.paths.length !== cutPaths.size || c.paths.some((p) => !cutPaths.has(key(p)))) setCut([]);
  });
});
