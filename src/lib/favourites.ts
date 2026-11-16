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
