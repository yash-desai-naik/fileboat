import type { ReactNode } from "react";

/**
 * Fileboat's own icons, drawn here (no icon files, nothing to license): a page or a folder in a
 * colour that says what it is, with a small mark for the kind. Languages get their usual colours,
 * well-known files (package.json, Cargo.toml, Dockerfile, .env…) and folders (src, tests, docs,
 * .github, node_modules…) their own marks.
 */

/** Marks drawn inside a page (16×16 box, stroked). */
const MARK = {
  code: <path d="M6.5 8.5 5 10l1.5 1.5M9.5 8.5 11 10l-1.5 1.5" />,
  braces: <path d="M6.2 8c-.7 0-.7.5-.7 1s0 1-.7 1c.7 0 .7.5.7 1s0 1 .7 1M9.8 8c.7 0 .7.5.7 1s0 1 .7 1c-.7 0-.7.5-.7 1s0 1-.7 1" />,
  lines: <path d="M5.5 8.5h5M5.5 10.5h5M5.5 12.5h3" />,
  table: <path d="M5.5 8.5h5M5.5 10.5h5M5.5 12.5h5M8 8.5v4" />,
  image: <path d="M5 12.5 7 10l1.5 1.5L9.5 10l1.5 2.5M6.5 8.25h.01" />,
  play: <path d="M6.75 8.5v4l3.5-2z" />,
  zip: <path d="M8 4v1M8 6v1M8 8v1M7.25 10h1.5v2h-1.5z" />,
  window: <path d="M5.5 9h5v3.5h-5zM5.5 10.25h5" />,
  font: <path d="M5.75 12.5 8 8l2.25 4.5M6.5 11h3" />,
  gear: <path d="M8 8.6v.6M8 11.8v.6M6.3 10.5h.6M9.1 10.5h.6M8 9.6a.9.9 0 1 1 0 1.8.9.9 0 0 1 0-1.8z" />,
  key: <path d="M6.6 10.5a1.1 1.1 0 1 1 0 .01M7.7 10.5h3M9.6 10.5v1.2M10.7 10.5v.9" />,
  lock: <path d="M6 10.2h4v2.6H6zM6.8 10.2V9.4a1.2 1.2 0 0 1 2.4 0v.8" />,
  info: <path d="M8 9.8v2.7M8 8.3h.01" />,
  check: <path d="M5.8 10.6 7.3 12l3-3.2" />,
  dollar: <path d="M5.5 9.2 7 10.5l-1.5 1.3M8 12h2.5" />,
  git: <path d="M6.3 8.5v4M6.3 10.5c2.2 0 3.4-.5 3.4-2M9.7 8.5h.01M6.3 12.5h.01" />,
  pkg: <path d="M5.5 9.2 8 8l2.5 1.2v2.6L8 13l-2.5-1.2zM5.5 9.2 8 10.4l2.5-1.2M8 10.4V13" />,
  db: <path d="M5.5 8.8c0-.6 1.1-1 2.5-1s2.5.4 2.5 1-1.1 1-2.5 1-2.5-.4-2.5-1zM5.5 8.8v3.4c0 .6 1.1 1 2.5 1s2.5-.4 2.5-1V8.8" />,
  none: null,
} satisfies Record<string, ReactNode>;

export type Mark = keyof typeof MARK;

/** How a file shows: its colour and mark. */
export interface FileLook {
  color: string;
  mark: Mark;
}

