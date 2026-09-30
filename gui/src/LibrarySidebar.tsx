import type { Source } from "./bindings/Source";
import type { SidebarRow } from "./libraryModel";
import { Swatch } from "./SourceBadge";

const SECTIONS = [
  "Liked Songs",
  "Albums",
  "Artists",
  "Playlists",
  "Recently played",
  "Local files",
  "Radio stations",
];

/** The library sections, Library health, and the playlists of the active source. Liked Songs and Library health are live. */
export function LibrarySidebar({
  source,
  playlists,
  unmatched,
  onOpenLiked,
  onOpenHealth,
}: {
  source: Source | null;
  playlists: SidebarRow[];
  unmatched: number;
  onOpenLiked: () => void;
  onOpenHealth: () => void;
}) {
  return (
    <aside className="sections" aria-label="Library">
      <nav>
        {SECTIONS.map((section, index) => (
          <a
            key={section}
            aria-current={index === 0 ? "page" : undefined}
            onClick={index === 0 ? onOpenLiked : undefined}
          >
            {section}
          </a>
        ))}
        <button type="button" className="health-row" onClick={onOpenHealth}>
          Library health
          {unmatched > 0 && (
            <span className="count warn">{unmatched} unmatched</span>
          )}
        </button>
      </nav>
      <div className="playlists">
        <span className="eyebrow">
          PLAYLISTS{source && ` · ${source.toUpperCase()}`}
        </span>
        {playlists.map((row) => (
          <a key={row.uri} title={row.name}>
            {source && <Swatch source={source} />}
            <span className="name">{row.name}</span>
            {row.count !== null && <span className="count">{row.count}</span>}
          </a>
        ))}
      </div>
    </aside>
  );
}
