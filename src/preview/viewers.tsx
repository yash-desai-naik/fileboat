import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { lazy, Suspense, useEffect, useMemo, useState, type ReactNode } from "react";
import { formatSize } from "../lib/format";
import { updateSettings, useSettings } from "../lib/settings";
import type { ArchiveList, FolderPreview, Info, Text } from "./types";

// Loaded with the first text file shown (images and PDFs don't need it).
const CodeView = lazy(() => import("./CodeView"));

/** The first read of a text file; "Load more" doubles it. */
export const TEXT_START = 512 * 1024;

/** A URL the WebView can load `path` from (through the preview scheme, never `file://`). */
export function usePreviewUrl(path: string, reload: string): string | null {
  const [url, setUrl] = useState<string | null>(null);
  useEffect(() => {
    let stale = false;
    setUrl(null);
    void invoke<string>("preview_url", { path }).then((token) => {
      // The reload key makes a changed file load again instead of from the cache.
      if (!stale) setUrl(`${convertFileSrc(token, "fbpreview")}?v=${encodeURIComponent(reload)}`);
    });
    return () => {
      stale = true;
    };
  }, [path, reload]);
  return url;
}

/** A command's result for `deps`, with loading and error states. */
export function useLoad<T>(load: () => Promise<T>, deps: unknown[]): { data: T | null; error: string | null } {
  const [state, setState] = useState<{ data: T | null; error: string | null }>({ data: null, error: null });
  useEffect(() => {
    let stale = false;
    setState({ data: null, error: null });
    load().then(
      (data) => !stale && setState({ data, error: null }),
      (err) => !stale && setState({ data: null, error: String(err) }),
    );
    return () => {
      stale = true;
    };
  }, deps);
  return state;
}

export function Note({ children }: { children: ReactNode }) {
  return <div className="pv-note">{children}</div>;
}

export function Loading() {
  return <div className="pv-note muted">Loading…</div>;
}

/** Text with line numbers (until the highlighting editor has loaded, and for plain text). */
export function PlainText({ text }: { text: string }) {
  const lines = useMemo(() => text.split(/\r\n|\r|\n/), [text]);
  // A final newline doesn't make an empty last line.
  const shown = lines.length > 1 && lines[lines.length - 1] === "" ? lines.slice(0, -1) : lines;
  return (
    <div className="pv-plain mono">
      <div className="pv-gutter" aria-hidden>
        {shown.map((_, i) => (
          <div key={i}>{i + 1}</div>
        ))}
      </div>
      <pre>{shown.join("\n")}</pre>
    </div>
  );
}

/** The encoding and line endings, and for a big file "Showing the first 512 KB of 2.0 GB" with Load more. */
export function TextBar({ text, onMore, children }: { text: Text; onMore: () => void; children?: ReactNode }) {
  const facts = [text.encoding, text.lineEnding].filter(Boolean).join(" · ");
  return (
    <div className="pv-bar">
      <span>
        {facts}
        {text.truncated && ` · first ${formatSize(text.text.length)} of ${formatSize(text.total)}`}
      </span>
      <span className="pv-bar-actions">
        {children}
        {text.truncated && (
          <button className="link-btn" onClick={onMore}>
            Load more
          </button>
        )}
      </span>
    </div>
  );
}

/** The text of a file, read in growing chunks. */
export function useText(path: string, reload: string) {
  const [limit, setLimit] = useState(TEXT_START);
  useEffect(() => setLimit(TEXT_START), [path]);
  const state = useLoad(() => invoke<Text>("preview_text", { path, limit }), [path, reload, limit]);
  return { ...state, more: () => setLimit((l) => l * 4) };
}

/** Highlighted text (plain until the editor has loaded), with Wrap. */
export function Code({ text, name }: { text: string; name: string }) {
  const { previewWrap } = useSettings();
  return (
    <Suspense fallback={<PlainText text={text} />}>
      <CodeView text={text} name={name} wrap={previewWrap} />
    </Suspense>
  );
}

export function WrapToggle() {
  const { previewWrap } = useSettings();
  return (
    <button className={`link-btn${previewWrap ? " on" : ""}`} aria-pressed={previewWrap} onClick={() => updateSettings({ previewWrap: !previewWrap })}>
      Wrap
    </button>
  );
}

export function TextViewer({ info, reload }: { info: Info; reload: string }) {
  const { data, error, more } = useText(info.path, reload);
  if (error) return <Note>{error}</Note>;
  if (!data) return <Loading />;
  return (
    <>
      <TextBar text={data} onMore={more}>
        <WrapToggle />
      </TextBar>
      <Code text={data.text} name={info.name} />
    </>
  );
}

export function ImageViewer({ info, reload }: { info: Info; reload: string }) {
  const url = usePreviewUrl(info.path, reload);
  const [dims, setDims] = useState<string | null>(null);
  const [actual, setActual] = useState(false);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    setDims(null);
    setFailed(false);
  }, [url]);
  if (!url) return <Loading />;
  if (failed) return <Note>This image can't be shown.</Note>;
  return (
    <div className={`pv-image${actual ? " actual" : ""}`} onClick={() => setActual((a) => !a)} title={actual ? "Fit (click)" : "Actual size (click)"}>
      <img
        src={url}
        alt={info.name}
        draggable={false}
        onLoad={(e) => setDims(`${e.currentTarget.naturalWidth} × ${e.currentTarget.naturalHeight}`)}
        onError={() => setFailed(true)}
      />
      {dims && <span className="pv-badge">{dims}</span>}
    </div>
  );
}

