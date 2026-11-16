import { useEffect, useRef } from "react";
import type { Command } from "./commands";
import { comboFromEvent, isBrowserReserved } from "./keys";

/** Keys that edit text: in a text field they never act on files. */
const TEXT_EDITING = new Set(["Ctrl+Z", "Ctrl+Y", "Ctrl+Shift+Z", "Ctrl+A", "Ctrl+C", "Ctrl+X", "Ctrl+V", "Ctrl+Backspace", "Ctrl+Delete"]);

/** One keydown listener for the whole app; matches shortcuts against the command list. */
export function useShortcuts(commands: Command[]) {
  const latest = useRef(commands);
  latest.current = commands;

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      const combo = comboFromEvent(e);
      if (!combo) return;
      // While typing in a field, plain keys (Backspace, Delete, letters) are text, not shortcuts.
      const target = e.target as HTMLElement | null;
      const typing = target?.closest?.("input, textarea, select, [contenteditable='true']");
      if (typing && (!/^(Ctrl|Alt)\+|^F\d/.test(combo) || TEXT_EDITING.has(combo))) return;
      const command = latest.current.find((c) => c.keys?.includes(combo));
      if (command) {
        e.preventDefault();
        e.stopPropagation();
        command.run();
      } else if (isBrowserReserved(combo)) {
        // Stop the WebView from printing, reloading or opening its find bar.
        e.preventDefault();
      }
    };
    window.addEventListener("keydown", onKeyDown, { capture: true });
    return () => window.removeEventListener("keydown", onKeyDown, { capture: true });
  }, []);
}

/** Hide the browser's right-click menu, except in text fields where copy/paste is useful. */
export function useNoBrowserContextMenu() {
  useEffect(() => {
    if (import.meta.env.DEV) return;
    const onContextMenu = (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      if (target.closest("input, textarea, [contenteditable='true']")) return;
      e.preventDefault();
    };
    window.addEventListener("contextmenu", onContextMenu);
    return () => window.removeEventListener("contextmenu", onContextMenu);
  }, []);
}
