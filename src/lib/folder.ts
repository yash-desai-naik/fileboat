import { Channel, invoke } from "@tauri-apps/api/core";
import type { Settings, SortBy } from "./settings";

/** One row, as src-tauri/src/fs/entry.rs sends it. */
export interface Entry {
  name: string;
  kind: "folder" | "file" | "other";
  size: number;
  /** Milliseconds since 1970 (0 when unknown). */
  modified: number;
  created: number;
  /** Lower-case extension without the dot, "" for none. */
  ext: string;
  hidden: boolean;
  link: boolean;
  readonly: boolean;
}

/** How a folder is sorted and filtered (src-tauri/src/fs/model.rs `View`). */
export interface FolderViewSpec {
  sort: SortBy;
  descending: boolean;
  foldersFirst: boolean;
  showHidden: boolean;
}

export function viewFromSettings(s: Settings): FolderViewSpec {
  return { sort: s.sortBy, descending: s.sortDescending, foldersFirst: s.foldersFirst, showHidden: s.showHidden };
}

type FolderEvent =
  | { type: "loaded"; count: number; total: number; done: boolean }
  | { type: "changed"; count: number; total: number }
  | { type: "error"; message: string };

interface Opened {
  id: string;
  path: string;
  name: string;
  parent: string | null;
}

/** Rows are fetched from Rust in blocks of this many. */
const BLOCK = 200;

/**
 * An open folder as a view sees it. The entries stay in Rust; this keeps a few blocks of rows
 * (the ones on screen), and tells the view when something changed. One per tab.
 */
export class FolderModel {
  id: string | null = null;
  path: string;
  name = "";
  parent: string | null = null;
  /** Rows shown (hidden files excluded unless shown). */
  count = 0;
  /** Entries in the folder, hidden ones included. */
  total = 0;
  loading = true;
  error: string | null = null;
  /** Bumped on every change, so React re-renders. */
  version = 0;

  private blocks = new Map<number, Entry[]>();
  private pending = new Set<number>();
  /** Changes whenever rows may have moved; replies fetched before that are dropped. */
  generation = 0;
  private listeners = new Set<() => void>();
  private closed = false;

  constructor(path: string) {
    this.path = path;
  }

  /** Open the folder (`path` may be typed: ~, relative to `base`, …). Throws a readable error. */
  async open(view: FolderViewSpec, base?: string): Promise<void> {
    const channel = new Channel<FolderEvent>();
    channel.onmessage = (e) => this.onEvent(e);
    const opened = await invoke<Opened>("folder_open", { path: this.path, base: base ?? null, view, onEvent: channel });
    if (this.closed) {
      void invoke("folder_close", { id: opened.id });
      return;
    }
    Object.assign(this, { id: opened.id, path: opened.path, name: opened.name, parent: opened.parent });
    this.bump();
  }

  private onEvent(e: FolderEvent) {
    if (e.type === "error") {
      this.error = e.message;
      this.loading = false;
    } else {
      this.count = e.count;
      this.total = e.total;
      if (e.type === "loaded") this.loading = !e.done;
      // Rows may have moved: forget them; the view asks again for what's on screen.
      this.blocks.clear();
      this.pending.clear();
      this.generation++;
    }
    this.bump();
  }

  /** The folder couldn't be opened. */
  fail(message: string) {
    this.error = message;
    this.loading = false;
    this.bump();
  }

  /** The row at `i` if it has been fetched; otherwise fetches its block and returns undefined. */
  row(i: number): Entry | undefined {
    const b = Math.floor(i / BLOCK);
    const block = this.blocks.get(b);
    if (block) return block[i - b * BLOCK];
    this.fetch(b);
    return undefined;
  }

  /** Rows `start..end`, fetched now (for actions on a selection that isn't on screen). */
  async rows(start: number, end: number): Promise<Entry[]> {
    return this.ask<Entry[]>("folder_rows", { start, end }, []);
  }

  /**
   * A question about the open folder. A view can ask just as its tab closes or navigates away;
   * the folder is gone then, and the answer is `fallback`.
   */
  private async ask<T>(command: string, args: Record<string, unknown>, fallback: T): Promise<T> {
    if (!this.id || this.closed) return fallback;
    try {
      return await invoke<T>(command, { id: this.id, ...args });
    } catch {
      return fallback;
    }
  }

  /** Names of rows `start..end`. */
  async names(start: number, end: number): Promise<string[]> {
    return this.ask<string[]>("folder_names", { start, end }, []);
  }

  private fetch(b: number) {
    if (!this.id || this.pending.has(b)) return;
    this.pending.add(b);
    const generation = this.generation;
    void invoke<Entry[]>("folder_rows", { id: this.id, start: b * BLOCK, end: (b + 1) * BLOCK })
      .then((rows) => {
        if (generation !== this.generation || this.closed) return;
        this.blocks.set(b, rows);
        this.pending.delete(b);
        this.bump();
      })
      .catch(() => this.pending.delete(b));
  }

  /** Re-sort or show hidden files (the folder may have closed meanwhile: then nothing to do). */
  async setView(view: FolderViewSpec) {
    await this.ask("folder_view", { view }, 0);
  }

  /** The row of a name, if shown (after rows moved). */
  async rowOf(name: string): Promise<number | null> {
    return this.ask<number | null>("folder_row_of", { name }, null);
  }

  /** Type-ahead: the first row from `from` (wrapping) whose name starts with `prefix`. */
  async seek(prefix: string, from: number): Promise<number | null> {
    return this.ask<number | null>("folder_seek", { prefix, from }, null);
  }

  close() {
    this.closed = true;
    if (this.id) void invoke("folder_close", { id: this.id });
    this.listeners.clear();
  }

  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  private bump() {
    this.version++;
    for (const l of this.listeners) l();
  }
}

/** The full path of a child of `folder`, with the platform's separator. */
export function childPath(folder: string, name: string): string {
  const sep = folder.includes("\\") ? "\\" : "/";
  return folder.endsWith(sep) ? folder + name : folder + sep + name;
}
