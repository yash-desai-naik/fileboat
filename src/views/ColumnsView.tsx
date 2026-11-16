import { useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { childPath, FolderModel, viewSpec, type Entry } from "../lib/folder";
import { keepColumns, useFolderView } from "../lib/folderViews";
import { useSettings } from "../lib/settings";
import { navigate, parentOf } from "../lib/tabs";
import { FileList } from "./FileList";
import { Selection } from "./selection";

/** The folders from the drive (or root) down to `path`'s parent, each with the child on the way. */
function ancestors(path: string, limit = 12): { path: string; child: string }[] {
  const out: { path: string; child: string }[] = [];
  let here = path;
  for (let p = parentOf(here); p && out.length < limit; here = p, p = parentOf(p)) {
    const child = here.replace(/[\\/]+$/, "").slice(Math.max(here.lastIndexOf("\\"), here.lastIndexOf("/")) + 1);
    out.unshift({ path: p, child });
  }
  return out;
}

/**
 * A read-only column: an ancestor of the open folder (its child on the way highlighted) or the
 * folder selected in it (a peek at what's inside). Clicking an item goes there.
 */
function SideColumn({ path, highlight, onPick }: { path: string; highlight: string | null; onPick: (entry: Entry) => void }) {
  const settings = useSettings();
  const model = useMemo(() => new FolderModel(path), [path]);
  const selection = useMemo(() => new Selection(), [model]);

  useEffect(() => {
    model.open(viewSpec(path, false)).catch((err) => model.fail(String(err)));
    return () => model.close();
  }, [model]);

  // Each column is sorted and grouped as its folder is.
  const { foldersFirst, showHidden, gitIgnored } = settings;
  const { sortBy, sortDescending, groupBy } = useFolderView(path);
  useEffect(() => {
    void model.setView(viewSpec(path, false));
  }, [model, path, sortBy, sortDescending, foldersFirst, showHidden, groupBy, gitIgnored]);

  return (
    <div className="column">
      <FileList
        model={model}
        selection={selection}
        mode="list"
        active={false}
        select={highlight}
        onSelected={() => {}}
        onOpen={onPick}
        onPlainClick={onPick}
        onRename={async () => null}
        onContextMenu={() => {}}
      />
    </div>
  );
}

interface Props {
  /** The open folder (the tab's), shown as the main column. */
  model: FolderModel;
  selection: Selection;
  /** The main column's list. */
  children: ReactNode;
}

/**
 * Columns, as in Finder: the folders on the way down to the open one, the open one (where the
 * selection and the keyboard are), and what's inside the folder selected in it.
 */
export function ColumnsView({ model, selection, children }: Props) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const [peek, setPeek] = useState<string | null>(null);
  const path = model.path;
  const chain = useMemo(() => ancestors(path), [path]);

  // The folder selected in the main column, shown in a column to its right (after a short pause,
  // so arrowing through a list doesn't open every folder on the way).
  useEffect(() => {
    let timer: number | undefined;
    const update = () => {
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        if (selection.size !== 1) return setPeek(null);
        const [name] = selection.names;
        void model.entries([name]).then(([e]) => setPeek(e?.kind === "folder" ? childPath(model.path, e.name) : null));
      }, 120);
    };
    update();
    const unsubscribe = selection.subscribe(update);
    return () => {
      unsubscribe();
      window.clearTimeout(timer);
    };
  }, [model, selection]);

  // The newest columns are on the right: keep them in view.
  useLayoutEffect(() => {
    const el = scrollRef.current;
    if (el) el.scrollLeft = el.scrollWidth;
  }, [path, peek]);

  /** Go to `folder`, with `name` selected in it (a click in a side column). */
  const goTo = (folder: string, entry: Entry) => {
    const target = entry.kind === "folder" ? childPath(folder, entry.name) : folder;
    keepColumns(target);
    navigate(target, entry.kind === "folder" ? null : entry.name, { replace: true });
  };

  return (
    <div className="columns" ref={scrollRef}>
      {chain.map((c) => (
        <SideColumn key={c.path} path={c.path} highlight={c.child} onPick={(e) => goTo(c.path, e)} />
      ))}
      <div className="column main">{children}</div>
      {peek && <SideColumn key={peek} path={peek} highlight={null} onPick={(e) => goTo(peek, e)} />}
    </div>
  );
}
