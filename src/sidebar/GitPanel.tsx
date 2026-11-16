import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState, type KeyboardEvent } from "react";
import { branchLabel, onGitChanged, useGitInfo, type RepoInfo } from "../lib/git";
import { fmtShortcut } from "../lib/platform";
import { activeTab, navigate, useTabs } from "../lib/tabs";
import { askConfirm } from "../shell/Confirm";
import { Menu, type MenuItem } from "../shell/Menu";
import { showToast } from "../shell/Toast";

/** Mirrors `FileStatus` in src-tauri/src/git/status.rs. */
interface FileStatus {
  path: string;
  index: string;
  worktree: string;
  orig: string | null;
  conflicted: boolean;
  untracked: boolean;
  ignored: boolean;
}

interface BranchInfo {
  name: string;
  remote: boolean;
  current: boolean;
  upstream: string | null;
  date: number;
}

/** Repo-relative "src/a.rs" as a full path. */
function full(root: string, rel: string): string {
  const sep = root.includes("\\") ? "\\" : "/";
  return root.replace(/[\\/]+$/, "") + sep + rel.replace(/\/$/, "").split("/").join(sep);
}

function useRepoFiles(info: RepoInfo | null): FileStatus[] {
  const [files, setFiles] = useState<FileStatus[]>([]);
  const root = info?.root ?? null;
  useEffect(() => {
    if (!root) return setFiles([]);
    let stale = false;
    const load = () =>
      void invoke<{ root: string; files: FileStatus[] } | null>("git_status", { path: root }).then((s) => {
        if (!stale) setFiles(s?.files ?? []);
      });
    load();
    const off = onGitChanged((r) => r.toLowerCase() === root.toLowerCase() && load());
    return () => {
      stale = true;
      off();
    };
  }, [root]);
  return files;
}

async function run(what: string, f: () => Promise<unknown>): Promise<boolean> {
  try {
    await f();
    return true;
  } catch (err) {
    showToast(`${what}: ${String(err)}`, "error");
    return false;
  }
}

/** One changed file: its letter, name and folder, and what can be done with it. */
function FileRow({ root, f, staged, onAction }: { root: string; f: FileStatus; staged: boolean; onAction: (op: string, files: FileStatus[]) => void }) {
  const path = f.path.replace(/\/$/, "");
  const slash = path.lastIndexOf("/");
  const name = path.slice(slash + 1) + (f.path.endsWith("/") ? "/" : "");
  const dir = slash > 0 ? path.slice(0, slash) : "";
  const letter = f.conflicted ? "!" : f.untracked ? "U" : (staged ? f.index : f.worktree).replace("?", "U");
  const kind = f.conflicted ? "conflicted" : f.untracked ? "untracked" : { M: "modified", A: "added", D: "deleted", R: "renamed", C: "renamed", T: "modified" }[letter] ?? "modified";
  const open = () => {
    const target = full(root, path);
    const sep = target.includes("\\") ? "\\" : "/";
    const i = target.lastIndexOf(sep);
    navigate(target.slice(0, i), target.slice(i + 1));
  };
  return (
    <div className={`git-file git-${kind}`} role="button" tabIndex={0} title={f.orig ? `${f.orig} → ${f.path}` : f.path} onClick={open} onKeyDown={(e) => e.key === "Enter" && open()}>
      <span className="git-file-name">{name}</span>
      <span className="git-file-dir">{dir}</span>
      <span className="git-file-actions" onClick={(e) => e.stopPropagation()}>
        {staged ? (
          <button title="Unstage" onClick={() => onAction("unstage", [f])}>
            −
          </button>
        ) : (
          <>
            <button title={f.untracked ? "Delete (to the Recycle Bin)" : "Discard changes"} onClick={() => onAction("discard", [f])}>
              ↺
            </button>
            <button title="Stage" onClick={() => onAction("stage", [f])}>
              +
            </button>
          </>
        )}
      </span>
      <span className={`git-file-letter git-${kind}`}>{letter}</span>
    </div>
  );
}

