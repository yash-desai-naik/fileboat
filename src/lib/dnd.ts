import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { showToast } from "../shell/Toast";
import { dropFavourites } from "./favourites";
import { activeTab } from "./tabs";

/**
 * Files dropped on the window, from other apps or from Fileboat's own drags (they arrive the same
 * way). Elements that accept a drop carry `data-drop-path` (a folder row, the open folder, a tab,
 * a sidebar place, a breadcrumb part); the one under the cursor is highlighted while dragging.
 */

/**
 * Where a drop lands: into a folder (`el` with `data-drop-path`), or into the Favourites at
 * `fav` (the gaps between favourites, their top and bottom edges, the section's heading).
 */
interface Target {
  el: HTMLElement;
  fav?: number;
  /** The class that shows it: "drop-target", or "insert-before"/"insert-after" on a favourite. */
  mark: string;
}

let highlighted: Target | null = null;

function highlight(t: Target | null) {
  if (t?.el === highlighted?.el && t?.mark === highlighted?.mark) return;
  highlighted?.el.classList.remove(highlighted.mark);
  highlighted = t;
  t?.el.classList.add(t.mark);
}

/** The drop target under a position given in physical pixels. */
function targetAt(position: { x: number; y: number }): Target | null {
  // devicePixelRatio includes both the screen's scaling and the app's zoom.
  const ratio = window.devicePixelRatio || 1;
  const [x, y] = [position.x / ratio, position.y / ratio];
  const el = document.elementFromPoint(x, y);
  const fav = el?.closest("[data-fav-index]") as HTMLElement | null;
  if (fav) {
    // A favourite's top and bottom edges insert beside it; its middle drops into the folder.
    const r = fav.getBoundingClientRect();
    const i = Number(fav.dataset.favIndex);
    if (y < r.top + r.height * 0.3) return { el: fav, fav: i, mark: "insert-before" };
    if (y > r.bottom - r.height * 0.3) return { el: fav, fav: i + 1, mark: "insert-after" };
  }
  const into = el?.closest("[data-drop-path]") as HTMLElement | null;
  if (into) return { el: into, mark: "drop-target" };
  const zone = el?.closest("[data-drop-favourites]") as HTMLElement | null;
  if (zone) return { el: zone, fav: Number(zone.dataset.dropFavourites), mark: "drop-target" };
  return null;
}

const sameFolder = (a: string, b: string) => a.replace(/[\\/]+$/, "").toLowerCase() === b.replace(/[\\/]+$/, "").toLowerCase();

export function listenForDrops() {
  void getCurrentWebview().onDragDropEvent((event) => {
    const e = event.payload;
    if (e.type === "enter" || e.type === "over") {
      highlight(targetAt(e.position));
    } else if (e.type === "leave") {
      highlight(null);
    } else if (e.type === "drop") {
      const target = targetAt(e.position);
      highlight(null);
      if (target?.fav !== undefined) {
        void dropFavourites(e.paths, target.fav);
        return;
      }
      const dest = target?.el.dataset.dropPath ?? activeTab().path;
      // Dropping items on themselves (a folder onto itself) does nothing.
      const sources = e.paths.filter((p) => !sameFolder(p, dest));
      if (!sources.length) return;
      void invoke("job_drop", { sources, dest }).catch((err) => showToast(String(err), "error"));
    }
  });
}

/** Start a system drag of these files (from a row being dragged). */
export function dragOut(paths: string[]) {
  if (paths.length) void invoke("drag_start", { paths }).catch(() => {});
}
