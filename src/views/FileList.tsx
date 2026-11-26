import { useVirtualizer } from "@tanstack/react-virtual";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useLayoutEffect, useMemo, useRef, useState, useSyncExternalStore, type Key, type KeyboardEvent, type MouseEvent } from "react";
import { childPath, type Entry, type FolderModel, type GitMark, type Group } from "../lib/folder";
import { isCut, useCutPaths } from "../lib/clipboard";
import { dragOut } from "../lib/dnd";
import { zoomOf } from "../lib/zoom";
import { describeType, formatDate, formatSize } from "../lib/format";
import { modPressed } from "../lib/platform";
import { updateSettings, useSettings, type Settings, type SortBy, type ViewMode } from "../lib/settings";
import { setFolderView, useFolderView } from "../lib/folderViews";
import { EntryIcon } from "../icons/EntryIcon";
import { uiActions } from "../lib/ui";
import type { Selection } from "./selection";
import { makeLayout } from "./layout";
import { ChevronIcon } from "../shell/Icons";

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
  /** "columns": one column of names (a column of Columns view). */
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
  /**
   * A name to select once it's listed, waiting for it to appear (a file just created shows up
   * through the watcher), and to rename in place if asked; `onRequestDone` clears it.
   */
  request?: { name: string; rename: boolean } | null;
  onRequestDone?: () => void;
  /** Rename `entry` to `newName`: resolves to an error to show, or null when done. */
  onRename: (entry: Entry, newName: string) => Promise<string | null>;
  /** Right-click: on the selection (`onItem`) or on empty space. */
  onContextMenu: (x: number, y: number, onItem: boolean) => void;
  /** A plain click on an item (Columns: a click in a side column goes there). */
  onPlainClick?: (entry: Entry) => void;
  /** → and ← (Columns: into the selected folder, up to the parent). */
  onRight?: () => void;
  onLeft?: () => void;
}

/**
 * A folder's rows, drawn only where they're on screen. Details shows columns you can sort and
 * resize; List shows names flowing down and across in columns; in Columns view each folder is one
 * column of names. Selection, keyboard and type-ahead work the same in all of them.
 */
