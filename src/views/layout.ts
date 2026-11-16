import type { Group } from "../lib/folder";

/**
 * Where things are drawn when rows are grouped: each group has a header line before its rows.
 * "Lines" are what the list draws (headers and rows); "rows" are the folder's rows.
 */
export interface Layout {
  /** Lines to draw. */
  lines: number;
  /** What a line is: a group header, or a folder row. */
  at(line: number): { group: Group; index: number } | { row: number };
  /** The line a folder row is drawn on. */
  lineOf(row: number): number;
}

/** The largest i with values[i] <= x (values ascending), or -1. */
function floorIndex(values: number[], x: number): number {
  let lo = 0;
  let hi = values.length - 1;
  let found = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (values[mid] <= x) {
      found = mid;
      lo = mid + 1;
    } else hi = mid - 1;
  }
  return found;
}

export function makeLayout(rows: number, groups: Group[]): Layout {
  if (!groups.length) return { lines: rows, at: (line) => ({ row: line }), lineOf: (row) => row };
  // A group's header sits on the line of its first row, pushed down by the headers before it.
  const headerLines = groups.map((g, i) => g.start + i);
  const starts = groups.map((g) => g.start);
  return {
    lines: rows + groups.length,
    at(line) {
      const i = Math.max(0, floorIndex(headerLines, line));
      return headerLines[i] === line ? { group: groups[i], index: i } : { row: line - (i + 1) };
    },
    // Headers before a row: every group starting at or before it (a collapsed group shares the
    // start of the next one, and its header comes first).
    lineOf: (row) => row + floorIndex(starts, row) + 1,
  };
}
