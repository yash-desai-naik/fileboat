import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { Command } from "./commands";
import { paneActions } from "./pane";
import { activeTab, navigate, newTab, parentOf } from "./tabs";

/**
 * Claude's hands in this window (from Tableboat): the tool server (src-tauri/src/ai/host.rs)
 * sends `ui:request` events, this does them with the same functions the UI uses, and answers
 * with `ui_reply`. Questions to the user are asked in Rust before a request gets here.
 */

/** The command bar's list, kept current by App (so Claude sees exactly what the bar shows). */
let commands: Command[] = [];
export function publishCommands(list: Command[]) {
  commands = list;
}

function split(path: string): [string, string] {
  const parent = parentOf(path) ?? path;
  return [parent, path.slice(parent.replace(/[\\/]+$/, "").length).replace(/^[\\/]+/, "")];
}

async function handle(action: string, args: Record<string, unknown>): Promise<unknown> {
  switch (action) {
    case "commands":
      return commands.filter((c) => !c.hidden).map((c) => ({ id: c.id, category: c.category, title: c.title, checked: c.checked, keys: c.keys?.[0] }));
    case "run": {
      const cmd = commands.find((c) => c.id === args.id);
      if (!cmd) throw new Error(`There's no command "${String(args.id)}" right now.`);
      await Promise.resolve(cmd.run());
      return null;
    }
    case "reveal": {
      const [folder, name] = split(String(args.path));
      navigate(folder, name || null);
      return null;
    }
    case "select": {
      const names = (args.names as string[]) ?? [];
      return await paneActions.selectNames(names);
    }
    case "open": {
      const path = String(args.path);
      const isDir = await invoke<{ kind: string }>("path_resolve", { input: path, base: activeTab().path }).then((r) => r.kind === "folder");
      if (isDir) newTab(path, true);
      else await invoke("open_path", { path });
      return null;
    }
    default:
      throw new Error(`Unknown action ${action}`);
  }
}

void getCurrentWebviewWindow().listen<{ id: string; action: string; args: Record<string, unknown> }>("ui:request", async (e) => {
  const { id, action, args } = e.payload;
  try {
    const value = await handle(action, args ?? {});
    await invoke("ui_reply", { id, ok: true, value: value ?? null, error: null });
  } catch (err) {
    await invoke("ui_reply", { id, ok: false, value: null, error: err instanceof Error ? err.message : String(err) });
  }
});
