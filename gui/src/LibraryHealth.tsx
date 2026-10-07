import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import type { Action } from "./bindings/Action";
import type { PlaylistSyncPayload } from "./bindings/PlaylistSyncPayload";
import {
  cellFor,
  lastSync,
  reasonText,
  selectedIndex,
  SYNC_SOURCES,
} from "./healthModel";
import { KeyHints } from "./KeyHints";
import "./LibraryHealth.css";
import { Swatch } from "./SourceBadge";

const CELL_TEXT = {
  master: "MASTER",
  synced: "synced",
  never: "never",
  none: "·",
};

/** Library health: the playlists mirrored across sources. The copy tabs need a source resolver that does not exist yet. */
export function LibraryHealth({
  sync,
  send,
  onBack,
}: {
  sync: PlaylistSyncPayload | null;
  send: (action: Action) => void;
  onBack: () => void;
}) {
  const links = sync?.links ?? [];
  const running = sync?.running ?? false;
  const [id, setId] = useState<string | null>(null);
  // The confirm names its link, so a run that reorders the links cannot redirect it.
  const [confirmId, setConfirmId] = useState<string | null>(null);
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const list = root.current?.querySelector<HTMLElement>("[data-focus]");
    (list ?? root.current)?.focus();
  }, []);
  const index = selectedIndex(links, id);
  const link = links[index] ?? null;
  const confirming = link !== null && confirmId === link.id;
  const remove = () => {
    if (link) send({ RemovePlaylistSyncLink: link.id });
    setConfirmId(null);
  };

  const pick = (next: number) => {
    const target = links[Math.min(Math.max(next, 0), links.length - 1)];
    if (target) setId(target.id);
    setConfirmId(null);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    const key = event.key;
    // A focused button acts on its own Enter and space.
    if (
      event.target instanceof HTMLButtonElement &&
      (key === "Enter" || key === " ")
    )
      return;
    if (confirming && (key === "Enter" || key === "y")) remove();
    else if (key === "Escape" && confirming) setConfirmId(null);
    else if (key === "Escape" || key === "Backspace") onBack();
    else if (key === "j" || key === "ArrowDown") pick(index + 1);
    else if (key === "k" || key === "ArrowUp") pick(index - 1);
    else if (key === "s" && !running) send("RunPlaylistSync");
    else if (key === "D" && link) setConfirmId(link.id);
    else return;
    event.preventDefault();
  };

  return (
    <div ref={root} className="health" tabIndex={-1} onKeyDown={onKeyDown}>
      <section className="health-main">
        <button type="button" className="eyebrow crumb" onClick={onBack}>
          LIBRARY / LIBRARY HEALTH
        </button>
        <div className="health-title">
          <h1>Library health</h1>
          <span>Playlists mirrored across your sources</span>
        </div>
        <div role="tablist" aria-label="Sections" className="health-tabs">
          <span role="tab" aria-selected="true">
            Playlist sync
          </span>
        </div>
        {links.length === 0 ? (
          <div className="empty">
            {running ? (
              <p>Loading the linked playlists…</p>
            ) : (
              <>
                <p>No linked playlists.</p>
                <p>
                  A build with Qobuz, Subsonic or YouTube can mirror a playlist:
                  quit this window, highlight the playlist in the terminal and
                  press m.
                </p>
                {sync?.last_failed && sync.last_summary && (
                  <p className="failed">{sync.last_summary}</p>
                )}
              </>
            )}
          </div>
        ) : (
          <>
            <div className="link-row head eyebrow" aria-hidden="true">
              <span>#</span>
              <span>PLAYLIST</span>
              {SYNC_SOURCES.map((source) => (
                <span key={source}>{source.toUpperCase()}</span>
              ))}
              <span className="when">LAST SYNC</span>
            </div>
            <div
              role="listbox"
              aria-label="Linked playlists"
              tabIndex={0}
              data-focus
              aria-activedescendant={link ? `link-${link.id}` : undefined}
            >
              {links.map((entry, row) => (
                <div
                  key={entry.id}
                  id={`link-${entry.id}`}
                  role="option"
                  aria-selected={row === index}
                  className="link-row"
                  onClick={() => pick(row)}
                >
                  <span className="n">{row + 1}</span>
                  <span className="name">
                    <Swatch source={entry.source} /> <b>{entry.name}</b>
                  </span>
                  {SYNC_SOURCES.map((source) => {
                    const cell = cellFor(entry, source);
                    return (
                      <span key={source} className={`cell ${cell.role}`}>
                        {cell.role === "unmatched"
                          ? `${cell.count} unmatched`
                          : CELL_TEXT[cell.role]}
                      </span>
                    );
                  })}
                  <span className="when">
                    {entry.mirrors.length === 0
                      ? "no mirrors"
                      : lastSync(entry)}
                  </span>
                </div>
              ))}
            </div>
          </>
        )}
        <KeyHints hints={["j k move", "s sync all", "D remove", "esc back"]} />
      </section>
      <aside aria-label="Selected playlist">
        {link ? (
          <>
            <span className="eyebrow">LINKED PLAYLIST</span>
            <h2>{link.name}</h2>
            <span className="sub">
              {link.source} playlist · {link.mirrors.length}{" "}
              {link.mirrors.length === 1 ? "mirror" : "mirrors"}
            </span>
            {link.mirrors.map((mirror) => (
              <div
                key={mirror.source}
                className={
                  mirror.unmatched.length > 0 ? "mirror-box warn" : "mirror-box"
                }
              >
                <div>
                  <span className="label">
                    <Swatch source={mirror.source} />
                    {mirror.source.toUpperCase()}
                  </span>
                  <span className="tag">
                    {mirror.last_run === null
                      ? "never ran"
                      : mirror.unmatched.length > 0
                        ? `${mirror.unmatched.length} unmatched`
                        : "synced"}
                  </span>
                </div>
                <span className="detail">{mirror.matched} matched</span>
                {mirror.unmatched.slice(0, 6).map((entry) => (
                  <span key={entry.master_key} className="miss">
                    {entry.title} – {entry.artist}: {reasonText(entry.reason)}
                  </span>
                ))}
              </div>
            ))}
            {link.last_line && (
              <p className={link.last_failed ? "line failed" : "line"}>
                {link.last_line}
              </p>
            )}
            <div className="health-actions">
              <button
                type="button"
                className="primary"
                disabled={running}
                onClick={() => send("RunPlaylistSync")}
              >
                {running ? "Syncing…" : "Sync all now"}
              </button>
              {confirming ? (
                <>
                  <button type="button" className="danger" onClick={remove}>
                    Remove the link
                  </button>
                  <button
                    type="button"
                    autoFocus
                    onClick={() => setConfirmId(null)}
                  >
                    Keep it
                  </button>
                </>
              ) : (
                <button type="button" onClick={() => setConfirmId(link.id)}>
                  Remove link
                </button>
              )}
            </div>
            {confirming && (
              <p className="sub">The mirror playlists stay where they are.</p>
            )}
          </>
        ) : (
          <span className="sub">
            {sync?.last_summary ?? "Nothing has synced yet."}
          </span>
        )}
      </aside>
    </div>
  );
}
