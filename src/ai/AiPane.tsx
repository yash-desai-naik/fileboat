import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { fmtShortcut, IS_MAC } from "../lib/platform";
import {
  answerPermission,
  applyEvent,
  claudeApi,
  clearModel,
  modelLabel,
  newChat,
  onExit,
  onPermission,
  openPastChat,
  restoreChat,
  send,
  stop,
  useChat,
  type AiContext,
  type ChatItem,
  type ClaudeStatus,
} from "../lib/claude";
import { getCurrent, useCurrent } from "../lib/current";
import { childPath } from "../lib/folder";
import { viewFor } from "../lib/folderViews";
import { getSearch } from "../lib/search";
import { getSettings, useSettings } from "../lib/settings";
import { activeTab, navigate, parentOf, tabTitle, useTabs } from "../lib/tabs";
import { CloseSmallIcon } from "../shell/Icons";
import { Menu, type MenuItem } from "../shell/Menu";
import { Composer } from "./Composer";
import { contextText, type Mention } from "./mentions";

/** What Claude can see through get_context, read fresh from the app's state. */
export function currentContext(): AiContext {
  const tab = activeTab();
  const cur = getCurrent();
  const folder = tab.path;
  const names = cur && cur.folder.toLowerCase() === folder.toLowerCase() ? (cur.names ?? []) : [];
  const search = getSearch(tab.id);
  return {
    folder,
    selection: names.map((n) => childPath(folder, n)),
    preview: getSettings().showPreview && cur?.entry ? childPath(cur.folder, cur.entry.name) : null,
    view: viewFor(folder).viewMode,
    search: search.open && search.query.trim() ? search.query : null,
  };
}

let listening = false;

/** Show a path from Claude's answer in Fileboat (relative ones to the chat's folder). */
function reveal(path: string, base: string) {
  const sep = base.includes("\\") ? "\\" : "/";
  const full = /^([a-z]:[\\/]|[\\/])/i.test(path) ? path : base.replace(/[\\/]+$/, "") + sep + path.replace(/^\.[\\/]/, "").split(/[\\/]/).join(sep);
  const parent = parentOf(full);
  if (parent) navigate(parent, full.slice(parent.replace(/[\\/]+$/, "").length).replace(/^[\\/]+/, ""));
}

