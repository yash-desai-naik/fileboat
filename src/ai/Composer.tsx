import { useEffect, useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { IS_MAC } from "../lib/platform";
import { getCurrent } from "../lib/current";
import { onQueued, takeQueued } from "../lib/claude";
import { activeTab } from "../lib/tabs";
import { attachPath, imageFromBlob, mentionKey, pathMention, suggest, type Mention, type Suggestion } from "./mentions";

interface Props {
  inputRef: RefObject<HTMLTextAreaElement | null>;
  disabled: boolean;
  placeholder: string;
  running: boolean;
  /** Left side of the bar under the input (connection and model chips). */
  chips: ReactNode;
  onSend: (text: string, mentions: Mention[]) => void;
  onStop: () => void;
}

/** Where an "@…" being typed starts, and what follows the "@". */
function triggerAt(text: string, caret: number): { start: number; query: string } | null {
  const m = /(^|[\s(,])@([^\s@]*)$/.exec(text.slice(0, caret));
  return m ? { start: caret - m[2].length - 1, query: m[2] } : null;
}

/**
 * The chat input: "@" adds files and folders (the selection, this folder, the index), images can
 * be pasted or dropped, files dropped from Fileboat or elsewhere are added, and on Windows the
 * mic starts voice typing.
 */
export function Composer({ inputRef, disabled, placeholder, running, chips, onSend, onStop }: Props) {
  const [draft, setDraft] = useState("");
  const [mentions, setMentions] = useState<Mention[]>([]);
  const [popup, setPopup] = useState<{ start: number; query: string } | null>(null);
  const [items, setItems] = useState<Suggestion[]>([]);
  const [index, setIndex] = useState(0);
  const [note, setNote] = useState<string | null>(null);
  const [dropping, setDropping] = useState(false);
  const boxRef = useRef<HTMLDivElement>(null);
  const seq = useRef(0);

  // The input grows with its text, up to a limit.
  useLayoutEffect(() => {
    const el = inputRef.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 220)}px`;
  }, [draft, inputRef]);

  // Suggestions for what follows "@" (latest request wins).
  useEffect(() => {
    if (!popup) return;
    const n = ++seq.current;
    // No stale rows while the new ones load: Enter can't pick something from the old query.
    setItems([]);
    const t = window.setTimeout(() => {
      void suggest(popup.query).then((list) => {
        if (n !== seq.current) return;
        setItems(list);
        setIndex(0);
      });
    }, 80);
    return () => window.clearTimeout(t);
  }, [popup?.query, popup]);

  // Files dropped on the pane are attached (Tauri reports drops with their paths).
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void getCurrentWebview()
      .onDragDropEvent((e) => {
        const box = boxRef.current?.closest(".ai")?.getBoundingClientRect();
        if (!box) return;
        const p = e.payload;
        const inside = (pos: { x: number; y: number }) => {
          const x = pos.x / devicePixelRatio;
          const y = pos.y / devicePixelRatio;
          return x >= box.left && x <= box.right && y >= box.top && y <= box.bottom;
        };
        if (p.type === "over" || p.type === "enter") setDropping(inside(p.position));
        else if (p.type === "leave") setDropping(false);
        else if (p.type === "drop") {
          setDropping(false);
          if (inside(p.position)) for (const path of p.paths) void attach(path);
        }
      })
      .then((fn) => (unlisten = fn));
    return () => unlisten?.();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // "Ask Claude" on items: they arrive as mentions, with the keyboard here.
  useEffect(() => {
    const take = () => {
      const list = takeQueued();
      if (!list.length) return;
      for (const q of list) add(pathMention(q.path, q.folder));
      inputRef.current?.focus();
    };
    take();
    return onQueued(take);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  /** The selected items (or the folder) as mentions. */
  function addSelection() {
    const cur = getCurrent();
    const folder = activeTab().path;
    const sep = folder.includes("\\") ? "\\" : "/";
    const names = cur?.names ?? [];
    if (!names.length) add(pathMention(folder, true));
    for (const n of names.slice(0, 50)) add(pathMention(folder.replace(/[\\/]+$/, "") + sep + n, cur?.entry?.name === n ? cur.entry.kind === "folder" : false));
    inputRef.current?.focus();
  }

  function flash(text: string) {
    setNote(text);
    window.setTimeout(() => setNote((n) => (n === text ? null : n)), 4000);
  }

  function add(m: Mention) {
    setMentions((list) => (list.some((x) => mentionKey(x) === mentionKey(m)) ? list : [...list, m]));
  }

  async function attach(path: string) {
    try {
      add(await attachPath(path));
    } catch (err) {
      flash(String(err));
    }
  }

  function updateTrigger(text: string, caret: number) {
    const t = triggerAt(text, caret);
    setPopup((p) => (t ? (p && p.start === t.start && p.query === t.query ? p : t) : null));
  }

  /** Replace the "@query" being typed (with the mention's name, or nothing for an action). */
  function replaceTrigger(insert: string) {
    const el = inputRef.current;
    if (!el || !popup) return;
    const caret = el.selectionStart ?? draft.length;
    const next = draft.slice(0, popup.start) + insert + draft.slice(caret);
    setDraft(next);
    setPopup(null);
    const pos = popup.start + insert.length;
    requestAnimationFrame(() => {
      el.focus();
      el.setSelectionRange(pos, pos);
    });
  }

  function pick(s: Suggestion) {
    add(s.mention);
    replaceTrigger(`@${s.mention.label} `);
  }

  function submit() {
    const text = draft.trim();
    if ((!text && !mentions.length) || running) return;
    onSend(text || "(see the attached context)", mentions);
    setDraft("");
    setMentions([]);
    setPopup(null);
  }

  const groups: { group: string; rows: { s: Suggestion; i: number }[] }[] = [];
  items.forEach((s, i) => {
    const g = groups.find((x) => x.group === s.group);
    if (g) g.rows.push({ s, i });
    else groups.push({ group: s.group, rows: [{ s, i }] });
  });

  return (
    <div className="ai-compose" ref={boxRef}>
      {popup && items.length > 0 && (
        <div className="ai-mention-pop" role="listbox" onMouseDown={(e) => e.preventDefault()}>
          {groups.map((g) => (
            <div key={g.group}>
              <div className="ai-mention-group">{g.group}</div>
              {g.rows.map(({ s, i }) => (
                <div
                  key={s.key}
                  role="option"
                  aria-selected={i === index}
                  className={i === index ? "ai-mention-row active" : "ai-mention-row"}
                  onMouseEnter={() => setIndex(i)}
                  onClick={() => void pick(s)}
                >
                  <span className={`ai-mention-icon k-${s.mention.kind}`}>{iconFor(s.mention)}</span>
                  <span className="ai-mention-label">{s.label}</span>
                  {s.detail && <span className="ai-mention-detail">{s.detail}</span>}
                </div>
              ))}
            </div>
          ))}
          <div className="ai-mention-hint">↑↓ choose · Enter add · Esc close</div>
        </div>
      )}

      <div
        className={["ai-composer", dropping ? "dropping" : ""].join(" ")}
        onClick={(e) => {
          if (e.target === e.currentTarget) inputRef.current?.focus();
        }}
      >
        {mentions.length > 0 && (
          <div className="ai-attachments">
            {mentions.map((m) => (
              <span key={mentionKey(m)} className={`ai-att k-${m.kind}`} title={titleFor(m)}>
                <span className="ai-att-icon">{iconFor(m)}</span>
                <span className="ai-att-label">{m.label}</span>
                <button
                  className="ai-att-remove"
                  title="Remove"
                  onClick={() => setMentions((list) => list.filter((x) => mentionKey(x) !== mentionKey(m)))}
                >
                  ×
                </button>
              </span>
            ))}
          </div>
        )}
        <textarea
          ref={inputRef}
          className="ai-input"
          rows={1}
          placeholder={placeholder}
          value={draft}
          disabled={disabled}
          onChange={(e) => {
            setDraft(e.target.value);
            updateTrigger(e.target.value, e.target.selectionStart ?? e.target.value.length);
          }}
          onSelect={(e) => updateTrigger(e.currentTarget.value, e.currentTarget.selectionStart ?? 0)}
          onBlur={() => window.setTimeout(() => setPopup(null), 150)}
          onPaste={(e) => {
            const files = [...e.clipboardData.files].filter((f) => f.type.startsWith("image/"));
            if (!files.length) return;
            e.preventDefault();
            for (const f of files)
              void imageFromBlob(f).then((m) => (m ? add(m) : flash("Only PNG, JPEG, GIF or WebP images up to 5 MB can be attached.")));
          }}
          onKeyDown={(e) => {
            // Typing an "@…" with nothing (yet) to pick: Enter closes the list instead of sending,
            // so a half-typed mention never goes out by accident. A second Enter sends.
            if (popup && !items.length && e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              setPopup(null);
              return;
            }
            if (popup && items.length) {
              if (e.key === "ArrowDown" || e.key === "ArrowUp") {
                e.preventDefault();
                setIndex((i) => (i + (e.key === "ArrowDown" ? 1 : items.length - 1)) % items.length);
                return;
              }
              if ((e.key === "Enter" || e.key === "Tab") && !e.shiftKey) {
                e.preventDefault();
                void pick(items[index]);
                return;
              }
              if (e.key === "Escape") {
                e.preventDefault();
                e.stopPropagation();
                setPopup(null);
                return;
              }
            }
            if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault();
              submit();
            }
          }}
        />
        {note && <div className="ai-compose-note">{note}</div>}
        <div className="ai-composer-bar">
          <button
            className="icon-btn small ai-bar-btn"
            title="Add files or folders (@)"
            onClick={() => {
              const el = inputRef.current;
              if (!el) return;
              const caret = el.selectionStart ?? draft.length;
              const before = draft.slice(0, caret);
              const insert = before && !/\s$/.test(before) ? " @" : "@";
              const next = before + insert + draft.slice(caret);
              setDraft(next);
              const pos = caret + insert.length;
              requestAnimationFrame(() => {
                el.focus();
                el.setSelectionRange(pos, pos);
                updateTrigger(next, pos);
              });
            }}
          >
            <AtIcon />
          </button>
          <button className="icon-btn small ai-bar-btn" title="Add the selected items (or this folder)" onClick={addSelection}>
            <PaperclipIcon />
          </button>
          {/* Windows voice typing (Win+H) can be started from here; macOS dictation has no such call. */}
          {!IS_MAC && (
            <button
              className="icon-btn small ai-bar-btn"
              title="Voice typing (Win+H). Speak, and the words go into the input."
              disabled={disabled}
              onClick={() => {
                inputRef.current?.focus();
                window.setTimeout(() => void invoke("voice_typing").catch((err) => flash(String(err))), 80);
              }}
            >
              <MicIcon />
            </button>
          )}
          {chips}
          {running ? (
            <button
              className="ai-send stop"
              title="Stop (Esc)"
              onClick={(e) => {
                e.stopPropagation();
                onStop();
              }}
            >
              <StopIcon />
            </button>
          ) : (
            <button
              className="ai-send"
              title="Send (Enter)"
              disabled={disabled || (!draft.trim() && !mentions.length)}
              onClick={(e) => {
                e.stopPropagation();
                submit();
              }}
            >
              <ArrowUpIcon />
            </button>
          )}
        </div>
        {dropping && <div className="ai-drop-hint">Drop files to add them (images are sent as images)</div>}
      </div>
    </div>
  );
}

function titleFor(m: Mention): string {
  return m.kind === "image" ? `${m.label} (${m.mediaType})` : m.path;
}

function iconFor(m: Mention): ReactNode {
  const p = { width: 13, height: 13, viewBox: "0 0 16 16", fill: "none", stroke: "currentColor", strokeWidth: 1.5, strokeLinecap: "round" as const, strokeLinejoin: "round" as const };
  switch (m.kind) {
    case "folder":
      return (
        <svg {...p}>
          <path d="M2 4.5A1.5 1.5 0 0 1 3.5 3h3l1.5 1.5h4.5A1.5 1.5 0 0 1 14 6v5.5a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 11.5z" />
        </svg>
      );
    case "image":
      return (
        <svg {...p}>
          <rect x="2" y="3" width="12" height="10" rx="1.5" />
          <circle cx="6" cy="6.5" r="1.2" />
          <path d="m2.5 12 3.5-3.5 3 3 2-2 2.5 2.5" />
        </svg>
      );
    default:
      return (
        <svg {...p}>
          <path d="M9 2H4.5A1.5 1.5 0 0 0 3 3.5v9A1.5 1.5 0 0 0 4.5 14h7a1.5 1.5 0 0 0 1.5-1.5V6z M9 2v4h4" />
        </svg>
      );
  }
}

const AtIcon = () => (
  <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden="true">
    <circle cx="8" cy="8" r="2.6" />
    <path d="M10.6 8v1.1a1.9 1.9 0 0 0 3.8 0V8a6.4 6.4 0 1 0-2.5 5" />
  </svg>
);
const PaperclipIcon = () => (
  <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="m13.5 7.5-5.3 5.3a3.2 3.2 0 0 1-4.5-4.5l5.6-5.6a2.1 2.1 0 0 1 3 3l-5.5 5.5a1.1 1.1 0 0 1-1.5-1.5l5-5" />
  </svg>
);
const MicIcon = () => (
  <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <rect x="5.5" y="1.5" width="5" height="8" rx="2.5" />
    <path d="M3 7.5a5 5 0 0 0 10 0M8 12.5v2" />
  </svg>
);
const ArrowUpIcon = () => (
  <svg width="14" height="14" viewBox="0 0 14 14" aria-hidden="true">
    <path d="M7 11.5V2.8M3 6.6 7 2.6l4 4" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" />
  </svg>
);
const StopIcon = () => (
  <svg width="10" height="10" viewBox="0 0 10 10" aria-hidden="true">
    <rect x="1" y="1" width="8" height="8" rx="1.5" fill="currentColor" />
  </svg>
);
