import { useVirtualizer } from "@tanstack/react-virtual";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useLayoutEffect, useRef, useState, useSyncExternalStore, type KeyboardEvent, type MouseEvent } from "react";
import { childPath, type Entry, type FolderModel } from "../lib/folder";
import { isCut, useCutPaths } from "../lib/clipboard";
import { dragOut } from "../lib/dnd";
import { describeType, formatDate, formatSize } from "../lib/format";
import { modPressed } from "../lib/platform";
import { updateSettings, useSettings, type Settings, type SortBy, type ViewMode } from "../lib/settings";
import { FileTypeIcon } from "../shell/FileTypeIcon";
import { FolderIcon } from "../shell/Icons";
import { uiActions } from "../lib/ui";
import type { Selection } from "./selection";

interface Column {
  id: "name" | "modified" | "type" | "size";
  label: string;
  sort: SortBy;
  width: number;
  /** Numbers and dates line up in monospace, right- or left-aligned. */
  mono?: boolean;
  right?: boolean;
}

export const COLUMNS: Column[] = [
  { id: "name", label: "Name", sort: "name", width: 320 },
  { id: "modified", label: "Modified", sort: "modified", width: 160, mono: true },
  { id: "type", label: "Type", sort: "type", width: 150 },
  { id: "size", label: "Size", sort: "size", width: 90, mono: true, right: true },
];

/** Sorts that start with the biggest or newest on the first click. */
const DESCENDING_FIRST: SortBy[] = ["size", "modified", "created"];

function useRowHeight(density: Settings["density"]): number {
  const [h, setH] = useState(24);
  useLayoutEffect(() => {
    const v = parseFloat(getComputedStyle(document.documentElement).getPropertyValue("--row-h"));
    if (v > 0) setH(v);
  }, [density]);
  return h;
}

interface Props {
  model: FolderModel;
  selection: Selection;
  mode: ViewMode;
  /** Double-click or Enter. */
  onOpen: (entry: Entry, row: number) => void;
  /** A name to select once it's listed (the folder you came up from); `onSelected` clears it. */
  select?: string | null;
  onSelected?: () => void;
  /** The tab is showing (hidden tabs keep their state but don't take the keyboard). */
  active: boolean;
  /** Middle-click on a folder. */
  onOpenInNewTab?: (entry: Entry) => void;
  /** A name to start renaming once it's listed (a folder just created); `onRenameStarted` clears it. */
  renameRequest?: string | null;
  onRenameStarted?: () => void;
  /** Rename `entry` to `newName`: resolves to an error to show, or null when done. */
  onRename: (entry: Entry, newName: string) => Promise<string | null>;
}

/**
 * A folder's rows, drawn only where they're on screen. Details shows columns you can sort and
 * resize; List shows names only. Selection, keyboard and type-ahead work the same in both.
 */
