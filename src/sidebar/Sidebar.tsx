import { invoke } from "@tauri-apps/api/core";
import { Fragment, useEffect, useRef, useState, type ReactNode } from "react";
import { formatSize } from "../lib/format";
import { fmtShortcut } from "../lib/platform";
import { addFavourite, placeFavourites, removeFavourite } from "../lib/favourites";
import { getSettings, updateSettings, useSettings } from "../lib/settings";
import { EntryIcon } from "../icons/EntryIcon";
import { setPlaces } from "../icons/icons";
import { Menu, type MenuItem } from "../shell/Menu";
import { placeMenu } from "../views/contextMenu";
import { GitPanel } from "./GitPanel";
import { activeTab, navigate, newTab, samePath, tabTitle, useTabs } from "../lib/tabs";
import { ChevronIcon, CloseSmallIcon, DriveIcon, NetworkDriveIcon, StarIcon } from "../shell/Icons";

interface Place {
  name: string;
  path: string;
  kind: string;
}

interface Drive {
  path: string;
  label: string;
  kind: "fixed" | "removable" | "network" | "optical" | "other";
  free: number | null;
  total: number | null;
}

interface Places {
  home: Place[];
  drives: Drive[];
}

/**
 * Favourites, home folders and drives. Click to go there, middle-click for a new tab, right-click
 * for more (Change Icon…, Move Up…). Favourites and home folders show their folder's icon, so
 * they follow the icon theme and Change Icon…. Drop folders on Favourites to pin them; drag a
 * favourite up or down to reorder. Drives are read again whenever the window gets focus.
 */
