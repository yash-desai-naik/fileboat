import type { Entry } from "./folder";

/** "980 B", "4.2 KB", "13 MB", "1.07 GB": binary units, as Explorer and most developers expect. */
export function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB", "PB"];
  let v = bytes / 1024;
  let u = 0;
  while (v >= 1024 && u < units.length - 1) {
    v /= 1024;
    u++;
  }
  const digits = v < 10 ? 2 : v < 100 ? 1 : 0;
  return `${v.toFixed(digits)} ${units[u]}`;
}

const pad = (n: number) => String(n).padStart(2, "0");

/**
 * "Today 10:42", "Yesterday 18:05", else "2026-11-03 14:05": unambiguous in every locale, 24-hour,
 * and the same width in a monospaced column.
 */
export function formatDate(ms: number, now: Date = new Date()): string {
  if (!ms) return "";
  const d = new Date(ms);
  const time = `${pad(d.getHours())}:${pad(d.getMinutes())}`;
  const day = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const diff = Math.round((day(now) - day(d)) / 86_400_000);
  if (diff === 0) return `Today ${time}`;
  if (diff === 1) return `Yesterday ${time}`;
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${time}`;
}

/** What a file is, in words, by extension. Unknown ones read "XYZ file". */
const TYPES: Record<string, string> = {
  // code
  rs: "Rust source",
  ts: "TypeScript",
  tsx: "TypeScript React",
  mts: "TypeScript",
  cts: "TypeScript",
  js: "JavaScript",
  jsx: "JavaScript React",
  mjs: "JavaScript module",
  cjs: "JavaScript (CommonJS)",
  py: "Python",
  go: "Go source",
  java: "Java source",
  kt: "Kotlin source",
  swift: "Swift source",
  c: "C source",
  h: "C header",
  cpp: "C++ source",
  hpp: "C++ header",
  cs: "C# source",
  php: "PHP",
  rb: "Ruby",
  lua: "Lua",
  dart: "Dart",
  sh: "Shell script",
  bash: "Shell script",
  zsh: "Shell script",
  ps1: "PowerShell script",
  bat: "Batch file",
  cmd: "Batch file",
  sql: "SQL",
  vue: "Vue component",
  svelte: "Svelte component",
  html: "HTML",
  htm: "HTML",
  css: "CSS",
  scss: "Sass",
  less: "Less",
  // data and config
  json: "JSON",
  jsonc: "JSON with comments",
  yaml: "YAML",
  yml: "YAML",
  toml: "TOML",
  xml: "XML",
  ini: "Settings",
  env: "Environment",
  lock: "Lock file",
  csv: "CSV",
  tsv: "TSV",
  log: "Log",
  // documents
  md: "Markdown",
  mdx: "MDX",
  txt: "Text",
  pdf: "PDF",
  doc: "Word document",
  docx: "Word document",
  xls: "Excel workbook",
  xlsx: "Excel workbook",
  ppt: "PowerPoint",
  pptx: "PowerPoint",
  // media
  png: "PNG image",
  jpg: "JPEG image",
  jpeg: "JPEG image",
  gif: "GIF image",
  webp: "WebP image",
  avif: "AVIF image",
  svg: "SVG image",
  ico: "Icon",
  bmp: "Bitmap image",
  heic: "HEIC image",
  mp3: "MP3 audio",
  wav: "WAV audio",
  flac: "FLAC audio",
  mp4: "MP4 video",
  mov: "QuickTime video",
  mkv: "MKV video",
  webm: "WebM video",
  ttf: "Font",
  otf: "Font",
  woff: "Web font",
  woff2: "Web font",
  // archives and programs
  zip: "ZIP archive",
  gz: "Gzip archive",
  tgz: "Gzip archive",
  tar: "Tar archive",
  "7z": "7-Zip archive",
  rar: "RAR archive",
  xz: "XZ archive",
  exe: "Application",
  msi: "Windows installer",
  dll: "Library",
  so: "Library",
  dylib: "Library",
  app: "Application",
  dmg: "Disk image",
  iso: "Disk image",
  lnk: "Shortcut",
  url: "Internet shortcut",
};

/** Files known by their whole name. */
const NAMED: Record<string, string> = {
  ".gitignore": "Git ignore rules",
  ".gitattributes": "Git attributes",
  ".gitmodules": "Git submodules",
  ".editorconfig": "EditorConfig",
  ".env": "Environment",
  ".npmrc": "npm settings",
  ".nvmrc": "Node version",
  ".prettierrc": "Prettier settings",
  ".dockerignore": "Docker ignore rules",
  dockerfile: "Dockerfile",
  makefile: "Makefile",
  license: "License",
  readme: "Readme",
  procfile: "Procfile",
};

export function describeType(e: Pick<Entry, "kind" | "ext" | "link" | "name">): string {
  if (e.kind === "folder") return e.link ? "Folder link" : "Folder";
  if (e.kind === "other") return e.link ? "Broken link" : "Special file";
  const named = NAMED[e.name.toLowerCase()];
  if (named) return named;
  if (e.name.toLowerCase().startsWith(".env.")) return "Environment";
  if (!e.ext) return "File";
  const known = TYPES[e.ext];
  if (known) return known;
  // "XYZ file" only for extension-like endings, not random suffixes like ".tmp.332620.400a3222785c".
  return /^[a-z][a-z0-9]{0,5}$/.test(e.ext) ? `${e.ext.toUpperCase()} file` : "File";
}
