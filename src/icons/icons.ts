import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { useSyncExternalStore } from "react";
import { getSettings } from "../lib/settings";

/**
 * Icon themes. "fileboat" (built in), "minimal" (plain pages and folders), "system" (the OS's own
 * icons) or "vscode:<id>": a VS Code icon theme in Fileboat's icon-themes folder, read in VS Code's
 * own format (iconDefinitions, file, folder, fileExtensions, fileNames, folderNames, light).
 * The user's own rules (Change icon…) come first, whatever the theme.
 */

/** Mirrors `IconTheme` in src-tauri/src/icons.rs: a VS Code theme, its paths relative to its folder. */
export interface VsTheme {
  id: string;
  label: string;
  /** The theme's folder name in icon-themes (for the icon URLs). */
  folder: string;
  defs: Record<string, string>;
  file?: string;
  folderIcon?: string;
  folderExpanded?: string;
  fileExtensions: Record<string, string>;
  fileNames: Record<string, string>;
  folderNames: Record<string, string>;
  folderNamesExpanded: Record<string, string>;
  light?: Omit<VsTheme, "id" | "label" | "folder" | "defs" | "light">;
}

/** A rule from Change icon… (settings: iconAssociations). */
export interface IconRule {
  /** What it matches: a file or folder name, an extension, or one exact path. */
  match: "name" | "ext" | "path";
  value: string;
  /** The theme's icon (a built-in mark, or a VS Code theme's icon id), or "file:<path>" for an image. */
  icon?: string;
  /** A colour (folders, and built-in file icons). */
  color?: string;
}

let theme: VsTheme | null = null;
let loading: string | null = null;
let version = 0;
const listeners = new Set<() => void>();
const images = new Map<string, string | null>();

function emit() {
  version++;
  for (const l of listeners) l();
}

/** Re-render when themes or icons load. */
export function useIconsVersion(): number {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => {
        listeners.delete(l);
      };
    },
    () => version,
  );
}

/** The VS Code theme chosen in settings, loading it the first time. */
export function vsTheme(id: string): VsTheme | null {
  if (theme?.id === id) return theme;
  if (loading !== id) {
    loading = id;
    void invoke<VsTheme>("icon_theme_load", { id }).then(
      (t) => {
        theme = t;
        emit();
      },
      () => {
        theme = null;
        emit();
      },
    );
  }
  return null;
}

/** A VS Code theme icon's URL (its file, served from the theme's folder). */
export function themeIconUrl(t: VsTheme, iconId: string | undefined): string | null {
  const rel = iconId ? t.defs[iconId] : undefined;
  return rel ? convertFileSrc(`${t.folder}/${rel}`, "fbicon") : null;
}

/** The icon id a VS Code theme gives a file or folder (light variants first in a light theme). */
export function vsIconId(t: VsTheme, name: string, folder: boolean, open: boolean, light: boolean): string | undefined {
  const lower = name.toLowerCase();
  const layers = light && t.light ? [t.light, t] : [t];
  if (folder) {
    for (const l of layers) {
      const id = (open ? l.folderNamesExpanded?.[lower] : undefined) ?? l.folderNames?.[lower];
      if (id) return id;
    }
    for (const l of layers) {
      const id = open ? (l.folderExpanded ?? l.folderIcon) : l.folderIcon;
      if (id) return id;
    }
    return undefined;
  }
  for (const l of layers) if (l.fileNames?.[lower]) return l.fileNames[lower];
  // "a.d.ts": try "d.ts", then "ts".
  const parts = lower.split(".").slice(1);
  for (let i = 0; i < parts.length; i++) {
    const ext = parts.slice(i).join(".");
    for (const l of layers) if (l.fileExtensions?.[ext]) return l.fileExtensions[ext];
  }
  for (const l of layers) if (l.file) return l.file;
  return undefined;
}

/** Files whose system icon is their own (programs, shortcuts): asked for by path, not extension. */
const OWN_ICON = new Set(["exe", "lnk", "ico", "url", "appref-ms", "app", "msc", "cpl", "scr"]);

/** The system's icon for a path (a data URL), cached by extension where icons are per type. */
export function systemIcon(path: string, ext: string, folder: boolean): string | null | undefined {
  const key = folder ? "<folder>" : OWN_ICON.has(ext) ? path.toLowerCase() : `.${ext}`;
  if (images.has(key)) return images.get(key);
  images.set(key, undefined as unknown as null);
  void invoke<string | null>("system_icon", { path, folder }).then(
    (url) => {
      images.set(key, url);
      emit();
    },
    () => {
      images.set(key, null);
      emit();
    },
  );
  return undefined;
}

/** An image file chosen in Change icon… (a data URL). */
export function customImage(path: string): string | null | undefined {
  const key = `file:${path.toLowerCase()}`;
  if (images.has(key)) return images.get(key);
  images.set(key, undefined as unknown as null);
  void invoke<string>("icon_image", { path }).then(
    (url) => {
      images.set(key, url);
      emit();
    },
    () => {
      images.set(key, null);
      emit();
    },
  );
  return undefined;
}

const placeKinds = new Map<string, string>();
const placeKey = (path: string) => path.replace(/[\\/]+$/, "").replace(/\//g, "\\").toLowerCase();

/** The home folder and its usual folders, by kind ("downloads"…), as the sidebar lists them. */
export function setPlaces(places: { path: string; kind: string }[]) {
  const before = JSON.stringify([...placeKinds]);
  placeKinds.clear();
  for (const p of places) placeKinds.set(placeKey(p.path), p.kind);
  if (JSON.stringify([...placeKinds]) !== before) emit();
}

/** The kind of place this folder is ("home", "downloads"…), if it's one. */
export function placeKind(path: string | null): string | undefined {
  return path ? placeKinds.get(placeKey(path)) : undefined;
}

/** The first rule for this entry: the exact path, then its name, then its extension. */
export function ruleFor(path: string | null, name: string, ext: string, rules: IconRule[] = getSettings().iconAssociations): IconRule | undefined {
  if (!rules.length) return undefined;
  const lower = name.toLowerCase();
  const p = path?.replace(/[\\/]+$/, "").toLowerCase();
  return (
    (p ? rules.find((r) => r.match === "path" && r.value.replace(/[\\/]+$/, "").toLowerCase() === p) : undefined) ??
    rules.find((r) => r.match === "name" && r.value.toLowerCase() === lower) ??
    (ext ? rules.find((r) => r.match === "ext" && r.value.toLowerCase().replace(/^\./, "") === ext) : undefined)
  );
}