/** Folder marks (drawn on the folder's front). */
const FOLDER_MARK = {
  none: null,
  code: <path d="M6.6 8.8 5.6 10l1 1.2M9.4 8.8l1 1.2-1 1.2" />,
  check: <path d="M6 10.1 7.3 11.3 10 8.7" />,
  lines: <path d="M5.8 9h4.4M5.8 10.8h3" />,
  git: <path d="M6.5 8.6v3M6.5 10c1.8 0 2.8-.3 2.8-1.4M9.3 8.6h.01" />,
  pkg: <path d="M6 9.4 8 8.4l2 1v2l-2 1-2-1zM6 9.4l2 1 2-1M8 10.4v2" />,
  image: <path d="M5.8 11.6 7.4 9.8l1.1 1.1.9-.9 1 1.6M6.9 9h.01" />,
  dollar: <path d="M5.8 9 7.1 10l-1.3 1M8.2 11.4h2" />,
  box: <path d="M8 8.6v3M6.6 10.4 8 11.8l1.4-1.4" />,
  gear: <path d="M8 8.5v.5M8 11.4v.5M6.6 10.2h.5M9 10.2h.5M8 9.4a.8.8 0 1 1 0 1.6.8.8 0 0 1 0-1.6z" />,
  blocks: <path d="M6 8.8h1.6v1.6H6zM8.4 8.8H10v1.6H8.4zM6 11h1.6v1.2H6z" />,
  globe: <path d="M8 8.4a1.9 1.9 0 1 1 0 3.8 1.9 1.9 0 0 1 0-3.8zM6.1 10.3h3.8M8 8.4c.6.6.6 3.2 0 3.8" />,
} satisfies Record<string, ReactNode>;

export type FolderMark = keyof typeof FOLDER_MARK;

export interface FolderLook {
  color: string;
  mark: FolderMark;
}

// Colours chosen to read on light and dark backgrounds alike.
const C = {
  rust: "#d4835a",
  ts: "#3178c6",
  js: "#d4a917",
  react: "#3fb3d6",
  py: "#4b8bbe",
  go: "#00a7cf",
  java: "#d9822b",
  kotlin: "#9b6dff",
  swift: "#f05138",
  c: "#7a8ca5",
  cpp: "#e0527b",
  cs: "#3fa34d",
  ruby: "#cc342d",
  php: "#7a86b8",
  html: "#e34c26",
  css: "#7b5fc0",
  sass: "#c6538c",
  vue: "#41b883",
  svelte: "#ff5a1f",
  json: "#c9a227",
  yaml: "#d14b4b",
  toml: "#a8613b",
  md: "#4a8fd8",
  shell: "#5aa64b",
  ps: "#4f8ad1",
  sql: "#e09422",
  dart: "#00b4ab",
  lua: "#5b6bd8",
  zig: "#ec915c",
  elixir: "#8e6bbf",
  haskell: "#7c6bb3",
  image: "#b267c9",
  media: "#d6559a",
  archive: "#b08850",
  program: "#6b7f99",
  font: "#d0485b",
  doc: "#4a8fd8",
  data: "#3fa38b",
  config: "#8a8f98",
  git: "#f05133",
  docker: "#2496ed",
  npm: "#cb3837",
  lock: "#9a9a9a",
  env: "#d4a917",
  license: "#c9a227",
  plain: "#8a8f98",
} as const;