/** The Claude Code chat in the right pane (laid out like Claude Code's VS Code panel). */
export function AiPane({ onClose }: { onClose: () => void }) {
  const chat = useChat();
  const settings = useSettings();
  useTabs();
  const current = useCurrent();
  const tabPath = activeTab().path;
  const [status, setStatus] = useState<ClaudeStatus | null>(null);
  // The model actually answering once Claude Code has started; before that, the one it will use.
  const model = chat.model ?? (settings.claudeModel.trim() || status?.defaultModel || null);
  const modelSetting = useRef(settings.claudeModel);
  useEffect(() => {
    if (modelSetting.current === settings.claudeModel) return;
    modelSetting.current = settings.claudeModel;
    clearModel();
  }, [settings.claudeModel]);
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  // Keep Claude's view current as the folder and selection change; the status says where a
  // new chat would start.
  useEffect(() => {
    const t = window.setTimeout(() => {
      void claudeApi
        .setContext(currentContext())
        .then(() => claudeApi.status())
        .then(setStatus);
    }, 150);
    return () => window.clearTimeout(t);
  }, [tabPath, current]);

  useEffect(() => {
    void claudeApi.status().then((s) => {
      setStatus(s);
      // The page was reloaded mid-chat: show the chat it's still in.
      if (s.session) void restoreChat(s.session);
    });
    if (listening) return;
    listening = true;
    const win = getCurrentWebviewWindow();
    void win.listen<Record<string, unknown>>("claude:event", (e) => applyEvent(e.payload, true));
    void win.listen<{ id: string; toolName: string; input: Record<string, unknown> }>("claude:permission", (e) => onPermission(e.payload));
    void win.listen<{ stderr: string }>("claude:exit", (e) => onExit(e.payload.stderr));
  }, []);

  // Follow new output.
  useLayoutEffect(() => {
    const el = listRef.current;
    if (el && el.scrollHeight - el.scrollTop - el.clientHeight < 160) el.scrollTop = el.scrollHeight;
  }, [chat.items, chat.running]);

  // Ctrl+Esc moves the keyboard into the chat input, or back to the folder.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape" || !e.ctrlKey || e.shiftKey || e.altKey) return;
      e.preventDefault();
      const input = inputRef.current;
      if (!input) return;
      if (document.activeElement === input) input.blur();
      else input.focus();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  /** Send with the @ context: paths as text, images as images. */
  async function sendWithMentions(text: string, mentions: Mention[]) {
    const context = await contextText(mentions);
    void send(text, currentContext(), {
      context,
      images: mentions.flatMap((m) => (m.kind === "image" ? [{ mediaType: m.mediaType, data: m.data }] : [])),
      attachments: mentions.map((m) => (m.kind === "image" ? m.label : `@${m.label}`)),
    });
    window.setTimeout(() => void claudeApi.status().then(setStatus), 1500);
  }

  async function pastChats(x: number, y: number) {
    const list = await claudeApi.sessions();
    setMenu({
      x,
      y,
      items: list.length
        ? list.slice(0, 25).map((c) => ({
            label: c.title.length > 60 ? `${c.title.slice(0, 60)}…` : c.title,
            hint: new Date(c.modified * 1000).toLocaleDateString(),
            onSelect: () => void openPastChat(c.id),
          }))
        : [{ label: "No earlier chats in this project yet", disabled: true, onSelect: () => {} }],
    });
  }

  const startNew = async () => {
    await newChat();
    setStatus(await claudeApi.status());
  };

  const firstUser = chat.items.find((i) => i.kind === "user");
  const title = firstUser?.kind === "user" ? firstUser.text.replace(/\s+/g, " ") : "New chat";
  const pending = chat.items.some((i) => i.kind === "permission" && i.state === "pending");
  const elsewhere = status && chat.items.length > 0 && status.here.toLowerCase() !== status.folder.toLowerCase();
  const project = status ? tabTitle(status.folder) : null;

  return (
    <div
      className="ai"
      onKeyDown={(e) => {
        // Esc stops Claude (unless a permission prompt is waiting: that handles its own keys).
        if (e.key === "Escape" && chat.running && !pending) {
          e.preventDefault();
          void stop();
        }
      }}
    >
      <div className="ai-head">
        <ClaudeMark />
        <span className="ai-chat-title" title={title}>
          {title}
        </span>
        <button
          className="icon-btn small"
          title="Earlier chats in this project"
          onClick={(e) => {
            const r = e.currentTarget.getBoundingClientRect();
            void pastChats(r.right - 320, r.bottom + 4);
          }}
        >
          <HistoryIcon />
        </button>
        <button className="icon-btn small" title="New chat" onClick={() => void startNew()}>
          <NewChatIcon />
        </button>
        <button className="icon-btn small" title={`Close (${fmtShortcut("Ctrl+Alt+B")})`} onClick={onClose}>
          <CloseSmallIcon />
        </button>
      </div>

      {elsewhere && (
        <div className="ai-switch">
          <span>
            This chat works in <b>{tabTitle(status.folder)}</b>; you're in <b>{tabTitle(status.here)}</b> now.
          </span>
          <button className="link-btn" onClick={() => void startNew()}>
            New chat there
          </button>
        </div>
      )}

      <div className="ai-list" ref={listRef}>
        {status && !status.installed && (
          <div className="ai-note error">
            Claude Code ({IS_MAC ? "claude" : "claude.exe"}) wasn't found. Install it from claude.com/claude-code and sign in once in a terminal, then reopen this pane.
          </div>
        )}
        {chat.items.length === 0 && status?.installed && (
          <div className="ai-empty">
            <ClaudeMark size={30} />
            <p className="ai-empty-title">Ask about the project you're browsing.</p>
            <p className="ai-empty-note">
              Claude Code works in <code>{status.folder}</code>, with that project's CLAUDE.md, chats and memory (the same ones your terminal uses). It sees what
              you have open and selected; anything that changes files asks you first.
            </p>
            <div className="ai-suggestions">
              {["What is this project, and how is it laid out?", "What changed in git, and what's left to commit?", "Find the largest files here and what they are"].map((s) => (
                <button key={s} className="ai-suggestion" onClick={() => void send(s, currentContext())}>
                  {s}
                </button>
              ))}
            </div>
          </div>
        )}
        {chat.items.map((item, i) => (
          <Item key={i} item={item} base={status?.folder ?? tabPath} />
        ))}
        {chat.running && !pending && <Working />}
      </div>

      <Composer
        inputRef={inputRef}
        disabled={status?.installed === false}
        placeholder={status?.installed === false ? "Claude Code isn't installed" : `Ask Claude… @ adds files · ${fmtShortcut("Ctrl+Esc")} focus`}
        running={chat.running}
        onSend={(text, mentions) => void sendWithMentions(text, mentions)}
        onStop={() => void stop()}
        chips={
          <>
            <span className="ai-chip" title={status ? `Claude Code works in ${status.folder}` : ""}>
              <span className="ai-chip-dot on" />
              <span className="ai-chip-text">{project ?? "…"}</span>
            </span>
            {status?.installed && (
              <span className="ai-chip ai-model" title={model ? `Model: ${model}${chat.model ? "" : " (from your next message)"}\nChange it in Settings › Claude` : "Claude Code's default model\nChange it in Settings › Claude"}>
                {model ? modelLabel(model) : "Default model"}
              </span>
            )}
            <span className="spacer" />
            <span className="ai-mode" title="Claude reads freely; anything that changes files, settings or runs commands asks you first">
              <ShieldIcon />
              <span className="ai-mode-text">Asks before changes</span>
            </span>
          </>
        }
      />
      {menu && <Menu x={menu.x} y={menu.y} items={menu.items} onClose={() => setMenu(null)} />}
    </div>
  );
}

