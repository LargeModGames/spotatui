import { EN, type GuiStrings, fmt } from "./i18n";
import { AREAS, areas, SETTINGS_KEY } from "./areas";
import type { ReactNode } from "react";
import type { UpdateAsk } from "./bindings/UpdateAsk";
import type { UpdateStatus } from "./bindings/UpdateStatus";
import type { WindowEdge } from "./bindings/WindowEdge";
import { Glyph, type GlyphName } from "./Icon";
import type { Page } from "./nav";
import { readyVersion } from "./settingsModel";

const PAGE_GLYPHS: Partial<Record<Page, GlyphName>> = {
  discover: "discover",
  session: "session",
  stats: "stats",
  party: "party",
  visualizer: "visualizer",
};

const HOME_KEY = AREAS.find((entry) => entry.area === "home")?.key;

export function TopBar({
  s = EN,
  page,
  canBack,
  canForward,
  onBack,
  onForward,
  onPage,
  query,
  onQuery,
  onRun,
  device,
  connected,
  update,
  onUpdate,
  edge,
  grip,
  mainHidden,
  onMain,
}: {
  s?: GuiStrings;
  page: Page;
  canBack: boolean;
  canForward: boolean;
  onBack: () => void;
  onForward: () => void;
  onPage: (page: Page) => void;
  query: string;
  onQuery: (query: string) => void;
  onRun: (query: string) => void;
  device: string | null;
  connected: boolean;
  update: UpdateStatus | null;
  onUpdate: (ask: UpdateAsk) => void;
  /** The window edge the bar sits on; a rail on the left or right. */
  edge?: WindowEdge;
  /** The handle the bar moves by. */
  grip?: ReactNode;
  /** Whether Main is hidden; undefined when it cannot hide, which drops the button. */
  mainHidden?: boolean;
  onMain?: () => void;
}) {
  const ready = readyVersion(update);
  return (
    <header className="topbar" data-edge={edge}>
      <div className="topbar-start">
        <span className="wordmark">
          spota<span>tui</span>
        </span>
        <button
          type="button"
          className="history-button"
          aria-label={s.go_back}
          disabled={!canBack}
          onClick={onBack}
        >
          <Glyph name="back" />
        </button>
        <button
          type="button"
          className="history-button"
          aria-label={s.go_forward}
          disabled={!canForward}
          onClick={onForward}
        >
          <Glyph name="forward" />
        </button>
      </div>
      <div className="topbar-center">
        <button
          type="button"
          className="home-button"
          aria-label={s.home}
          title={fmt(s.label_shortcut, { label: s.home, key: HOME_KEY ?? "" })}
          aria-keyshortcuts={HOME_KEY}
          aria-current={page === "home" ? "page" : undefined}
          onClick={() => onPage("home")}
        >
          <Glyph name="home" size={24} filled={page === "home"} />
        </button>
        <label
          className={page === "search" ? "top-search active" : "top-search"}
        >
          <Glyph name="search" size={20} />
          <input
            id="top-search"
            type="search"
            aria-label={s.search}
            aria-keyshortcuts="/"
            placeholder={s.search_placeholder}
            autoComplete="off"
            value={query}
            onChange={(event) => onQuery(event.target.value)}
            onKeyDown={(event) => {
              // Enter also confirms an IME composition; only a finished query runs.
              if (event.nativeEvent.isComposing) return;
              if (event.key === "Enter") {
                event.preventDefault();
                onRun(query);
              } else if (event.key === "Escape") {
                event.preventDefault();
                event.currentTarget.blur();
              } else if (event.key === "ArrowDown" && page === "search") {
                // Move into the results like the old Search input did.
                const results = document.querySelector<HTMLElement>(
                  '[data-area="search"] [role="listbox"]',
                );
                if (results?.querySelector('[role="option"]')) {
                  event.preventDefault();
                  results.focus({ preventScroll: true });
                }
              }
            }}
          />
        </label>
      </div>
      <div className="topbar-end">
        <nav aria-label={s.spotatui_pages}>
          {areas(s)
            .filter((entry) => PAGE_GLYPHS[entry.area])
            .map((entry) => (
              <button
                key={entry.area}
                type="button"
                className="page-button"
                aria-label={entry.label}
                title={fmt(s.label_shortcut, {
                  label: entry.label,
                  key: entry.key,
                })}
                aria-keyshortcuts={entry.key}
                aria-current={entry.area === page ? "page" : undefined}
                onClick={() => onPage(entry.area)}
              >
                <Glyph name={PAGE_GLYPHS[entry.area] ?? "music"} size={20} />
              </button>
            ))}
        </nav>
        {ready && (
          <button
            type="button"
            className="update-pill"
            title={fmt(s.install_version, { version: ready })}
            onClick={() => onUpdate("install")}
          >
            {s.update_ready}
          </button>
        )}
        {mainHidden !== undefined && (
          <button
            type="button"
            className="page-button"
            aria-label={mainHidden ? s.show_main : s.hide_main}
            title={mainHidden ? s.show_main : s.hide_main}
            onClick={onMain}
          >
            <Glyph name="library" size={20} />
          </button>
        )}
        <button
          type="button"
          className="page-button"
          aria-label={s.settings}
          title={fmt(s.label_shortcut, {
            label: s.settings,
            key: SETTINGS_KEY.label,
          })}
          aria-keyshortcuts={SETTINGS_KEY.aria}
          aria-current={page === "settings" ? "page" : undefined}
          onClick={() => onPage("settings")}
        >
          <Glyph name="gear" size={20} />
        </button>
        <span className="device">
          <span className={connected && device ? "dot on" : "dot"} />
          <span className="device-name" title={device ?? undefined}>
            {device ?? s.no_device}
          </span>
        </span>
      </div>
      {grip}
    </header>
  );
}
