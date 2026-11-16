import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { childPath, FolderModel, viewSpec, type Entry } from "../lib/folder";
import { keepColumns, useFolderView } from "../lib/folderViews";
import { useSettings } from "../lib/settings";
import { paneActions } from "../lib/pane";
import { IS_MAC } from "../lib/platform";
import { askConfirm } from "../shell/Confirm";
import { startTransfer } from "../lib/jobs";
import { copyFiles, cutFiles, pasteInto } from "../lib/clipboard";
import { loadTools, openInEditor, openTerminal, type Tools } from "../lib/tools";
import { Menu, type MenuItem } from "../shell/Menu";
import { folderMenu, itemMenu } from "./contextMenu";
import { showToast } from "../shell/Toast";
import { folderGone, goUp, navigate, newTab, parentOf, samePath, selected, settlePath, type Tab } from "../lib/tabs";
import { FileList } from "./FileList";
import { SearchResults } from "./SearchResults";
import { getSearch, useSearch } from "../lib/search";
import { ColumnsView } from "./ColumnsView";
import { StatusBar } from "./StatusBar";
import { Selection } from "./selection";
import { setCurrent } from "../lib/current";
import type { RepoInfo } from "../lib/git";

/**
 * A tab's folder in the center: its model, selection and view. Every tab keeps one (hidden when
 * the tab isn't active), so switching back finds the same scroll position and selection.
 */
