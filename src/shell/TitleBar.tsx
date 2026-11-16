import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { fmtShortcut, IS_MAC } from "../lib/platform";
import {
  AppMark,
  CloseGlyph,
  CommandIcon,
  MaximizeGlyph,
  MinimizeGlyph,
  PanelLeftIcon,
  PreviewIcon,
  RestoreGlyph,
} from "./Icons";

const appWindow = getCurrentWindow();

interface TitleBarProps {
  sidebarOn: boolean;
  previewOn: boolean;
  onToggleSidebar: () => void;
  onTogglePreview: () => void;
  onOpenCommands: () => void;
}

/**
 * Custom 40 px title bar. Empty areas drag the window (and double-click maximises); the tab strip
 * will live in the middle. Windows gets Fileboat's caption buttons on the right; macOS keeps its
 * traffic lights on the left, over the bar (see `.titlebar.mac`).
 */
export function TitleBar({ sidebarOn, previewOn, onToggleSidebar, onTogglePreview, onOpenCommands }: TitleBarProps) {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    const refresh = () => appWindow.isMaximized().then(setMaximized).catch(() => {});
    refresh();
    appWindow.onResized(refresh).then((fn) => (unlisten = fn));
    return () => unlisten?.();
  }, []);

  return (
    <header className={IS_MAC ? "titlebar mac" : "titlebar"} data-tauri-drag-region>
      <div className="tb-group" data-tauri-drag-region>
        <span className="tb-mark" data-tauri-drag-region>
          <AppMark size={16} />
        </span>
        <button
          className="icon-btn"
          title={`Toggle sidebar (${fmtShortcut("Ctrl+B")})`}
          aria-pressed={sidebarOn}
          onClick={onToggleSidebar}
        >
          <PanelLeftIcon />
        </button>
      </div>

      <div className="tb-center" data-tauri-drag-region />

      <div className="tb-group" data-tauri-drag-region>
        <button className="icon-btn" title={`Show all commands (${fmtShortcut("Ctrl+Shift+P")})`} onClick={onOpenCommands}>
          <CommandIcon />
        </button>
        <button
          className="icon-btn"
          title={`Toggle preview pane (${fmtShortcut("Alt+P")})`}
          aria-pressed={previewOn}
          onClick={onTogglePreview}
        >
          <PreviewIcon />
        </button>
      </div>

      {/* macOS draws its own traffic lights on the left; the custom buttons are Windows-only. */}
      {!IS_MAC && (
        <div className="caption-buttons">
          <button className="caption-btn" title="Minimize" onClick={() => appWindow.minimize()}>
            <MinimizeGlyph />
          </button>
          <button
            className="caption-btn"
            title={maximized ? "Restore Down" : "Maximize"}
            onClick={() => appWindow.toggleMaximize()}
          >
            {maximized ? <RestoreGlyph /> : <MaximizeGlyph />}
          </button>
          <button className="caption-btn close" title="Close" onClick={() => appWindow.close()}>
            <CloseGlyph />
          </button>
        </div>
      )}
    </header>
  );
}
