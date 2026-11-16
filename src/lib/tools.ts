import { invoke } from "@tauri-apps/api/core";
import { showToast } from "../shell/Toast";

/** An editor or terminal found on this computer (src-tauri/src/tools.rs). */
export interface Tool {
  id: string;
  name: string;
}

export interface Tools {
  editors: Tool[];
  terminals: Tool[];
}

let cached: Promise<Tools> | null = null;

/** The editors and terminals found (looked up once per window; cheap, but no need to repeat). */
export function loadTools(): Promise<Tools> {
  cached ??= invoke<Tools>("tools_list").catch(() => ({ editors: [], terminals: [] }));
  return cached;
}

export function openInEditor(paths: string[], editor?: string) {
  void invoke("open_in_editor", { paths, editor: editor ?? null }).catch((err) => showToast(String(err), "error"));
}

export function openTerminal(dir: string, terminal?: string) {
  void invoke("open_terminal", { dir, terminal: terminal ?? null }).catch((err) => showToast(String(err), "error"));
}