export function FileList(props: Props) {
  const { model, selection, mode, onOpen, select, onSelected, active, onOpenInNewTab } = props;
  const { request, onRequestDone, onRename, onContextMenu, onPlainClick, onRight, onLeft } = props;
  useSyncExternalStore(model.subscribe, () => model.version);
  useSyncExternalStore(selection.subscribe, () => selection.version);
  const settings = useSettings();
  const view = useFolderView(model.path);
  const cut = useCutPaths();
  const scrollRef = useRef<HTMLDivElement>(null);
  const rowH = useRowHeight(settings.density);
  // The Details header scrolls sideways with the rows but sticks to the top: rows start below it,
  // and a row scrolled into view must not end up under it.
  const headerH = mode === "details" ? rowH + 4 : 0;
  // Group headers are lines of their own between the rows.
  const layout = useMemo(() => makeLayout(model.count, model.groups), [model.count, model.groups]);
  const groupH = rowH + 10;
  const widths = COLUMNS.map((c) => settings.columnWidths[c.id] ?? c.width);

  // List flows the lines down a column as tall as the view, then into the next one to the right
  // (as Explorer's List does), so a folder fills the width instead of one narrow strip. Columns
  // are at least as wide as Details' Name column. A folder that fits on screen is spread over the
  // columns that fit (at least 8 lines in each), and they share the width.
  const flow = mode === "list";
  const [box, setBox] = useState({ w: 0, h: 0 });
  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (!flow || !el) return;
    const measure = () => setBox((v) => (v.w === el.clientWidth && v.h === el.clientHeight ? v : { w: el.clientWidth, h: el.clientHeight }));
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [flow]);
  const nameW = widths[0];
  const rowsFit = Math.max(1, Math.floor(box.h / rowH));
  const colsFit = Math.max(1, Math.floor(box.w / nameW));
  const perCol =
    layout.lines <= rowsFit * colsFit ? Math.min(rowsFit, Math.max(8, Math.ceil(layout.lines / colsFit))) : rowsFit;
  const flowCols = Math.ceil(layout.lines / perCol);
  const flowW = flowCols * nameW <= box.w ? Math.max(nameW, Math.floor(box.w / flowCols)) : nameW;
  // Until List knows its height it can't tell which column a row is in: requested rows wait.
  const measuring = flow && box.h === 0;

  const virtualizer = useVirtualizer({
    horizontal: flow,
    count: flow ? flowCols : layout.lines,
    getScrollElement: () => scrollRef.current,
    estimateSize: (i) => (flow ? flowW : "group" in layout.at(i) ? groupH : rowH),
    overscan: flow ? 2 : 12,
    scrollMargin: headerH,
    scrollPaddingStart: headerH,
  });

  useEffect(() => virtualizer.measure(), [rowH, headerH, layout, virtualizer, flow, flowW, perCol]);
  /** Scroll so a folder row shows. */
  const scrollToRow = (row: number, align: "auto" | "center" = "auto") =>
    virtualizer.scrollToIndex(flow ? Math.floor(layout.lineOf(row) / perCol) : layout.lineOf(row), { align });

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
    if (!select || measuring) return;
    if (!model.id || model.count === 0) {
      if (model.id && !model.loading) onSelected?.();
      return;
    }
    let cancelled = false;
    void model.rowOf(select).then((row) => {
      if (cancelled) return;
      if (row !== null) {
        selection.set([select], row, select);
        scrollToRow(row, "center");
        onSelected?.();
      } else if (!model.loading) onSelected?.();
    });
    return () => {
      cancelled = true;
    };
  }, [select, model, model.id, model.count, model.loading, selection, virtualizer, onSelected, measuring]);

  // The name being renamed in place (F2, or a new folder), or null.
  const [renaming, setRenaming] = useState<string | null>(null);

  // Select (and maybe rename) a requested name once it's listed (new items appear through the watcher).
  useEffect(() => {
    if (!request || measuring || !model.id || model.count === 0) return;
    let cancelled = false;
    void model.rowOf(request.name).then((row) => {
      if (cancelled || row === null) return;
      selection.set([request.name], row, request.name);
      scrollToRow(row);
      if (request.rename) setRenaming(request.name);
      onRequestDone?.();
    });
    return () => {
      cancelled = true;
    };
  }, [request, model, model.id, model.count, model.generation, selection, virtualizer, onRequestDone, measuring]);

  const finishRename = (focusList = true) => {
    setRenaming(null);
    if (focusList) scrollRef.current?.focus();
  };

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
    scrollToRow(to);
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
        scrollToRow(row);
        return;
      }
      void go(to, e.shiftKey);
    };
    if (flow && (e.key === "ArrowRight" || e.key === "ArrowLeft")) {
      // Into the next (or previous) column, beside the cursor; a group header there gives way to
      // its first row.
      if (at < 0) return move(0);
      const line = Math.max(0, Math.min(layout.lines - 1, layout.lineOf(at) + (e.key === "ArrowRight" ? perCol : -perCol)));
      const there = layout.at(line);
      return move("row" in there ? there.row : Math.min(there.group.start, model.count - 1));
    }
    if (e.key === "ArrowRight" && onRight && !mod) {
      e.preventDefault();
      return onRight();
    }
    if (e.key === "ArrowLeft" && onLeft && !mod) {
      e.preventDefault();
      return onLeft();
    }
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
        // Space is Quick Look, unless it's part of a name being typed ("my file").
        if (!e.altKey && !(typed.current.text && Date.now() - typed.current.at < 900)) {
          e.preventDefault();
          uiActions.toggleQuickLook();
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
    setFolderView(
      model.path,
      view.sortBy === sort ? { sortDescending: !view.sortDescending } : { sortBy: sort, sortDescending: DESCENDING_FIRST.includes(sort) },
    );

  // The lines on screen and where each goes: one under another, or (List) down each column shown.
  const placed: { line: number; key: Key; x: number; y: number }[] = [];
  for (const v of virtualizer.getVirtualItems()) {
    if (!flow) placed.push({ line: v.index, key: v.key, x: 0, y: v.start - headerH });
    else
      for (let s = 0, line = v.index * perCol; s < perCol && line < layout.lines; s++, line++)
        placed.push({ line, key: line, x: v.start, y: s * rowH });
  }

  let empty: string | null = null;
  if (model.error) empty = model.error;
  else if (!model.loading && model.count === 0) {
    const hidden = model.total - model.count;
    empty = hidden > 0 ? `This folder has only hidden items (${hidden}).` : "This folder is empty.";
  }

  return (
    <div className={`filelist ${mode === "columns" ? "names" : mode}`}>
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
        onWheel={(e) => {
          // List scrolls sideways: the wheel moves along the columns (Ctrl and Alt still zoom).
          if (!flow || e.ctrlKey || e.altKey || e.metaKey || e.shiftKey || e.deltaX || !e.deltaY) return;
          const px = e.deltaMode === 1 ? e.deltaY * rowH : e.deltaMode === 2 ? e.deltaY * e.currentTarget.clientWidth : e.deltaY;
          e.currentTarget.scrollLeft += px;
        }}
        onMouseDown={(e) => {
          // Clicking empty space clears the selection.
          if (e.target === e.currentTarget || (e.target as HTMLElement).classList.contains("fl-rows")) selection.clear();
        }}
        onContextMenu={(e) => {
          if ((e.target as HTMLElement).closest("input")) return; // the rename box keeps the browser's menu
          e.preventDefault();
          const onItem = !!(e.target as HTMLElement).closest(".fl-row") && selection.size > 0;
          onContextMenu(e.clientX, e.clientY, onItem);
        }}
      >
        {mode === "details" && (
          <div className="fl-header" role="row" style={{ gridTemplateColumns: template, width: innerWidth, height: headerH }}>
            {COLUMNS.map((c, ci) => (
              <div key={c.id} className={`fl-head${c.right ? " right" : ""}`} role="columnheader">
                <button
                  className="fl-head-btn"
                  onClick={() => sortBy(c.sort)}
                  aria-sort={view.sortBy === c.sort ? (view.sortDescending ? "descending" : "ascending") : "none"}
                >
                  <span>{c.label}</span>
                  {view.sortBy === c.sort && <span className="fl-sort">{view.sortDescending ? "▼" : "▲"}</span>}
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
        <div
          className="fl-rows"
          style={flow ? { height: perCol * rowH, width: virtualizer.getTotalSize() } : { height: virtualizer.getTotalSize(), width: innerWidth }}
        >
          {placed.map(({ line, key, x, y }) => {
            const at = layout.at(line);
            if ("group" in at) {
              return (
                <GroupHeader
                  key={`g:${at.group.key}`}
                  group={at.group}
                  x={x}
                  y={y}
                  width={flow ? flowW - 8 : undefined}
                  height={flow ? rowH : groupH}
                  onToggle={() => void model.collapse(at.group.key, !at.group.collapsed)}
                />
              );
            }
            const row = at.row;
            const entry = model.row(row);
            const selected = entry ? selection.has(entry.name) : false;
            const focused = row === selection.focus;
            return (
              <div
                key={key}
                data-drop-path={entry?.kind === "folder" ? childPath(model.path, entry.name) : undefined}
                onClick={(e) => {
                  if (!entry || e.button !== 0 || modPressed(e) || e.shiftKey) return;
                  // A plain click on one of several selected rows selects just it (once no drag began).
                  if (selection.size > 1 && selection.has(entry.name)) selection.set([entry.name], row, entry.name);
                  onPlainClick?.(entry);
                }}
                className={`fl-row${selected ? " selected" : ""}${focused ? " focused" : ""}${entry?.hidden ? " hidden" : ""}${
                  entry?.git ? ` git-${entry.git}` : ""
                }${
                  entry && isCut(cut, childPath(model.path, entry.name)) ? " cut" : ""
                }`}
                role="row"
                aria-selected={selected}
                aria-rowindex={row + 1}
                style={{
                  transform: `translate(${x}px, ${y}px)`,
                  height: rowH,
                  gridTemplateColumns: template,
                  ...(flow && { right: "auto", width: flowW - 8 }),
                }}
                onMouseDown={(e) => onRowMouseDown(e, row)}
                onDoubleClick={() => entry && onOpen(entry, row)}
              >
                {entry ? (
                  <Cells
                    entry={entry}
                    path={childPath(model.path, entry.name)}
                    mode={mode}
                    rename={
                      renaming === entry.name
                        ? {
                            submit: async (newName) => {
                              const error = await onRename(entry, newName);
                              if (!error) {
                                // Keep the renamed item selected; its row is found again once it moves.
                                selection.set([newName], row, newName);
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

const GIT_LETTER: Record<GitMark, [string, string]> = {
  modified: ["M", "Modified"],
  added: ["A", "Added"],
  deleted: ["D", "Deleted"],
  renamed: ["R", "Renamed"],
  untracked: ["U", "Untracked"],
  ignored: ["", "Ignored by git"],
  conflicted: ["!", "Conflict"],
  changed: ["•", "Changes inside"],
};

/** The git letter (M, A, U…) or, for a folder, a dot when something inside changed. */
function GitBadge({ mark, staged }: { mark: GitMark; staged: boolean }) {
  const [letter, label] = GIT_LETTER[mark];
  if (!letter) return null;
  return (
    <span className={`fl-git git-${mark}`} title={staged ? `${label} (staged)` : label}>
      {letter}
    </span>
  );
}

function Cells({ entry, path, mode, rename }: { entry: Entry; path: string; mode: ViewMode; rename: RenameHandlers | null }) {
  const folder = entry.kind === "folder";
  const name = (
    <span className="fl-cell fl-name" title={rename ? undefined : entry.name}>
      <span className={`fl-icon${folder ? " folder" : ""}`}>
        <EntryIcon name={entry.name} ext={entry.ext} folder={folder} path={path} />
      </span>
      {rename ? <RenameInput name={entry.name} folder={folder} {...rename} /> : <span className="fl-text">{entry.name}</span>}
      {entry.link && !rename && <span className="fl-link" title="Link">↗</span>}
      {entry.git && !rename && <GitBadge mark={entry.git} staged={entry.staged} />}
    </span>
  );
  if (mode !== "details") return name;
  return (
    <>
      {name}
      <span className="fl-cell mono muted">{formatDate(entry.modified)}</span>
      <span className="fl-cell muted">{describeType(entry)}</span>
      <span className="fl-cell mono muted right">{folder && !entry.sized ? "" : formatSize(entry.size)}</span>
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

/** A group's header line: click to collapse or expand it. Extensions are named ("Rust source"). */
function GroupHeader(props: { group: Group; x: number; y: number; width?: number; height: number; onToggle: () => void }) {
  const { group, x, y, width, height, onToggle } = props;
  const label = group.key.startsWith("ext:")
    ? `${describeType({ kind: "file", ext: group.label, link: false, name: `x.${group.label}` })} (.${group.label})`
    : group.label;
  return (
    <div
      className="fl-group"
      role="row"
      title={label}
      style={{ transform: `translate(${x}px, ${y}px)`, height, ...(width !== undefined && { right: "auto", width }) }}
      onClick={onToggle}
    >
      <ChevronIcon open={!group.collapsed} size={10} />
      <span className="fl-group-label">{label}</span>
      <span className="fl-group-meta">
        {group.count.toLocaleString()} {group.count === 1 ? "item" : "items"}
        {group.bytes > 0 && ` · ${formatSize(group.bytes)}`}
      </span>
    </div>
  );
}

/** The grab area at a column header's right edge. Double-click restores the default width. */
function ColumnResizer({ width, onResize, onReset }: { width: number; onResize: (w: number) => void; onReset: () => void }) {
  const start = useRef<{ x: number; w: number; zoom: number } | null>(null);
  return (
    <span
      className="fl-resizer"
      role="separator"
      aria-orientation="vertical"
      onPointerDown={(e) => {
        e.stopPropagation();
        e.currentTarget.setPointerCapture(e.pointerId);
        start.current = { x: e.clientX, w: width, zoom: zoomOf(e.currentTarget) };
        document.body.classList.add("resizing");
      }}
      onPointerMove={(e) => {
        if (start.current) onResize(Math.round(Math.max(40, Math.min(2000, start.current.w + (e.clientX - start.current.x) / start.current.zoom))));
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