function Places() {
  const settings = useSettings();
  useTabs();
  const current = activeTab().path;
  const [places, setPlacesState] = useState<Places | null>(null);
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});
  const [menu, setMenu] = useState<{ x: number; y: number; items: MenuItem[] } | null>(null);
  const favourites = settings.favourites;

  useEffect(() => {
    const load = () =>
      void invoke<Places>("places_list").then((p) => {
        setPlacesState(p);
        setPlaces(p.home);
      }, () => {});
    load();
    window.addEventListener("focus", load);
    return () => window.removeEventListener("focus", load);
  }, []);

  // Reordering favourites: press on one and move; the line shows where it goes.
  const favList = useRef<HTMLDivElement>(null);
  const [drag, setDrag] = useState<{ from: number; to: number } | null>(null);
  const press = useRef<{ index: number; y: number; moved: boolean } | null>(null);
  const dropIndex = (y: number) => {
    const rows = [...(favList.current?.querySelectorAll<HTMLElement>("[data-fav-index]") ?? [])];
    const i = rows.findIndex((r) => {
      const b = r.getBoundingClientRect();
      return y < b.top + b.height / 2;
    });
    return i < 0 ? rows.length : i;
  };
  useEffect(() => {
    const onMove = (e: MouseEvent) => {
      const p = press.current;
      if (!p) return;
      if ((e.buttons & 1) === 0) {
        press.current = null;
        setDrag(null);
        return;
      }
      if (!p.moved && Math.abs(e.clientY - p.y) < 5) return;
      p.moved = true;
      setDrag({ from: p.index, to: dropIndex(e.clientY) });
    };
    const onUp = (e: MouseEvent) => {
      const p = press.current;
      press.current = null;
      if (!p?.moved) return;
      setDrag(null);
      const to = dropIndex(e.clientY);
      const path = getSettings().favourites[p.index];
      if (path && to !== p.index && to !== p.index + 1) placeFavourites([path], to);
      // The click that ends a drag doesn't open the folder.
      const swallow = (c: MouseEvent) => {
        c.stopPropagation();
        c.preventDefault();
      };
      window.addEventListener("click", swallow, { capture: true, once: true });
      setTimeout(() => window.removeEventListener("click", swallow, { capture: true }), 0);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && press.current?.moved) {
        press.current = null;
        setDrag(null);
      }
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
      window.removeEventListener("keydown", onKey);
    };
  }, []);

  const section = (id: string, title: string, children: ReactNode, action?: ReactNode, drop?: number) => (
    <section className="side-section" key={id} data-drop-favourites={drop}>
      <div className="side-head">
        <button className="side-head-btn" onClick={() => setCollapsed((c) => ({ ...c, [id]: !c[id] }))} aria-expanded={!collapsed[id]}>
          <ChevronIcon open={!collapsed[id]} size={10} />
          <span className="pane-title">{title}</span>
        </button>
        {action}
      </div>
      {!collapsed[id] && children}
    </section>
  );

  interface ItemOptions {
    extra?: ReactNode;
    remove?: () => void;
    favIndex?: number;
    menu?: () => MenuItem[];
  }
  const item = (key: string, path: string, name: string, icon: ReactNode, o: ItemOptions = {}) => (
    <div
      key={key}
      role="button"
      tabIndex={0}
      className={`side-item${samePath(path, current) ? " current" : ""}${drag && o.favIndex === drag.from ? " dragging" : ""}`}
      data-drop-path={path}
      data-fav-index={o.favIndex}
      title={path}
      onClick={() => navigate(path)}
      onKeyDown={(e) => {
        if (e.key === "Enter") navigate(path);
        else if (o.menu && (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10"))) {
          e.preventDefault();
          const r = e.currentTarget.getBoundingClientRect();
          setMenu({ x: r.left + 24, y: r.bottom, items: o.menu() });
        }
      }}
      onMouseDown={(e) => {
        if (e.button === 1) {
          e.preventDefault();
          newTab(path, false);
        } else if (e.button === 0 && o.favIndex !== undefined) {
          press.current = { index: o.favIndex, y: e.clientY, moved: false };
        }
      }}
      onContextMenu={(e) => {
        if (!o.menu) return;
        e.preventDefault();
        setMenu({ x: e.clientX, y: e.clientY, items: o.menu() });
      }}
    >
      <span className="side-icon">{icon}</span>
      <span className="side-text">
        <span className="side-name">{name}</span>
        {o.extra}
      </span>
      {o.remove && (
        <button
          className="side-remove"
          title="Remove from Favourites"
          onMouseDown={(e) => e.stopPropagation()}
          onClick={(e) => {
            e.stopPropagation();
            o.remove!();
          }}
        >
          <CloseSmallIcon size={12} />
        </button>
      )}
    </div>
  );

  const folderIcon = (path: string, name: string) => <EntryIcon name={name} ext="" folder path={path} size={15} />;

  return (
    <div className="sidebar-body">
      {section(
        "favourites",
        "Favourites",
        <div className="side-items" ref={favList}>
          {favourites.length ? (
            favourites.map((p, i) => {
              const name = tabTitle(p);
              return (
                <Fragment key={`fav:${p}`}>
                  {drag && drag.to === i && drag.to !== drag.from && drag.to !== drag.from + 1 && <div className="side-insert" />}
                  {item(`fav:${p}`, p, name, folderIcon(p, name), {
                    remove: () => removeFavourite(p),
                    favIndex: i,
                    menu: () => placeMenu(p, name, { index: i, count: favourites.length }),
                  })}
                </Fragment>
              );
            })
          ) : (
            <p className="side-hint">Drop folders here, or {fmtShortcut("Ctrl+D")} to pin the folder you're in.</p>
          )}
          {drag && drag.to === favourites.length && drag.from !== favourites.length - 1 && <div className="side-insert" />}
        </div>,
        <button className="icon-btn small" title={`Add this folder (${fmtShortcut("Ctrl+D")})`} onClick={() => addFavourite()}>
          <StarIcon size={13} />
        </button>,
        favourites.length,
      )}
      {places &&
        section(
          "home",
          "Home",
          <div className="side-items">{places.home.map((p) => item(`home:${p.path}`, p.path, p.name, folderIcon(p.path, p.name), { menu: () => placeMenu(p.path, p.name, null) }))}</div>,
        )}
      {places &&
        section(
          "drives",
          "Drives",
          <div className="side-items">
            {places.drives.map((d) => {
              const used = d.free !== null && d.total ? 1 - d.free / d.total : null;
              const name = d.path.length <= 3 ? `${d.label} (${d.path.replace(/\\$/, "")})` : d.label;
              const extra =
                used !== null && d.total ? (
                  <>
                    <span className="side-detail" title={`${formatSize(d.free ?? 0)} free of ${formatSize(d.total)}`}>
                      {formatSize(d.free ?? 0)} free of {formatSize(d.total)}
                    </span>
                    <span className={`side-usage${used > 0.9 ? " full" : ""}`}>
                      <span style={{ width: `${Math.round(used * 100)}%` }} />
                    </span>
                  </>
                ) : d.kind === "network" ? (
                  <span className="side-detail">Not reachable right now</span>
                ) : null;
              return item(`drive:${d.path}`, d.path, name, d.kind === "network" ? <NetworkDriveIcon /> : <DriveIcon />, { extra });
            })}
          </div>,
        )}
      {menu && <Menu x={menu.x} y={menu.y} items={menu.items} onClose={() => setMenu(null)} />}
    </div>
  );
}

/** The sidebar: places (favourites, home, drives) or the git panel. */
export function Sidebar() {
  const { sidebarView } = useSettings();
  return (
    <div className="sidebar-inner">
      <div className="segmented sidebar-switch" role="tablist" aria-label="Sidebar">
        <button className={`seg${sidebarView === "places" ? " active" : ""}`} role="tab" aria-selected={sidebarView === "places"} onClick={() => updateSettings({ sidebarView: "places" })}>
          Places
        </button>
        <button
          className={`seg${sidebarView === "git" ? " active" : ""}`}
          role="tab"
          aria-selected={sidebarView === "git"}
          title={`Git (${fmtShortcut("Ctrl+Shift+G")})`}
          onClick={() => updateSettings({ sidebarView: "git" })}
        >
          Git
        </button>
      </div>
      {sidebarView === "git" ? <GitPanel /> : <Places />}
    </div>
  );
}
