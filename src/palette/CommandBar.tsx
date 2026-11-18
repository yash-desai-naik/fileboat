import { Command as Cmdk, defaultFilter } from "cmdk";
import { Channel, invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState, type MutableRefObject } from "react";
import type { Command } from "../lib/commands";
import { Kbd } from "../shell/Kbd";
import { showToast } from "../shell/Toast";
import { formatDate } from "../lib/format";
import type { RepoInfo } from "../lib/git";
import { activeTab, navigate, newTab, parentOf, samePath, tabTitle } from "../lib/tabs";
import { getRecents } from "../lib/recents";
import { getSettings } from "../lib/settings";
import { askConfirm } from "../shell/Confirm";
import type { Hit, SearchResult } from "../lib/search";
import { requestLine } from "../lib/current";
import { updateSettings } from "../lib/settings";
import { previewTheme } from "../themes";

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
  { prefix: "", name: "Go to", hint: "Go to a file or folder (name:line, name@symbol)", placeholder: "Go to a file or folder (name:32 for a line, name@ for symbols), or a path…", ready: true },
  { prefix: ">", name: "Commands", hint: "Run a command", placeholder: "Type a command…", ready: true },
  { prefix: "%", name: "Search contents", hint: "Search inside files in this folder", placeholder: "Text to find in files here (/regex/ for a pattern)…", ready: true },
  { prefix: "#", name: "Git", hint: "Branches, changed files and commits", placeholder: "Branches, changes, commits…", ready: true },
  { prefix: "@", name: "Places", hint: "Favourites, recent folders and drives", placeholder: "Favourites, recent folders, drives…", ready: true },
  { prefix: "!", name: "Shell", hint: "Run a shell command in this folder", placeholder: "Shell command (Enter runs it here)…", ready: true },
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

/** A path typed in the Go to box ("C:\\", "~/", "/usr", "..", "\\\\server"): path mode. */
function looksLikePath(text: string): boolean {
  return /^([a-z]:[\\/]?|~|\.\.?([\\/]|$)|\/|\\\\)/i.test(text);
}

/** The theme the bar is previewing (null: the saved one), to skip repeat repaints. */
let previewing: string | null = null;

function setPreview(id: string | null) {
  if (id === previewing) return;
  previewing = id;
  previewTheme(id);
}

/** What cmdk calls a command item: `${category}: ${title}`. */
const commandValue = (cmd: Command) => `${cmd.category}: ${cmd.title}`;

