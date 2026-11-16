import { lazy, Suspense, useEffect, useMemo, useState } from "react";
import { useAppearance } from "./lib/appearance";
import { buildCommands } from "./lib/commands";
import { hasOwnView, useFolderView } from "./lib/folderViews";
import { updateSettings, useSettings } from "./lib/settings";
import { useNoBrowserContextMenu, useShortcuts } from "./lib/shortcuts";
import { uiActions } from "./lib/ui";
import { CommandBar } from "./palette/CommandBar";
import { SettingsPage } from "./settings/SettingsPage";
import { AboutDialog } from "./shell/AboutDialog";
import { UpdateNotice } from "./shell/UpdateNotice";
import { ConfirmHost } from "./shell/Confirm";
import { Toasts } from "./shell/Toast";
import { JobsPanel } from "./jobs/JobsPanel";
import { Splitter } from "./shell/Splitter";
import { TitleBar } from "./shell/TitleBar";
import { FolderPane } from "./views/FolderPane";
import { Sidebar } from "./sidebar/Sidebar";
import { Toolbar } from "./shell/Toolbar";
import { activeTab, goBack, goForward, tabTitle, useTabs } from "./lib/tabs";
import { getCurrentWindow } from "@tauri-apps/api/window";

// Loaded when the pane first opens: its viewers stay out of the start-up bundle.
const PreviewPane = lazy(() => import("./preview/PreviewPane").then((m) => ({ default: m.PreviewPane })));
const QuickLook = lazy(() => import("./preview/PreviewPane").then((m) => ({ default: m.QuickLook })));

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
  const [quickLook, setQuickLook] = useState(false);
  uiActions.toggleQuickLook = () => setQuickLook((q) => !q);

  const { tabs } = useTabs();
  const tab = activeTab();
  const view = useFolderView(tab.path);
  const viewKey = [view.viewMode, view.sortBy, view.sortDescending, view.groupBy, hasOwnView(tab.path)].join();

  // Rebuilt when settings or the open folder's view change so check marks stay current.
  const commands = useMemo(() => buildCommands(), [settings, viewKey, tab.path]);
  useShortcuts(commands);
  useNoBrowserContextMenu();
  useMouseNavigation();

  useEffect(() => {
    void getCurrentWindow().setTitle(`${tabTitle(tab.path)} – Fileboat`).catch(() => {});
  }, [tab.path]);

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
              <Sidebar />
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
          {tabs.map((t) => (
            <FolderPane key={t.id} tab={t} active={t.id === tab.id} />
          ))}
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
            <div className="rightpane-slot" style={{ width: settings.previewWidth }}>
              <Suspense fallback={<aside className="rightpane" />}>
                <PreviewPane onClose={togglePreview} />
              </Suspense>
            </div>
          </>
        )}
      </div>

      {commandBar !== null && <CommandBar commands={commands} initialText={commandBar} onClose={() => setCommandBar(null)} />}
      {settingsOpen && <SettingsPage onClose={() => setSettingsOpen(false)} />}
      {aboutOpen && <AboutDialog onClose={() => setAboutOpen(false)} />}
      {quickLook && (
        <Suspense fallback={null}>
          <QuickLook onClose={() => setQuickLook(false)} />
        </Suspense>
      )}
      <UpdateNotice />
      <ConfirmHost />
      <Toasts />
      <JobsPanel />
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
