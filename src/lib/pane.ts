import { invoke } from "@tauri-apps/api/core";
import { showToast } from "../shell/Toast";

/**
 * What the active folder can do, for commands and shortcuts: the active tab's pane fills these in
 * (and empties them when another tab takes over).
 */
export const paneActions = {
  newFolder: () => {},
  newFile: () => {},
  rename: () => {},
  refresh: () => {},
  /** Delete the selection: to the Recycle Bin / Trash, or permanently (asks first). */
  delete: (_permanently: boolean) => {},
  /** Copy the selection beside itself ("a - Copy.txt"). */
  duplicate: () => {},
};

export type PaneActions = typeof paneActions;

/** Ctrl+Z: undo the most recent file operation, wherever it was, and say what was undone. */
export async function undoLast() {
  try {
    const done = await invoke<string | null>("op_undo");
    showToast(done ? `Undone: ${done}` : "Nothing to undo.");
  } catch (err) {
    showToast(`Couldn't undo: ${String(err)}`, "error");
  }
}
