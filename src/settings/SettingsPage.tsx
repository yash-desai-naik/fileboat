import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { fmtShortcut, OS_NAME } from "../lib/platform";
import { updateSettings, useSettings, type Settings } from "../lib/settings";
import { shownMode, usePrefersDark } from "../lib/themeMode";
import { Dialog } from "../shell/Dialog";
import { useAppVersion, versionLine } from "../shell/AboutDialog";
import { uiActions } from "../lib/ui";
import type { ThemeMode } from "../themes";
import { ThemePicker } from "./ThemePicker";
import { IndexSettings } from "./IndexSettings";
import { loadTools, type Tool } from "../lib/tools";

type Def =
  | { key: keyof Settings; type: "select"; options: [string | number, string][] }
  | { key: keyof Settings; type: "toggle" }
  | { key: "editor" | "terminal"; type: "tool" }
  | { key: "lightTheme" | "darkTheme"; type: "themes"; mode: ThemeMode }
  | { key: "indexRoots"; type: "index" };

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

  {
    section: "Folders",
    key: "viewMode",
    type: "select",
    label: "Default view",
    help: `${fmtShortcut("Ctrl+Shift+1")} Columns, ${fmtShortcut("Ctrl+Shift+2")} List, ${fmtShortcut("Ctrl+Shift+3")} Details`,
    options: [
      ["details", "Details"],
      ["list", "List"],
      ["columns", "Columns"],
    ],
  },
  {
    section: "Folders",
    key: "sortBy",
    type: "select",
    label: "Default sort",
    help: "Or click a column header in the Details view.",
    options: [
      ["name", "Name"],
      ["modified", "Date modified"],
      ["created", "Date created"],
      ["type", "Type"],
      ["size", "Size"],
    ],
  },
  { section: "Folders", key: "sortDescending", type: "toggle", label: "Descending order" },
  {
    section: "Folders",
    key: "groupBy",
    type: "select",
    label: "Default grouping",
    help: "Rows under headers you can collapse.",
    options: [
      ["none", "None"],
      ["type", "Type"],
      ["modified", "Date modified"],
      ["size", "Size"],
      ["letter", "First letter"],
      ["git", "Git status"],
    ],
  },
  {
    section: "Folders",
    key: "rememberViews",
    type: "toggle",
    label: "Remember each folder's view",
    help: "A folder keeps the view, sort and grouping you choose in it. Off: one view for all folders.",
  },
  { section: "Folders", key: "foldersFirst", type: "toggle", label: "Folders before files" },
  {
    section: "Folders",
    key: "gitIgnored",
    type: "select",
    label: "Files git ignores",
    help: "Inside a repository: build output, node_modules and the like.",
    options: [
      ["dim", "Dimmed"],
      ["hide", "Hidden"],
      ["show", "Shown like the rest"],
    ],
  },
  {
    section: "Folders",
    key: "folderSizes",
    type: "toggle",
    label: "Show folder sizes",
    help: "Worked out in the background. Links and cloud-only folders aren't followed. Or run Calculate Folder Sizes Here on one folder.",
  },
  {
    section: "Folders",
    key: "showHidden",
    type: "toggle",
    label: "Show hidden files",
    help: `Files the system marks hidden. ${fmtShortcut("Ctrl+Shift+.")}`,
  },

  {
    section: "Tools",
    key: "editor",
    type: "tool",
    label: "Editor",
    help: `For "Open in editor" (${fmtShortcut("Ctrl+Shift+E")}). Editors installed on this computer.`,
  },
  {
    section: "Tools",
    key: "terminal",
    type: "tool",
    label: "Terminal",
    help: `For "Terminal here" (${fmtShortcut("Ctrl+`")}).`,
  },

  {
    section: "Search index",
    key: "indexEnabled",
    type: "toggle",
    label: "Keep a search index",
    help: `Names and dates only, never contents. Search everywhere (${fmtShortcut("Ctrl+Shift+F")}) and Go to use it.`,
  },
  { section: "Search index", key: "indexGitignore", type: "toggle", label: "Honour .gitignore inside projects", help: "Ignored build output is still found when you search that folder directly." },
  { section: "Search index", key: "indexHidden", type: "toggle", label: "Index hidden files and folders", help: "The ones the system marks hidden (dotfiles are indexed either way)." },
  { section: "Search index", key: "indexRoots", type: "index", label: "Folders, exclusions and rules", help: "indexRoots indexExcludes indexIncludes node_modules rules" },

  {
    section: "Claude",
    key: "claudeModel",
    type: "select",
    label: "Model",
    help: "The model your Claude Code uses in the pane (Ctrl+Alt+B). Your own login and plan; Fileboat sends nothing anywhere itself.",
    options: [
      ["", "Claude Code's default"],
      ["opus", "Opus"],
      ["sonnet", "Sonnet"],
      ["haiku", "Haiku"],
    ],
  },
  { section: "Layout", key: "showClaude", type: "toggle", label: "Show the Claude pane", help: fmtShortcut("Ctrl+Alt+B") },

  { section: "Layout", key: "showSidebar", type: "toggle", label: "Show the sidebar", help: fmtShortcut("Ctrl+B") },
  { section: "Layout", key: "showPreview", type: "toggle", label: "Show the preview pane", help: fmtShortcut("Alt+P") },

  {
    section: "Start-up",
    key: "checkUpdates",
    type: "toggle",
    label: "Check for updates automatically",
    help: "At start-up and once a day, Fileboat asks GitHub for its latest version (nothing about you is sent). You decide when to install.",
  },
];

/** Ctrl+, : every setting, searchable. Changes apply and save immediately. */
export function SettingsPage({ onClose }: { onClose: () => void }) {
  const settings = useSettings();
  const [query, setQuery] = useState("");
  const [fileNote, setFileNote] = useState<string | null>(null);
  const version = useAppVersion();
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
          <button
            className="link-btn settings-version"
            title="About Fileboat: version, build, folders"
            onClick={() => {
              onClose();
              uiActions.openAbout();
            }}
          >
            {version ? versionLine(version) : "Fileboat"}
          </button>
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
              e.type === "index" ? (
                <IndexSettings key={e.key} settings={settings} />
              ) : e.type === "themes" ? (
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
    case "tool":
      return <ToolSelect which={entry.key} value={String(value)} />;
    case "themes":
    case "index":
      return null; // drawn full width
  }
}

/** The editors or terminals found on this computer, with "the first found" as the default. */
function ToolSelect({ which, value }: { which: "editor" | "terminal"; value: string }) {
  const [tools, setTools] = useState<Tool[] | null>(null);
  useEffect(() => {
    void loadTools().then((t) => setTools(which === "editor" ? t.editors : t.terminals));
  }, [which]);
  const first = tools?.[0]?.name;
  return (
    <select className="input" value={value} onChange={(e) => updateSettings({ [which]: e.target.value } as Partial<Settings>)}>
      <option value="">{first ? `Automatic (${first})` : "Automatic"}</option>
      {tools?.map((t) => (
        <option key={t.id} value={t.id}>
          {t.name}
        </option>
      ))}
      {value && tools && !tools.some((t) => t.id === value) && <option value={value}>{value} (not found)</option>}
    </select>
  );
}