const WORKING_WORDS = ["Thinking", "Reading", "Looking around", "Working"];

/** The live "Claude is working" row: spinning mark, elapsed time, how to stop. */
function Working() {
  const [started] = useState(() => Date.now());
  const [now, setNow] = useState(started);
  useEffect(() => {
    const t = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(t);
  }, []);
  const secs = Math.floor((now - started) / 1000);
  return (
    <div className="ai-working">
      <ClaudeMark spin />
      <span className="ai-working-word">{WORKING_WORDS[Math.floor(secs / 6) % WORKING_WORDS.length]}…</span>
      <span className="muted">{secs}s · Esc to stop</span>
    </div>
  );
}

/** Friendly names for Fileboat's own tools. */
const TOOL_NAMES: Record<string, string> = {
  get_context: "Context",
  search_files: "Search",
  list_folder: "List folder",
  git_status: "Git status",
  reveal: "Show in Fileboat",
  select: "Select",
  open_in_app: "Open",
  list_app_commands: "App commands",
  run_app_command: "Run command",
  get_settings: "Settings",
  update_settings: "Change settings",
  create_folder: "New folder",
  create_file: "New file",
  rename: "Rename",
  move: "Move",
  trash: "Delete",
};

function toolTitle(name: string): string {
  const own = /^fileboat › (.+)$/.exec(name) ?? /^mcp__fileboat__(.+)$/.exec(name);
  return own ? (TOOL_NAMES[own[1]] ?? own[1]) : name;
}

/** One timeline step: a dot in the left gutter, then the content. */
function Step({ dot, children }: { dot: "text" | "ok" | "error" | "running"; children: ReactNode }) {
  return (
    <div className="ai-step">
      <span className={`ai-dot ${dot}`} />
      <div className="ai-step-body">{children}</div>
    </div>
  );
}