const EXT: Record<string, FileLook> = {};
const add = (exts: string[], color: string, mark: Mark) => exts.forEach((e) => (EXT[e] = { color, mark }));
add(["rs"], C.rust, "code");
add(["ts", "mts", "cts"], C.ts, "code");
add(["tsx", "jsx"], C.react, "code");
add(["js", "mjs", "cjs"], C.js, "code");
add(["py", "pyi", "pyw", "ipynb"], C.py, "code");
add(["go"], C.go, "code");
add(["java", "class", "jar"], C.java, "code");
add(["kt", "kts"], C.kotlin, "code");
add(["swift"], C.swift, "code");
add(["c", "h"], C.c, "code");
add(["cpp", "cc", "cxx", "hpp", "hh"], C.cpp, "code");
add(["cs", "csx", "fs", "vb"], C.cs, "code");
add(["rb", "erb", "gemspec"], C.ruby, "code");
add(["php"], C.php, "code");
add(["html", "htm"], C.html, "code");
add(["css"], C.css, "code");
add(["scss", "sass", "less"], C.sass, "code");
add(["vue"], C.vue, "code");
add(["svelte", "astro"], C.svelte, "code");
add(["dart"], C.dart, "code");
add(["lua"], C.lua, "code");
add(["zig"], C.zig, "code");
add(["ex", "exs", "erl"], C.elixir, "code");
add(["hs", "ml", "clj", "scala", "r", "jl", "nim", "pl", "groovy", "gradle", "wasm"], C.haskell, "code");
add(["sh", "bash", "zsh", "fish"], C.shell, "dollar");
add(["ps1", "psm1", "bat", "cmd"], C.ps, "dollar");
add(["json", "jsonc", "json5", "webmanifest"], C.json, "braces");
add(["yaml", "yml"], C.yaml, "braces");
add(["toml"], C.toml, "braces");
add(["xml", "plist", "ini", "cfg", "conf", "properties"], C.config, "gear");
add(["lock"], C.lock, "lock");
add(["md", "mdx", "markdown", "rst", "adoc", "tex"], C.md, "lines");
add(["txt", "log", "pdf", "doc", "docx", "odt", "rtf"], C.doc, "lines");
add(["csv", "tsv", "xls", "xlsx", "ods", "parquet", "jsonl", "ndjson", "avro"], C.data, "table");
add(["sql", "db", "sqlite", "sqlite3"], C.sql, "db");
add(["png", "jpg", "jpeg", "gif", "webp", "avif", "svg", "ico", "bmp", "tif", "tiff", "heic", "psd", "ai", "fig", "sketch"], C.image, "image");
add(["mp3", "wav", "flac", "ogg", "m4a", "aac", "opus", "mp4", "mov", "mkv", "webm", "avi", "wmv", "m4v"], C.media, "play");
add(["zip", "gz", "tgz", "tar", "7z", "rar", "xz", "bz2", "zst", "war", "whl", "nupkg", "deb", "rpm", "iso", "dmg", "vsix"], C.archive, "zip");
add(["exe", "msi", "dll", "so", "dylib", "app", "appimage", "apk", "com", "lnk", "url"], C.program, "window");
add(["ttf", "otf", "woff", "woff2", "eot"], C.font, "font");
add(["pem", "crt", "key", "pfx", "p12"], C.env, "key");

/** Files known by name (lower case). */
const NAMES: Record<string, FileLook> = {
  "package.json": { color: C.npm, mark: "pkg" },
  "package-lock.json": { color: C.npm, mark: "lock" },
  "pnpm-lock.yaml": { color: C.npm, mark: "lock" },
  "yarn.lock": { color: C.npm, mark: "lock" },
  "bun.lockb": { color: C.npm, mark: "lock" },
  "cargo.toml": { color: C.rust, mark: "pkg" },
  "cargo.lock": { color: C.rust, mark: "lock" },
  "go.mod": { color: C.go, mark: "pkg" },
  "go.sum": { color: C.go, mark: "lock" },
  "pyproject.toml": { color: C.py, mark: "pkg" },
  "requirements.txt": { color: C.py, mark: "pkg" },
  "poetry.lock": { color: C.py, mark: "lock" },
  "gemfile": { color: C.ruby, mark: "pkg" },
  "gemfile.lock": { color: C.ruby, mark: "lock" },
  "composer.json": { color: C.php, mark: "pkg" },
  "pom.xml": { color: C.java, mark: "pkg" },
  "build.gradle": { color: C.java, mark: "pkg" },
  "tsconfig.json": { color: C.ts, mark: "gear" },
  "jsconfig.json": { color: C.js, mark: "gear" },
  "vite.config.ts": { color: C.svelte, mark: "gear" },
  "vite.config.js": { color: C.svelte, mark: "gear" },
  "tauri.conf.json": { color: C.zig, mark: "gear" },
  dockerfile: { color: C.docker, mark: "pkg" },
  "docker-compose.yml": { color: C.docker, mark: "gear" },
  "docker-compose.yaml": { color: C.docker, mark: "gear" },
  "compose.yml": { color: C.docker, mark: "gear" },
  makefile: { color: C.shell, mark: "dollar" },
  ".gitignore": { color: C.git, mark: "git" },
  ".gitattributes": { color: C.git, mark: "git" },
  ".gitmodules": { color: C.git, mark: "git" },
  ".editorconfig": { color: C.config, mark: "gear" },
  ".prettierrc": { color: C.config, mark: "gear" },
  ".eslintrc": { color: C.config, mark: "gear" },
  ".npmrc": { color: C.npm, mark: "gear" },
  ".nvmrc": { color: C.shell, mark: "gear" },
  readme: { color: C.md, mark: "info" },
  "readme.md": { color: C.md, mark: "info" },
  license: { color: C.license, mark: "check" },
  "license.md": { color: C.license, mark: "check" },
  "license.txt": { color: C.license, mark: "check" },
  "changelog.md": { color: C.md, mark: "lines" },
  "claude.md": { color: C.zig, mark: "info" },
};

