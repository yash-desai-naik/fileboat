import { useEffect } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { applyTheme, resolveTheme } from "../themes";
import type { Settings } from "./settings";

/**
 * Keep the window's look in step with the settings: colour theme (following the system when the
 * mode is "system"), density and zoom. Settings changed here, in another window or by Claude all
 * arrive through the settings store.
 */
export function useAppearance({ theme, lightTheme, darkTheme, density, uiZoom }: Settings) {
  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const apply = () => applyTheme(resolveTheme({ theme, lightTheme, darkTheme }, media.matches));
    apply();
    media.addEventListener("change", apply);
    return () => media.removeEventListener("change", apply);
  }, [theme, lightTheme, darkTheme]);

  useEffect(() => {
    document.documentElement.dataset.density = density;
  }, [density]);

  useEffect(() => {
    getCurrentWebview()
      .setZoom(uiZoom)
      .catch((err) => console.error("Could not set zoom", err));
  }, [uiZoom]);
}