export function CommandBar({ commands, initialText, onClose }: Props) {
  const [text, setText] = useState(initialText);
  const mode = modeOf(text);
  // Enter's modifiers, for Ctrl+Enter (show in folder) and Alt+Enter (new tab).
  const mods = useRef({ ctrl: false, alt: false });
  const target = mode.prefix === "" ? parseTarget(text) : null;
  const pathMode = mode.prefix === "" && looksLikePath(text) && target?.symbol === undefined;

  // Like VS Code, a highlighted theme command (arrows or mouse) shows its theme until another is
  // highlighted; Enter keeps it, Esc or leaving the commands puts the saved theme back.
  const inCommands = mode.prefix === ">";
  useEffect(() => {
    if (!inCommands) setPreview(null);
  }, [inCommands]);
  useEffect(() => () => setPreview(null), []);

  return (
    <div className="overlay" onMouseDown={onClose}>
      <div className="palette" onMouseDown={(e) => e.stopPropagation()}>
        <Cmdk
          label="Command bar"
          loop
          // Content search results are already matches: no filtering on top.
          shouldFilter={!["%", "", "!"].includes(mode.prefix)}
          onValueChange={(value) => {
            if (!inCommands) return;
            setPreview(commands.find((c) => !c.hidden && commandValue(c) === value)?.previewTheme ?? null);
          }}
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
              if (e.key === "Enter") mods.current = { ctrl: e.ctrlKey || e.metaKey, alt: e.altKey };
              if (e.key === "Tab" && pathMode) {
                // Tab takes the highlighted completion.
                e.preventDefault();
                const v = document.querySelector<HTMLElement>("[cmdk-item][data-selected='true']")?.dataset.path;
                if (v) setText(v);
                return;
              }
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
            {mode.prefix === "%" && <ContentItems query={queryOf(text)} onClose={onClose} />}
            {target?.symbol !== undefined && <SymbolItems file={target.name} query={target.symbol} mods={mods} onClose={onClose} />}
            {mode.prefix === "" && !pathMode && target?.symbol === undefined && <GoToItems query={target?.name ?? text} at={target} mods={mods} onClose={onClose} />}
            {pathMode && <PathItems text={target?.line ? target.name : text} at={target} mods={mods} onClose={onClose} />}
            {mode.prefix === "@" && <PlaceItems mods={mods} onClose={onClose} />}
            {mode.prefix === "!" && <ShellItems line={queryOf(text)} />}
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
              value={commandValue(cmd)}
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

interface LineMatch {
  path: string;
  line: number;
  text: string;
  ranges: [number, number][];
}

type GrepEvent =
  | { type: "matches"; items: LineMatch[] }
  | { type: "done"; files: number; lines: number; limited: boolean; ms: number }
  | { type: "error"; message: string };

function Marked({ text, ranges }: { text: string; ranges: [number, number][] }) {
  const chars = [...text];
  const out: React.ReactNode[] = [];
  let at = 0;
  for (const [a, b] of ranges) {
    if (a < at) continue;
    out.push(chars.slice(at, a).join(""));
    out.push(<mark key={a}>{chars.slice(a, b).join("")}</mark>);
    at = b;
  }
  out.push(chars.slice(at).join(""));
  return <>{out}</>;
}

/** `%`: text inside the files of the folder you're in, streamed as found. */
function ContentItems({ query, onClose }: { query: string; onClose: () => void }) {
  const folder = activeTab().path;
  const [lines, setLines] = useState<LineMatch[]>([]);
  const [state, setState] = useState<string>("");
  useEffect(() => {
    setLines([]);
    const q = query.trim();
    if (!q) return setState("Type to search inside the files here (.gitignore and the index's rules apply).");
    const regex = q.length > 2 && q.startsWith("/") && q.endsWith("/");
    const pattern = regex ? q.slice(1, -1) : q;
    let id: string | null = null;
    let stale = false;
    setState("Searching…");
    const timer = window.setTimeout(() => {
      const channel = new Channel<GrepEvent>();
      channel.onmessage = (e) => {
        if (stale) return;
        if (e.type === "matches") setLines((l) => (l.length > 2000 ? l : [...l, ...e.items]));
        else if (e.type === "done") setState(`${e.lines.toLocaleString()} lines in ${e.files.toLocaleString()} files searched, ${e.ms} ms${e.limited ? " (stopped at the limit)" : ""}`);
        else setState(e.message);
      };
      void invoke<string>("grep_start", { path: folder, pattern, regex, case: false, wholeWord: false, onEvent: channel }).then((g) => {
        id = g;
        if (stale) void invoke("grep_cancel", { id: g });
      });
    }, 250);
    return () => {
      stale = true;
      window.clearTimeout(timer);
      if (id) void invoke("grep_cancel", { id });
    };
  }, [query, folder]);

  const sep = folder.includes("\\") ? "\\" : "/";
  const base = folder.replace(/[\\/]+$/, "") + sep;
  const groups = new Map<string, LineMatch[]>();
  for (const l of lines) groups.set(l.path, [...(groups.get(l.path) ?? []), l]);
  const show = (m: LineMatch) => {
    onClose();
    const i = m.path.lastIndexOf(sep);
    updateSettings({ showPreview: true });
    navigate(m.path.slice(0, i), m.path.slice(i + 1));
    requestLine(m.path, m.line);
  };
  return (
    <>
      <div className="palette-note">{state}</div>
      {[...groups].map(([path, ls]) => (
        <Cmdk.Group key={path} heading={path.startsWith(base) ? path.slice(base.length) : path}>
          {ls.map((m) => (
            <Cmdk.Item key={`${path}:${m.line}`} value={`${path}:${m.line}`} onSelect={() => show(m)}>
              <span className="palette-prefix mono">{m.line}</span>
              <span className="palette-title mono grep-line">
                <Marked text={m.text} ranges={m.ranges} />
              </span>
            </Cmdk.Item>
          ))}
        </Cmdk.Group>
      ))}
    </>
  );
}

type Mods = MutableRefObject<{ ctrl: boolean; alt: boolean }>;

/** What the Go to box asks for: a name, maybe with a line ("daily_logs.ts:32", ":32:5") or a symbol ("daily_logs.ts@load"). */
interface Target {
  name: string;
  line?: number;
  column?: number;
  /** Set (maybe "") for name@symbol. */
  symbol?: string;
}

function parseTarget(text: string): Target {
  const at = text.lastIndexOf("@");
  if (at > 0) return { name: text.slice(0, at).trim(), symbol: text.slice(at + 1).trim() };
  const m = /^(.*\S):(\d+)(?::(\d+))?$/.exec(text);
  if (m && !/^[a-z]$/i.test(m[1])) return { name: m[1].trim(), line: Number(m[2]), column: m[3] ? Number(m[3]) : undefined };
  return { name: text };
}

/**
 * A file at a line: Enter opens it in your editor there, Ctrl+Enter shows it here (selected, the
 * preview at that line), Alt+Enter does that in a new tab.
 */
function openAt(path: string, line: number, column: number | undefined, mods: Mods, onClose: () => void) {
  const { ctrl, alt } = mods.current;
  mods.current = { ctrl: false, alt: false };
  onClose();
  if (!ctrl && !alt) {
    void invoke("open_in_editor_at", { path, line, column: column ?? null, editor: null }).catch((err) => showToast(String(err), "error"));
    return;
  }
  const parent = parentOf(path) ?? path;
  const name = path.slice(parent.replace(/[\\/]+$/, "").length).replace(/^[\\/]+/, "");
  updateSettings({ showPreview: true });
  if (alt) newTab(parent, true, name);
  else navigate(parent, name);
  requestLine(path, line);
}

/** Go to a file or folder: Enter opens it (a folder: goes there), Ctrl+Enter shows it in its folder, Alt+Enter opens a tab. */
function go(path: string, folder: boolean, mods: Mods, onClose: () => void) {
  const { ctrl, alt } = mods.current;
  mods.current = { ctrl: false, alt: false };
  onClose();
  const parent = parentOf(path) ?? path;
  const name = path.slice(parent.replace(/[\\/]+$/, "").length).replace(/^[\\/]+/, "");
  if (alt) return newTab(folder ? path : parent, true, folder ? null : name);
  if (ctrl || !folder) {
    if (!ctrl && !folder) return void invoke("open_path", { path }).catch((err) => showToast(String(err), "error"));
    return navigate(parent, name);
  }
  navigate(path);
}

/** Letters of `q` in order in `s` (case aside). */
function fuzzy(s: string, q: string): boolean {
  let i = 0;
  const lower = s.toLowerCase();
  for (const c of q.toLowerCase()) {
    if (c === " ") continue;
    i = lower.indexOf(c, i);
    if (i < 0) return false;
    i++;
  }
  return true;
}

function HitName({ h }: { h: Hit }) {
  const set = new Set(h.indices);
  return (
    <>
      {[...h.name].map((c, i) => (set.has(i) ? <mark key={i}>{c}</mark> : c))}
    </>
  );
}

/** (nothing): recent folders and favourites first, then the index. */
function GoToItems({ query, at, mods, onClose }: { query: string; at: Target | null; mods: Mods; onClose: () => void }) {
  const line = at?.line;
  const [hits, setHits] = useState<Hit[]>([]);
  const q = query.trim();
  useEffect(() => {
    if (!q) return setHits([]);
    let stale = false;
    const timer = window.setTimeout(() => {
      void invoke<SearchResult>("index_search", { query: q, scope: null, limit: 40 }).then(
        (r) => !stale && setHits(r.hits),
        () => !stale && setHits([]),
      );
    }, 80);
    return () => {
      stale = true;
      window.clearTimeout(timer);
    };
  }, [q]);
  const favourites = getSettings().favourites;
  const places = [...favourites.map((p) => ({ path: p, why: "Favourite" })), ...getRecents().filter((p) => !favourites.some((f) => samePath(f, p))).map((p) => ({ path: p, why: "Recent" }))]
    .filter((p) => !q || fuzzy(tabTitle(p.path), q) || fuzzy(p.path, q))
    .slice(0, q ? 6 : 12);
  return (
    <>
      {places.length > 0 && (
        <Cmdk.Group heading={q ? "Places" : "Recent and favourite folders"}>
          {places.map((p) => (
            <Cmdk.Item key={`p:${p.path}`} value={`p:${p.path}`} onSelect={() => go(p.path, true, mods, onClose)}>
              <span className="palette-title">{tabTitle(p.path)}</span>
              <span className="palette-category">{p.path}</span>
              <span className="palette-category"> · {p.why}</span>
            </Cmdk.Item>
          ))}
        </Cmdk.Group>
      )}
      {hits.length > 0 && (
        <Cmdk.Group heading="Files and folders">
          {hits.map((h) => (
            <Cmdk.Item
              key={`h:${h.path}`}
              value={`h:${h.path}`}
              onSelect={() => (line && !h.folder ? openAt(h.path, line, at?.column, mods, onClose) : go(h.path, h.folder, mods, onClose))}
            >
              <span className="palette-title">
                <HitName h={h} />
                {h.folder ? "/" : line ? <span className="palette-line">:{line}</span> : ""}
              </span>
              <span className="palette-category">{parentOf(h.path)}</span>
            </Cmdk.Item>
          ))}
        </Cmdk.Group>
      )}
      {!q && (
        <div className="palette-note">
          Type a name, or a path (C:\\, ~/, ..). Add :32 to open a file at line 32 in your editor, or @ for its functions, classes and headings. Ctrl+Enter shows it here, Alt+Enter in a new tab.
        </div>
      )}
      {q && line && <div className="palette-note">Enter opens the file at line {line} in your editor; Ctrl+Enter shows it here with the preview at that line.</div>}
      {q && !places.length && !hits.length && <div className="palette-note">No matches in the index.</div>}
    </>
  );
}

/** A path typed: each part completed (Tab takes the highlighted one), Enter goes there. */
function PathItems({ text, at, mods, onClose }: { text: string; at: Target | null; mods: Mods; onClose: () => void }) {
  const [list, setList] = useState<string[]>([]);
  const base = activeTab().path;
  useEffect(() => {
    let stale = false;
    void invoke<string[]>("path_complete", { input: text, base, limit: 30 }).then((l) => !stale && setList(l));
    return () => {
      stale = true;
    };
  }, [text, base]);
  const goTyped = async () => {
    const r = await invoke<{ path: string; kind: "folder" | "file" | "missing" }>("path_resolve", { input: text, base }).catch(() => null);
    if (r && r.kind === "file" && at?.line) openAt(r.path, at.line, at.column, mods, onClose);
    else if (r && r.kind !== "missing") go(r.path, r.kind === "folder", mods, onClose);
    else showToast(`${text} doesn't exist.`, "error");
  };
  return (
    <>
      <Cmdk.Group heading="Path">
        <Cmdk.Item value="typed" onSelect={() => void goTyped()}>
          <span className="palette-title">
            Go to {text}
            {at?.line ? `, line ${at.line}` : ""}
          </span>
        </Cmdk.Item>
        {list.map((p) => (
          <Cmdk.Item key={p} value={p} data-path={p} onSelect={() => go(p, true, mods, onClose)}>
            <span className="palette-title mono">{p}</span>
          </Cmdk.Item>
        ))}
      </Cmdk.Group>
    </>
  );
}

interface PlaceList {
  home: { name: string; path: string }[];
  drives: { path: string; label: string }[];
}

/** `@`: favourites, recent folders, home places and drives. */
function PlaceItems({ mods, onClose }: { mods: Mods; onClose: () => void }) {
  const [places, setPlaces] = useState<PlaceList | null>(null);
  useEffect(() => void invoke<PlaceList>("places_list").then(setPlaces), []);
  const item = (key: string, path: string, title: string, detail: string) => (
    <Cmdk.Item key={key} value={`${title} ${path}`} onSelect={() => go(path, true, mods, onClose)}>
      <span className="palette-title">{title}</span>
      <span className="palette-category">{detail}</span>
    </Cmdk.Item>
  );
  return (
    <>
      <Cmdk.Group heading="Favourites">{getSettings().favourites.map((p) => item(`f:${p}`, p, tabTitle(p), p))}</Cmdk.Group>
      <Cmdk.Group heading="Recent folders">{getRecents().slice(0, 20).map((p) => item(`r:${p}`, p, tabTitle(p), p))}</Cmdk.Group>
      {places && <Cmdk.Group heading="Home">{places.home.map((p) => item(`h:${p.path}`, p.path, p.name, p.path))}</Cmdk.Group>}
      {places && <Cmdk.Group heading="Drives">{places.drives.map((d) => item(`d:${d.path}`, d.path, d.label, d.path))}</Cmdk.Group>}
    </>
  );
}

type ShellEvent = { type: "output"; text: string } | { type: "exit"; code: number | null; ms: number };

/** Asked once per run of the app. */
let shellAllowed = false;

/** `!`: run a command in the folder you're in; its output shows here (closing stops it). */
function ShellItems({ line }: { line: string }) {
  const cwd = activeTab().path;
  const [output, setOutput] = useState("");
  const [status, setStatus] = useState<string | null>(null);
  const running = useRef<string | null>(null);
  const out = useRef<HTMLPreElement>(null);
  useEffect(
    () => () => {
      if (running.current) void invoke("shell_cancel", { id: running.current });
    },
    [],
  );
  useEffect(() => {
    if (out.current) out.current.scrollTop = out.current.scrollHeight;
  }, [output]);
  const run = async () => {
    if (!line.trim() || running.current) return;
    if (!shellAllowed) {
      const ok = await askConfirm({
        title: "Run shell commands from the command bar?",
        body: `What you type after ! runs in ${cwd} with ${navigator.userAgent.includes("Windows") ? "cmd.exe" : "your shell"}, as if typed in a terminal.`,
        confirm: "Run",
      });
      if (!ok) return;
      shellAllowed = true;
    }
    setOutput("");
    setStatus("Running…");
    const channel = new Channel<ShellEvent>();
    channel.onmessage = (e) => {
      if (e.type === "output") setOutput((o) => (o + e.text).slice(-200_000));
      else {
        running.current = null;
        setStatus(`Exited with ${e.code ?? "no code"} after ${(e.ms / 1000).toFixed(1)} s`);
      }
    };
    try {
      running.current = await invoke<string>("shell_run", { cwd, line, onEvent: channel });
    } catch (err) {
      setStatus(String(err));
    }
  };
  return (
    <>
      <Cmdk.Group heading={`In ${cwd}`}>
        <Cmdk.Item value="run" onSelect={() => void run()}>
          <span className="palette-title mono">{line.trim() ? `Run: ${line}` : "Type a command, then Enter"}</span>
        </Cmdk.Item>
      </Cmdk.Group>
      {(status || output) && (
        <div className="shell-output">
          {status && <div className="palette-note">{status} {running.current ? "(Esc stops it)" : ""}</div>}
          <pre ref={out} className="mono">{output}</pre>
        </div>
      )}
      {!output && !status && (
        <div className="palette-note">
          Esc closes the bar <Kbd combo="Esc" /> and stops a running command. Try <code>git log --oneline -5</code> or <code>npm test</code>.
        </div>
      )}
    </>
  );
}

interface FileSymbol {
  name: string;
  kind: string;
  line: number;
  column: number;
  depth: number;
}

const KIND_LABEL: Record<string, string> = {
  function: "fn",
  method: "method",
  class: "class",
  interface: "interface",
  type: "type",
  enum: "enum",
  struct: "struct",
  trait: "trait",
  module: "module",
  constant: "const",
  variable: "var",
  heading: "#",
  selector: "css",
  macro: "macro",
  impl: "impl",
};

/** name@symbol: the functions, classes, headings… of the file the name finds, filtered by symbol. */
function SymbolItems({ file, query, mods, onClose }: { file: string; query: string; mods: Mods; onClose: () => void }) {
  const [found, setFound] = useState<{ path: string; symbols: FileSymbol[] } | null | "none">(null);
  useEffect(() => {
    if (!file) return setFound("none");
    let stale = false;
    const timer = window.setTimeout(async () => {
      let path: string | null = null;
      if (looksLikePath(file)) {
        const r = await invoke<{ path: string; kind: string }>("path_resolve", { input: file, base: activeTab().path }).catch(() => null);
        if (r?.kind === "file") path = r.path;
      } else {
        const r = await invoke<SearchResult>("index_search", { query: file, scope: null, limit: 20 }).catch(() => null);
        path = r?.hits.find((h) => !h.folder)?.path ?? null;
      }
      if (stale) return;
      if (!path) return setFound("none");
      const symbols = await invoke<FileSymbol[]>("file_symbols", { path }).catch(() => []);
      if (!stale) setFound({ path, symbols });
    }, 80);
    return () => {
      stale = true;
      window.clearTimeout(timer);
    };
  }, [file]);

  if (found === null) return <div className="palette-note">Reading…</div>;
  if (found === "none") return <div className="palette-note">No file named like "{file}" in the index.</div>;
  const q = query.toLowerCase();
  const shown = !q
    ? found.symbols
    : found.symbols
        .filter((s) => fuzzy(s.name, q))
        // Names that start with what's typed first, then those containing it, then the rest; each in file order.
        .sort((a, b) => rank(a.name, q) - rank(b.name, q) || a.line - b.line);
  const name = found.path.slice(Math.max(found.path.lastIndexOf("\\"), found.path.lastIndexOf("/")) + 1);
  return (
    <>
    <Cmdk.Group heading={`Symbols in ${name} · ${parentOf(found.path) ?? ""}`}>
      {shown.slice(0, 300).map((s) => (
        <Cmdk.Item key={`${s.line}:${s.column}:${s.name}`} value={`s:${s.line}:${s.column}:${s.name}`} onSelect={() => openAt(found.path, s.line, s.column, mods, onClose)}>
          <span className="palette-prefix mono symbol-kind">{KIND_LABEL[s.kind] ?? s.kind}</span>
          <span className="palette-title" style={{ paddingLeft: q ? 0 : Math.min(s.depth, 6) * 12 }}>
            {s.name}
          </span>
          <span className="palette-category mono">:{s.line}</span>
        </Cmdk.Item>
      ))}
    </Cmdk.Group>
    {!shown.length && <div className="palette-note">{found.symbols.length ? `No symbol like "${query}" in ${name}.` : `Fileboat doesn't find symbols in ${name} (it knows code in common languages, Markdown headings and CSS).`}</div>}
    </>
  );
}

function rank(name: string, q: string): number {
  const n = name.toLowerCase();
  return n.startsWith(q) ? 0 : n.includes(q) ? 1 : 2;
}
