import { createRoot } from "react-dom/client";
import { App } from "./App";
import { loadSettings } from "./lib/settings";
import { applyTheme, resolveTheme } from "./themes";
// JetBrains Mono, bundled: used only when it isn't installed (the installed copy comes first).
import "@fontsource-variable/jetbrains-mono/wght.css";
import "./styles.css";
import "./controls.css";
import "./type.css";

loadSettings()
  .then((settings) => {
    // Set the theme before the first render so the window never shows the wrong colours.
    const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
    applyTheme(resolveTheme(settings, prefersDark));
    document.documentElement.dataset.density = settings.density;
    createRoot(document.getElementById("root")!).render(<App />);
  })
  .catch((err) => {
    document.body.textContent = `Fileboat could not load its settings: ${err}`;
  });