/** The git panel: the repository of the folder you're in, its changes, commit, branches and sync. */
export function GitPanel() {
  useTabs();
  const path = activeTab().path;
  const info = useGitInfo(path);
  const files = useRepoFiles(info);
  const [message, setMessage] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const [progress, setProgress] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const [newBranch, setNewBranch] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const box = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    const un = listen<{ root: string; op: string; line: string }>("git-progress", (e) => {
      if (info && e.payload.root.toLowerCase() === info.root.toLowerCase()) setProgress(e.payload.line);
    });
    return () => void un.then((f) => f());
  }, [info?.root]);

  const root = info?.root ?? "";
  const conflicts = files.filter((f) => f.conflicted);
  const staged = files.filter((f) => !f.conflicted && !f.untracked && f.index !== ".");
  const changes = files.filter((f) => !f.conflicted && !f.untracked && f.worktree !== ".");
  const untracked = files.filter((f) => f.untracked);

  const act = useCallback(
    async (op: string, list: FileStatus[]) => {
      if (!list.length) return;
      const paths = list.map((f) => full(root, f.path));
      if (op === "discard") {
        const gone = list.filter((f) => f.untracked);
        const tracked = list.filter((f) => !f.untracked);
        const what = list.length === 1 ? `“${list[0].path}”` : `${list.length} files`;
        const ok = await askConfirm({
          title: "Discard changes?",
          body: gone.length && !tracked.length ? `${what} will go to the Recycle Bin.` : `The unstaged changes to ${what} will be lost. Untracked files go to the Recycle Bin.`,
          confirm: "Discard",
          danger: true,
        });
        if (!ok) return;
        if (tracked.length) await run("Discard", () => invoke("git_paths", { op: "discard", paths: tracked.map((f) => full(root, f.path)) }));
        if (gone.length) await run("Delete", () => invoke("op_trash", { paths: gone.map((f) => full(root, f.path)) }));
        return;
      }
      await run(op === "stage" ? "Stage" : "Unstage", () => invoke("git_paths", { op, paths }));
    },
    [root],
  );

  const commit = async () => {
    if (!info || busy) return;
    if (!staged.length) return showToast("Stage the changes to commit first (+).");
    setBusy("commit");
    const ok = await run("Commit", async () => {
      const line = await invoke<string>("git_commit", { root, message, amend: false });
      showToast(line || "Committed");
    });
    setBusy(null);
    if (ok) setMessage("");
  };

  const sync = async (op: "fetch" | "pull" | "push") => {
    if (!info || busy) return;
    setBusy(op);
    setProgress(null);
    await run(op[0].toUpperCase() + op.slice(1), async () => showToast(await invoke<string>("git_sync", { root, op })));
    setBusy(null);
    setProgress(null);
  };

  const branchOp = (op: string, name: string, done: string) =>
    run(done, async () => {
      await invoke("git_branch_op", { root, op, name });
      showToast(done);
    });

  const branchMenu = async (x: number, y: number) => {
    const list = await invoke<BranchInfo[]>("git_branches", { root }).catch(() => []);
    const local = list.filter((b) => !b.remote);
    const remote = list.filter((b) => b.remote && !local.some((l) => b.name.endsWith(`/${l.name}`)));
    const items: MenuItem[] = [
      { label: "New Branch…", onSelect: () => setNewBranch("") },
      "separator",
      ...local.slice(0, 30).map((b): MenuItem => ({ label: b.name, checked: b.current, onSelect: () => !b.current && void branchOp("switch", b.name, `Switched to ${b.name}`) })),
    ];
    if (remote.length) {
      items.push("separator", {
        label: "Remote Branches",
        submenu: remote.slice(0, 40).map((b): MenuItem => ({ label: b.name, onSelect: () => void branchOp("switch", b.name.replace(/^[^/]+\//, ""), `Switched to ${b.name}`) })),
      });
    }
    setMenu({ x, y, items });
  };

  const moreMenu = (x: number, y: number) =>
    setMenu({
      x,
      y,
      items: [
        { label: "Stash Changes", onSelect: () => void branchOp("stash", message, "Stashed") },
        { label: "Pop Latest Stash", onSelect: () => void branchOp("pop", "", "Applied the stash") },
        "separator",
        { label: "Stage All", disabled: !changes.length && !untracked.length, onSelect: () => void act("stage", [...changes, ...untracked]) },
        { label: "Unstage All", disabled: !staged.length, onSelect: () => void act("unstage", staged) },
      ],
    });

  if (!info)
    return (
      <div className="git-panel">
        <p className="side-hint">This folder isn't in a git repository.</p>
      </div>
    );

  const group = (id: string, title: string, list: FileStatus[], isStaged: boolean, actions: { label: string; op: string }[]) =>
    list.length > 0 && (
      <section className="git-group">
        <div className="git-group-head">
          <button className="side-head-btn" onClick={() => setCollapsed((c) => ({ ...c, [id]: !c[id] }))}>
            <span className="pane-title">{title}</span>
            <span className="git-count">{list.length}</span>
          </button>
          <span className="git-group-actions">
            {actions.map((a) => (
              <button key={a.op} className="link-btn" onClick={() => void act(a.op, list)}>
                {a.label}
              </button>
            ))}
          </span>
        </div>
        {!collapsed[id] && list.map((f) => <FileRow key={`${id}:${f.path}`} root={root} f={f} staged={isStaged} onAction={(op, l) => void act(op, l)} />)}
      </section>
    );

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
      e.preventDefault();
      void commit();
    }
  };

  return (
    <div className="git-panel">
      <div className="git-repo">
        <button className="git-branch" title="Switch or create a branch" onClick={(e) => void branchMenu(e.clientX, e.clientY)}>
          ⎇ {branchLabel(info)} ▾
        </button>
        <span className="git-ab" title={info.upstream ? `Tracking ${info.upstream}` : "No upstream branch"}>
          {info.ahead > 0 && `↑${info.ahead} `}
          {info.behind > 0 && `↓${info.behind}`}
        </span>
        <span className="git-sync">
          <button className="link-btn" disabled={!!busy} onClick={() => void sync("fetch")} title="Fetch">
            Fetch
          </button>
          <button className="link-btn" disabled={!!busy} onClick={() => void sync("pull")} title="Pull">
            Pull
          </button>
          <button className="link-btn" disabled={!!busy} onClick={() => void sync("push")} title="Push">
            Push
          </button>
          <button className="icon-btn small" title="More" onClick={(e) => moreMenu(e.clientX, e.clientY)}>
            ⋯
          </button>
        </span>
      </div>
      {busy && busy !== "commit" && <div className="git-progress">{progress ?? `${busy[0].toUpperCase()}${busy.slice(1)}ing…`}</div>}
      {info.state && <div className="git-state-note">A {info.state} is in progress. Resolve conflicts, stage, then commit (or abort it in a terminal).</div>}
      {newBranch !== null && (
        <form
          className="git-new-branch"
          onSubmit={(e) => {
            e.preventDefault();
            const name = newBranch.trim();
            if (name) void branchOp("create", name, `Created ${name}`).then((ok) => ok && setNewBranch(null));
          }}
        >
          <input autoFocus placeholder="New branch name" value={newBranch} onChange={(e) => setNewBranch(e.target.value)} onKeyDown={(e) => e.key === "Escape" && setNewBranch(null)} />
        </form>
      )}
      <div className="git-commit">
        <textarea
          ref={box}
          rows={3}
          placeholder={`Commit message (${fmtShortcut("Ctrl+Enter")} to commit)`}
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          onKeyDown={onKey}
        />
        <button className="btn primary" disabled={!staged.length || !message.trim() || !!busy} onClick={() => void commit()}>
          {busy === "commit" ? "Committing…" : staged.length ? `Commit ${staged.length} staged` : "Commit"}
        </button>
      </div>
      <div className="git-groups">
        {group("conflicts", "Conflicts", conflicts, false, [{ label: "Stage all", op: "stage" }])}
        {group("staged", "Staged", staged, true, [{ label: "Unstage all", op: "unstage" }])}
        {group("changes", "Changes", changes, false, [
          { label: "Discard all", op: "discard" },
          { label: "Stage all", op: "stage" },
        ])}
        {group("untracked", "Untracked", untracked, false, [{ label: "Stage all", op: "stage" }])}
        {info.ready && !files.length && <p className="side-hint">No changes.</p>}
      </div>
      {menu && <Menu x={menu.x} y={menu.y} items={menu.items} onClose={() => setMenu(null)} />}
    </div>
  );
}
