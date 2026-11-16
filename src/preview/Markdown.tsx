import { LanguageDescription } from "@codemirror/language";
import { languages } from "@codemirror/language-data";
import { classHighlighter, highlightCode } from "@lezer/highlight";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import DOMPurify from "dompurify";
import { marked } from "marked";
import { useEffect, useMemo, useRef } from "react";
import { navigate } from "../lib/tabs";
import { showToast } from "../shell/Toast";

// Images keep their address in data-src until the view decides how to load them: a local file
// through the preview scheme, a remote one not at all (no tracking pixels, nothing fetched).
DOMPurify.addHook("afterSanitizeAttributes", (node) => {
  if (node.tagName === "IMG") {
    const src = node.getAttribute("src");
    if (src) node.setAttribute("data-src", src);
    node.removeAttribute("src");
    node.removeAttribute("srcset");
  }
  if (node.tagName === "A") node.removeAttribute("target");
});

const FORBID_TAGS = ["style", "iframe", "object", "embed", "form", "input", "button", "textarea", "select", "link", "meta", "base"];

/** Markdown as safe HTML: no scripts, styles, frames or forms; events and `javascript:` links removed. */
function render(text: string): string {
  const html = marked.parse(text, { async: false, gfm: true }) as string;
  return DOMPurify.sanitize(html, { FORBID_TAGS, FORBID_ATTR: ["style"] });
}

/** `href` relative to `folder` ("../docs/a.md", "img/x.png") as a path; null for a web address. */
function resolve(folder: string, href: string): string | null {
  if (/^[a-z][a-z0-9+.-]*:/i.test(href) && !/^[a-z]:[\\/]/i.test(href)) return null;
  const sep = folder.includes("\\") ? "\\" : "/";
  const clean = decodeURIComponent(href.split(/[?#]/)[0]);
  if (!clean) return null;
  const absolute = /^([a-z]:)?[\\/]/i.test(clean);
  const parts = (absolute ? clean : `${folder}/${clean}`).split(/[\\/]+/);
  const out: string[] = [];
  for (const p of parts) {
    if (p === "." || (p === "" && out.length > 0)) continue;
    if (p === "..") {
      if (out.length > 1) out.pop();
    } else out.push(p);
  }
  return out.join(sep) || sep;
}

function splitPath(path: string): [string, string] {
  const i = Math.max(path.lastIndexOf("\\"), path.lastIndexOf("/"));
  return [path.slice(0, i) || path.slice(0, i + 1), path.slice(i + 1)];
}

/** Highlight `code` blocks that name their language, with the theme's syntax colours. */
async function highlightBlocks(root: HTMLElement, stale: () => boolean) {
  for (const code of root.querySelectorAll<HTMLElement>("pre > code[class*='language-']")) {
    const lang = [...code.classList].find((c) => c.startsWith("language-"))?.slice(9);
    const desc = lang && LanguageDescription.matchLanguageName(languages, lang, true);
    if (!desc) continue;
    const support = await desc.load().catch(() => null);
    if (!support || stale()) return;
    const text = code.textContent ?? "";
    const out = document.createDocumentFragment();
    highlightCode(
      text,
      support.language.parser.parse(text),
      classHighlighter,
      (t, classes) => {
        if (!classes) return out.append(t);
        const span = document.createElement("span");
        span.className = classes;
        span.textContent = t;
        out.append(span);
      },
      () => out.append("\n"),
    );
    code.replaceChildren(out);
  }
}

interface Props {
  text: string;
  /** The folder the file is in: relative links and images resolve against it. */
  folder: string;
}

export function Markdown({ text, folder }: Props) {
  const html = useMemo(() => render(text), [text]);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const root = ref.current;
    if (!root) return;
    let stale = false;
    // Headings get GitHub's anchors ("#getting-started"), prefixed so they can't shadow globals.
    const seen = new Map<string, number>();
    for (const h of root.querySelectorAll<HTMLElement>("h1, h2, h3, h4, h5, h6")) {
      const slug = (h.textContent ?? "").trim().toLowerCase().replace(/[^\p{L}\p{N}\s_-]/gu, "").replace(/\s/g, "-");
      const n = seen.get(slug) ?? 0;
      seen.set(slug, n + 1);
      h.id = `md-${n ? `${slug}-${n}` : slug}`;
    }
    for (const img of root.querySelectorAll<HTMLImageElement>("img[data-src]")) {
      const src = img.dataset.src!;
      if (src.startsWith("data:image/")) {
        img.src = src;
        continue;
      }
      const path = resolve(folder, src);
      if (!path) {
        img.replaceWith(Object.assign(document.createElement("span"), { className: "pv-md-remote", textContent: `[${img.alt || "image"}: not loaded]`, title: src }));
        continue;
      }
      void invoke<string>("preview_url", { path }).then((token) => {
        if (!stale) img.src = convertFileSrc(token, "fbpreview");
      });
    }
    void highlightBlocks(root, () => stale);
    return () => {
      stale = true;
    };
  }, [html, folder]);

  const onClick = (e: React.MouseEvent) => {
    const a = (e.target as HTMLElement).closest("a");
    if (!a) return;
    e.preventDefault();
    const href = a.getAttribute("href") ?? "";
    if (href.startsWith("#")) {
      const id = decodeURIComponent(href.slice(1));
      ref.current?.querySelector(`[id="md-${CSS.escape(id)}"]`)?.scrollIntoView({ block: "start" });
      return;
    }
    const path = resolve(folder, href);
    if (path) {
      // A file beside it: show it in Fileboat.
      const [dir, name] = splitPath(path);
      navigate(dir, name);
      return;
    }
    void invoke("open_url", { url: href }).catch((err) => showToast(String(err), "error"));
  };

  return <div ref={ref} className="pv-md" onClick={onClick} dangerouslySetInnerHTML={{ __html: html }} />;
}

export default Markdown;
