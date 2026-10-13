import { memo, useEffect, useRef } from "react";
import type { Action } from "./bindings/Action";
import type { DiscoverPayload } from "./bindings/DiscoverPayload";
import "./Discover.css";
import type { OpenView } from "./nav";
import "./Pages.css";
import { asksDiscover, discoverView } from "./pagesModel";
import { Card, Shelf } from "./ui";

/** Discover: the shelves of Spotify's Made For You page. */
export const Discover = memo(function Discover({
  discover,
  active,
  open,
  send,
}: {
  discover: DiscoverPayload | null;
  active: boolean;
  open: OpenView;
  send: (action: Action) => void;
}) {
  // The shell's open skips the fetch before a session is known; ask once it is.
  const asked = useRef(false);
  useEffect(() => {
    if (!active || asked.current || !asksDiscover(discover)) return;
    asked.current = true;
    send({ OpenLibrary: "Discover" });
  }, [active, discover, send]);

  const available = discover?.available ?? false;
  const shelves = discover?.shelves ?? [];
  const notice =
    shelves.length > 0
      ? null
      : discover?.loading
        ? "Loading…"
        : "Nothing here yet.";
  return (
    <section className="discover" tabIndex={-1} data-focus>
      <div className="discover-head">
        <h1>Discover</h1>
        <span>Made For You, from Spotify.</span>
      </div>
      {!available ? (
        <p className="empty">Discover needs a Spotify session.</p>
      ) : (
        <>
          <p className={notice ? "empty" : "sr-only"} role="status">
            {notice}
          </p>
          {shelves.map((shelf, index) => (
            <Shelf key={`${index}-${shelf.title}`} title={shelf.title}>
              {shelf.playlists.map((playlist) => (
                <Card
                  key={playlist.uri}
                  image={playlist.image_url}
                  title={playlist.name}
                  subtitle={playlist.description ?? "By Spotify"}
                  onOpen={() => open(discoverView(playlist))}
                  onPlay={() =>
                    send({ PlayContext: { uri: playlist.uri, offset: null } })
                  }
                />
              ))}
            </Shelf>
          ))}
        </>
      )}
    </section>
  );
});
