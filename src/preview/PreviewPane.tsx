import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { useCurrent } from "../lib/current";
import { childPath, type Entry } from "../lib/folder";
import { formatDate, formatSize } from "../lib/format";
import { openInEditor } from "../lib/tools";
import { CloseSmallIcon, FileIcon } from "../shell/Icons";
import { showToast } from "../shell/Toast";
import type { Info, Text } from "./types";
import {
  ArchiveViewer,
  CsvViewer,
  DiffViewer,
  Modes,
  MarkdownViewer,
  StructuredViewer,
  SvgViewer,
  FolderViewer,
  FontViewer,
  HexViewer,
  ImageViewer,
  MediaViewer,
  Note,
  PdfViewer,
  TextViewer,
} from "./viewers";
import "./preview.css";

/** Changes a diff can show, and viewers whose files are text. */
const DIFFABLE_MARKS = new Set(["modified", "added", "renamed", "conflicted"]);
const DIFFABLE_VIEWERS = new Set(["text", "markdown", "json", "yaml", "toml", "csv", "svg"]);

/** Viewers that show text (and can be copied or opened in the editor). */
const TEXTUAL = new Set(["text", "markdown", "json", "yaml", "toml", "csv", "svg"]);

/** What to preview: the single selected item, or the folder when nothing is selected. */
function useTarget(): { path: string; reload: string; entry: Entry | null } | { count: number } | null {
  const current = useCurrent();
  if (!current) return null;
  if (current.count > 1) return { count: current.count };
  if (current.entry) {
    const e = current.entry;
    // The git mark is part of the key: staging a file shows its diff afresh.
    return { path: childPath(current.folder, e.name), reload: `${e.modified}:${e.size}:${e.git}:${e.staged}`, entry: e };
  }
  return { path: current.folder, reload: "", entry: null };
}

/** The body for a file's viewer. */
export function Body({ info, reload }: { info: Info; reload: string }) {
  switch (info.viewer) {
    case "image":
      return <ImageViewer info={info} reload={reload} />;
    case "svg":
      return <SvgViewer info={info} reload={reload} />;
    case "markdown":
      return <MarkdownViewer info={info} reload={reload} />;
    case "json":
    case "yaml":
    case "toml":
      return <StructuredViewer info={info} reload={reload} />;
    case "csv":
      return <CsvViewer info={info} reload={reload} />;
    case "audio":
    case "video":
      return <MediaViewer info={info} reload={reload} />;
    case "pdf":
      return <PdfViewer info={info} reload={reload} />;
    case "font":
      return <FontViewer info={info} reload={reload} />;
    case "archive":
      return <ArchiveViewer info={info} reload={reload} />;
    case "binary":
      return <HexViewer info={info} reload={reload} />;
    case "folder":
      return <FolderViewer info={info} reload={reload} />;
    case "cloud":
      return <Note>This file is in the cloud and not on this computer. Open it to download it.</Note>;
    default:
      return <TextViewer info={info} reload={reload} />;
  }
}

/** Size, dates, attributes and the path (click to copy). */
function InfoStrip({ info }: { info: Info }) {
  const attrs = [info.readonly && "Read-only", info.hidden && "Hidden", info.link && "Link"].filter(Boolean).join(", ");
  return (
    <dl className="pv-info">
      {info.viewer !== "folder" && (
        <>
          <dt>Size</dt>
          <dd>
            {formatSize(info.size)} {info.size >= 1024 && <span className="muted">({info.size.toLocaleString()} bytes)</span>}
          </dd>
        </>
      )}
      <dt>Modified</dt>
      <dd>{formatDate(info.modified)}</dd>
      <dt>Created</dt>
      <dd>{formatDate(info.created)}</dd>
      {attrs && (
        <>
          <dt>Attributes</dt>
          <dd>{attrs}</dd>
        </>
      )}
      <dt>Path</dt>
      <dd>
        <button
          className="pv-path mono"
          title="Copy the path"
          onClick={() => void navigator.clipboard.writeText(info.path).then(() => showToast("Copied the path"))}
        >
          {info.path}
        </button>
      </dd>
    </dl>
  );
}

