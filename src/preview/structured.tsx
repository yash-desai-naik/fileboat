import { useMemo, useState } from "react";

type Json = null | boolean | number | string | Json[] | { [k: string]: Json };

/** Children shown before "… more". */
const PAGE = 200;

function summary(v: Json): string {
  if (Array.isArray(v)) return `[${v.length}]`;
  if (v && typeof v === "object") return `{${Object.keys(v).length}}`;
  return "";
}

function Leaf({ value }: { value: Json }) {
  if (value === null) return <span className="tok-atom">null</span>;
  if (typeof value === "boolean") return <span className="tok-bool">{String(value)}</span>;
  if (typeof value === "number") return <span className="tok-number">{value}</span>;
  return <span className="tok-string">{JSON.stringify(value)}</span>;
}

function Node({ name, value, depth }: { name: string | null; value: Json; depth: number }) {
  const branch = value !== null && typeof value === "object";
  // The first two levels start open.
  const [open, setOpen] = useState(depth < 2);
  const [shown, setShown] = useState(PAGE);
  const entries: [string, Json][] = branch ? (Array.isArray(value) ? value.map((v, i) => [String(i), v]) : Object.entries(value)) : [];
  return (
    <div className="pv-tree-node">
      <div className={`pv-tree-row${branch ? " branch" : ""}`} onClick={() => branch && setOpen((o) => !o)}>
        {branch ? <span className="pv-tree-twisty">{open ? "▾" : "▸"}</span> : <span className="pv-tree-twisty" />}
        {name !== null && <span className="tok-propertyName">{name}</span>}
        {name !== null && <span className="tok-punctuation">: </span>}
        {branch ? <span className="muted">{summary(value)}</span> : <Leaf value={value} />}
      </div>
      {branch && open && (
        <div className="pv-tree-children">
          {entries.slice(0, shown).map(([k, v]) => (
            <Node key={k} name={k} value={v} depth={depth + 1} />
          ))}
          {entries.length > shown && (
            <button className="link-btn pv-tree-more" onClick={() => setShown((n) => n + PAGE * 5)}>
              … {(entries.length - shown).toLocaleString()} more
            </button>
          )}
        </div>
      )}
    </div>
  );
}

/** JSON (and YAML, TOML) as a tree you can open and close. */
export function Tree({ value }: { value: unknown }) {
  return (
    <div className="pv-tree mono">
      <Node name={null} value={value as Json} depth={0} />
    </div>
  );
}

/** Rows of a CSV or TSV (quotes, doubled quotes and newlines in quotes), up to `limit` rows. */
export function parseDelimited(text: string, delimiter: string, limit: number): { rows: string[][]; more: boolean } {
  const rows: string[][] = [];
  let row: string[] = [];
  let cell = "";
  let quoted = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quoted) {
      if (c === '"') {
        if (text[i + 1] === '"') {
          cell += '"';
          i++;
        } else quoted = false;
      } else cell += c;
    } else if (c === '"' && cell === "") quoted = true;
    else if (c === delimiter) {
      row.push(cell);
      cell = "";
    } else if (c === "\n" || c === "\r") {
      if (c === "\r" && text[i + 1] === "\n") i++;
      row.push(cell);
      rows.push(row);
      row = [];
      cell = "";
      if (rows.length >= limit) return { rows, more: i < text.length - 1 };
    } else cell += c;
  }
  if (cell !== "" || row.length) {
    row.push(cell);
    rows.push(row);
  }
  return { rows, more: false };
}

/** The delimiter of a CSV: whichever of , ; | or tab the first line has most of. */
export function guessDelimiter(text: string, name: string): string {
  if (name.toLowerCase().endsWith(".tsv")) return "\t";
  const first = text.slice(0, text.search(/\r?\n|$/));
  const counts = [",", ";", "\t", "|"].map((d) => [d, first.split(d).length - 1] as const);
  const best = counts.reduce((a, b) => (b[1] > a[1] ? b : a));
  return best[1] > 0 ? best[0] : ",";
}

const ROWS = 1000;

/** A CSV as a table with its first row as the header. */
export function CsvTable({ text, name }: { text: string; name: string }) {
  const { rows, more } = useMemo(() => parseDelimited(text, guessDelimiter(text, name), ROWS + 1), [text, name]);
  if (rows.length === 0) return <div className="pv-note">Empty</div>;
  const [head, ...body] = rows;
  return (
    <div className="pv-table-wrap">
      <table className="pv-table">
        <thead>
          <tr>
            <th className="num" />
            {head.map((h, i) => (
              <th key={i}>{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {body.slice(0, ROWS).map((r, i) => (
            <tr key={i}>
              <td className="num">{i + 1}</td>
              {r.map((c, j) => (
                <td key={j}>{c}</td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
      {(more || body.length > ROWS) && <div className="pv-note muted">The first {ROWS.toLocaleString()} rows are shown.</div>}
    </div>
  );
}
