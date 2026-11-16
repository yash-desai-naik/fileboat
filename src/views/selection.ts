/**
 * Which rows are selected in one view: names (so a selection survives rows moving when files
 * change or the sort changes), plus the focused row (the keyboard cursor) and the anchor that
 * Shift extends from.
 */
export class Selection {
  names = new Set<string>();
  /** Row of the keyboard cursor, -1 for none. */
  focus = -1;
  /** Name at the cursor, to find it again after rows move. */
  focusName: string | null = null;
  /** Row Shift+click and Shift+arrows extend from. */
  anchor = -1;
  version = 0;
  private listeners = new Set<() => void>();

  has(name: string): boolean {
    return this.names.has(name);
  }

  get size(): number {
    return this.names.size;
  }

  /** Replace the selection. `anchor` defaults to the focus. */
  set(names: Iterable<string>, focus: number, focusName: string | null, anchor = focus) {
    this.names = new Set(names);
    this.focus = focus;
    this.focusName = focusName;
    this.anchor = anchor;
    this.bump();
  }

  /** Ctrl+click: add or remove one name, moving the cursor there. */
  toggle(name: string, row: number) {
    const next = new Set(this.names);
    if (next.has(name)) next.delete(name);
    else next.add(name);
    this.set(next, row, name);
  }

  /** The cursor moved to a known row (after rows moved) without changing the selection. */
  moveFocus(row: number) {
    this.focus = row;
    this.bump();
  }

  clear() {
    this.set([], -1, null);
  }

  subscribe = (listener: () => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  private bump() {
    this.version++;
    for (const l of this.listeners) l();
  }
}