export function MediaViewer({ info, reload }: { info: Info; reload: string }) {
  const url = usePreviewUrl(info.path, reload);
  const [failed, setFailed] = useState(false);
  if (!url) return <Loading />;
  if (failed) return <Note>This file can't be played here.</Note>;
  return info.viewer === "audio" ? (
    <div className="pv-media">
      <audio controls src={url} onError={() => setFailed(true)} />
    </div>
  ) : (
    <div className="pv-media">
      <video controls src={url} onError={() => setFailed(true)} />
    </div>
  );
}

export function PdfViewer({ info, reload }: { info: Info; reload: string }) {
  const url = usePreviewUrl(info.path, reload);
  if (!url) return <Loading />;
  return <iframe className="pv-frame" src={url} title={info.name} />;
}

const SAMPLE = ["The quick brown fox jumps over the lazy dog", "ABCDEFGHIJKLMNOPQRSTUVWXYZ", "abcdefghijklmnopqrstuvwxyz", "0123456789 !@#$%&*(){}[]<>=+-/"];

export function FontViewer({ info, reload }: { info: Info; reload: string }) {
  const url = usePreviewUrl(info.path, reload);
  const [family, setFamily] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  useEffect(() => {
    if (!url) return;
    const name = `fbpreview-${Math.random().toString(36).slice(2)}`;
    const face = new FontFace(name, `url(${url})`);
    let stale = false;
    face.load().then(
      () => {
        if (stale) return;
        document.fonts.add(face);
        setFamily(name);
      },
      () => !stale && setFailed(true),
    );
    return () => {
      stale = true;
      document.fonts.delete(face);
    };
  }, [url]);
  if (failed) return <Note>This font can't be shown.</Note>;
  if (!family) return <Loading />;
  return (
    <div className="pv-font" style={{ fontFamily: family }}>
      {[40, 28, 18, 14].map((px, i) => (
        <p key={px} style={{ fontSize: px }}>
          {SAMPLE[i]}
        </p>
      ))}
    </div>
  );
}

export function ArchiveViewer({ info, reload }: { info: Info; reload: string }) {
  const { data, error } = useLoad(() => invoke<ArchiveList>("preview_archive", { path: info.path }), [info.path, reload]);
  if (error) return <Note>{error}</Note>;
  if (!data) return <Loading />;
  const count = data.total ?? data.entries.length;
  return (
    <>
      <div className="pv-bar">
        <span>
          {count.toLocaleString()}
          {data.total === null ? "+" : ""} {count === 1 ? "item" : "items"} · {formatSize(data.unpacked)} unpacked
          {data.truncated ? ` · first ${data.entries.length.toLocaleString()} shown` : ""}
        </span>
      </div>
      <div className="pv-list mono">
        {data.entries.map((e, i) => (
          <div key={i} className={e.dir ? "dir" : ""}>
            <span className="name">{e.name}</span>
            <span className="muted">{e.dir ? "" : formatSize(e.size)}</span>
          </div>
        ))}
      </div>
    </>
  );
}

const HEX_STEP = 4096;

export function HexViewer({ info, reload }: { info: Info; reload: string }) {
  const [bytes, setBytes] = useState<number[]>([]);
  const [error, setError] = useState<string | null>(null);
  const load = (offset: number) =>
    invoke<number[]>("preview_bytes", { path: info.path, offset, len: HEX_STEP }).then(
      (b) => setBytes((prev) => (offset === 0 ? b : [...prev, ...b])),
      (err) => setError(String(err)),
    );
  useEffect(() => {
    setBytes([]);
    setError(null);
    void load(0);
  }, [info.path, reload]);
  if (error) return <Note>{error}</Note>;
  const rows: string[] = [];
  for (let at = 0; at < bytes.length; at += 16) {
    const chunk = bytes.slice(at, at + 16);
    const hex = chunk.map((b) => b.toString(16).padStart(2, "0")).join(" ");
    const ascii = chunk.map((b) => (b >= 0x20 && b < 0x7f ? String.fromCharCode(b) : ".")).join("");
    rows.push(`${at.toString(16).padStart(8, "0")}  ${hex.padEnd(47)}  ${ascii}`);
  }
  return (
    <>
      <div className="pv-bar">
        <span>Binary file · {formatSize(info.size)}</span>
        {bytes.length < info.size && (
          <button className="link-btn" onClick={() => void load(bytes.length)}>
            Load more
          </button>
        )}
      </div>
      <pre className="pv-hex mono">{rows.join("\n")}</pre>
    </>
  );
}

export function FolderViewer({ info, reload }: { info: Info; reload: string }) {
  const { data, error } = useLoad(() => invoke<FolderPreview>("preview_folder", { path: info.path }), [info.path, reload]);
  if (error) return <Note>{error}</Note>;
  if (!data) return <Loading />;
  const size = data.size;
  return (
    <>
      <div className="pv-facts">
        <div>
          <b>{data.folders.toLocaleString()}</b> folders · <b>{data.files.toLocaleString()}</b> files
        </div>
        {size && (
          <div>
            <b>{formatSize(size.bytes)}</b> in {size.files.toLocaleString()} files{size.partial ? " (some parts couldn't be read)" : ""}
          </div>
        )}
      </div>
      {data.readme && (
        <div className="pv-readme">
          <div className="pv-subhead">{data.readme[0]}</div>
          <PlainText text={data.readme[1]} />
        </div>
      )}
    </>
  );
}