/** What the active tab is on and its info: shared by the pane and Quick Look. */
function usePreview() {
  const target = useTarget();
  const path = target && "path" in target ? target.path : null;
  const reload = target && "path" in target ? target.reload : "";
  const [info, setInfo] = useState<Info | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!path) return;
    let stale = false;
    // A short pause: arrowing through files doesn't read each one.
    const timer = window.setTimeout(() => {
      invoke<Info>("preview_info", { path }).then(
        (i) => {
          if (stale) return;
          setInfo(i);
          setError(null);
        },
        (err) => {
          if (stale) return;
          setInfo(null);
          setError(String(err));
        },
      );
    }, 60);
    return () => {
      stale = true;
      window.clearTimeout(timer);
    };
  }, [path, reload]);

  const shown = info && path && info.path.toLowerCase() === path.toLowerCase() ? info : null;
  const entry = target && "entry" in target ? target.entry : null;
  return { target, shown, error, reload, entry };
}

/** Open, Editor and Copy for the file shown. */
function Actions({ info }: { info: Info }) {
  const copyContents = async () => {
    try {
      const t = await invoke<Text>("preview_text", { path: info.path, limit: 64 << 20 });
      await navigator.clipboard.writeText(t.text);
      showToast(t.truncated ? "Copied the first 64 MB" : "Copied the contents");
    } catch (err) {
      showToast(String(err), "error");
    }
  };
  return (
    <div className="pv-actions">
      <button className="link-btn" onClick={() => void invoke("open_path", { path: info.path }).catch((e) => showToast(String(e), "error"))}>
        Open
      </button>
      {TEXTUAL.has(info.viewer) && (
        <>
          <button className="link-btn" onClick={() => openInEditor([info.path])}>
            Editor
          </button>
          <button className="link-btn" onClick={() => void copyContents()}>
            Copy
          </button>
        </>
      )}
    </div>
  );
}

/** What's shown for the target: the file, "N items selected", or an error. */
function Content({ preview, info: withInfo }: { preview: ReturnType<typeof usePreview>; info: boolean }) {
  const { target, shown, error, reload, entry } = preview;
  const [tab, setTab] = useState<"content" | "diff">("content");
  // A changed text file under git can show its diff.
  const diffable = !!shown && !!entry?.git && DIFFABLE_MARKS.has(entry.git) && DIFFABLE_VIEWERS.has(shown.viewer);
  const showDiff = diffable && tab === "diff";
  if (target && "count" in target)
    return (
      <div className="pane-empty">
        <FileIcon size={20} />
        <p>{target.count.toLocaleString()} items selected</p>
      </div>
    );
  if (error) return <Note>{error}</Note>;
  if (!shown)
    return (
      <div className="pane-empty">
        <FileIcon size={20} />
        <p>Select a file to preview it</p>
      </div>
    );
  return (
    <>
      <div className="pv-head">
        <span className="pv-name" title={shown.name}>
          {shown.name}
        </span>
        {diffable && <Modes value={tab} onChange={(v) => setTab(v as "content" | "diff")} options={[["content", "Content"], ["diff", "Diff"]]} />}
        <Actions info={shown} />
      </div>
      <div className="pv-body" key={shown.path + (showDiff ? ":diff" : "")}>
        {showDiff ? <DiffViewer info={shown} reload={reload} staged={!!entry?.staged} /> : <Body info={shown} reload={reload} />}
      </div>
      {withInfo && <InfoStrip info={shown} />}
    </>
  );
}

/** The preview pane on the right: follows the selection in the active tab. */
export function PreviewPane({ onClose }: { onClose: () => void }) {
  const preview = usePreview();
  return (
    <aside className="rightpane preview">
      <div className="pane-header">
        <span className="pane-title">Preview</span>
        <button className="icon-btn small" title="Close preview pane" onClick={onClose}>
          <CloseSmallIcon size={14} />
        </button>
      </div>
      <Content preview={preview} info />
    </aside>
  );
}

/**
 * Quick Look (Space): the selected item large, over the window. The keyboard stays in the list,
 * so the arrow keys move on to the next file; Space or Esc closes it.
 */
export function QuickLook({ onClose }: { onClose: () => void }) {
  const preview = usePreview();
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" || (e.key === " " && !(e.target as HTMLElement)?.closest?.("input, textarea, [contenteditable='true']"))) {
        e.preventDefault();
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener("keydown", onKey, { capture: true });
    return () => window.removeEventListener("keydown", onKey, { capture: true });
  }, [onClose]);
  return (
    <div className="quicklook-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="quicklook" role="dialog" aria-label="Quick Look">
        <button className="icon-btn small quicklook-close" title="Close (Space)" onClick={onClose}>
          <CloseSmallIcon size={14} />
        </button>
        <Content preview={preview} info={false} />
      </div>
    </div>
  );
}

export default PreviewPane;
