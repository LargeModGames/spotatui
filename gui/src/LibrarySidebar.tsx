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

/** The library sections, Library health, and the playlists of the active source. Liked Songs, Library health and the playlists are live. */
export function LibrarySidebar({
  source,
  playlists,
  openUri,
  unmatched,
  onOpenLiked,
  onOpenRow,
  onOpenHealth,
}: {
  source: Source | null;
  playlists: SidebarRow[];
  openUri: string | null;
  unmatched: number;
  onOpenLiked: () => void;
  onOpenRow: (row: SidebarRow) => void;
  onOpenHealth: () => void;
}) {
  return (
    <aside className="sections" aria-label="Library">
      <nav>
        {SECTIONS.map((section, index) =>
          index === 0 ? (
            <button
              key={section}
              type="button"
              aria-current={openUri === null ? "page" : undefined}
              onClick={onOpenLiked}
            >
              {section}
            </button>
          ) : (
            <a key={section}>{section}</a>
          ),
        )}
        <button type="button" onClick={onOpenHealth}>
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
          <button
            key={row.uri}
            type="button"
            title={row.name}
            aria-current={row.uri === openUri ? "page" : undefined}
            onClick={() => onOpenRow(row)}
          >
            {source && <Swatch source={source} />}
            <span className="name">{row.name}</span>
            {row.count !== null && <span className="count">{row.count}</span>}
          </button>
        ))}
      </div>
    </aside>
  );
}