function Item({ item, base }: { item: ChatItem; base: string }) {
  const [open, setOpen] = useState(false);
  switch (item.kind) {
    case "user":
      return (
        <div className="ai-user-wrap">
          {item.at !== undefined && (
            <div className="ai-time" title={new Date(item.at).toLocaleString()}>
              {new Date(item.at).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" })}
            </div>
          )}
          <div className="ai-user">
            {item.text}
            {item.attachments && (
              <div className="ai-user-atts">
                {item.attachments.map((a, i) => (
                  <span key={i} className="ai-user-att">
                    {a}
                  </span>
                ))}
              </div>
            )}
          </div>
        </div>
      );
    case "assistant":
      return (
        <Step dot="text">
          <div className="ai-msg">
            <Markdown text={item.text} base={base} />
            {item.streaming && <span className="ai-caret" />}
          </div>
        </Step>
      );
    case "tool": {
      // Claude Code's own tool lookup is internal noise.
      if (item.name === "ToolSearch") return null;
      const summary = toolSummary(item.input);
      const dot = item.result === undefined ? "running" : item.isError ? "error" : "ok";
      return (
        <Step dot={dot}>
          <button className="ai-tool-head" onClick={() => setOpen((o) => !o)} title={open ? "Hide details" : "Show details"}>
            <span className="ai-tool-name">{toolTitle(item.name)}</span>
            {summary && <span className="ai-tool-sum">{summary}</span>}
          </button>
          {open && (
            <div className="ai-io">
              <div className="ai-io-row">
                <span className="ai-io-label">IN</span>
                <pre>{toolInput(item.input)}</pre>
              </div>
              {item.result !== undefined && (
                <div className="ai-io-row">
                  <span className="ai-io-label">OUT</span>
                  <pre className={item.isError ? "error" : undefined}>{item.result.slice(0, 6000) || "(no output)"}</pre>
                </div>
              )}
            </div>
          )}
        </Step>
      );
    }
    case "permission":
      return <Permission item={item} />;
    case "done":
      return item.note ? <div className="ai-note-line">{item.note}</div> : <div className="ai-done">{item.ms !== undefined ? `${(item.ms / 1000).toFixed(1)} s` : ""}</div>;
    case "error":
      return (
        <Step dot="error">
          <div className="ai-error-text">{item.text}</div>
        </Step>
      );
  }
}

function Permission({ item }: { item: Extract<ChatItem, { kind: "permission" }> }) {
  const name = toolTitle(item.toolName.replace(/^mcp__fileboat__/, "fileboat › "));
  // Fileboat's own tools send their question and a readable summary.
  const detail =
    (item.input.summary as string) ??
    (item.input.command as string) ??
    (item.input.file_path as string) ??
    JSON.stringify(item.input, null, 2);
  const first = useRef<HTMLButtonElement>(null);
  const pending = item.state === "pending";
  useEffect(() => {
    if (pending) first.current?.focus();
  }, [pending]);
  const choose = (allow: boolean, always: boolean) => void answerPermission(item.id, allow, always);
  const question = (item.input.question as string) ?? (item.toolName === "Bash" ? "Run this command?" : /^(Edit|Write|MultiEdit|NotebookEdit)$/.test(item.toolName) ? "Change this file?" : `Allow Claude to use ${name}?`);
  return (
    <div
      className={`ai-perm ${item.state}`}
      onKeyDown={(e) => {
        if (!pending) return;
        const k = e.key;
        if (k === "1" || k === "2" || k === "3" || k === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          if (k === "1") choose(true, false);
          else if (k === "2") choose(true, true);
          else choose(false, false);
        }
      }}
    >
      <div className="ai-perm-title">{pending ? question : item.state === "allowed" ? `Allowed: ${name}` : `Denied: ${name}`}</div>
      <pre className="ai-perm-detail">{String(detail).slice(0, 4000)}</pre>
      {pending && (
        <div className="ai-perm-options">
          <button ref={first} className="ai-perm-option" onClick={() => choose(true, false)}>
            <kbd>1</kbd> Yes
          </button>
          <button className="ai-perm-option" onClick={() => choose(true, true)}>
            <kbd>2</kbd> Yes, and don't ask again for {name} in this chat
          </button>
          <button className="ai-perm-option" onClick={() => choose(false, false)}>
            <kbd>3</kbd> No <span className="muted">(Esc)</span>
          </button>
        </div>
      )}
    </div>
  );
}

/** A small starburst in Claude's clay colour, drawn here (not Anthropic's logo file). */
export function ClaudeMark({ size = 14, spin = false }: { size?: number; spin?: boolean }) {
  return (
    <svg className={spin ? "ai-mark spin" : "ai-mark"} width={size} height={size} viewBox="0 0 16 16" aria-hidden="true">
      {[0, 30, 60, 90, 120, 150].map((a) => (
        <rect key={a} x="7.1" y="1" width="1.8" height="14" rx="0.9" transform={`rotate(${a} 8 8)`} />
      ))}
    </svg>
  );
}

const HistoryIcon = () => (
  <svg width="15" height="15" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round">
    <circle cx="8" cy="8" r="6" />
    <path d="M8 4.8V8l2.2 1.6" />
  </svg>
);
const NewChatIcon = () => (
  <svg width="15" height="15" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round">
    <path d="M8 2.2a5.8 5.8 0 1 0 2.7 10.9L13.8 14l-.8-3A5.8 5.8 0 0 0 8 2.2Z" />
    <path d="M8 5.6v4.8M5.6 8h4.8" />
  </svg>
);
const ShieldIcon = () => (
  <svg width="12" height="12" viewBox="0 0 16 16" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinejoin="round">
    <path d="M8 1.8 2.8 3.8v4c0 3.2 2.2 5.4 5.2 6.4 3-1 5.2-3.2 5.2-6.4v-4Z" />
  </svg>
);

/** A tool's input for the IN box: a command or path as is, anything else as JSON. */
function toolInput(input: unknown): string {
  if (!input || typeof input !== "object") return String(input ?? "");
  const i = input as Record<string, unknown>;
  for (const k of ["command", "query", "pattern"]) if (typeof i[k] === "string") return i[k] as string;
  return JSON.stringify(i, null, 2);
}

function toolSummary(input: unknown): string {
  if (!input || typeof input !== "object") return "";
  const i = input as Record<string, unknown>;
  if (i.changes && typeof i.changes === "object") return JSON.stringify(i.changes).slice(0, 80);
  const s = (i.query ?? i.command ?? i.file_path ?? i.path ?? i.pattern ?? i.id ?? i.new_name ?? (Array.isArray(i.paths) ? (i.paths as string[]).join(", ") : "") ?? "") as string;
  return typeof s === "string" ? s.replace(/\s+/g, " ").slice(0, 80) : "";
}

/**
 * A small, safe Markdown renderer: code fences (with Copy), inline code (a path can be clicked
 * to show it in Fileboat), bold, lists, tables and headings. Text is never injected as HTML.
 */
function Markdown({ text, base }: { text: string; base: string }) {
  const parts: ReactNode[] = [];
  const fence = /```(\w*)\n?([\s\S]*?)(```|$)/g;
  let last = 0;
  let m: RegExpExecArray | null;
  let key = 0;
  while ((m = fence.exec(text))) {
    if (m.index > last) parts.push(<Prose key={key++} text={text.slice(last, m.index)} base={base} />);
    const lang = m[1].toLowerCase();
    const code = m[2].replace(/\n$/, "");
    parts.push(
      <div key={key++} className="ai-code">
        <div className="ai-code-bar">
          <span>{lang || "code"}</span>
          <span className="spacer" />
          <button className="link-btn" onClick={() => void navigator.clipboard.writeText(code)}>
            Copy
          </button>
        </div>
        <pre>{code}</pre>
      </div>,
    );
    last = fence.lastIndex;
    if (m[3] === "") break;
  }
  if (last < text.length) parts.push(<Prose key={key++} text={text.slice(last)} base={base} />);
  return <>{parts}</>;
}

