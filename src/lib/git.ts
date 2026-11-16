import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";

/** Mirrors `RepoInfo` in src-tauri/src/git_cmd.rs. */
export interface RepoInfo {
  root: string;
  name: string;
  branch: string | null;
  detached: string | null;
  upstream: string | null;
  ahead: number;
  behind: number;
  state: "merge" | "rebase" | "cherry-pick" | "revert" | "bisect" | null;
  staged: number;
  changed: number;
  untracked: number;
  conflicted: number;
  ready: boolean;
  error: string | null;
}

const listeners = new Set<(root: string) => void>();
let listening = false;

/** Call `f` with a repository's root whenever its status changes (one event listener for all). */
export function onGitChanged(f: (root: string) => void): () => void {
  if (!listening) {
    listening = true;
    void listen<string>("git-changed", (e) => {
      for (const l of listeners) l(e.payload);
    });
  }
  listeners.add(f);
  return () => {
    listeners.delete(f);
  };
}

const sameRoot = (a: string, b: string) => a.replace(/[\\/]+$/, "").toLowerCase() === b.replace(/[\\/]+$/, "").toLowerCase();

/** The repository `path` is in, kept current; null outside one. */
export function useGitInfo(path: string | null): RepoInfo | null {
  const [info, setInfo] = useState<RepoInfo | null>(null);
  useEffect(() => {
    if (!path) {
      setInfo(null);
      return;
    }
    let stale = false;
    let root: string | null = null;
    const load = () =>
      void invoke<RepoInfo | null>("git_info", { path }).then((i) => {
        if (stale) return;
        root = i?.root ?? null;
        setInfo(i);
      });
    load();
    const off = onGitChanged((changed) => {
      if (root && sameRoot(changed, root)) load();
    });
    return () => {
      stale = true;
      off();
    };
  }, [path]);
  return info;
}

/** "main", or "detached at 1a2b3c4". */
export function branchLabel(info: RepoInfo): string {
  return info.branch ?? (info.detached ? `detached at ${info.detached}` : "no branch");
}
