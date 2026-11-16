import { Command as Cmdk, defaultFilter } from "cmdk";
import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import type { Command } from "../lib/commands";
import { Kbd } from "../shell/Kbd";
import { showToast } from "../shell/Toast";
import { formatDate } from "../lib/format";
import type { RepoInfo } from "../lib/git";
import { activeTab, navigate } from "../lib/tabs";

/**
 * The command bar: one box for everything, VS Code style. The first character picks the mode
 * (docs/PLAN.md section 6). Modes arrive with the features behind them; until then the bar says
 * so instead of showing nothing.
 */
interface Mode {
  prefix: string;
  name: string;
  /** What typing after the prefix does. */
  hint: string;
  placeholder: string;
  ready: boolean;
}

const MODES: Mode[] = [
  { prefix: "", name: "Go to", hint: "Go to a file or folder", placeholder: "Go to a file or folder…", ready: false },
  { prefix: ">", name: "Commands", hint: "Run a command", placeholder: "Type a command…", ready: true },
  { prefix: "%", name: "Search contents", hint: "Search inside files in this folder", placeholder: "Search file contents…", ready: false },
  { prefix: "#", name: "Git", hint: "Branches, changed files and commits", placeholder: "Branches, changes, commits…", ready: true },
  { prefix: "@", name: "Places", hint: "Favourites, projects and drives", placeholder: "Favourites, projects, drives…", ready: false },
  { prefix: "!", name: "Shell", hint: "Run a shell command in this folder", placeholder: "Shell command…", ready: false },
  { prefix: "?", name: "Help", hint: "List the modes", placeholder: "Modes…", ready: true },
];

const PREFIXES = MODES.map((m) => m.prefix).filter(Boolean);

function modeOf(text: string): Mode {
  return MODES.find((m) => m.prefix && text.startsWith(m.prefix)) ?? MODES[0];
}

/** The text after the mode's prefix. */
function queryOf(text: string): string {
  return PREFIXES.includes(text[0]) ? text.slice(1).trimStart() : text;
}

interface Props {
  commands: Command[];
  /** Text to open with: ">" for commands, "?" for help… */
  initialText: string;
  onClose: () => void;
}

export function CommandBar({ commands, initialText, onClose }: Props) {
  const [text, setText] = useState(initialText);
  const mode = modeOf(text);

  return (
    <div className="overlay" onMouseDown={onClose}>
      <div className="palette" onMouseDown={(e) => e.stopPropagation()}>
        <Cmdk
          label="Command bar"
          loop
          // Match on what follows the prefix: ">zoom" finds "View: Zoom In".
          filter={(value, search, keywords) => {
            const q = queryOf(search);
            return q ? defaultFilter(value, q, keywords) : 1;
          }}
        >
          <Cmdk.Input
            autoFocus
            value={text}
            onValueChange={setText}
            placeholder={`${mode.placeholder}  (? for help)`}
            onKeyDown={(e) => {
              if (e.key === "Escape") {
                e.preventDefault();
                onClose();
              } else if (e.key === "Backspace" && text.length === 1 && PREFIXES.includes(text)) {
                // Backspace on a bare prefix goes back to the default mode, like VS Code.
                e.preventDefault();
                setText("");
              }
            }}
          />
          <Cmdk.List>
            {mode.prefix === ">" && <CommandItems commands={commands} onClose={onClose} />}
            {mode.prefix === "?" && <HelpItems onPick={setText} />}
            {mode.prefix === "#" && <GitItems onClose={onClose} />}
            {!mode.ready && (
              <div className="palette-note">
                <strong>{mode.name}</strong>: {mode.hint.toLowerCase()}. This arrives in a later version; type{" "}
                <kbd>&gt;</kbd> for commands.
              </div>
            )}
            {mode.ready && <Cmdk.Empty>No matches</Cmdk.Empty>}
          </Cmdk.List>
          <div className="palette-footer">
            <span>
              <kbd>↵</kbd> {mode.prefix === "?" ? "Choose" : "Run"}
            </span>
            <span>
              <kbd>↑</kbd>
              <kbd>↓</kbd> Move
            </span>
            <span>
              <kbd>Esc</kbd> Close
            </span>
            <span className="spacer" />
            <span>{mode.name}</span>
          </div>
        </Cmdk>
      </div>
    </div>
  );
}

function CommandItems({ commands, onClose }: { commands: Command[]; onClose: () => void }) {
  const groups = new Map<string, Command[]>();
  for (const cmd of commands) {
    if (cmd.hidden) continue;
    const list = groups.get(cmd.category) ?? [];
    list.push(cmd);
    groups.set(cmd.category, list);
  }
  return (
    <>
      {[...groups].map(([category, items]) => (
        <Cmdk.Group heading={category} key={category}>
          {items.map((cmd) => (
            <Cmdk.Item
              key={cmd.id}
              value={`${cmd.category}: ${cmd.title}`}
              onSelect={() => {
                onClose();
                cmd.run();
              }}
            >
              <span className="palette-check" aria-hidden="true">
                {cmd.checked ? "✓" : ""}
              </span>
              <span className="palette-title">
                <span className="palette-category">{cmd.category}: </span>
                {cmd.title}
              </span>
              {cmd.keys?.[0] && <Kbd combo={cmd.keys[0]} />}
            </Cmdk.Item>
          ))}
        </Cmdk.Group>
      ))}
    </>
  );
}

