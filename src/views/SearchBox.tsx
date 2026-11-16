import { useEffect, useRef, type KeyboardEvent } from "react";
import { fmtShortcut } from "../lib/platform";
import { closeSearch, getSearch, openSearch, setQuery, setScope, useIndexStatus, useSearch } from "../lib/search";
import { activeTab, useTabs } from "../lib/tabs";
import { uiActions } from "../lib/ui";
import { SearchIcon } from "../shell/Icons";
import { getCursor, openHit, setCursor } from "./SearchResults";

/** The search box in the toolbar: this folder and below, or everywhere the index reaches. */
export function SearchBox() {
  useTabs();
  const tab = activeTab();
  const search = useSearch(tab.id);
  const status = useIndexStatus();
  const input = useRef<HTMLInputElement>(null);

  // Ctrl+F / Ctrl+Shift+F put the keyboard here.
  useEffect(() => {
    if (search.focus) {
      input.current?.focus();
      input.current?.select();
    }
  }, [search.focus]);

  const onKey = (e: KeyboardEvent) => {
    const hits = search.result?.hits ?? [];
    const cursor = getCursor(tab.id);
    if (e.key === "Escape") {
      e.preventDefault();
      closeSearch(tab.id);
      uiActions.focusList();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      setCursor(tab.id, Math.min(hits.length - 1, Math.max(0, cursor + (e.key === "ArrowDown" ? 1 : -1))));
    } else if (e.key === "PageDown" || e.key === "PageUp") {
      e.preventDefault();
      setCursor(tab.id, Math.min(hits.length - 1, Math.max(0, cursor + (e.key === "PageDown" ? 15 : -15))));
    } else if (e.key === "Enter" && hits[cursor]) {
      e.preventDefault();
      openHit(tab.id, hits[cursor], e.ctrlKey || e.metaKey ? "reveal" : e.altKey ? "tab" : "open");
    }
  };

  const building = status && (status.phase === "building" || status.phase === "loading");
  const scope = search.scope;
  return (
    <div className={`search-box${search.open ? " open" : ""}`}>
      <SearchIcon size={14} />
      <input
        ref={input}
        value={search.query}
        placeholder={scope === "everywhere" ? `Search everywhere (${fmtShortcut("Ctrl+Shift+F")})` : `Search here (${fmtShortcut("Ctrl+F")})`}
        spellCheck={false}
        onFocus={() => !getSearch(tab.id).open && openSearch(tab.id, tab.path, scope)}
        onChange={(e) => setQuery(tab.id, e.target.value)}
        onKeyDown={onKey}
        title="Words match names fuzzily. Filters: ext:rs,ts kind:image size:>10mb modified:<7d in:src /regex/"
      />
      {search.busy && <span className="search-spin" aria-label="Searching" />}
      {search.open && (
        <span className="segmented search-scope" role="radiogroup" aria-label="Where">
          <button className={`seg${scope === "here" ? " active" : ""}`} role="radio" aria-checked={scope === "here"} title={tab.path} onMouseDown={(e) => e.preventDefault()} onClick={() => setScope(tab.id, "here")}>
            Here
          </button>
          <button
            className={`seg${scope === "everywhere" ? " active" : ""}`}
            role="radio"
            aria-checked={scope === "everywhere"}
            title={building ? `Indexing… ${status?.progress.toLocaleString()} so far` : status?.roots.join("\n")}
            onMouseDown={(e) => e.preventDefault()}
            onClick={() => setScope(tab.id, "everywhere")}
          >
            Everywhere
          </button>
        </span>
      )}
    </div>
  );
}
