import { useRef } from "react";
import { fmtShortcut } from "../lib/platform";
import { activateTab, closeTab, moveTab, newTab, tabTitle, useTabs } from "../lib/tabs";
import { CloseSmallIcon, FolderIcon, PlusIcon } from "./Icons";

/**
 * The tabs, in the title bar. Click to switch, middle-click or × to close, drag to reorder;
 * + (or Ctrl+T) opens a new tab. Empty space around the tabs still drags the window.
 */
export function TabStrip() {
  const { tabs, activeId } = useTabs();
  const stripRef = useRef<HTMLDivElement>(null);
  const drag = useRef<{ id: string; x: number; moved: boolean } | null>(null);

  /** The tab position under the pointer while dragging. */
  const indexAt = (x: number): number => {
    const els = Array.from(stripRef.current?.querySelectorAll<HTMLElement>(".tab") ?? []);
    const i = els.findIndex((el) => {
      const r = el.getBoundingClientRect();
      return x < r.left + r.width / 2;
    });
    return i < 0 ? els.length - 1 : i;
  };

  return (
    <div className="tabs" role="tablist" ref={stripRef} data-tauri-drag-region>
      {tabs.map((t) => {
        const active = t.id === activeId;
        return (
          <div
            key={t.id}
            role="tab"
            aria-selected={active}
            tabIndex={active ? 0 : -1}
            className={`tab${active ? " active" : ""}`}
            data-drop-path={t.path}
            title={t.path === "~" ? "Home" : t.path}
            onMouseDown={(e) => {
              if (e.button === 1) {
                e.preventDefault();
                closeTab(t.id);
              } else if (e.button === 0) {
                activateTab(t.id);
              }
            }}
            onPointerDown={(e) => {
              if (e.button !== 0 || (e.target as HTMLElement).closest(".tab-close")) return;
              e.currentTarget.setPointerCapture(e.pointerId);
              drag.current = { id: t.id, x: e.clientX, moved: false };
            }}
            onPointerMove={(e) => {
              const d = drag.current;
              if (!d) return;
              if (!d.moved && Math.abs(e.clientX - d.x) < 6) return;
              d.moved = true;
              moveTab(d.id, indexAt(e.clientX));
            }}
            onPointerUp={(e) => {
              e.currentTarget.releasePointerCapture(e.pointerId);
              drag.current = null;
            }}
          >
            <span className="tab-icon">
              <FolderIcon size={14} />
            </span>
            <span className="tab-title">{tabTitle(t.path)}</span>
            <button
              className="tab-close"
              title={`Close tab (${fmtShortcut("Ctrl+W")})`}
              aria-label="Close tab"
              onMouseDown={(e) => e.stopPropagation()}
              onClick={() => closeTab(t.id)}
            >
              <CloseSmallIcon size={12} />
            </button>
          </div>
        );
      })}
      <button className="icon-btn small tab-new" title={`New tab (${fmtShortcut("Ctrl+T")})`} onClick={() => newTab()}>
        <PlusIcon size={14} />
      </button>
    </div>
  );
}