function Prose({ text, base }: { text: string; base: string }) {
  const lines = text.replace(/^\n+|\n+$/g, "").split("\n");
  const blocks: ReactNode[] = [];
  const inl = (s: string) => inline(s, base);
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (line.trim().startsWith("|")) {
      const rows: string[][] = [];
      while (i < lines.length && lines[i].trim().startsWith("|")) {
        const cells = lines[i].trim().replace(/^\|/, "").replace(/\|$/, "").split("|").map((c) => c.trim());
        if (!cells.every((c) => /^:?-{2,}:?$/.test(c))) rows.push(cells);
        i++;
      }
      const [head, ...body] = rows;
      blocks.push(
        <table key={blocks.length} className="ai-table">
          {head && (
            <thead>
              <tr>
                {head.map((c, k) => (
                  <th key={k}>{inl(c)}</th>
                ))}
              </tr>
            </thead>
          )}
          <tbody>
            {body.map((r, n) => (
              <tr key={n}>
                {r.map((c, k) => (
                  <td key={k}>{inl(c)}</td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>,
      );
      continue;
    }
    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    const bullet = /^(\s*)[-*]\s+(.*)$/.exec(line);
    const numbered = /^(\s*)(\d+)\.\s+(.*)$/.exec(line);
    if (heading)
      blocks.push(
        <div key={blocks.length} className="ai-h">
          {inl(heading[2])}
        </div>,
      );
    else if (bullet)
      blocks.push(
        <div key={blocks.length} className="ai-li" style={{ paddingLeft: 4 + bullet[1].length * 8 }}>
          <span className="ai-li-mark">•</span>
          <span>{inl(bullet[2])}</span>
        </div>,
      );
    else if (numbered)
      blocks.push(
        <div key={blocks.length} className="ai-li" style={{ paddingLeft: 4 + numbered[1].length * 8 }}>
          <span className="ai-li-mark">{numbered[2]}.</span>
          <span>{inl(numbered[3])}</span>
        </div>,
      );
    else if (line.trim() === "") blocks.push(<div key={blocks.length} className="ai-gap" />);
    else blocks.push(<div key={blocks.length}>{inl(line)}</div>);
    i++;
  }
  return <div className="ai-prose">{blocks}</div>;
}

/** Looks like a file path ("src/main.rs", "C:\a\b.txt", "./x"): clickable. */
const PATHISH = /^(\.{0,2}[\\/]|[a-z]:[\\/]|[\w@.-]+[\\/])[\w@ .\\/-]*$|^[\w@-]+\.[a-z0-9]{1,8}$/i;

function inline(line: string, base: string): ReactNode[] {
  const out: ReactNode[] = [];
  const re = /(\*\*[^*]+\*\*|`[^`]+`)/g;
  let last = 0;
  let m: RegExpExecArray | null;
  let k = 0;
  while ((m = re.exec(line))) {
    if (m.index > last) out.push(line.slice(last, m.index));
    const tok = m[0];
    if (tok.startsWith("`")) {
      const code = tok.slice(1, -1);
      out.push(
        PATHISH.test(code) ? (
          <code key={k++} className="ai-path" title="Show it in Fileboat" onClick={() => reveal(code.replace(/:\d+(:\d+)?$/, ""), base)}>
            {code}
          </code>
        ) : (
          <code key={k++}>{code}</code>
        ),
      );
    } else out.push(<strong key={k++}>{inline(tok.slice(2, -2), base)}</strong>);
    last = re.lastIndex;
  }
  if (last < line.length) out.push(line.slice(last));
  return out;
}