export function FileList(props: Props) {
  const { model, selection, mode, onOpen, select, onSelected, active, onOpenInNewTab } = props;
  const { renameRequest, onRenameStarted, onRename } = props;
  useSyncExternalStore(model.subscribe, () => model.version);
  useSyncExternalStore(selection.subscribe, () => selection.version);
  const settings = useSettings();
  const cut = useCutPaths();
  const scrollRef = useRef<HTMLDivElement>(null);
  const rowH = useRowHeight(settings.density);
  // The Details header scrolls sideways with the rows but sticks to the top: rows start below it,
  // and a row scrolled into view must not end up under it.
  const headerH = mode === "details" ? rowH + 4 : 0;
  const virtualizer = useVirtualizer({
    count: model.count,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => rowH,
    overscan: 12,
    scrollMargin: headerH,
    scrollPaddingStart: headerH,
  });

  useEffect(() => virtualizer.measure(), [rowH, headerH, virtualizer]);

  // Rows moved (files changed, sort changed): find the cursor's name again.
  useEffect(() => {
    const name = selection.focusName;
    if (!name) return;
    void model.rowOf(name).then((row) => {
      if (row !== null && row !== selection.focus) selection.moveFocus(row);
    });
  }, [model, model.generation, selection]);

  // The keyboard stays in the view when you open a folder (unless you were typing elsewhere).
  useEffect(() => {
    if (!active) return;
    uiActions.focusList = () => scrollRef.current?.focus();
    const focused = document.activeElement;
    if (!focused || focused === document.body || focused.closest(".filelist, .tabs")) scrollRef.current?.focus();
  }, [active]);

  // Select the requested name once it's listed; give up when the listing ends without it.
  // (The first rows can arrive before the folder's id does: wait for both.)
  useEffect(() => {
    if (!select) return;
    if (!model.id || model.count === 0) {
      if (model.id && !model.loading) onSelected?.();
      return;
    }
    let cancelled = false;
    void model.rowOf(select).then((row) => {
      if (cancelled) return;
      if (row !== null) {
        selection.set([select], row, select);
        virtualizer.scrollToIndex(row, { align: "center" });
        onSelected?.();
      } else if (!model.loading) onSelected?.();
    });
    return () => {
      cancelled = true;
    };
  }, [select, model, model.id, model.count, model.loading, selection, virtualizer, onSelected]);

  // The name being renamed in place (F2, or a new folder), or null.
  const [renaming, setRenaming] = useState<string | null>(null);

  // Start renaming a requested name once it's listed (a new folder appears through the watcher).
  useEffect(() => {
    if (!renameRequest || !model.id || model.count === 0) return;
    let cancelled = false;
    void model.rowOf(renameRequest).then((row) => {
      if (cancelled || row === null) return;
      selection.set([renameRequest], row, renameRequest);
      virtualizer.scrollToIndex(row, { align: "auto" });
      setRenaming(renameRequest);
      onRenameStarted?.();
    });
    return () => {
      cancelled = true;
    };
  }, [renameRequest, model, model.id, model.count, model.generation, selection, virtualizer, onRenameStarted]);

  const finishRename = (focusList = true) => {
    setRenaming(null);
    if (focusList) scrollRef.current?.focus();
  };

  const widths = COLUMNS.map((c) => settings.columnWidths[c.id] ?? c.width);
  const template = mode === "details" ? widths.map((w) => `${w}px`).join(" ") : "minmax(0, 1fr)";
  const innerWidth = mode === "details" ? widths.reduce((a, b) => a + b, 0) + 16 : undefined;

  /** The row at `i`, fetching it if it isn't on screen. */
  const entryAt = async (i: number): Promise<Entry | undefined> => model.row(i) ?? (await model.rows(i, i + 1))[0];

  /** Select rows from the anchor to `to`, cursor on `to`. */
  const selectRange = async (to: number, toName: string) => {
    const anchor = selection.anchor >= 0 ? selection.anchor : to;
    const [lo, hi] = anchor < to ? [anchor, to] : [to, anchor];
    const names = await model.names(lo, hi + 1);
    selection.set(names, to, toName, anchor);
  };

  /**
   * Move the cursor to `to`, selecting it (or the range from the anchor). The cursor moves at once,
   * before the row's name arrives, so quick repeated keys each move one more row.
   */
  const go = async (to: number, extend: boolean) => {
    if (model.count === 0) return;
    to = Math.max(0, Math.min(model.count - 1, to));
    if (selection.anchor < 0) selection.anchor = to;
    selection.moveFocus(to);
    virtualizer.scrollToIndex(to, { align: "auto" });
    const entry = await entryAt(to);
    if (!entry || selection.focus !== to) return;
    if (extend) await selectRange(to, entry.name);
    else selection.set([entry.name], to, entry.name);
  };

  const typed = useRef({ text: "", at: 0 });

  // Dragging rows: a press on a row that then moves a few pixels starts a system drag of the
  // selection (other apps get the files; Fileboat's own drop handling catches drops here).
  const press = useRef<{ x: number; y: number } | null>(null);
  useEffect(() => {
    const onMove = (e: globalThis.MouseEvent) => {
      const p = press.current;
      if (!p || (e.buttons & 1) === 0) {
        press.current = null;
        return;
      }
      if (Math.abs(e.clientX - p.x) + Math.abs(e.clientY - p.y) < 6) return;
      press.current = null;
      dragOut([...selection.names].map((n) => childPath(model.path, n)));
    };
    const onUp = () => (press.current = null);
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, [model, selection]);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const page = Math.max(1, Math.floor((scrollRef.current?.clientHeight ?? rowH * 10) / rowH) - 1);
    const at = selection.focus;
    const mod = modPressed(e);
    const move = (to: number) => {
      e.preventDefault();
      if (mod && !e.shiftKey) {
        // Ctrl+arrows move the cursor without changing the selection (Ctrl+Space toggles).
        const row = Math.max(0, Math.min(model.count - 1, to));
        void entryAt(row).then((entry) => entry && selection.set(selection.names, row, entry.name, selection.anchor));
        virtualizer.scrollToIndex(row, { align: "auto" });
        return;
      }
      void go(to, e.shiftKey);
    };
    switch (e.key) {
      case "ArrowDown":
        return move(at < 0 ? 0 : at + 1);
      case "ArrowUp":
        return move(at < 0 ? 0 : at - 1);
      case "PageDown":
        return move(at + page);
      case "PageUp":
        return move(at - page);
      case "Home":
        return move(0);
      case "End":
        return move(model.count - 1);
      case "Enter":
        if (at >= 0) {
          e.preventDefault();
          void entryAt(at).then((entry) => entry && onOpen(entry, at));
        }
        return;
      case "Escape":
        if (selection.size) {
          e.preventDefault();
          selection.set([], at, selection.focusName);
        }
        return;
      case " ":
        if (mod && at >= 0) {
          e.preventDefault();
          void entryAt(at).then((entry) => entry && selection.toggle(entry.name, at));
          return;
        }
        break;
    }
    if (mod && e.key.toLowerCase() === "a") {
      e.preventDefault();
      void model.names(0, model.count).then((names) => selection.set(names, at, selection.focusName, selection.anchor));
      return;
    }
    // Type-ahead: letters jump to the next name starting with what was typed.
    if (e.key.length === 1 && !mod && !e.altKey && (e.key !== " " || typed.current.text)) {
      e.preventDefault();
      const now = Date.now();
      const t = typed.current;
      t.text = now - t.at < 900 ? t.text + e.key : e.key;
      t.at = now;
      // One letter again moves on to the next match; a longer prefix stays on the current one.
      const from = t.text.length === 1 ? at + 1 : Math.max(at, 0);
      void model.seek(t.text, from).then((row) => {
        if (row !== null) void go(row, false);
      });
    }
  };

  const onRowMouseDown = (e: MouseEvent, i: number) => {
    const entry = model.row(i);
    if (!entry) return;
    if (e.button === 1) {
      // Middle-click opens a folder in a new tab, as in browsers and Explorer.
      e.preventDefault();
      if (entry.kind === "folder") onOpenInNewTab?.(entry);
      return;
    }
    if (e.button === 2) {
      // Right-click keeps a selection that includes the row (for the context menu).
      if (!selection.has(entry.name)) selection.set([entry.name], i, entry.name);
      return;
    }
    if (e.button !== 0) return;
    // Pressing on a selected row keeps the selection, so several rows can be dragged together.
    press.current = { x: e.clientX, y: e.clientY };
    if (modPressed(e)) selection.toggle(entry.name, i);
    else if (e.shiftKey) void selectRange(i, entry.name);
    else if (!selection.has(entry.name) || selection.size === 1) selection.set([entry.name], i, entry.name);
  };

  const sortBy = (sort: SortBy) =>
    updateSettings(
      settings.sortBy === sort
        ? { sortDescending: !settings.sortDescending }
        : { sortBy: sort, sortDescending: DESCENDING_FIRST.includes(sort) },
    );

  let empty: string | null = null;
  if (model.error) empty = model.error;
  else if (!model.loading && model.count === 0) {
    const hidden = model.total - model.count;
    empty = hidden > 0 ? `This folder has only hidden items (${hidden}).` : "This folder is empty.";
  }

  return (
    <div className={`filelist ${mode}`}>
      <div
        ref={scrollRef}
        className="fl-scroll"
        data-drop-path={model.path}
        tabIndex={0}
        role="grid"
        aria-label={`Files in ${model.name}`}
        aria-rowcount={model.count}
        aria-multiselectable="true"
        onKeyDown={onKeyDown}
        onMouseDown={(e) => {
          // Clicking empty space clears the selection.
          if (e.target === e.currentTarget || (e.target as HTMLElement).classList.contains("fl-rows")) selection.clear();
        }}
      >
        {mode === "details" && (
          <div className="fl-header" role="row" style={{ gridTemplateColumns: template, width: innerWidth, height: headerH }}>
            {COLUMNS.map((c, ci) => (
              <div key={c.id} className={`fl-head${c.right ? " right" : ""}`} role="columnheader">
                <button
                  className="fl-head-btn"
                  onClick={() => sortBy(c.sort)}
                  aria-sort={settings.sortBy === c.sort ? (settings.sortDescending ? "descending" : "ascending") : "none"}
                >
                  <span>{c.label}</span>
                  {settings.sortBy === c.sort && <span className="fl-sort">{settings.sortDescending ? "▼" : "▲"}</span>}
                </button>
                <ColumnResizer
                  width={widths[ci]}
                  onResize={(w) => updateSettings({ columnWidths: { ...settings.columnWidths, [c.id]: w } }, { debounceMs: 400 })}
                  onReset={() => {
                    const rest = { ...settings.columnWidths };
                    delete rest[c.id];
                    updateSettings({ columnWidths: rest });
                  }}
                />
              </div>
            ))}
          </div>
        )}
        <div className="fl-rows" style={{ height: virtualizer.getTotalSize(), width: innerWidth }}>
          {virtualizer.getVirtualItems().map((v) => {
            const entry = model.row(v.index);
            const selected = entry ? selection.has(entry.name) : false;
            const focused = v.index === selection.focus;
            return (
              <div
                key={v.key}
                data-drop-path={entry?.kind === "folder" ? childPath(model.path, entry.name) : undefined}
                onClick={(e) => {
                  // A plain click on one of several selected rows selects just it (once no drag began).
                  if (entry && e.button === 0 && !modPressed(e) && !e.shiftKey && selection.size > 1 && selection.has(entry.name)) {
                    selection.set([entry.name], v.index, entry.name);
                  }
                }}
                className={`fl-row${selected ? " selected" : ""}${focused ? " focused" : ""}${entry?.hidden ? " hidden" : ""}${
                  entry && isCut(cut, childPath(model.path, entry.name)) ? " cut" : ""
                }`}
                role="row"
                aria-selected={selected}
                aria-rowindex={v.index + 1}
                style={{ transform: `translateY(${v.start - headerH}px)`, height: rowH, gridTemplateColumns: template }}
                onMouseDown={(e) => onRowMouseDown(e, v.index)}
                onDoubleClick={() => entry && onOpen(entry, v.index)}
              >
                {entry ? (
                  <Cells
                    entry={entry}
                    mode={mode}
                    rename={
                      renaming === entry.name
                        ? {
                            submit: async (newName) => {
                              const error = await onRename(entry, newName);
                              if (!error) {
                                // Keep the renamed item selected; its row is found again once it moves.
                                selection.set([newName], v.index, newName);
                                finishRename();
                              }
                              return error;
                            },
                            cancel: () => finishRename(),
                          }
                        : null
                    }
                  />
                ) : (
                  <span className="fl-cell muted">…</span>
                )}
              </div>
            );
          })}
        </div>
        {empty && <div className={`fl-empty${model.error ? " error" : ""}`}>{empty}</div>}
      </div>
    </div>
  );
}

