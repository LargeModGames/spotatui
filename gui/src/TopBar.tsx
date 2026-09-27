import { Icon } from "./Icon";

const AREAS = ["Library", "Search", "Discover", "Session", "Stats", "Party"];

export function TopBar({
  device,
  connected,
  queued,
}: {
  device: string | null;
  connected: boolean;
  queued: number;
}) {
  return (
    <header className="topbar">
      <span className="wordmark">
        spotatui<span>▌</span>
      </span>
      <nav aria-label="Main">
        {AREAS.map((area, index) => (
          <a key={area} aria-current={index === 0 ? "page" : undefined}>
            {area}
            <kbd>{index + 1}</kbd>
          </a>
        ))}
      </nav>
      <div className="search">
        <button type="button">
          <Icon>
            <circle cx="11" cy="11" r="7" />
            <path d="m20 20-3.5-3.5" />
          </Icon>
          <span>Search your music, or type a command</span>
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
        <button type="button" className="queue-button">
          Queue {queued} <kbd>q</kbd>
        </button>
      </div>
    </header>
  );
}
