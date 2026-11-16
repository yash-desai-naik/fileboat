import type { ReactNode } from "react";
import type { Entry } from "../lib/folder";

/**
 * Fileboat's built-in file icons: a page with a mark for the kind of file, coloured from the
 * theme. Icon themes (VS Code format, customisable) replace these later; this set keeps a folder
 * easy to scan until then.
 */
export type FileCategory = "code" | "config" | "doc" | "image" | "media" | "archive" | "program" | "font" | "data" | "file";

const CATEGORIES: Record<Exclude<FileCategory, "file">, string[]> = {
  code: [
    "rs", "ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs", "py", "go", "java", "kt", "kts", "swift", "c", "h", "cpp", "cc",
    "hpp", "cs", "fs", "php", "rb", "lua", "dart", "sh", "bash", "zsh", "fish", "ps1", "psm1", "bat", "cmd", "vue", "svelte",
    "astro", "html", "htm", "css", "scss", "sass", "less", "ex", "exs", "erl", "hs", "ml", "clj", "scala", "r", "jl", "zig",
    "nim", "vb", "pl", "groovy", "gradle", "wasm", "ipynb",
  ],
  config: ["json", "jsonc", "json5", "yaml", "yml", "toml", "xml", "ini", "cfg", "conf", "env", "lock", "properties", "editorconfig", "gitignore", "gitattributes", "npmrc", "nvmrc", "dockerignore"],
  doc: ["md", "mdx", "txt", "rst", "adoc", "pdf", "doc", "docx", "odt", "rtf", "xls", "xlsx", "ods", "ppt", "pptx", "odp", "log", "tex"],
  data: ["csv", "tsv", "sql", "db", "sqlite", "sqlite3", "parquet", "avro", "ndjson", "jsonl"],
  image: ["png", "jpg", "jpeg", "gif", "webp", "avif", "svg", "ico", "bmp", "tif", "tiff", "heic", "psd", "ai", "fig", "sketch"],
  media: ["mp3", "wav", "flac", "ogg", "m4a", "aac", "mp4", "mov", "mkv", "webm", "avi", "wmv"],
  archive: ["zip", "gz", "tgz", "tar", "7z", "rar", "xz", "bz2", "zst", "jar", "war", "whl", "nupkg", "deb", "rpm", "iso", "dmg"],
  program: ["exe", "msi", "dll", "so", "dylib", "app", "appimage", "apk", "com", "lnk", "url"],
  font: ["ttf", "otf", "woff", "woff2", "eot"],
};

const BY_EXT = new Map<string, FileCategory>();
for (const [cat, exts] of Object.entries(CATEGORIES)) for (const e of exts) BY_EXT.set(e, cat as FileCategory);

/** Files known by name rather than extension. */
const BY_NAME: Record<string, FileCategory> = {
  dockerfile: "config",
  makefile: "code",
  license: "doc",
  readme: "doc",
  ".gitignore": "config",
  ".gitattributes": "config",
  ".editorconfig": "config",
  ".env": "config",
  ".npmrc": "config",
  ".prettierrc": "config",
  ".eslintrc": "config",
};

export function fileCategory(entry: Pick<Entry, "name" | "ext">): FileCategory {
  const lower = entry.name.toLowerCase();
  return BY_NAME[lower] ?? BY_NAME[lower.replace(/\.[^.]*$/, "")] ?? BY_EXT.get(entry.ext) ?? (lower.startsWith(".env") ? "config" : "file");
}

/** The mark drawn inside the page, per category. */
const MARKS: Record<FileCategory, ReactNode> = {
  code: <path d="M6.5 8.5 5 10l1.5 1.5M9.5 8.5 11 10l-1.5 1.5" />,
  config: <path d="M6.2 8c-.7 0-.7.5-.7 1s0 1-.7 1c.7 0 .7.5.7 1s0 1 .7 1M9.8 8c.7 0 .7.5.7 1s0 1 .7 1c-.7 0-.7.5-.7 1s0 1-.7 1" />,
  doc: <path d="M5.5 8.5h5M5.5 10.5h5M5.5 12.5h3" />,
  data: <path d="M5.5 8.5h5M5.5 10.5h5M5.5 12.5h5M8 8.5v4" />,
  image: <path d="M5 12.5 7 10l1.5 1.5L9.5 10l1.5 2.5M6.5 8.25h.01" />,
  media: <path d="M6.75 8.5v4l3.5-2z" />,
  archive: <path d="M8 4v1M8 6v1M8 8v1M7.25 10h1.5v2h-1.5z" />,
  program: <path d="M5.5 9h5v3.5h-5zM5.5 10.25h5" />,
  font: <path d="M5.75 12.5 8 8l2.25 4.5M6.5 11h3" />,
  file: null,
};

export function FileTypeIcon({ entry, size = 15 }: { entry: Pick<Entry, "name" | "ext">; size?: number }) {
  const cat = fileCategory(entry);
  return (
    <svg
      className={`ft ft-${cat}`}
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.4}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M4.5 1.75h4.75l3 3v8.25a1.25 1.25 0 0 1-1.25 1.25h-6.5a1.25 1.25 0 0 1-1.25-1.25v-10a1.25 1.25 0 0 1 1.25-1.25zM9.25 1.75v3h3" />
      {MARKS[cat]}
    </svg>
  );
}