export function FolderPane({ tab, active }: { tab: Tab; active: boolean }) {
  const settings = useSettings();
  const [model, setModel] = useState<FolderModel | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  // Shown in the next folder: why the tab moved there.
  const goneNote = useRef<string | null>(null);
  const modelRef = useRef<FolderModel | null>(null);
  const activeRef = useRef(active);
  activeRef.current = active;
  const selection = useMemo(() => new Selection(), [model]);
  const view = useFolderView(tab.path);

  // A new path opens a new folder; the same folder under its real name ("~" becomes the home
  // folder's path once opened) doesn't.
  useEffect(() => {
    const current = modelRef.current;
    if (current && samePath(current.path, tab.path)) return;
    current?.close();
    const m = new FolderModel(tab.path);
    m.onGone = (to, message) => {
      goneNote.current = `${message} This is the folder it was in.`;
      folderGone(tab.id, to);
    };
    modelRef.current = m;
    setModel(m);
    setNotice(goneNote.current);
    goneNote.current = null;
    m.open(viewSpec(m.path, activeRef.current))
      .then(() => settlePath(tab.id, m.path))
      .catch((err) => m.fail(String(err)));
  }, [tab.id, tab.path]);

  useEffect(() => () => modelRef.current?.close(), []);

  const { foldersFirst, showHidden, folderSizes, gitIgnored } = settings;
  const { sortBy, sortDescending, groupBy } = view;
  useEffect(() => {
    if (model) void model.setView(viewSpec(model.path, active));
  }, [model, sortBy, sortDescending, foldersFirst, showHidden, groupBy, folderSizes, gitIgnored, active]);

  const onSelected = useCallback(() => selected(tab.id), [tab.id]);
  const search = useSearch(tab.id);
  const searching = search.open && search.query.trim() !== "";

  // The active pane tells the preview what it's on: after a short pause (arrowing through a
  // list doesn't preview every file on the way), and again when the folder changes on disk.
  useEffect(() => {
    if (!active || !model) return;
    let timer: number | undefined;
    let stale = false;
    const update = () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        // While results show, the preview follows them instead.
        const s = getSearch(tab.id);
        if (s.open && s.query.trim()) return;
        const count = selection.size;
        const names = [...selection.names].slice(0, 200);
        if (count !== 1) return setCurrent({ folder: model.path, entry: null, count, names });
        void model.entries([...selection.names]).then(([entry]) => {
          if (!stale) setCurrent({ folder: model.path, entry: entry ?? null, count: entry ? 1 : 0, names });
        });
      }, 80);
    };
    update();
    const offSelection = selection.subscribe(update);
    const offModel = model.subscribe(update);
    return () => {
      stale = true;
      window.clearTimeout(timer);
      offSelection();
      offModel();
    };
  }, [active, model, selection, searching]);

  // The context menu showing, if any.
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[]; keyboard: boolean } | null>(null);
  const [tools, setTools] = useState<Tools | null>(null);
  useEffect(() => {
    void loadTools().then(setTools);
  }, []);

  // A name to select (and maybe rename) as soon as it's listed (a new folder or file).
  const [request, setRequest] = useState<{ name: string; rename: boolean } | null>(null);
  const onRequestDone = useCallback(() => setRequest(null), []);

  // The active tab's pane answers the New / Rename / Refresh commands.
  useEffect(() => {
    if (!active || !model) return;
    const create = (command: "op_new_folder" | "op_new_file") => async () => {
      try {
        const name = await invoke<string>(command, { dir: model.path });
        setNotice(null);
        setRequest({ name, rename: true });
        // If the folder can't be watched (some network shares), read it again to show the new item.
        window.setTimeout(() => {
          void model.rowOf(name).then((row) => {
            if (row === null) void model.refresh();
          });
        }, 700);
      } catch (err) {
        setNotice(String(err));
      }
    };
    paneActions.newFolder = create("op_new_folder");
    paneActions.newFile = create("op_new_file");
    paneActions.rename = async () => {
      // The row at the cursor, asked fresh: a key pressed just before (End, an arrow) may not have
      // recorded the row's name yet.
      const [entry] = selection.focus >= 0 ? await model.rows(selection.focus, selection.focus + 1) : [];
      const name = entry?.name ?? selection.focusName;
      if (name) setRequest({ name, rename: true });
    };
    paneActions.refresh = () => void model.refresh();
    paneActions.calculateSizes = () => void model.calculateSizes();
    const selectedPaths = async () => (await selectedNames(model, selection)).map((n) => childPath(model.path, n));
    paneActions.copy = async () => copyFiles(await selectedPaths());
    paneActions.cut = async () => cutFiles(await selectedPaths());
    paneActions.paste = () => void pasteInto(model.path);
    paneActions.copyPath = async () => {
      const paths = await selectedPaths();
      const text = paths.length ? paths.join("\n") : model.path;
      await navigator.clipboard.writeText(text);
      showToast(paths.length > 1 ? `Copied ${paths.length} paths` : `Copied ${text}`);
    };
    paneActions.openInEditor = async () => {
      const paths = await selectedPaths();
      openInEditor(paths.length ? paths : [model.path]);
    };
    paneActions.terminalHere = () => openTerminal(model.path);
    paneActions.selectNames = async (names: string[]) => {
      const rows = await Promise.all(names.map((n) => model.rowOf(n).then((r) => [n, r] as const)));
      const found = rows.filter(([, r]) => r !== null) as [string, number][];
      if (found.length) selection.set(found.map(([n]) => n), found[0][1], found[0][0]);
      return found.length;
    };
    paneActions.properties = async () => {
      const [path] = await selectedPaths();
      void invoke("show_properties", { path: path ?? model.path }).catch((err) => setNotice(String(err)));
    };
    paneActions.contextMenu = () => {
      // At the cursor's row (or the top of the list), as the Menu key does in Explorer.
      const row = document.querySelector(".folder-pane:not([hidden]) .fl-row.focused") ?? document.querySelector(".folder-pane:not([hidden]) .fl-scroll");
      const r = row?.getBoundingClientRect();
      if (r) void showMenu(r.left + 40, r.top + r.height / 2, selection.size > 0, true);
    };
    paneActions.duplicate = async () => {
      const names = await selectedNames(model, selection);
      if (names.length) await startTransfer(names.map((n) => childPath(model.path, n)), model.path, "copy");
    };
    paneActions.delete = async (permanently: boolean) => {
      const names = await selectedNames(model, selection);
      if (!names.length) return;
      const paths = names.map((n) => childPath(model.path, n));
      const what = names.length === 1 ? `"${names[0]}"` : `${names.length} items`;
      const bin = IS_MAC ? "the Trash" : "the Recycle Bin";
      if (permanently) {
        const ok = await askConfirm({
          title: `Delete ${what} permanently?`,
          body: <DeleteList names={names} note="They won't go to the Recycle Bin. This can't be undone." />,
          confirm: "Delete permanently",
          danger: true,
        });
        if (!ok) return;
      }
      const next = await survivor(model, selection);
      try {
        await invoke(permanently ? "op_delete" : "op_trash", { paths });
      } catch (err) {
        if (permanently) return setNotice(String(err));
        // No Recycle Bin here (some network drives): offer to delete for good instead.
        const ok = await askConfirm({
          title: `Delete ${what} permanently?`,
          body: <DeleteList names={names} note={`${String(err)}\n\nThey can't go to ${bin}. Delete them permanently instead? This can't be undone.`} />,
          confirm: "Delete permanently",
          danger: true,
        });
        if (!ok) return;
        try {
          await invoke("op_delete", { paths });
        } catch (err2) {
          return setNotice(String(err2));
        }
      }
      setNotice(null);
      // The next item takes the cursor, as in Explorer.
      if (next) navigate(model.path, next);
    };
  }, [active, model, selection]);

  /** The context menu for the selection (onItem) or the folder itself. */
  const showMenu = async (x: number, y: number, onItem: boolean, keyboard = false) => {
    if (!model) return;
    const entries = onItem ? await model.entries([...selection.names]) : [];
    const ctx = {
      folder: model.path,
      entries,
      tools,
      open: (entry: Entry) => open(entry),
      newFromTemplate: (name: string, content: string) => {
        void invoke<string>("op_new_file", { dir: model.path, name, content })
          .then((created) => setRequest({ name: created, rename: false }))
          .catch((err) => setNotice(String(err)));
      },
      repo: await invoke<RepoInfo | null>("git_info", { path: model.path }).catch(() => null),
    };
    setMenu({ x, y, items: entries.length ? itemMenu(ctx) : folderMenu(ctx), keyboard });
  };

  const rename = useCallback(
    async (entry: Entry, newName: string): Promise<string | null> => {
      if (!model) return null;
      try {
        await invoke("op_rename", { path: childPath(model.path, entry.name), newName });
        return null;
      } catch (err) {
        return String(err);
      }
    },
    [model],
  );

  if (!model) return null;
  const columnsMode = view.viewMode === "columns";

  /** Columns: → goes into the selected folder. */
  const enterSelected = async () => {
    if (selection.size !== 1) return;
    const [entry] = await model.entries([...selection.names]);
    if (entry?.kind !== "folder") return;
    const target = childPath(model.path, entry.name);
    keepColumns(target);
    navigate(target, null, { replace: true });
  };

  /** Columns: ← goes up. */
  const leaveColumn = () => {
    const parent = parentOf(model.path);
    if (parent) keepColumns(parent);
    goUp({ replace: true });
  };

  const open = (entry: Entry) => {
    const target = childPath(model.path, entry.name);
    if (entry.kind === "folder") navigate(target);
    else
      invoke("open_path", { path: target })
        .then(() => setNotice(null))
        .catch((err) => setNotice(String(err)));
  };

  const list = (
    <FileList
      key={model.path + (model.id ?? "")}
      model={model}
      selection={selection}
      mode={columnsMode ? "list" : view.viewMode}
      onOpen={open}
      // Only for the folder it belongs to: for one render after navigating, the old folder is
      // still shown and would give up on a name it doesn't have.
      select={samePath(model.path, tab.path) ? tab.select : null}
      onSelected={onSelected}
      active={active}
      onOpenInNewTab={(entry) => newTab(childPath(model.path, entry.name), false)}
      request={request}
      onRequestDone={onRequestDone}
      onRename={rename}
      onContextMenu={(x, y, onItem) => void showMenu(x, y, onItem)}
      onRight={columnsMode ? () => void enterSelected() : undefined}
      onLeft={columnsMode ? leaveColumn : undefined}
    />
  );

  return (
    <div className="folder-pane" hidden={!active}>
      {notice && (
        <div className="pane-notice" role="alert">
          <span>{notice}</span>
          <button className="link-btn" onClick={() => setNotice(null)}>
            Dismiss
          </button>
        </div>
      )}
      {searching && <SearchResults tabId={tab.id} search={search} active={active} />}
      {/* The folder stays loaded under the results (selection and scroll kept). */}
      <div className="pane-list" hidden={searching}>
        {columnsMode ? <ColumnsView model={model} selection={selection}>{list}</ColumnsView> : list}
      </div>
      {menu && <Menu x={menu.x} y={menu.y} items={menu.items} keyboard={menu.keyboard} onClose={() => setMenu(null)} />}
      <StatusBar model={model} selection={selection} active={active} />
    </div>
  );
}

