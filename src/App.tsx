import { lazy, Suspense, useEffect, useMemo, useRef, useState } from "react";
import { useAppearance } from "./lib/appearance";
import { buildCommands } from "./lib/commands";
import { publishCommands } from "./lib/agentBridge";
import type { IconTarget } from "./icons/ChangeIcon";
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
const ChangeIconDialog = lazy(() => import("./icons/ChangeIcon").then((m) => ({ default: m.ChangeIconDialog })));
const AiPane = lazy(() => import("./ai/AiPane").then((m) => ({ default: m.AiPane })));
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
  const [iconTarget, setIconTarget] = useState<IconTarget | null>(null);
  uiActions.changeIcon = setIconTarget;
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
  const toggleClaude = () => updateSettings({ showClaude: !settings.showClaude });
  // Both panes open: one at a time, as tabs.
  const [rightTab, setRightTab] = useState<"preview" | "claude">("claude");
  uiActions.showClaude = () => {
    updateSettings({ showClaude: true });
    setRightTab("claude");
  };
  // The Claude pane closing stops Claude Code (its memory goes); the next message resumes the chat.
  const claudeWasOn = useRef(settings.showClaude);
  useEffect(() => {
    if (claudeWasOn.current && !settings.showClaude) void import("./lib/claude").then((m) => m.shutdown());
    claudeWasOn.current = settings.showClaude;
  }, [settings.showClaude]);
  useEffect(() => publishCommands(commands), [commands]);
  const right = settings.showPreview && settings.showClaude ? rightTab : settings.showClaude ? "claude" : settings.showPreview ? "preview" : null;

  return (
    <div className="app">
      <TitleBar
        sidebarOn={settings.showSidebar}
        previewOn={settings.showPreview}
        claudeOn={settings.showClaude}
        onToggleSidebar={toggleSidebar}
        onTogglePreview={togglePreview}
        onToggleClaude={toggleClaude}
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

        <main className="center" data-zoom="files" style={{ zoom: settings.paneZoom.files }}>
          <Toolbar />
          {tabs.map((t) => (
            <FolderPane key={t.id} tab={t} active={t.id === tab.id} />
          ))}
        </main>

        {right && (
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
              <div className="rightpane-stack" data-zoom={right} style={{ zoom: settings.paneZoom[right] }}>
                {settings.showPreview && settings.showClaude && (
                  <div className="right-tabs" role="tablist">
                    <button role="tab" aria-selected={right === "preview"} className={right === "preview" ? "on" : ""} onClick={() => setRightTab("preview")}>
                      Preview
                    </button>
                    <button role="tab" aria-selected={right === "claude"} className={right === "claude" ? "on" : ""} onClick={() => setRightTab("claude")}>
                      Claude
                    </button>
                  </div>
                )}
                <Suspense fallback={<aside className="rightpane" />}>
                  {right === "preview" ? (
                    <PreviewPane onClose={togglePreview} />
                  ) : (
                    <aside className="rightpane">
                      <AiPane onClose={toggleClaude} />
                    </aside>
                  )}
                </Suspense>
              </div>
            </div>
          </>
        )}
      </div>

      {commandBar !== null && <CommandBar commands={commands} initialText={commandBar} onClose={() => setCommandBar(null)} />}
      {settingsOpen && <SettingsPage onClose={() => setSettingsOpen(false)} />}
      {aboutOpen && <AboutDialog onClose={() => setAboutOpen(false)} />}
      {iconTarget && (
        <Suspense fallback={null}>
          <ChangeIconDialog target={iconTarget} onClose={() => setIconTarget(null)} />
        </Suspense>
      )}
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