function HelpItems({ onPick }: { onPick: (text: string) => void }) {
  return (
    <Cmdk.Group heading="Modes">
      {MODES.filter((m) => m.prefix !== "?").map((m) => (
        <Cmdk.Item key={m.name} value={`${m.prefix} ${m.name} ${m.hint}`} onSelect={() => onPick(m.prefix)}>
          <span className="palette-prefix mono">{m.prefix || "…"}</span>
          <span className="palette-title">
            {m.hint}
            {!m.ready && <span className="palette-category"> (later)</span>}
          </span>
        </Cmdk.Item>
      ))}
    </Cmdk.Group>
  );
}

interface GitFile {
  path: string;
  index: string;
  worktree: string;
  conflicted: boolean;
  untracked: boolean;
}
interface GitBranch {
  name: string;
  remote: boolean;
  current: boolean;
}
interface GitCommit {
  hash: string;
  short: string;
  author: string;
  date: number;
  subject: string;
}

/** `#`: the changed files, branches and recent commits of the repository you're in. */
function GitItems({ onClose }: { onClose: () => void }) {
  const [data, setData] = useState<{ root: string; files: GitFile[]; branches: GitBranch[]; commits: GitCommit[] } | null | "none">(null);
  useEffect(() => {
    const path = activeTab().path;
    void (async () => {
      const info = await invoke<RepoInfo | null>("git_info", { path });
      if (!info) return setData("none");
      const [status, branches, commits] = await Promise.all([
        invoke<{ files: GitFile[] } | null>("git_status", { path: info.root }),
        invoke<GitBranch[]>("git_branches", { root: info.root }).catch(() => []),
        invoke<GitCommit[]>("git_log", { root: info.root, count: 30 }).catch(() => []),
      ]);
      setData({ root: info.root, files: status?.files ?? [], branches, commits });
    })();
  }, []);
  if (data === null) return <div className="palette-note">Reading the repository…</div>;
  if (data === "none") return <div className="palette-note">This folder isn't in a git repository.</div>;
  const sep = data.root.includes("\\") ? "\\" : "/";
  const showFile = (rel: string) => {
    const full = data.root.replace(/[\\/]+$/, "") + sep + rel.replace(/\/$/, "").split("/").join(sep);
    const i = full.lastIndexOf(sep);
    onClose();
    navigate(full.slice(0, i), full.slice(i + 1));
  };
  const letter = (f: GitFile) => (f.conflicted ? "!" : f.untracked ? "U" : f.worktree !== "." ? f.worktree : f.index);
  return (
    <>
      {data.files.length > 0 && (
        <Cmdk.Group heading="Changed files">
          {data.files.map((f) => (
            <Cmdk.Item key={`f:${f.path}`} value={`file ${f.path}`} onSelect={() => showFile(f.path)}>
              <span className="palette-prefix mono">{letter(f)}</span>
              <span className="palette-title">{f.path}</span>
            </Cmdk.Item>
          ))}
        </Cmdk.Group>
      )}
      <Cmdk.Group heading="Branches">
        {data.branches.map((b) => (
          <Cmdk.Item
            key={`b:${b.name}`}
            value={`branch ${b.name}`}
            onSelect={() => {
              onClose();
              if (b.current) return;
              const name = b.remote ? b.name.replace(/^[^/]+\//, "") : b.name;
              void invoke("git_branch_op", { root: data.root, op: "switch", name }).then(
                () => showToast(`Switched to ${name}`),
                (err) => showToast(String(err), "error"),
              );
            }}
          >
            <span className="palette-check" aria-hidden="true">
              {b.current ? "✓" : ""}
            </span>
            <span className="palette-title">
              {b.name}
              {b.remote && <span className="palette-category"> (remote)</span>}
            </span>
          </Cmdk.Item>
        ))}
      </Cmdk.Group>
      {data.commits.length > 0 && (
        <Cmdk.Group heading="Recent commits (Enter copies the hash)">
          {data.commits.map((c) => (
            <Cmdk.Item
              key={`c:${c.hash}`}
              value={`commit ${c.short} ${c.subject} ${c.author}`}
              onSelect={() => {
                onClose();
                void navigator.clipboard.writeText(c.hash).then(() => showToast(`Copied ${c.short}`));
              }}
            >
              <span className="palette-prefix mono">{c.short}</span>
              <span className="palette-title">{c.subject}</span>
              <span className="palette-category">
                {c.author} · {formatDate(c.date * 1000)}
              </span>
            </Cmdk.Item>
          ))}
        </Cmdk.Group>
      )}
    </>
  );
}
