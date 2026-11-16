import { fmtShortcut } from "../lib/platform";
import { setFolderView, useFolderView } from "../lib/folderViews";
import { goBack, goForward, goUp, navigate, parentOf, useTabs } from "../lib/tabs";
import { AddressBar } from "../views/AddressBar";
import { ArrowLeftIcon, ArrowRightIcon, ArrowUpIcon, ColumnsIcon, DetailsIcon, ListIcon } from "./Icons";

/** Under the title bar: Back, Forward, Up, the address bar and the view switch. */
export function Toolbar() {
  const { tabs, activeId } = useTabs();
  const tab = tabs.find((t) => t.id === activeId) ?? tabs[0];
  const view = useFolderView(tab.path);

  return (
    <div className="toolbar">
      <div className="tb-group">
        <button className="icon-btn" title={`Back (${fmtShortcut("Alt+Left")})`} disabled={!tab.back.length} onClick={goBack}>
          <ArrowLeftIcon />
        </button>
        <button className="icon-btn" title={`Forward (${fmtShortcut("Alt+Right")})`} disabled={!tab.forward.length} onClick={goForward}>
          <ArrowRightIcon />
        </button>
        <button className="icon-btn" title={`Up (${fmtShortcut("Alt+Up")})`} disabled={!parentOf(tab.path)} onClick={() => goUp()}>
          <ArrowUpIcon />
        </button>
      </div>
      <AddressBar path={tab.path} onNavigate={(p, select) => navigate(p, select ?? null)} />
      <div className="segmented view-switch" role="radiogroup" aria-label="View">
        <button
          className={`seg${view.viewMode === "columns" ? " active" : ""}`}
          role="radio"
          aria-checked={view.viewMode === "columns"}
          title={`Columns (${fmtShortcut("Ctrl+Shift+1")})`}
          onClick={() => setFolderView(tab.path, { viewMode: "columns" })}
        >
          <ColumnsIcon size={14} />
        </button>
        <button
          className={`seg${view.viewMode === "list" ? " active" : ""}`}
          role="radio"
          aria-checked={view.viewMode === "list"}
          title={`List (${fmtShortcut("Ctrl+Shift+2")})`}
          onClick={() => setFolderView(tab.path, { viewMode: "list" })}
        >
          <ListIcon size={14} />
        </button>
        <button
          className={`seg${view.viewMode === "details" ? " active" : ""}`}
          role="radio"
          aria-checked={view.viewMode === "details"}
          title={`Details (${fmtShortcut("Ctrl+Shift+3")})`}
          onClick={() => setFolderView(tab.path, { viewMode: "details" })}
        >
          <DetailsIcon size={14} />
        </button>
      </div>
    </div>
  );
}
