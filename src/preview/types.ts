/** Mirrors src-tauri/src/preview (the core) and preview_cmd.rs. */

export type Viewer =
  | "text"
  | "markdown"
  | "json"
  | "yaml"
  | "toml"
  | "csv"
  | "image"
  | "svg"
  | "audio"
  | "video"
  | "pdf"
  | "font"
  | "archive"
  | "binary"
  | "folder"
  | "cloud";

export interface Info {
  path: string;
  name: string;
  viewer: Viewer;
  mime: string;
  size: number;
  modified: number;
  created: number;
  accessed: number;
  readonly: boolean;
  hidden: boolean;
  link: boolean;
}

export interface Text {
  text: string;
  encoding: string;
  lineEnding: string;
  truncated: boolean;
  total: number;
}

export interface ArchiveList {
  entries: { name: string; size: number; dir: boolean }[];
  total: number | null;
  unpacked: number;
  truncated: boolean;
}

export interface TreeSize {
  bytes: number;
  files: number;
  folders: number;
  partial: boolean;
}

export interface FolderPreview {
  files: number;
  folders: number;
  readme: [string, string] | null;
  size: TreeSize | null;
}
