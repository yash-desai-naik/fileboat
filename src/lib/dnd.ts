import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { showToast } from "../shell/Toast";
import { activeTab } from "./tabs";

/**
 * Files dropped on the window, from other apps or from Fileboat's own drags (they arrive the same
 * way). Elements that accept a drop carry `data-drop-path` (a folder row, the open folder, a tab,
 * a sidebar place, a breadcrumb part); the one under the cursor is highlighted while dragging.
 */

let highlighted: HTMLElement | null = null;

function highlight(el: HTMLElement | null) {
  if (el === highlighted) return;
  highlighted?.classList.remove("drop-target");
  highlighted = el;
  el?.classList.add("drop-target");
}

/** The drop target under a position given in physical pixels. */
function targetAt(position: { x: number; y: number }): HTMLElement | null {
  // devicePixelRatio includes both the screen's scaling and the app's zoom.
  const ratio = window.devicePixelRatio || 1;
  const el = document.elementFromPoint(position.x / ratio, position.y / ratio);
  return (el?.closest("[data-drop-path]") as HTMLElement | null) ?? null;
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
      const dest = target?.dataset.dropPath ?? activeTab().path;
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
