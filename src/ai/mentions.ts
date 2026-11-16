// "@" context for the Claude pane: files and folders (from the index, the selection, the folder
// you're in) and images. Files go as paths: Claude Code reads what it needs with its own tools.
import { invoke } from "@tauri-apps/api/core";
import type { SearchResult } from "../lib/search";
import { getCurrent } from "../lib/current";
import { activeTab, tabTitle } from "../lib/tabs";

export type Mention =
  | { kind: "file"; label: string; path: string }
  | { kind: "folder"; label: string; path: string }
  | { kind: "image"; label: string; mediaType: string; data: string };

/** One row of the "@" list. */
export interface Suggestion {
  key: string;
  group: string;
  label: string;
  detail?: string;
  mention: Mention;
}

export function mentionKey(m: Mention): string {
  return m.kind === "image" ? `i:${m.label}:${m.data.length}` : `p:${m.path.toLowerCase()}`;
}

function nameOf(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  return trimmed.slice(Math.max(trimmed.lastIndexOf("\\"), trimmed.lastIndexOf("/")) + 1) || trimmed;
}

/** A path as a mention. */
export function pathMention(path: string, folder: boolean): Mention {
  return folder ? { kind: "folder", label: `${nameOf(path)}/`, path } : { kind: "file", label: nameOf(path), path };
}

/** The "@" list for what was typed after "@": the selection and this folder, then the index. */
export async function suggest(q: string): Promise<Suggestion[]> {
  const out: Suggestion[] = [];
  const cur = getCurrent();
  const folder = activeTab().path;
  const has = (s: string) => !q || s.toLowerCase().includes(q.toLowerCase());
  const sep = folder.includes("\\") ? "\\" : "/";
  for (const name of (cur?.names ?? []).slice(0, 8)) {
    const path = folder.replace(/[\\/]+$/, "") + sep + name;
    if (has(name)) out.push({ key: `s:${path}`, group: "Selected", label: name, mention: pathMention(path, false) });
  }
  if (has(tabTitle(folder)) || has("folder")) out.push({ key: `f:${folder}`, group: "Here", label: `${tabTitle(folder)}/`, detail: "this folder", mention: pathMention(folder, true) });
  if (q) {
    // Under this folder first (the project), then everywhere.
    const here = await invoke<SearchResult>("index_search", { query: q, scope: folder, limit: 10 }).catch(() => null);
    const seen = new Set(out.map((s) => s.mention.kind !== "image" && s.mention.path.toLowerCase()));
    for (const h of here?.hits ?? []) {
      if (seen.has(h.path.toLowerCase())) continue;
      seen.add(h.path.toLowerCase());
      out.push({ key: `h:${h.path}`, group: "In this folder", label: h.name + (h.folder ? "/" : ""), detail: h.path.slice(folder.length).replace(/^[\\/]+/, ""), mention: pathMention(h.path, h.folder) });
    }
    if ((here?.hits.length ?? 0) < 5) {
      const all = await invoke<SearchResult>("index_search", { query: q, scope: null, limit: 8 }).catch(() => null);
      for (const h of all?.hits ?? []) {
        if (seen.has(h.path.toLowerCase())) continue;
        out.push({ key: `a:${h.path}`, group: "Elsewhere", label: h.name + (h.folder ? "/" : ""), detail: h.path, mention: pathMention(h.path, h.folder) });
      }
    }
  }
  return out;
}

export interface AttachmentFile {
  kind: "text" | "image";
  name: string;
  path: string;
  mediaType?: string;
  data?: string;
}

/** A dropped file as a mention: images are sent as images, anything else as its path. */
export async function attachPath(path: string): Promise<Mention> {
  if (/\.(png|jpe?g|gif|webp)$/i.test(path)) {
    const a = await invoke<AttachmentFile>("read_attachment", { path });
    if (a.kind === "image") return { kind: "image", label: a.name, mediaType: a.mediaType!, data: a.data! };
  }
  const isDir = await invoke<{ kind: string }>("path_resolve", { input: path, base: null }).then((r) => r.kind === "folder", () => false);
  return pathMention(path, isDir);
}

const IMAGE_TYPES = ["image/png", "image/jpeg", "image/gif", "image/webp"];

/** An image from the clipboard (paste) as a mention; null when it isn't a supported image. */
export function imageFromBlob(file: File): Promise<Mention | null> {
  if (!IMAGE_TYPES.includes(file.type) || file.size > 5 * 1024 * 1024) return Promise.resolve(null);
  return new Promise((resolve) => {
    const reader = new FileReader();
    reader.onload = () => {
      const url = String(reader.result);
      const data = url.slice(url.indexOf(",") + 1);
      const ext = file.type.split("/")[1];
      resolve({ kind: "image", label: file.name && file.name !== "image.png" ? file.name : `pasted image.${ext}`, mediaType: file.type, data });
    };
    reader.onerror = () => resolve(null);
    reader.readAsDataURL(file);
  });
}

/**
 * The context sent with a message for its mentions (images go separately): the paths the user
 * pointed at. Wrapped in a <fileboat-context> block, which the chat hides when showing it.
 */
export async function contextText(mentions: Mention[]): Promise<string> {
  const paths = mentions.filter((m): m is Exclude<Mention, { kind: "image" }> => m.kind !== "image");
  if (!paths.length) return "";
  const lines = paths.map((m) => `- ${m.kind === "folder" ? "Folder" : "File"}: ${m.path}`);
  return `<fileboat-context>\nThe user pointed at these with @ (read them with your tools as needed):\n${lines.join("\n")}\n</fileboat-context>`;
}

/** The message as the user typed it, without the attached context (for saved chats). */
export function stripContext(text: string): { text: string; hadContext: boolean } {
  return {
    text: text.replace(/\s*<fileboat-context>[\s\S]*?<\/fileboat-context>\s*/g, "\n").trim(),
    hadContext: text.includes("<fileboat-context>"),
  };
}
