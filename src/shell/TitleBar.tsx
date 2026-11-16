import { useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { fmtShortcut, IS_MAC } from "../lib/platform";
import { TabStrip } from "./TabStrip";
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
  claudeOn: boolean;
  onToggleSidebar: () => void;
  onTogglePreview: () => void;
  onToggleClaude: () => void;
  onOpenCommands: () => void;
}

/**
 * Custom 40 px title bar with the tabs in the middle. Empty areas drag the window (and
 * double-click maximises). Windows gets Fileboat's caption buttons on the right; macOS keeps its
 * traffic lights on the left, over the bar (see `.titlebar.mac`).
 */
export function TitleBar({ sidebarOn, previewOn, claudeOn, onToggleSidebar, onTogglePreview, onToggleClaude, onOpenCommands }: TitleBarProps) {
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

      <div className="tb-center" data-tauri-drag-region>
        <TabStrip />
      </div>

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
        <button className="icon-btn" title={`Toggle Claude (${fmtShortcut("Ctrl+Alt+B")})`} aria-pressed={claudeOn} onClick={onToggleClaude}>
          <ClaudeIcon />
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

/** Claude's starburst, in the title bar's icon style. */
function ClaudeIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true" fill="currentColor">
      {[0, 30, 60, 90, 120, 150].map((a) => (
        <rect key={a} x="7.25" y="1.5" width="1.5" height="13" rx="0.75" transform={`rotate(${a} 8 8)`} />
      ))}
    </svg>
  );
}
