import { createRoot } from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { App } from "./App";
import { loadSettings } from "./lib/settings";
import { openFromOutside, restoreTabs, saveTabs } from "./lib/tabs";
import { listenForDrops } from "./lib/dnd";
import { listen } from "@tauri-apps/api/event";
import { applyTheme, resolveTheme } from "./themes";
// JetBrains Mono, bundled: used only when it isn't installed (the installed copy comes first).
import "@fontsource-variable/jetbrains-mono/wght.css";
import "./styles.css";
import "./controls.css";
import "./shell.css";
import "./palette.css";
import "./views.css";
import "./sidebar.css";
import "./update.css";
import "./type.css";

const appWindow = getCurrentWindow();

/**
 * The window starts hidden (Rust has sized and centered it). Maximize it, then show it after
 * the first paint so there's no white flash. "Restore Down" returns to the centered size.
 */
function showWhenPainted() {
  requestAnimationFrame(() =>
    requestAnimationFrame(() => {
      void appWindow
        .maximize()
        .catch(() => {})
        .then(() => appWindow.show())
        .then(() => appWindow.setFocus())
        // A window maximized while still hidden has no proper restore position, so set one.
        .then(() => invoke("window_fit_restore"))
        .catch(() => {});
    }),
  );
}

// Never leave the user with only a taskbar or Dock icon: if loading settings is slow or fails,
// show the window anyway.
window.setTimeout(() => {
  void appWindow.show().then(() => appWindow.setFocus()).catch(() => {});
}, 1500);

// Save open tabs before the window closes (they're also saved a moment after each change).
void appWindow.onCloseRequested(async () => {
  await saveTabs();
});

// A reloaded page starts clean: folders the previous page opened are closed in Rust first.
Promise.all([loadSettings(), restoreTabs(), invoke("folders_reset").catch(() => {})])
  .then(([settings]) => {
    // Set the theme before the first render so the window never shows the wrong colours.
    const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    applyTheme(resolveTheme(settings, prefersDark));
    document.documentElement.dataset.density = settings.density;
    createRoot(document.getElementById("root")!).render(<App />);
    // `fileboat D:\x`: the path this launch was given, then any a second launch hands over.
    void invoke<string | null>("take_launch_path").then((p) => {
      if (p) void openFromOutside(p);
    });
    void listen<string>("open-path", (e) => void openFromOutside(e.payload));
    listenForDrops();
    showWhenPainted();
  })
  .catch((err) => {
    document.body.textContent = `Fileboat could not load its settings: ${err}`;
    void appWindow.show().then(() => appWindow.setFocus());
  });
