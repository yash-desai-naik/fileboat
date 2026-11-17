import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { ChevronIcon } from "./Icons";

export type MenuItem =
  | {
      label: string;
      onSelect?: () => void;
      hint?: string;
      danger?: boolean;
      disabled?: boolean;
      /** A check mark (for choices like the view or sort). */
      checked?: boolean;
      /** Opens a menu beside this one instead of running something. */
      submenu?: MenuItem[];
    }
  | "separator";

interface MenuProps {
  x: number;
  y: number;
  items: MenuItem[];
  /** Close this menu (and, from the top menu, everything). */
  onClose: () => void;
  /** For a submenu: close the whole menu (after a choice). */
  onDone?: () => void;
  /** For a submenu: the parent's item rectangle, to open on its left when there's no room right. */
  parentLeft?: number;
  /** Opened from the keyboard: the first item starts highlighted. */
  keyboard?: boolean;
}

/**
 * A menu at a position: closes on a click outside, Esc or a choice. Items can open submenus
 * (hover, Enter or →; ← goes back). Arrow keys move, letters jump.
 */
export function Menu({ x, y, items, onClose, onDone, parentLeft, keyboard }: MenuProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x, y });
  const firstItem = items.findIndex((it) => it !== "separator" && !it.disabled);
  const [active, setActive] = useState(keyboard ? firstItem : -1);
  const [open, setOpen] = useState<{ index: number; x: number; y: number; left: number; keyboard: boolean } | null>(null);
  const done = onDone ?? onClose;

  // Keep the menu inside the window (a submenu flips to the parent's left side if needed).
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const r = el.getBoundingClientRect();
    const fitsRight = x + r.width <= window.innerWidth - 4;
    setPos({
      x: fitsRight ? x : parentLeft !== undefined ? Math.max(4, parentLeft - r.width) : Math.max(4, window.innerWidth - r.width - 4),
      y: Math.max(4, Math.min(y, window.innerHeight - r.height - 4)),
    });
    el.focus();
  }, [x, y, parentLeft]);

  // Only the top menu listens for outside clicks; a click in any open menu level is inside.
  useEffect(() => {
    if (onDone) return;
    const onDown = (e: MouseEvent) => {
      if (!(e.target as Element | null)?.closest?.(".menu")) onClose();
    };
    window.addEventListener("mousedown", onDown, true);
    window.addEventListener("blur", onClose);
    return () => {
      window.removeEventListener("mousedown", onDown, true);
      window.removeEventListener("blur", onClose);
    };
  }, [onClose, onDone]);

  const selectable = items.map((item, i) => ({ item, i })).filter(({ item }) => item !== "separator" && !item.disabled);

  // Buttons don't include separators: item i is button buttonIndex(i).
  const buttonIndex = (i: number) => items.slice(0, i).filter((it) => it !== "separator").length;

  const openSub = (i: number, fromKeyboard = false) => {
    const el = ref.current?.querySelectorAll<HTMLElement>(":scope > .menu-item")[buttonIndex(i)];
    const r = el?.getBoundingClientRect();
    if (r) setOpen({ index: i, x: r.right + 2, y: r.top - 5, left: r.left - 2, keyboard: fromKeyboard });
  };

  const choose = (i: number, fromKeyboard = false) => {
    const item = items[i];
    if (!item || item === "separator" || item.disabled) return;
    if (item.submenu) return openSub(i, fromKeyboard);
    done();
    item.onSelect?.();
  };

  return createPortal(
    <>
      <div
        ref={ref}
        className="menu"
        role="menu"
        tabIndex={-1}
        style={{ left: pos.x, top: pos.y }}
        onKeyDown={(e) => {
          if (open) return; // the submenu has the keyboard
          const at = selectable.findIndex((s) => s.i === active);
          if (e.key === "Escape" || (e.key === "ArrowLeft" && onDone)) {
            e.preventDefault();
            e.stopPropagation();
            onClose();
          } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
            e.preventDefault();
            const next = e.key === "ArrowDown" ? at + 1 : at - 1;
            setActive(selectable[(next + selectable.length) % selectable.length]?.i ?? -1);
          } else if (e.key === "Enter" || e.key === " " || (e.key === "ArrowRight" && active >= 0)) {
            e.preventDefault();
            e.stopPropagation();
            const item = items[active];
            if (item && item !== "separator" && (e.key !== "ArrowRight" || item.submenu)) choose(active, true);
          } else if (e.key.length === 1) {
            // A letter jumps to the next item starting with it.
            const key = e.key.toLowerCase();
            const from = Math.max(at, 0);
            const hit = [...selectable.slice(from + 1), ...selectable.slice(0, from + 1)].find(
              (s) => s.item !== "separator" && s.item.label.toLowerCase().startsWith(key),
            );
            if (hit) setActive(hit.i);
          }
        }}
      >
        {items.map((item, i) =>
          item === "separator" ? (
            <div key={i} className="menu-separator" role="separator" />
          ) : (
            <button
              key={i}
              role="menuitem"
              aria-haspopup={item.submenu ? "menu" : undefined}
              aria-expanded={item.submenu ? open?.index === i : undefined}
              className={`menu-item${item.danger ? " danger" : ""}${active === i || open?.index === i ? " active" : ""}`}
              disabled={item.disabled}
              onMouseEnter={() => {
                setActive(i);
                if (item.submenu) openSub(i);
                else setOpen(null);
              }}
              onClick={() => choose(i)}
            >
              <span className="menu-check" aria-hidden="true">
                {item.checked ? "✓" : ""}
              </span>
              <span className="menu-label">{item.label}</span>
              {item.hint && <span className="menu-hint">{item.hint}</span>}
              {item.submenu && (
                <span className="menu-arrow" aria-hidden="true">
                  <ChevronIcon open={false} size={10} />
                </span>
              )}
            </button>
          ),
        )}
      </div>
      {open && items[open.index] !== "separator" && (
        <Menu
          x={open.x}
          y={open.y}
          parentLeft={open.left}
          keyboard={open.keyboard}
          items={(items[open.index] as { submenu?: MenuItem[] }).submenu ?? []}
          onClose={() => {
            setOpen(null);
            ref.current?.focus();
            setActive(open.index);
          }}
          onDone={done}
        />
      )}
    </>,
    document.body,
  );
}
