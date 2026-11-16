import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { childPath, FolderModel, viewFromSettings, type Entry } from "../lib/folder";
import { getSettings, useSettings } from "../lib/settings";
import { paneActions } from "../lib/pane";
import { navigate, newTab, samePath, selected, settlePath, type Tab } from "../lib/tabs";
import { FileList } from "./FileList";
import { StatusBar } from "./StatusBar";
import { Selection } from "./selection";

/**
 * A tab's folder in the center: its model, selection and view. Every tab keeps one (hidden when
 * the tab isn't active), so switching back finds the same scroll position and selection.
 */
export function FolderPane({ tab, active }: { tab: Tab; active: boolean }) {
  const settings = useSettings();
  const [model, setModel] = useState<FolderModel | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const modelRef = useRef<FolderModel | null>(null);
  const selection = useMemo(() => new Selection(), [model]);

  // A new path opens a new folder; the same folder under its real name ("~" becomes the home
  // folder's path once opened) doesn't.
  useEffect(() => {
    const current = modelRef.current;
    if (current && samePath(current.path, tab.path)) return;
    current?.close();
    const m = new FolderModel(tab.path);
    modelRef.current = m;
    setModel(m);
    setNotice(null);
    m.open(viewFromSettings(getSettings()))
      .then(() => settlePath(tab.id, m.path))
      .catch((err) => m.fail(String(err)));
  }, [tab.id, tab.path]);

  useEffect(() => () => modelRef.current?.close(), []);

  const { sortBy, sortDescending, foldersFirst, showHidden } = settings;
  useEffect(() => {
    void model?.setView(viewFromSettings(getSettings()));
  }, [model, sortBy, sortDescending, foldersFirst, showHidden]);

  const onSelected = useCallback(() => selected(tab.id), [tab.id]);

  // A name to rename as soon as it's listed (a new folder or file).
  const [renameRequest, setRenameRequest] = useState<string | null>(null);
  const onRenameStarted = useCallback(() => setRenameRequest(null), []);

  // The active tab's pane answers the New / Rename / Refresh commands.
  useEffect(() => {
    if (!active || !model) return;
    const create = (command: "op_new_folder" | "op_new_file") => async () => {
      try {
        const name = await invoke<string>(command, { dir: model.path });
        setNotice(null);
        setRenameRequest(name);
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
      if (name) setRenameRequest(name);
    };
    paneActions.refresh = () => void model.refresh();
  }, [active, model, selection]);

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

  const open = (entry: Entry) => {
    const target = childPath(model.path, entry.name);
    if (entry.kind === "folder") navigate(target);
    else
      invoke("open_path", { path: target })
        .then(() => setNotice(null))
        .catch((err) => setNotice(String(err)));
  };

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
      <FileList
        key={model.path + (model.id ?? "")}
        model={model}
        selection={selection}
        mode={settings.viewMode}
        onOpen={open}
        // Only for the folder it belongs to: for one render after navigating, the old folder is
        // still shown and would give up on a name it doesn't have.
        select={samePath(model.path, tab.path) ? tab.select : null}
        onSelected={onSelected}
        active={active}
        onOpenInNewTab={(entry) => newTab(childPath(model.path, entry.name), false)}
        renameRequest={renameRequest}
        onRenameStarted={onRenameStarted}
        onRename={rename}
      />
      <StatusBar model={model} selection={selection} active={active} />
    </div>
  );
}
