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

/** The library sections and the playlists of the active source. Only Liked Songs is live. */
export function LibrarySidebar({
  source,
  playlists,
  onOpenLiked,
}: {
  source: Source | null;
  playlists: SidebarRow[];
  onOpenLiked: () => void;
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
