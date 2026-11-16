import { useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { fmtShortcut, OS_NAME } from "../lib/platform";
import { updateSettings, useSettings, type Settings } from "../lib/settings";
import { shownMode, usePrefersDark } from "../lib/themeMode";
import { Dialog } from "../shell/Dialog";
import type { ThemeMode } from "../themes";
import { ThemePicker } from "./ThemePicker";

type Def =
  | { key: keyof Settings; type: "select"; options: [string | number, string][] }
  | { key: keyof Settings; type: "toggle" }
  | { key: "lightTheme" | "darkTheme"; type: "themes"; mode: ThemeMode };

type Entry = Def & { section: string; label: string; help?: string };

/** Every setting, in the order shown. Features add their settings here as they arrive. */
const ENTRIES: Entry[] = [
  {
    section: "Appearance",
    key: "theme",
    type: "select",
    label: "Mode",
    help: `Light or dark theme. "Match ${OS_NAME}" follows your system's appearance setting.`,
    options: [
      ["system", `Match ${OS_NAME}`],
      ["light", "Light"],
      ["dark", "Dark"],
    ],
  },
  {
    section: "Appearance",
    key: "lightTheme",
    type: "themes",
    mode: "light",
    label: "Light theme",
    help: "Colours used when the mode is light. Point at a card to try it.",
  },
  {
    section: "Appearance",
    key: "darkTheme",
    type: "themes",
    mode: "dark",
    label: "Dark theme",
    help: "Colours used when the mode is dark. Point at a card to try it.",
  },
  {
    section: "Appearance",
    key: "density",
    type: "select",
    label: "Density",
    help: "Row height in file lists, the sidebar and menus.",
    options: [
      ["compact", "Compact"],
      ["default", "Default"],
      ["comfortable", "Comfortable"],
    ],
  },
  {
    section: "Appearance",
    key: "uiZoom",
    type: "select",
    label: "Zoom",
    help: `${fmtShortcut("Ctrl+=")} / ${fmtShortcut("Ctrl+-")} / ${fmtShortcut("Ctrl+0")}`,
    options: [0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2].map((z) => [z, `${Math.round(z * 100)}%`]),
  },

  { section: "Layout", key: "showSidebar", type: "toggle", label: "Show the sidebar", help: fmtShortcut("Ctrl+B") },
  { section: "Layout", key: "showPreview", type: "toggle", label: "Show the preview pane", help: fmtShortcut("Alt+P") },
];

/** Ctrl+, : every setting, searchable. Changes apply and save immediately. */
export function SettingsPage({ onClose }: { onClose: () => void }) {
  const settings = useSettings();
  const [query, setQuery] = useState("");
  const [fileNote, setFileNote] = useState<string | null>(null);
  const mode = shownMode(settings.theme, usePrefersDark());

  const visible = useMemo(() => {
    const q = query.trim().toLowerCase();
    return q ? ENTRIES.filter((e) => `${e.section} ${e.label} ${e.help ?? ""}`.toLowerCase().includes(q)) : ENTRIES;
  }, [query]);

  const sections = [...new Set(visible.map((e) => e.section))];

  return (
    <Dialog
      title="Settings"
      onClose={onClose}
      width={680}
      footer={
        <>
          <button
            className="link-btn"
            onClick={() =>
              void invoke<string>("open_settings_file")
                .then((p) => setFileNote(`Opened ${p}`))
                .catch((e) => setFileNote(String(e)))
            }
          >
            Open settings.json
          </button>
          {fileNote && <span className="muted small">{fileNote}</span>}
          <span className="spacer" />
          <button className="btn primary" onClick={onClose}>
            Done
          </button>
        </>
      }
    >
      <input
        className="input"
        placeholder="Search settings"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        spellCheck={false}
      />
      {visible.length === 0 && <p className="muted">No setting matches “{query}”.</p>}
      {sections.map((section) => (
        <section key={section} className="form-section">
          <div className="form-section-head">
            <span>{section}</span>
          </div>
          {visible
            .filter((e) => e.section === section)
            .map((e) =>
              e.type === "themes" ? (
                <ThemePicker
                  key={e.key}
                  mode={e.mode}
                  chosen={settings[e.key]}
                  label={e.label}
                  help={e.help}
                  usedNow={mode === e.mode}
                />
              ) : (
                <div key={e.key} className="setting-row">
                  <div className="setting-text">
                    <div>{e.label}</div>
                    {e.help && <div className="muted small">{e.help}</div>}
                  </div>
                  <div className="setting-control">
                    <Control entry={e} settings={settings} />
                  </div>
                </div>
              ),
            )}
        </section>
      ))}
    </Dialog>
  );
}

function Control({ entry, settings }: { entry: Entry; settings: Settings }) {
  const value = settings[entry.key];
  switch (entry.type) {
    case "select":
      return (
        <select
          className="input"
          value={String(value)}
          onChange={(e) => {
            const raw = entry.options.find(([v]) => String(v) === e.target.value)?.[0];
            updateSettings({ [entry.key]: raw } as Partial<Settings>);
          }}
        >
          {entry.options.map(([v, label]) => (
            <option key={String(v)} value={String(v)}>
              {label}
            </option>
          ))}
        </select>
      );
    case "toggle":
      return (
        <label className="switch">
          <input
            type="checkbox"
            checked={Boolean(value)}
            onChange={(e) => updateSettings({ [entry.key]: e.target.checked } as Partial<Settings>)}
          />
          <span>{value ? "On" : "Off"}</span>
        </label>
      );
    case "themes":
      return null; // drawn full width by ThemePicker
  }
}
