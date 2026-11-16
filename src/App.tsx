import { useEffect, useMemo, useState } from "react";
import { useAppearance } from "./lib/appearance";
import { buildCommands } from "./lib/commands";
import { updateSettings, useSettings } from "./lib/settings";
import { useNoBrowserContextMenu, useShortcuts } from "./lib/shortcuts";
import { uiActions } from "./lib/ui";
import { CommandBar } from "./palette/CommandBar";
import { SettingsPage } from "./settings/SettingsPage";
import { AboutDialog } from "./shell/AboutDialog";
import { UpdateNotice } from "./shell/UpdateNotice";
import { FileIcon, FolderIcon, CloseSmallIcon } from "./shell/Icons";
import { Splitter } from "./shell/Splitter";
import { TitleBar } from "./shell/TitleBar";
import { FolderPane } from "./views/FolderPane";
import { Toolbar } from "./shell/Toolbar";
import { activeTab, goBack, goForward, useTabs } from "./lib/tabs";

const SIDEBAR_DEFAULT = 240;
const PREVIEW_DEFAULT = 360;

export function App() {
  const settings = useSettings();
  useAppearance(settings);

  // The command bar's opening text, or null when it's closed.
  const [commandBar, setCommandBar] = useState<string | null>(null);
  uiActions.openCommandBar = (text = ">") => setCommandBar(text);
  const [settingsOpen, setSettingsOpen] = useState(false);
  uiActions.openSettings = () => setSettingsOpen(true);
  const [aboutOpen, setAboutOpen] = useState(false);
  uiActions.openAbout = () => setAboutOpen(true);

  // Rebuilt when settings change so check marks stay current.
  const commands = useMemo(() => buildCommands(), [settings]);
  useShortcuts(commands);
  useNoBrowserContextMenu();

  useTabs();
  const tab = activeTab();
  useMouseNavigation();

  const toggleSidebar = () => updateSettings({ showSidebar: !settings.showSidebar });
  const togglePreview = () => updateSettings({ showPreview: !settings.showPreview });

  return (
    <div className="app">
      <TitleBar
        sidebarOn={settings.showSidebar}
        previewOn={settings.showPreview}
        onToggleSidebar={toggleSidebar}
        onTogglePreview={togglePreview}
        onOpenCommands={() => setCommandBar(">")}
      />

      <div className="workspace">
        {settings.showSidebar && (
          <>
            <aside className="sidebar" style={{ width: settings.sidebarWidth }}>
              <div className="pane-header">
                <span className="pane-title">Places</span>
              </div>
              <div className="pane-empty">
                <FolderIcon size={20} />
                <p>Favourites, projects and drives</p>
                <p className="muted small">They appear here once folders can be opened.</p>
              </div>
            </aside>
            <Splitter
              pane="left"
              label="Resize sidebar"
              width={settings.sidebarWidth}
              min={160}
              max={900}
              onResize={(w) => updateSettings({ sidebarWidth: w }, { debounceMs: 400 })}
              onReset={() => updateSettings({ sidebarWidth: SIDEBAR_DEFAULT })}
            />
          </>
        )}

        <main className="center">
          <Toolbar />
          <FolderPane tab={tab} />
        </main>

        {settings.showPreview && (
          <>
            <Splitter
              pane="right"
              label="Resize preview pane"
              width={settings.previewWidth}
              min={240}
              max={1600}
              onResize={(w) => updateSettings({ previewWidth: w }, { debounceMs: 400 })}
              onReset={() => updateSettings({ previewWidth: PREVIEW_DEFAULT })}
            />
            <aside className="rightpane" style={{ width: settings.previewWidth }}>
              <div className="pane-header">
                <span className="pane-title">Preview</span>
                <button className="icon-btn small" title="Close preview pane" onClick={togglePreview}>
                  <CloseSmallIcon size={14} />
                </button>
              </div>
              <div className="pane-empty">
                <FileIcon size={20} />
                <p>Select a file to preview it</p>
              </div>
            </aside>
          </>
        )}
      </div>

      {commandBar !== null && <CommandBar commands={commands} initialText={commandBar} onClose={() => setCommandBar(null)} />}
      {settingsOpen && <SettingsPage onClose={() => setSettingsOpen(false)} />}
      {aboutOpen && <AboutDialog onClose={() => setAboutOpen(false)} />}
      <UpdateNotice />
    </div>
  );
}

/** The mouse's back and forward side buttons. */
function useMouseNavigation() {
  useEffect(() => {
    const onUp = (e: MouseEvent) => {
      if (e.button === 3) goBack();
      else if (e.button === 4) goForward();
      else return;
      e.preventDefault();
    };
    window.addEventListener("mouseup", onUp);
    return () => window.removeEventListener("mouseup", onUp);
  }, []);
}