/** How a file looks in the built-in theme. */
export function fileLook(name: string, ext: string): FileLook {
  const lower = name.toLowerCase();
  if (NAMES[lower]) return NAMES[lower];
  if (lower === ".env" || lower.startsWith(".env.")) return { color: C.env, mark: "key" };
  if (/\.(test|spec)\.[jt]sx?$/.test(lower)) return { color: C.shell, mark: "check" };
  if (/^eslint\.config\.|^\.eslintrc/.test(lower)) return { color: C.config, mark: "gear" };
  return EXT[ext] ?? { color: C.plain, mark: "none" };
}

const FOLDERS: Record<string, FolderLook> = {};
const folders = (names: string[], color: string, mark: FolderMark) => names.forEach((n) => (FOLDERS[n] = { color, mark }));
folders(["src", "source", "lib", "app", "pkg", "internal", "crates"], "#4a8fd8", "code");
folders(["test", "tests", "__tests__", "spec", "e2e", "cypress"], "#5aa64b", "check");
folders(["docs", "doc", "documentation", "notes"], "#4a8fd8", "lines");
folders([".git", ".github", ".gitlab"], C.git, "git");
folders(["node_modules", "vendor", "packages", "third_party", ".venv", "venv"], "#7e9a3c", "pkg");
folders(["assets", "images", "img", "icons", "public", "static", "media"], C.image, "image");
folders(["scripts", "bin", "tools", ".husky"], C.shell, "dollar");
folders(["dist", "build", "out", "target", "release", "debug", "coverage"], "#a8763b", "box");
folders(["config", "configs", ".config", ".vscode", ".idea", ".devcontainer", "settings"], C.config, "gear");
folders(["components", "views", "pages", "routes", "screens", "widgets", "ui"], "#41a3b8", "blocks");
folders(["api", "server", "services", "web", "www"], "#7b5fc0", "globe");
folders([".claude"], C.zig, "gear");

/** The plain folder colour (from the theme's accent, as before). */
export const FOLDER_PLAIN: FolderLook = { color: "var(--folder, var(--accent))", mark: "none" };

/** How a folder looks in the built-in theme. */
export function folderLook(name: string): FolderLook {
  return FOLDERS[name.toLowerCase()] ?? FOLDER_PLAIN;
}

const PAGE = "M4.5 1.75h4.75l3 3v8.25a1.25 1.25 0 0 1-1.25 1.25h-6.5a1.25 1.25 0 0 1-1.25-1.25v-10a1.25 1.25 0 0 1 1.25-1.25zM9.25 1.75v3h3";

/** A page in a colour, with its mark. */
export function FileGlyph({ look, size = 15 }: { look: FileLook; size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={1.4} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" style={{ color: look.color }}>
      <path d={PAGE} />
      {MARK[look.mark]}
    </svg>
  );
}

/** A folder in a colour (open: the front tilted), with its mark. */
export function FolderGlyph({ look, size = 15, open = false }: { look: FolderLook; size?: number; open?: boolean }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={1.4} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" style={{ color: look.color }}>
      {open ? (
        <>
          <path d="M2 12.5V4.25C2 3.56 2.56 3 3.25 3h2.9l1.5 1.5h4.6c.69 0 1.25.56 1.25 1.25V7" />
          <path d="M2 12.5 3.6 8c.17-.6.72-1 1.34-1h8.66c.68 0 1.14.66.9 1.3L12.9 12c-.2.58-.75 1-1.36 1H2.5" fill="currentColor" fillOpacity={0.14} />
        </>
      ) : (
        <path d="M2 4.25C2 3.56 2.56 3 3.25 3h2.9l1.5 1.5h5.1c.69 0 1.25.56 1.25 1.25v6.5c0 .69-.56 1.25-1.25 1.25h-9.5C2.56 13.5 2 12.94 2 12.25z" fill="currentColor" fillOpacity={0.14} />
      )}
      {!open && FOLDER_MARK[look.mark]}
    </svg>
  );
}
