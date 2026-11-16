import { invoke } from "@tauri-apps/api/core";
import { useEffect, useMemo, useState } from "react";
import { getSettings, updateSettings, useSettings } from "../lib/settings";
import { Dialog } from "../shell/Dialog";
import { FileGlyph, FolderGlyph, fileLook, folderLook, type FolderMark, type Mark } from "./builtin";
import { EntryIcon } from "./EntryIcon";
import { customImage, themeIconUrl, useIconsVersion, vsTheme, type IconRule } from "./icons";

/** What Change icon… was opened for. */
export interface IconTarget {
  path: string;
  name: string;
  ext: string;
  folder: boolean;
}

const COLORS = ["#e5534b", "#e0823d", "#d4a917", "#5aa64b", "#3fa38b", "#3fb3d6", "#4a8fd8", "#7b5fc0", "#c6538c", "#8a8f98"];
const FILE_MARKS = ["none", "code", "braces", "lines", "table", "image", "play", "zip", "window", "font", "gear", "key", "lock", "info", "check", "dollar", "git", "pkg", "db"] as const;
const FOLDER_MARKS = ["none", "code", "check", "lines", "git", "pkg", "image", "dollar", "box", "gear", "blocks", "globe"] as const;

/** Right-click → Change Icon…: an icon or colour for this item, its name, or its extension. */
export function ChangeIconDialog({ target, onClose }: { target: IconTarget; onClose: () => void }) {
  const settings = useSettings();
  useIconsVersion();
  const [scope, setScope] = useState<IconRule["match"]>(target.folder ? "path" : "name");
  const value = scope === "path" ? target.path : scope === "name" ? target.name : target.ext;
  const existing = settings.iconAssociations.find((r) => r.match === scope && r.value.toLowerCase() === value.toLowerCase());
  const [color, setColor] = useState<string | undefined>(existing?.color);
  const [icon, setIcon] = useState<string | undefined>(existing?.icon);
  const [picture, setPicture] = useState(existing?.icon?.startsWith("file:") ? existing.icon.slice(5) : "");
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState("");

  // The rule for another scope, when switching.
  useEffect(() => {
    const r = getSettings().iconAssociations.find((x) => x.match === scope && x.value.toLowerCase() === value.toLowerCase());
    setColor(r?.color);
    setIcon(r?.icon);
    setPicture(r?.icon?.startsWith("file:") ? r.icon.slice(5) : "");
  }, [scope, value]);

  const themeId = settings.iconTheme;
  const vs = themeId.startsWith("vscode:") ? vsTheme(themeId.slice(7)) : null;
  const vsIcons = useMemo(() => {
    if (!vs) return [];
    const q = filter.trim().toLowerCase();
    return Object.keys(vs.defs)
      .filter((id) => !q || id.toLowerCase().includes(q))
      .slice(0, 160);
  }, [vs, filter]);

  const save = async () => {
    let chosen = icon;
    if (picture.trim()) {
      try {
        await invoke("icon_image", { path: picture.trim() });
        chosen = `file:${picture.trim()}`;
      } catch (err) {
        return setError(String(err));
      }
    } else if (chosen?.startsWith("file:")) chosen = undefined;
    const rest = getSettings().iconAssociations.filter((r) => !(r.match === scope && r.value.toLowerCase() === value.toLowerCase()));
    const rule: IconRule = { match: scope, value, ...(chosen ? { icon: chosen } : {}), ...(color ? { color } : {}) };
    updateSettings({ iconAssociations: chosen || color ? [rule, ...rest] : rest });
    onClose();
  };

  const reset = () => {
    updateSettings({ iconAssociations: getSettings().iconAssociations.filter((r) => !(r.match === scope && r.value.toLowerCase() === value.toLowerCase())) });
    onClose();
  };

  const previewRule: IconRule = { match: "path", value: target.path, icon: picture.trim() ? `file:${picture.trim()}` : icon, color };
  const previewPicture = previewRule.icon?.startsWith("file:") ? customImage(previewRule.icon.slice(5)) : null;
  const base = target.folder ? folderLook(target.name) : fileLook(target.name, target.ext);
  // The mark shown: the chosen one (not a picture), else the theme's.
  const mark = icon && !icon.startsWith("file:") ? icon : base.mark;

  return (
    <Dialog
      title={`Change icon: ${target.name}`}
      onClose={onClose}
      width={560}
      footer={
        <>
          {existing && (
            <button className="link-btn" onClick={reset}>
              Back to the theme's icon
            </button>
          )}
          {error && <span className="error small">{error}</span>}
          <span className="spacer" />
          <button className="btn" onClick={onClose}>
            Cancel
          </button>
          <button className="btn primary" onClick={() => void save()}>
            Save
          </button>
        </>
      }
    >
      <div className="change-icon">
        <div className="ci-preview">
          {previewPicture ? (
            <img src={previewPicture} width={32} height={32} alt="" />
          ) : vs && icon && vs.defs[icon] ? (
            <img src={themeIconUrl(vs, icon) ?? ""} width={32} height={32} alt="" />
          ) : target.folder ? (
            <FolderGlyph look={{ color: color ?? base.color, mark: mark as FolderMark }} size={32} />
          ) : (
            <FileGlyph look={{ color: color ?? base.color, mark: mark as Mark }} size={32} />
          )}
          <span className="muted small">Now: </span>
          <EntryIcon name={target.name} ext={target.ext} folder={target.folder} path={target.path} size={20} />
        </div>

        <div className="setting-sub">For</div>
        <div className="ci-scope" role="radiogroup">
          <label>
            <input type="radio" checked={scope === "path"} onChange={() => setScope("path")} /> Just this {target.folder ? "folder" : "file"}
          </label>
          <label>
            <input type="radio" checked={scope === "name"} onChange={() => setScope("name")} /> Everything named “{target.name}”
          </label>
          {!target.folder && target.ext && (
            <label>
              <input type="radio" checked={scope === "ext"} onChange={() => setScope("ext")} /> Every .{target.ext} file
            </label>
          )}
        </div>

        <div className="setting-sub">Colour</div>
        <div className="ci-colors">
          <button className={`ci-color default${!color ? " on" : ""}`} title="The theme's colour" onClick={() => setColor(undefined)}>
            ⌀
          </button>
          {COLORS.map((c) => (
            <button key={c} className={`ci-color${color === c ? " on" : ""}`} style={{ background: c }} title={c} onClick={() => setColor(c)} />
          ))}
        </div>

        <div className="setting-sub">Icon</div>
        {vs ? (
          <>
            <input className="input" placeholder={`Search ${Object.keys(vs.defs).length} icons in this theme`} value={filter} onChange={(e) => setFilter(e.target.value)} />
            <div className="ci-grid">
              <button className={`ci-icon${!icon ? " on" : ""}`} title="The theme's choice" onClick={() => setIcon(undefined)}>
                ⌀
              </button>
              {vsIcons.map((id) => (
                <button key={id} className={`ci-icon${icon === id ? " on" : ""}`} title={id} onClick={() => setIcon(id)}>
                  <img src={themeIconUrl(vs, id) ?? ""} width={18} height={18} alt="" loading="lazy" />
                </button>
              ))}
            </div>
          </>
        ) : (
          <div className="ci-grid">
            {(target.folder ? FOLDER_MARKS : FILE_MARKS).map((m) => (
              <button key={m} className={`ci-icon${mark === m ? " on" : ""}`} title={m} onClick={() => setIcon(m)}>
                {target.folder ? <FolderGlyph look={{ color: color ?? base.color, mark: m as FolderMark }} size={18} /> : <FileGlyph look={{ color: color ?? base.color, mark: m as Mark }} size={18} />}
              </button>
            ))}
          </div>
        )}

        <div className="setting-sub">Or a picture</div>
        <input className="input" placeholder="Full path of an SVG, PNG or ICO (up to 256 KB)" value={picture} onChange={(e) => setPicture(e.target.value)} spellCheck={false} />
      </div>
    </Dialog>
  );
}
