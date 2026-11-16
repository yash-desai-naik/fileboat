import { invoke } from "@tauri-apps/api/core";
import { useEffect, useMemo, useState } from "react";
import { childPath, FolderModel, viewFromSettings, type Entry } from "../lib/folder";
import { getSettings, useSettings } from "../lib/settings";
import { FileList } from "./FileList";
import { Selection } from "./selection";

interface Props {
  path: string;
  /** Go to another folder (opening a subfolder). */
  onNavigate: (path: string) => void;
}

/** One folder in the center: its model, selection and view. */
export function FolderPane({ path, onNavigate }: Props) {
  const settings = useSettings();
  const selection = useMemo(() => new Selection(), [path]);
  const [model, setModel] = useState<FolderModel | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    const m = new FolderModel(path);
    setModel(m);
    setNotice(null);
    m.open(viewFromSettings(getSettings())).catch((err) => m.fail(String(err)));
    return () => m.close();
  }, [path]);

  const { sortBy, sortDescending, foldersFirst, showHidden } = settings;
  useEffect(() => {
    void model?.setView(viewFromSettings(getSettings()));
  }, [model, sortBy, sortDescending, foldersFirst, showHidden]);

  if (!model) return null;

  const open = (entry: Entry) => {
    const target = childPath(model.path, entry.name);
    if (entry.kind === "folder") onNavigate(target);
    else
      invoke("open_path", { path: target })
        .then(() => setNotice(null))
        .catch((err) => setNotice(String(err)));
  };

  return (
    <>
      {notice && (
        <div className="pane-notice" role="alert">
          <span>{notice}</span>
          <button className="link-btn" onClick={() => setNotice(null)}>
            Dismiss
          </button>
        </div>
      )}
      <FileList key={path} model={model} selection={selection} mode={settings.viewMode} onOpen={open} />
    </>
  );
}
