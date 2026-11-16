import { fmtShortcut } from "../lib/platform";
import { updateSettings, useSettings } from "../lib/settings";
import { goBack, goForward, goUp, navigate, parentOf, useTabs } from "../lib/tabs";
import { AddressBar } from "../views/AddressBar";
import { ArrowLeftIcon, ArrowRightIcon, ArrowUpIcon, DetailsIcon, ListIcon } from "./Icons";

/** Under the title bar: Back, Forward, Up, the address bar and the view switch. */
export function Toolbar() {
  const { tabs, activeId } = useTabs();
  const tab = tabs.find((t) => t.id === activeId) ?? tabs[0];
  const settings = useSettings();

  return (
    <div className="toolbar">
      <div className="tb-group">
        <button className="icon-btn" title={`Back (${fmtShortcut("Alt+Left")})`} disabled={!tab.back.length} onClick={goBack}>
          <ArrowLeftIcon />
        </button>
        <button className="icon-btn" title={`Forward (${fmtShortcut("Alt+Right")})`} disabled={!tab.forward.length} onClick={goForward}>
          <ArrowRightIcon />
        </button>
        <button className="icon-btn" title={`Up (${fmtShortcut("Alt+Up")})`} disabled={!parentOf(tab.path)} onClick={goUp}>
          <ArrowUpIcon />
        </button>
      </div>
      <AddressBar path={tab.path} onNavigate={(p, select) => navigate(p, select ?? null)} />
      <div className="segmented view-switch" role="radiogroup" aria-label="View">
        <button
          className={`seg${settings.viewMode === "list" ? " active" : ""}`}
          role="radio"
          aria-checked={settings.viewMode === "list"}
          title={`List (${fmtShortcut("Ctrl+Shift+2")})`}
          onClick={() => updateSettings({ viewMode: "list" })}
        >
          <ListIcon size={14} />
        </button>
        <button
          className={`seg${settings.viewMode === "details" ? " active" : ""}`}
          role="radio"
          aria-checked={settings.viewMode === "details"}
          title={`Details (${fmtShortcut("Ctrl+Shift+3")})`}
          onClick={() => updateSettings({ viewMode: "details" })}
        >
          <DetailsIcon size={14} />
        </button>
      </div>
    </div>
  );
}