interface RenameHandlers {
  /** Resolves to an error to show (the box stays open), or null once renamed. */
  submit: (newName: string) => Promise<string | null>;
  cancel: () => void;
}

function Cells({ entry, mode, rename }: { entry: Entry; mode: ViewMode; rename: RenameHandlers | null }) {
  const folder = entry.kind === "folder";
  const name = (
    <span className="fl-cell fl-name" title={rename ? undefined : entry.name}>
      <span className={`fl-icon${folder ? " folder" : ""}`}>{folder ? <FolderIcon size={15} /> : <FileTypeIcon entry={entry} />}</span>
      {rename ? <RenameInput name={entry.name} folder={folder} {...rename} /> : <span className="fl-text">{entry.name}</span>}
      {entry.link && !rename && <span className="fl-link" title="Link">↗</span>}
    </span>
  );
  if (mode === "list") return name;
  return (
    <>
      {name}
      <span className="fl-cell mono muted">{formatDate(entry.modified)}</span>
      <span className="fl-cell muted">{describeType(entry)}</span>
      <span className="fl-cell mono muted right">{folder ? "" : formatSize(entry.size)}</span>
    </>
  );
}

/**
 * The name box for renaming in place. The name is selected without its extension (as in Explorer
 * and Finder); problems with the name show as you type. Enter or clicking away renames, Esc cancels.
 */
