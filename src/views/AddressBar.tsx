import { invoke } from "@tauri-apps/api/core";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { uiActions } from "../lib/ui";
import { ChevronIcon } from "../shell/Icons";
import { Menu, type MenuItem } from "../shell/Menu";

interface Segment {
  label: string;
  path: string;
}

/** "C:\Users\me" → C: › Users › me; "\\server\share\x" and "/Users/me" work too. */
export function splitPath(path: string): Segment[] {
  const win = path.includes("\\") || /^[A-Za-z]:/.test(path);
  const sep = win ? "\\" : "/";
  let root: Segment;
  let rest: string;
  const unc = /^\\\\[^\\]+\\[^\\]+/.exec(path);
  const drive = /^[A-Za-z]:\\?/.exec(path);
  if (unc) {
    root = { label: unc[0], path: unc[0] + "\\" };
    rest = path.slice(unc[0].length);
  } else if (drive) {
    root = { label: drive[0].slice(0, 2), path: drive[0].slice(0, 2) + "\\" };
    rest = path.slice(drive[0].length);
  } else {
    root = { label: "/", path: "/" };
    rest = path.replace(/^\/+/, "");
  }
  const out = [root];
  let acc = root.path;
  for (const part of rest.split(/[\\/]/).filter(Boolean)) {
    acc = acc.endsWith(sep) ? acc + part : acc + sep + part;
    out.push({ label: part, path: acc });
  }
  return out;
}

interface Resolved {
  path: string;
  kind: "folder" | "file" | "missing";
  parent: string | null;
  name: string | null;
}

interface Props {
  path: string;
  /** Go to a folder, selecting `select` in it. */
  onNavigate: (path: string, select?: string | null) => void;
}

/**
 * The folder's path as a breadcrumb: click a part to go there, its arrow lists the folders
 * beside it. Click empty space (or Ctrl+L) to type a path, with completion: Tab completes,
 * Enter goes there (a file opens its folder with the file selected), Esc cancels.
 */
export function AddressBar({ path, onNavigate }: Props) {
  const [editing, setEditing] = useState(false);
  const [text, setText] = useState(path);
  const [suggestions, setSuggestions] = useState<string[]>([]);
  const [active, setActive] = useState(-1);
  const [error, setError] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const crumbsRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const startEdit = () => {
    setText(path);
    setError(null);
    setSuggestions([]);
    setActive(-1);
    setEditing(true);
  };
  uiActions.focusAddress = startEdit;

  useEffect(() => {
    if (editing) inputRef.current?.select();
  }, [editing]);

  // A long path shows its end (the folder you're in), not its start.
  useLayoutEffect(() => {
    const el = crumbsRef.current;
    if (el) el.scrollLeft = el.scrollWidth;
  }, [path, editing]);

  // Suggestions follow the typing (after a short pause).
  useEffect(() => {
    if (!editing || text === path) {
      setSuggestions([]);
      return;
    }
    const timer = window.setTimeout(() => {
      void invoke<string[]>("path_complete", { input: text, base: path, limit: 12 })
        .then((list) => {
          setSuggestions(list);
          setActive(-1);
        })
        .catch(() => setSuggestions([]));
    }, 80);
    return () => window.clearTimeout(timer);
  }, [text, editing, path]);

  const finish = () => {
    setEditing(false);
    setSuggestions([]);
    setError(null);
    uiActions.focusList();
  };

  const go = async (input: string) => {
    const r = await invoke<Resolved>("path_resolve", { input, base: path });
    if (r.kind === "folder") {
      onNavigate(r.path);
      finish();
    } else if (r.kind === "file" && r.parent) {
      onNavigate(r.parent, r.name);
      finish();
    } else {
      setError(`${r.path} doesn't exist.`);
    }
  };

  const sepOf = (p: string) => (p.includes("\\") ? "\\" : "/");

  const openSiblings = async (segment: Segment, anchor: HTMLElement) => {
    const r = anchor.getBoundingClientRect();
    const sep = sepOf(segment.path);
    const list = await invoke<string[]>("path_complete", {
      input: segment.path.endsWith(sep) ? segment.path : segment.path + sep,
      limit: 500,
    }).catch(() => [] as string[]);
    const items: MenuItem[] = list.length
      ? list.map((p) => ({ label: p.slice(p.lastIndexOf(sep) + 1), onSelect: () => onNavigate(p) }))
      : [{ label: "No folders", onSelect: () => {}, disabled: true }];
    setMenu({ x: r.left, y: r.bottom + 2, items });
  };

  if (editing) {
    return (
      <div className="address editing">
        <input
          ref={inputRef}
          className="address-input mono"
          value={text}
          spellCheck={false}
          aria-label="Folder path"
          onChange={(e) => {
            setText(e.target.value);
            setError(null);
          }}
          onBlur={() => window.setTimeout(() => setEditing(false), 120)}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              e.preventDefault();
              finish();
            } else if (e.key === "Enter") {
              e.preventDefault();
              void go(active >= 0 ? suggestions[active] : text);
            } else if ((e.key === "Tab" || e.key === "ArrowRight") && suggestions.length > 0 && (e.key === "Tab" || active >= 0)) {
              // Tab (or → on a highlighted suggestion) completes it, ready for the next part.
              e.preventDefault();
              const pick = suggestions[Math.max(active, 0)];
              setText(pick + sepOf(pick));
            } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
              e.preventDefault();
              const n = suggestions.length;
              if (n) setActive((a) => (e.key === "ArrowDown" ? (a + 1) % n : (a - 1 + n) % n));
            }
          }}
        />
        {(suggestions.length > 0 || error) && (
          <div className="address-suggest" role="listbox">
            {error && <div className="address-error">{error}</div>}
            {suggestions.map((s, i) => (
              <div
                key={s}
                role="option"
                aria-selected={i === active}
                className={`address-option mono${i === active ? " active" : ""}`}
                onMouseDown={(e) => {
                  e.preventDefault();
                  void go(s);
                }}
              >
                {s}
              </div>
            ))}
          </div>
        )}
      </div>
    );
  }

  const segments = splitPath(path);
  return (
    <div
      className="address"
      title="Click to type a path (Ctrl+L)"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget || e.target === crumbsRef.current) {
          e.preventDefault();
          startEdit();
        }
      }}
    >
      <div className="crumbs" ref={crumbsRef}>
        {segments.map((s, i) => (
          <span key={s.path} className="crumb-part">
            <button className={`crumb${i === segments.length - 1 ? " current" : ""}`} onClick={() => onNavigate(s.path)} title={s.path}>
              {s.label}
            </button>
            <button className="crumb-sep" title={`Folders in ${s.label}`} onClick={(e) => void openSiblings(s, e.currentTarget)}>
              <ChevronIcon open={false} size={10} />
            </button>
          </span>
        ))}
      </div>
      {menu && <Menu x={menu.x} y={menu.y} items={menu.items} onClose={() => setMenu(null)} />}
    </div>
  );
}
