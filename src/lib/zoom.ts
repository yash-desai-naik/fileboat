import { getSettings, updateSettings, type ZoomSection } from "./settings";

/**
 * Zooming with the wheel: Ctrl+wheel (or a pinch) zooms the whole window, through the same steps
 * as Ctrl+= and Ctrl+-; Alt+wheel zooms only the section under the pointer (the sidebar, the
 * files, the preview or the Claude pane), which carries `data-zoom`. Each section keeps its own
 * level in settings (`paneZoom`), on top of the window's.
 */

export const ZOOM_STEPS = [0.5, 0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3];

/** The next zoom step up or down from `zoom`, or undefined at the end. */
export function nextZoom(zoom: number, direction: 1 | -1): number | undefined {
  return direction > 0 ? ZOOM_STEPS.find((z) => z > zoom + 0.001) : [...ZOOM_STEPS].reverse().find((z) => z < zoom - 0.001);
}

export function stepZoom(direction: 1 | -1) {
  const next = nextZoom(getSettings().uiZoom, direction);
  if (next !== undefined) {
    updateSettings({ uiZoom: next });
    showLevel(null, "Zoom", next);
  }
}

const SECTION_NAME: Record<ZoomSection, string> = { sidebar: "Sidebar", files: "Files", preview: "Preview", claude: "Claude" };

export function stepPaneZoom(section: ZoomSection, direction: 1 | -1, el: Element | null) {
  const zooms = getSettings().paneZoom;
  const next = nextZoom(zooms[section] ?? 1, direction);
  if (next === undefined) return;
  updateSettings({ paneZoom: { ...zooms, [section]: next } }, { debounceMs: 300 });
  showLevel(el, SECTION_NAME[section], next);
}

export function resetPaneZoom() {
  updateSettings({ paneZoom: { sidebar: 1, files: 1, preview: 1, claude: 1 } });
}

// A short-lived "Files 125%" over the section (or the window) being zoomed.
let badge: HTMLDivElement | null = null;
let hideTimer: number | undefined;
function showLevel(el: Element | null, what: string, zoom: number) {
  if (!badge) {
    badge = document.createElement("div");
    badge.className = "zoom-badge";
    badge.setAttribute("role", "status");
    document.body.appendChild(badge);
  }
  const r = el?.getBoundingClientRect() ?? { left: 0, top: 0, width: window.innerWidth, height: window.innerHeight };
  badge.textContent = `${what} ${Math.round(zoom * 100)}%`;
  badge.style.left = `${r.left + r.width / 2}px`;
  badge.style.top = `${r.top + r.height / 2}px`;
  badge.classList.add("on");
  window.clearTimeout(hideTimer);
  hideTimer = window.setTimeout(() => badge?.classList.remove("on"), 900);
}

/** How much wheel turns one step: a mouse notch is 100; a trackpad sends many small ones. */
const STEP = 50;

export function listenForWheelZoom() {
  let acc = 0;
  let last = 0;
  window.addEventListener(
    "wheel",
    (e) => {
      const window_ = e.ctrlKey || e.metaKey;
      if (!window_ && !e.altKey) return;
      const section = window_ ? null : ((e.target as Element | null)?.closest?.("[data-zoom]") as HTMLElement | null);
      if (!window_ && !section) return;
      e.preventDefault();
      // Some wheels report Alt+wheel sideways; either way is a turn.
      const delta = e.deltaY || e.deltaX;
      if (e.timeStamp - last > 400 || Math.sign(delta) !== Math.sign(acc)) acc = 0;
      last = e.timeStamp;
      acc += delta;
      if (Math.abs(acc) < STEP) return;
      const direction = acc < 0 ? 1 : -1;
      acc = 0;
      if (section) stepPaneZoom(section.dataset.zoom as ZoomSection, direction, section);
      else stepZoom(direction);
    },
    { passive: false, capture: true },
  );
}

/** The zoom an element is drawn at relative to the page (CSS zoom on it or above it). */
export function zoomOf(el: Element | null): number {
  return (el as (Element & { currentCSSZoom?: number }) | null)?.currentCSSZoom ?? 1;
}