function RenameInput({ name, folder, submit, cancel }: { name: string; folder: boolean } & RenameHandlers) {
  const ref = useRef<HTMLInputElement>(null);
  const [value, setValue] = useState(name);
  const [error, setError] = useState<string | null>(null);
  const busy = useRef(false);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.focus();
    const dot = name.lastIndexOf(".");
    el.setSelectionRange(0, !folder && dot > 0 ? dot : name.length);
  }, [name, folder]);

  // Check the name as it's typed (after a short pause).
  useEffect(() => {
    if (value === name) {
      setError(null);
      return;
    }
    const timer = window.setTimeout(() => void invoke<string | null>("op_check_name", { name: value }).then(setError), 150);
    return () => window.clearTimeout(timer);
  }, [value, name]);

  const commit = async () => {
    if (busy.current) return;
    if (value.trim() === name || !value.trim()) return cancel();
    busy.current = true;
    const problem = await submit(value.trim());
    busy.current = false;
    if (problem) {
      setError(problem);
      ref.current?.focus();
    }
  };

  return (
    <span className="rename">
      <input
        ref={ref}
        className="rename-input"
        value={value}
        spellCheck={false}
        aria-label="New name"
        aria-invalid={!!error}
        onChange={(e) => setValue(e.target.value)}
        onMouseDown={(e) => e.stopPropagation()}
        onDoubleClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === "Enter") {
            e.preventDefault();
            void commit();
          } else if (e.key === "Escape") {
            e.preventDefault();
            cancel();
          }
        }}
        onBlur={() => void commit()}
      />
      {error && <span className="rename-error" role="alert">{error}</span>}
    </span>
  );
}

/** The grab area at a column header's right edge. Double-click restores the default width. */
function ColumnResizer({ width, onResize, onReset }: { width: number; onResize: (w: number) => void; onReset: () => void }) {
  const start = useRef<{ x: number; w: number } | null>(null);
  return (
    <span
      className="fl-resizer"
      role="separator"
      aria-orientation="vertical"
      onPointerDown={(e) => {
        e.stopPropagation();
        e.currentTarget.setPointerCapture(e.pointerId);
        start.current = { x: e.clientX, w: width };
        document.body.classList.add("resizing");
      }}
      onPointerMove={(e) => {
        if (start.current) onResize(Math.round(Math.max(40, Math.min(2000, start.current.w + e.clientX - start.current.x))));
      }}
      onPointerUp={(e) => {
        e.currentTarget.releasePointerCapture(e.pointerId);
        start.current = null;
        document.body.classList.remove("resizing");
      }}
      onDoubleClick={onReset}
    />
  );
}
