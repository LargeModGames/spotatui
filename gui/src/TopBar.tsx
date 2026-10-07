import { AREAS, type Area } from "./areas";
import { Icon } from "./Icon";

export function TopBar({
  area,
  ready,
  onArea,
  query,
  onSearch,
  device,
  connected,
  queued,
  queueOpen,
  onQueue,
}: {
  area: Area;
  ready: (area: Area) => boolean;
  onArea: (area: Area) => void;
  query: string;
  onSearch: () => void;
  device: string | null;
  connected: boolean;
  queued: number;
  queueOpen: boolean;
  onQueue: () => void;
}) {
  return (
    <header className="topbar">
      <span className="wordmark">
        spotatui<span>▌</span>
      </span>
      <nav aria-label="Main">
        {AREAS.map((entry) => (
          <button
            key={entry.area}
            type="button"
            aria-current={entry.area === area ? "page" : undefined}
            aria-keyshortcuts={entry.key}
            disabled={!ready(entry.area)}
            onClick={() => onArea(entry.area)}
          >
            {entry.label}
            <kbd>{entry.key}</kbd>
          </button>
        ))}
      </nav>
      <div className="search">
        <button type="button" onClick={onSearch}>
          <Icon>
            <circle cx="11" cy="11" r="7" />
            <path d="m20 20-3.5-3.5" />
          </Icon>
          <span className={query ? "typed" : undefined}>
            {query || "Search your music, or type a command"}
          </span>
          <kbd>/ search · : command</kbd>
        </button>
      </div>
      <div className="status">
        <span className="device">
          <span className={connected && device ? "dot on" : "dot"} />
          <span className="device-name" title={device ?? undefined}>
            {device ?? "No device"}
          </span>
        </span>
        <button
          type="button"
          className="queue-button"
          aria-expanded={queueOpen}
          aria-keyshortcuts="Shift+Q"
          onClick={onQueue}
        >
          Queue {queued} <kbd>Q</kbd>
        </button>
      </div>
    </header>
  );
}
