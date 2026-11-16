import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { formatSize } from "../lib/format";
import { useIndexStatus } from "../lib/search";
import { updateSettings, type Settings } from "../lib/settings";
import { activeTab } from "../lib/tabs";

interface RuleInfo {
  id: string;
  group: string;
  note: string;
}

const PHASE: Record<string, string> = {
  off: "Off",
  loading: "Loading…",
  building: "Building…",
  reconciling: "Catching up with changes…",
  ready: "Up to date",
  paused: "Paused",
};

/** A list of strings with add and remove (folders to index, extra excludes…). */
function ListEditor({ items, onChange, placeholder, suggest }: { items: string[]; onChange: (items: string[]) => void; placeholder: string; suggest?: { label: string; value: () => string } }) {
  const [draft, setDraft] = useState("");
  const add = (v: string) => {
    const t = v.trim();
    if (t && !items.some((i) => i.toLowerCase() === t.toLowerCase())) onChange([...items, t]);
    setDraft("");
  };
  return (
    <div className="list-editor">
      {items.map((i) => (
        <div key={i} className="list-editor-row">
          <span className="mono" title={i}>
            {i}
          </span>
          <button className="link-btn" onClick={() => onChange(items.filter((x) => x !== i))}>
            Remove
          </button>
        </div>
      ))}
      <form
        className="list-editor-add"
        onSubmit={(e) => {
          e.preventDefault();
          add(draft);
        }}
      >
        <input className="input" value={draft} placeholder={placeholder} onChange={(e) => setDraft(e.target.value)} spellCheck={false} />
        <button className="btn" type="submit" disabled={!draft.trim()}>
          Add
        </button>
        {suggest && (
          <button className="link-btn" type="button" onClick={() => add(suggest.value())}>
            {suggest.label}
          </button>
        )}
      </form>
    </div>
  );
}

/** Settings ▸ Search index: its state, the folders it covers, and what it leaves out. */
export function IndexSettings({ settings }: { settings: Settings }) {
  const status = useIndexStatus();
  const [rules, setRules] = useState<RuleInfo[]>([]);
  useEffect(() => void invoke<RuleInfo[]>("index_rules").then(setRules), []);
  const off = new Set(settings.indexRulesOff.map((r) => r.toLowerCase()));
  const groups = [...new Set(rules.map((r) => r.group))];
  const toggleRule = (id: string, on: boolean) =>
    updateSettings({ indexRulesOff: on ? settings.indexRulesOff.filter((r) => r.toLowerCase() !== id.toLowerCase()) : [...settings.indexRulesOff, id] });

  return (
    <div className="index-settings">
      {status && (
        <div className="index-status">
          <span>
            <b>{PHASE[status.phase] ?? status.phase}</b>
            {status.phase === "building" ? ` ${status.progress.toLocaleString()} so far` : ` · ${status.entries.toLocaleString()} files and folders · ${formatSize(status.bytes)} in memory`}
          </span>
          <span className="spacer" />
          <button className="link-btn" onClick={() => void invoke("index_pause", { paused: status.phase !== "paused" })} disabled={status.phase === "off"}>
            {status.phase === "paused" ? "Resume" : "Pause"}
          </button>
          <button className="link-btn" onClick={() => void invoke("index_rebuild")} disabled={status.phase === "off"}>
            Rebuild
          </button>
        </div>
      )}

      <div className="setting-sub">Folders to index</div>
      <p className="muted small">Your home folder when this list is empty. Drives and network folders only when you add them.</p>
      <ListEditor
        // The folders actually indexed: with none chosen that's the home folder, shown (and
        // kept) explicitly once the list is edited.
        items={settings.indexRoots.length ? settings.indexRoots : (status?.roots ?? [])}
        onChange={(indexRoots) => updateSettings({ indexRoots })}
        placeholder="A folder, e.g. D:\projects"
        suggest={{ label: "Add this folder", value: () => activeTab().path }}
      />

      <div className="setting-sub">Also leave out</div>
      <p className="muted small">Folder or file names (anywhere), or full paths.</p>
      <ListEditor items={settings.indexExcludes} onChange={(indexExcludes) => updateSettings({ indexExcludes })} placeholder="e.g. photos, or C:\big\archive" />

      <div className="setting-sub">Index anyway</div>
      <p className="muted small">Paths indexed even where a rule below would leave them out.</p>
      <ListEditor items={settings.indexIncludes} onChange={(indexIncludes) => updateSettings({ indexIncludes })} placeholder="A full path" />

      <div className="setting-sub">Left out by default</div>
      <p className="muted small">What a developer rarely searches. Untick a rule to index it.</p>
      {groups.map((g) => (
        <div key={g} className="rule-group">
          <div className="muted small">{g}</div>
          <div className="rule-list">
            {rules
              .filter((r) => r.group === g)
              .map((r) => (
                <label key={r.id} className="rule" title={r.note || undefined}>
                  <input type="checkbox" checked={!off.has(r.id.toLowerCase())} onChange={(e) => toggleRule(r.id, e.target.checked)} />
                  <span className="mono">{r.id === "system" ? "System folders" : r.id === "system-files" ? "System files" : r.id}</span>
                  {r.note && <span className="muted small">{r.note}</span>}
                </label>
              ))}
          </div>
        </div>
      ))}
    </div>
  );
}
