import { invoke } from "@tauri-apps/api/core";
import { showToast } from "../shell/Toast";
import { getSettings, updateSettings } from "./settings";
import { activeTab, samePath } from "./tabs";

/** Pin a folder in the sidebar (the active tab's folder by default). */
export function addFavourite(path: string = activeTab().path) {
  const favs = getSettings().favourites;
  if (!favs.some((f) => samePath(f, path))) updateSettings({ favourites: [...favs, path] });
}

export function removeFavourite(path: string) {
  updateSettings({ favourites: getSettings().favourites.filter((f) => !samePath(f, path)) });
}

/** Put `paths` at `index` in the favourites: new ones are added, ones already there move. */
export function placeFavourites(paths: string[], index: number) {
  const favs = getSettings().favourites;
  const moving = favs.filter((f) => paths.some((p) => samePath(p, f)));
  const before = favs.slice(0, index).filter((f) => !moving.includes(f));
  const after = favs.slice(index).filter((f) => !moving.includes(f));
  const added = paths.filter((p) => !favs.some((f) => samePath(f, p)));
  const next = [...before, ...paths.map((p) => moving.find((f) => samePath(f, p)) ?? p), ...after];
  // Keep each folder once (paths may repeat).
  const unique = next.filter((p, i) => next.findIndex((q) => samePath(p, q)) === i);
  if (unique.join("\n") !== favs.join("\n")) updateSettings({ favourites: unique });
  return added.length;
}

/** Folders dropped on the Favourites: pinned at `index` (files are left out, with a word why). */
export async function dropFavourites(paths: string[], index: number) {
  const kinds = await Promise.all(paths.map((p) => invoke<{ path: string; kind: string }>("path_resolve", { input: p, base: null }).catch(() => null)));
  const folders = kinds.filter((k) => k?.kind === "folder").map((k) => k!.path);
  if (folders.length) placeFavourites(folders, index);
  if (folders.length < paths.length) showToast(folders.length ? "Only the folders were added: favourites are folders." : "Favourites are folders: drop a folder here.", folders.length ? "info" : "error");
}