/** The selected names, or the item at the cursor when nothing is selected. */
async function selectedNames(model: FolderModel, selection: Selection): Promise<string[]> {
  if (selection.size) return [...selection.names];
  if (selection.focus < 0) return [];
  const [entry] = await model.rows(selection.focus, selection.focus + 1);
  return entry ? [entry.name] : [];
}

/** The item to select once the selection is gone: the next one not selected, else the one before. */
async function survivor(model: FolderModel, selection: Selection): Promise<string | null> {
  const at = Math.max(0, selection.focus);
  const start = Math.max(0, at - 200);
  const names = await model.names(start, at + 200);
  const gone = selection.size ? selection.names : new Set(names.slice(at - start, at - start + 1));
  const after = names.slice(at - start).find((n) => !gone.has(n));
  if (after) return after;
  return [...names.slice(0, at - start)].reverse().find((n) => !gone.has(n)) ?? null;
}

function DeleteList({ names, note }: { names: string[]; note: string }) {
  const shown = names.slice(0, 8);
  return (
    <>
      <ul className="delete-list">
        {shown.map((n) => (
          <li key={n} className="mono">
            {n}
          </li>
        ))}
        {names.length > shown.length && <li className="muted">and {names.length - shown.length} more</li>}
      </ul>
      <p className="muted" style={{ whiteSpace: "pre-wrap" }}>
        {note}
      </p>
    </>
  );
}
