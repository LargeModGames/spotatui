import { AREAS, SETTINGS_KEY } from "./areas";
import type { UpdateAsk } from "./bindings/UpdateAsk";
import type { UpdateStatus } from "./bindings/UpdateStatus";
import { Glyph, type GlyphName } from "./Icon";
import type { Page } from "./nav";
import { readyVersion } from "./settingsModel";

const PAGE_GLYPHS: Partial<Record<Page, GlyphName>> = {
  discover: "discover",
  session: "session",
  stats: "stats",
  party: "party",
};

const PAGES = AREAS.filter((entry) => PAGE_GLYPHS[entry.area]);
const HOME_KEY = AREAS.find((entry) => entry.area === "home")?.key;

export function TopBar({
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
}: {
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
}) {
  const ready = readyVersion(update);
  return (
    <header className="topbar">
      <div className="topbar-start">
        <span className="wordmark">
          spota<span>tui</span>
        </span>
        <button
          type="button"
          className="history-button"
          aria-label="Go back"
          disabled={!canBack}
          onClick={onBack}
        >
          <Glyph name="back" />
        </button>
        <button
          type="button"
          className="history-button"
          aria-label="Go forward"
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
          aria-label="Home"
          title={`Home (${HOME_KEY})`}
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
            aria-label="Search"
            aria-keyshortcuts="/"
            placeholder="What do you want to play?"
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
        <nav aria-label="spotatui pages">
          {PAGES.map((entry) => (
            <button
              key={entry.area}
              type="button"
              className="page-button"
              aria-label={entry.label}
              title={`${entry.label} (${entry.key})`}
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
            title={`Restart to install version ${ready}`}
            onClick={() => onUpdate("install")}
          >
            Update ready
          </button>
        )}
        <button
          type="button"
          className="page-button"
          aria-label="Settings"
          title={`Settings (${SETTINGS_KEY.label})`}
          aria-keyshortcuts={SETTINGS_KEY.aria}
          aria-current={page === "settings" ? "page" : undefined}
          onClick={() => onPage("settings")}
        >
          <Glyph name="gear" size={20} />
        </button>
        <span className="device">
          <span className={connected && device ? "dot on" : "dot"} />
          <span className="device-name" title={device ?? undefined}>
            {device ?? "No device"}
          </span>
        </span>
      </div>
    </header>
  );
}
