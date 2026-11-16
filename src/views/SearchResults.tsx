import { useVirtualizer } from "@tanstack/react-virtual";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useSyncExternalStore } from "react";
import { setCurrent } from "../lib/current";
import type { Entry } from "../lib/folder";
import { formatDate, formatSize } from "../lib/format";
import { closeSearch, type Hit, type SearchState } from "../lib/search";
import { navigate, newTab, parentOf } from "../lib/tabs";
import { FileTypeIcon } from "../shell/FileTypeIcon";
import { FolderIcon } from "../shell/Icons";
import { showToast } from "../shell/Toast";

/** The cursor in each tab's results (the keyboard stays in the search box). */
const cursors = new Map<string, number>();
const cursorListeners = new Set<() => void>();
let cursorVersion = 0;

export function getCursor(tabId: string): number {
  return cursors.get(tabId) ?? 0;
}

export function setCursor(tabId: string, i: number) {
  cursors.set(tabId, Math.max(0, i));
  cursorVersion++;
  for (const l of cursorListeners) l();
}

function useCursor(tabId: string): number {
  useSyncExternalStore(
    (l) => {
      cursorListeners.add(l);
      return () => {
        cursorListeners.delete(l);
      };
    },
    () => cursorVersion,
  );
  return getCursor(tabId);
}

/** A result as the preview and file icons want it. */
export function hitEntry(h: Hit): Entry {
  const dot = h.name.lastIndexOf(".");
  return {
    name: h.name,
    kind: h.folder ? "folder" : "file",
    size: h.size,
    modified: h.modified,
    created: 0,
    ext: h.folder || dot <= 0 ? "" : h.name.slice(dot + 1).toLowerCase(),
    hidden: false,
    link: false,
    readonly: false,
    sized: false,
    git: null,
    staged: false,
  };
}

/** Enter: open (a folder here, a file with its app); Ctrl+Enter: show it in its folder; Alt+Enter: in a new tab. */
export function openHit(tabId: string, h: Hit, how: "open" | "reveal" | "tab") {
  const folder = parentOf(h.path) ?? h.path;
  if (how === "tab") {
    newTab(h.folder ? h.path : folder, true, h.folder ? null : h.name);
  } else if (how === "reveal" || !h.folder) {
    if (how === "open") {
      void invoke("open_path", { path: h.path }).catch((err) => showToast(String(err), "error"));
      return;
    }
    navigate(folder, h.name);
  } else {
    navigate(h.path);
  }
  closeSearch(tabId);
}

function Highlighted({ name, indices }: { name: string; indices: number[] }) {
  if (!indices.length) return <>{name}</>;
  const set = new Set(indices);
  const chars = [...name];
  const out: React.ReactNode[] = [];
  let run = "";
  let marked = false;
  chars.forEach((c, i) => {
    const m = set.has(i);
    if (m !== marked && run) {
      out.push(marked ? <mark key={i}>{run}</mark> : run);
      run = "";
    }
    marked = m;
    run += c;
  });
  if (run) out.push(marked ? <mark key="end">{run}</mark> : run);
  return <>{out}</>;
}

/** Where a result is, relative to the folder searched ("here"), or in full. */
function where(h: Hit, base: string | null): string {
  const folder = parentOf(h.path) ?? "";
  if (base && folder.toLowerCase().startsWith(base.replace(/[\\/]+$/, "").toLowerCase())) {
    const rel = folder.slice(base.replace(/[\\/]+$/, "").length).replace(/^[\\/]+/, "");
    return rel || ".";
  }
  return folder;
}

/** The results of a search, in the tab's view. */
export function SearchResults({ tabId, search, active }: { tabId: string; search: SearchState; active: boolean }) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const hits = search.result?.hits ?? [];
  const cursor = Math.min(useCursor(tabId), Math.max(0, hits.length - 1));
  const rows = useVirtualizer({ count: hits.length, getScrollElement: () => scrollRef.current, estimateSize: () => 26, overscan: 12 });
  const base = search.scope === "here" ? search.folder : null;

  // New results: back to the top.
  useEffect(() => setCursor(tabId, 0), [tabId, search.result]);
  useEffect(() => {
    if (hits.length) rows.scrollToIndex(cursor, { align: "auto" });
  }, [cursor, hits.length, rows]);
  // The preview shows the result under the cursor.
  useEffect(() => {
    const h = hits[cursor];
    if (!active || !h) return;
    setCurrent({ folder: parentOf(h.path) ?? h.path, entry: hitEntry(h), count: 1 });
  }, [active, hits, cursor]);

  let note: string | null = null;
  if (search.error) note = search.error;
  else if (search.result && !hits.length) note = search.busy ? "Searching…" : "Nothing found.";
  else if (!search.result) note = search.busy ? "Searching…" : null;

  return (
    <div className="search-results">
      <div className="sr-head">
        <span>Name</span>
        <span>{base ? "Folder" : "Path"}</span>
        <span>Modified</span>
        <span className="right">Size</span>
      </div>
      <div className="sr-scroll" ref={scrollRef}>
        <div style={{ height: rows.getTotalSize(), position: "relative" }}>
          {rows.getVirtualItems().map((v) => {
            const h = hits[v.index];
            return (
              <div
                key={h.path}
                className={`sr-row${v.index === cursor ? " cursor" : ""}`}
                style={{ transform: `translateY(${v.start}px)`, height: v.size }}
                title={h.path}
                onMouseDown={() => setCursor(tabId, v.index)}
                onDoubleClick={() => openHit(tabId, h, "open")}
              >
                <span className="sr-name">
                  <span className={`fl-icon${h.folder ? " folder" : ""}`}>{h.folder ? <FolderIcon size={15} /> : <FileTypeIcon entry={hitEntry(h)} />}</span>
                  <span className="fl-text">
                    <Highlighted name={h.name} indices={h.indices} />
                  </span>
                </span>
                <span className="sr-where muted">{where(h, base)}</span>
                <span className="mono muted">{formatDate(h.modified)}</span>
                <span className="mono muted right">{h.folder ? "" : formatSize(h.size)}</span>
              </div>
            );
          })}
        </div>
        {note && <div className="fl-empty">{note}</div>}
      </div>
      {search.result && (
        <div className="sr-foot">
          {search.result.total.toLocaleString()} {search.result.total === 1 ? "match" : "matches"}
          {search.result.total > hits.length && ` · the best ${hits.length.toLocaleString()} shown`}
          {search.result.unsupported.length > 0 && ` · ${search.result.unsupported.join(", ")} isn't available in search yet`}
          <span className="spacer" />
          <span className="muted">↵ open · Ctrl+↵ show in folder · Alt+↵ new tab · Esc back</span>
        </div>
      )}
    </div>
  );
}
