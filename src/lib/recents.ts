import { invoke } from "@tauri-apps/api/core";
import { activeTab, samePath, subscribeTabs } from "./tabs";

/** Folders you've been in lately, newest first (for the command bar's Go to). */
let recents: string[] = [];
let saveTimer: number | undefined;
const LIMIT = 50;

export function getRecents(): string[] {
  return recents;
}

function save() {
  window.clearTimeout(saveTimer);
  saveTimer = window.setTimeout(() => void invoke("state_write", { name: "recents", contents: JSON.stringify(recents) }).catch(() => {}), 1000);
}

function visit(path: string) {
  if (recents[0] && samePath(recents[0], path)) return;
  recents = [path, ...recents.filter((p) => !samePath(p, path))].slice(0, LIMIT);
  save();
}

/** Start remembering (once, at start-up). */
export async function loadRecents() {
  try {
    const text = await invoke<string | null>("state_read", { name: "recents" });
    const list = text ? (JSON.parse(text) as unknown) : [];
    if (Array.isArray(list)) recents = list.filter((p): p is string => typeof p === "string").slice(0, LIMIT);
  } catch {
    recents = [];
  }
  let last = "";
  subscribeTabs(() => {
    const p = activeTab().path;
    if (p !== last) {
      last = p;
      visit(p);
    }
  });
}
