import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState, useSyncExternalStore } from "react";
import type { FolderModel, SelectionInfo } from "../lib/folder";
import { formatSize } from "../lib/format";
import { branchLabel, useGitInfo, type RepoInfo } from "../lib/git";
import { useIndexStatus } from "../lib/search";
import type { Selection } from "./selection";

const plural = (n: number, one: string, many = `${one}s`) => `${n.toLocaleString()} ${n === 1 ? one : many}`;

/**
 * Under the folder: how many items (and hidden ones), what's selected and its size, loading
 * progress for big folders, and the drive's free space.
 */
export function StatusBar({ model, selection, active }: { model: FolderModel; selection: Selection; active: boolean }) {
  useSyncExternalStore(model.subscribe, () => model.version);
  useSyncExternalStore(selection.subscribe, () => selection.version);
  const [info, setInfo] = useState<SelectionInfo | null>(null);
  const [space, setSpace] = useState<[number, number] | null>(null);

  // The selection's size, after a short pause (a held-down Shift+arrow changes it many times).
  const names = selection.names;
  useEffect(() => {
    if (names.size === 0) {
      setInfo(null);
      return;
    }
    const timer = window.setTimeout(() => void model.selectionInfo([...names]).then(setInfo), 120);
    return () => window.clearTimeout(timer);
  }, [names, model, model.generation]);

  // Free space: when the folder opens or its tab comes back.
  useEffect(() => {
    if (!active) return;
    let stale = false;
    void invoke<[number, number] | null>("drive_space", { path: model.path })
      .then((s) => !stale && setSpace(s))
      .catch(() => {});
    return () => {
      stale = true;
    };
  }, [model.path, active]);

  const git = useGitInfo(active ? model.path : null);
  const index = useIndexStatus();

  const hidden = model.total - model.count;
  const parts: string[] = [];
  if (model.loading && model.count > 0) parts.push(`Loading… ${model.count.toLocaleString()}`);
  else if (!model.error) parts.push(plural(model.count, "item"));
  if (hidden > 0 && !model.loading) parts.push(`${hidden.toLocaleString()} hidden`);

  let selected: string | null = null;
  if (names.size > 0) {
    selected = `${plural(names.size, "selected", "selected")}`;
    if (info && (info.files > 0 || info.bytes > 0)) selected += ` · ${formatSize(info.bytes)}`;
    if (info && info.folders > 0 && info.files > 0) selected += ` in ${plural(info.files, "file")}`;
  }

  return (
    <footer className="statusbar">
      <span>{parts.join(" · ")}</span>
      {selected && <span className="status-selected">{selected}</span>}
      <span className="spacer" />
      {index && (index.phase === "building" || index.phase === "reconciling") && (
        <span className="muted status-index" title="The search index is being brought up to date">
          {index.phase === "building" ? `Indexing… ${index.progress.toLocaleString()}` : "Updating the index…"}
        </span>
      )}
      {git && <GitStatus info={git} />}
      {space && (
        <span className="muted" title={`${formatSize(space[0])} free of ${formatSize(space[1])}`}>
          {formatSize(space[0])} free
        </span>
      )}
    </footer>
  );
}

const STATE_LABEL: Record<NonNullable<RepoInfo["state"]>, string> = {
  merge: "Merging",
  rebase: "Rebasing",
  "cherry-pick": "Cherry-picking",
  revert: "Reverting",
  bisect: "Bisecting",
};

/** The branch, ahead/behind, what's in progress and what changed, for the repository here. */
function GitStatus({ info }: { info: RepoInfo }) {
  const changes = info.staged + info.changed + info.untracked + info.conflicted;
  const title = [
    `Repository: ${info.root}`,
    info.upstream ? `Tracking ${info.upstream}` : "No upstream branch",
    info.ready ? `${info.staged} staged, ${info.changed} changed, ${info.untracked} untracked, ${info.conflicted} conflicted` : "Reading status…",
    info.error ?? "",
  ]
    .filter(Boolean)
    .join("\n");
  return (
    <span className="status-git" title={title}>
      <span className="branch">⎇ {branchLabel(info)}</span>
      {info.ahead > 0 && <span title={`${info.ahead} to push`}>↑{info.ahead}</span>}
      {info.behind > 0 && <span title={`${info.behind} to pull`}>↓{info.behind}</span>}
      {info.state && <span className="git-state">{STATE_LABEL[info.state]}</span>}
      {info.conflicted > 0 && <span className="git-conflicted-count">{info.conflicted} conflicted</span>}
      {info.ready && changes > 0 && <span className="muted">{changes} changed</span>}
      {info.error && <span className="muted">git unavailable</span>}
    </span>
  );
}
