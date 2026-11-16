import { useEffect, useState } from "react";
import { getTauriVersion, getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { FILE_MANAGER, IS_MAC } from "../lib/platform";
import { AppMark } from "./Icons";
import { checkForUpdates, installUpdate, useUpdate } from "../lib/update";
import { Dialog } from "./Dialog";

interface AppPaths {
  dataDir: string;
  cacheDir: string;
  exe: string;
}

interface AboutInfo {
  version: string;
  tauri: string;
  webview: string;
}

/** "Fileboat 0.1.0 · 0c01f93" (plus "dev" / "+" for local changes), for a footer or a title. */
export function versionLine(version: string): string {
  const b = __APP_BUILD__;
  return `Fileboat ${version} · ${b.commit}${b.dirty ? "+" : ""}${b.dev ? " · dev" : ""}`;
}

export function useAppVersion(): string | null {
  const [v, setV] = useState<string | null>(null);
  useEffect(() => {
    void getVersion().then(setV).catch(() => setV("?"));
  }, []);
  return v;
}

/** Version, build and where Fileboat keeps its files. */
export function AboutDialog({ onClose }: { onClose: () => void }) {
  const [info, setInfo] = useState<AboutInfo | null>(null);
  const [paths, setPaths] = useState<AppPaths | null>(null);
  const [copied, setCopied] = useState(false);
  const b = __APP_BUILD__;
  const update = useUpdate();

  useEffect(() => {
    const ua = navigator.userAgent;
    const webview =
      (IS_MAC ? (/Version\/([\d.]+)/.exec(ua)?.[1] ?? /AppleWebKit\/([\d.]+)/.exec(ua)?.[1]) : /Edg\/([\d.]+)/.exec(ua)?.[1]) ?? "?";
    void Promise.all([getVersion(), getTauriVersion()])
      .then(([version, tauri]) => setInfo({ version, tauri, webview }))
      .catch(() => setInfo({ version: "?", tauri: "?", webview }));
    void invoke<AppPaths>("app_paths").then(setPaths).catch(() => {});
  }, []);

  const built = new Date(b.date);
  const rows: [string, string][] = info
    ? [
        ["Version", info.version],
        ["Build", `${b.commit}${b.dirty ? " (with uncommitted changes)" : ""}${b.dev ? " · development run" : ""}`],
        ["Built", built.toLocaleString()],
        ["Tauri", info.tauri],
        [IS_MAC ? "WebKit" : "WebView2", info.webview],
      ]
    : [];

  const details = [
    ...rows.map(([k, v]) => `${k}: ${v}`),
    ...(paths ? [`Program: ${paths.exe}`, `Data: ${paths.dataDir}`, `Cache: ${paths.cacheDir}`] : []),
  ].join("\n");

  const folder = (which: "data" | "cache", dir: string, what: string) => (
    <td className="mono about-path">
      {dir}{" "}
      <button
        className="link-btn"
        title={`Open in ${FILE_MANAGER}`}
        onClick={() => void invoke("open_app_folder", { which })}
      >
        Open
      </button>
      <div className="muted small about-what">{what}</div>
    </td>
  );

  return (
    <Dialog
      title="About Fileboat"
      onClose={onClose}
      width={560}
      footer={
        <>
          <button
            className="btn"
            onClick={() =>
              void navigator.clipboard.writeText(details).then(() => {
                setCopied(true);
                window.setTimeout(() => setCopied(false), 1500);
              })
            }
          >
            {copied ? "Copied" : "Copy details"}
          </button>
          <span className="spacer" />
          <button className="btn primary" onClick={onClose}>
            Close
          </button>
        </>
      }
    >
      <div className="about-head">
        <AppMark size={44} />
        <div>
          <div className="about-name">Fileboat {info?.version ?? ""}</div>
          <div className="muted small">A small, fast file explorer for developers.</div>
          <div className="muted small">
            Themes based on Catppuccin, Tokyo Night, GitHub Primer, One Dark, Rosé Pine, Gruvbox, Kanagawa, Nord and Dracula
            (MIT).
          </div>
          <div className="about-update">
            {update.info ? (
              <>
                <span>Version {update.info.version} is available.</span>
                <button
                  className="btn primary small-btn"
                  disabled={update.phase === "downloading" || update.phase === "installing"}
                  onClick={() => void installUpdate()}
                >
                  Install and restart
                </button>
              </>
            ) : (
              <>
                <button className="btn small-btn" disabled={update.phase === "checking"} onClick={() => void checkForUpdates()}>
                  {update.phase === "checking" ? "Checking…" : "Check for updates"}
                </button>
                {update.phase === "uptodate" && <span className="muted small">You're up to date.</span>}
                {update.phase === "error" && <span className="muted small">{update.error}</span>}
              </>
            )}
          </div>
        </div>
      </div>
      <table className="kv about-table">
        <tbody>
          {rows.map(([k, v]) => (
            <tr key={k}>
              <th>{k}</th>
              <td className="mono">{v}</td>
            </tr>
          ))}
          {paths && (
            <>
              <tr>
                <th>Program</th>
                <td className="mono about-path">{paths.exe}</td>
              </tr>
              <tr>
                <th>Data</th>
                {folder("data", paths.dataDir, "Settings, open tabs and windows")}
              </tr>
              <tr>
                <th>Cache</th>
                {folder("cache", paths.cacheDir, "The search index and icon caches; safe to delete")}
              </tr>
            </>
          )}
        </tbody>
      </table>
    </Dialog>
  );
}
