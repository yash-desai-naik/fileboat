import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState, type ReactNode } from "react";
import { formatSize } from "../lib/format";
import { fmtShortcut } from "../lib/platform";
import { addFavourite, removeFavourite } from "../lib/favourites";
import { useSettings } from "../lib/settings";
import { activeTab, navigate, newTab, samePath, tabTitle, useTabs } from "../lib/tabs";
import { ChevronIcon, CloseSmallIcon, DownloadIcon, DriveIcon, FolderIcon, HomeIcon, NetworkDriveIcon, StarIcon } from "../shell/Icons";

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
 * Favourites, home folders and drives. Click to go there, middle-click for a new tab. Drives
 * are read again whenever the window gets focus (a USB stick may have been plugged in).
 */
export function Sidebar() {
  const settings = useSettings();
  useTabs();
  const current = activeTab().path;
  const [places, setPlaces] = useState<Places | null>(null);
  const [collapsed, setCollapsed] = useState<Record<string, boolean>>({});

  useEffect(() => {
    const load = () => void invoke<Places>("places_list").then(setPlaces, () => {});
    load();
    window.addEventListener("focus", load);
    return () => window.removeEventListener("focus", load);
  }, []);

  const section = (id: string, title: string, children: ReactNode, action?: ReactNode) => (
    <section className="side-section" key={id}>
      <div className="side-head">
        <button className="side-head-btn" onClick={() => setCollapsed((c) => ({ ...c, [id]: !c[id] }))} aria-expanded={!collapsed[id]}>
          <ChevronIcon open={!collapsed[id]} size={10} />
          <span className="pane-title">{title}</span>
        </button>
        {action}
      </div>
      {!collapsed[id] && <div className="side-items">{children}</div>}
    </section>
  );

  const item = (key: string, path: string, name: string, icon: ReactNode, extra?: ReactNode, remove?: () => void) => (
    <div
      key={key}
      role="button"
      tabIndex={0}
      className={`side-item${samePath(path, current) ? " current" : ""}`}
      data-drop-path={path}
      title={path}
      onClick={() => navigate(path)}
      onKeyDown={(e) => e.key === "Enter" && navigate(path)}
      onMouseDown={(e) => {
        if (e.button === 1) {
          e.preventDefault();
          newTab(path, false);
        }
      }}
    >
      <span className="side-icon">{icon}</span>
      <span className="side-text">
        <span className="side-name">{name}</span>
        {extra}
      </span>
      {remove && (
        <button
          className="side-remove"
          title="Remove from Favourites"
          onClick={(e) => {
            e.stopPropagation();
            remove();
          }}
        >
          <CloseSmallIcon size={12} />
        </button>
      )}
    </div>
  );

  const homeIcon = (kind: string) => (kind === "home" ? <HomeIcon /> : kind === "downloads" ? <DownloadIcon /> : <FolderIcon />);

  return (
    <div className="sidebar-body">
      {section(
        "favourites",
        "Favourites",
        settings.favourites.length ? (
          settings.favourites.map((p) => item(`fav:${p}`, p, tabTitle(p), <StarIcon />, undefined, () => removeFavourite(p)))
        ) : (
          <p className="side-hint">{fmtShortcut("Ctrl+D")} pins the folder you're in.</p>
        ),
        <button className="icon-btn small" title={`Add this folder (${fmtShortcut("Ctrl+D")})`} onClick={() => addFavourite()}>
          <StarIcon size={13} />
        </button>,
      )}
      {places && section("home", "Home", places.home.map((p) => item(`home:${p.path}`, p.path, p.name, homeIcon(p.kind))))}
      {places &&
        section(
          "drives",
          "Drives",
          places.drives.map((d) => {
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
            return item(`drive:${d.path}`, d.path, name, d.kind === "network" ? <NetworkDriveIcon /> : <DriveIcon />, extra);
          }),
        )}
    </div>
  );
}
