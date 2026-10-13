import { useEffect, useRef } from "react";
import type { Action } from "./bindings/Action";
import type { DiscoverPayload } from "./bindings/DiscoverPayload";
import type { PlaylistInfo } from "./bindings/PlaylistInfo";
import type { SourcePlaylists } from "./bindings/SourcePlaylists";
import type { TrackInfo } from "./bindings/TrackInfo";
import { Glyph } from "./Icon";
import type { OpenView, View } from "./nav";
import "./Pages.css";
import {
  asksDiscover,
  discoverView,
  greeting,
  madeForYou,
  QUICK_PLAYLISTS,
  recentAlbums,
  SHELF_MAX,
} from "./pagesModel";
import { Card, Cover, PlayButton, Shelf } from "./ui";

const playlistView = (playlist: PlaylistInfo): View => ({
  page: "playlist",
  uri: playlist.uri,
  name: playlist.name,
  source: "Spotify",
});

/** Home: shelves of recently played, the user's playlists, saved albums and Made For You. */
export function Home({
  recent,
  playlists,
  discover,
  connected,
  active,
  open,
  send,
}: {
  /** Recently played, most recent first; null before the first fetch landed. */
  recent: TrackInfo[] | null;
  playlists: SourcePlaylists | null;
  discover: DiscoverPayload | null;
  connected: boolean;
  active: boolean;
  playingUri: string | null;
  open: OpenView;
  send: (action: Action) => void;
}) {
  const asked = useRef(false);
  useEffect(() => {
    if (!active || asked.current || !asksDiscover(discover, connected)) return;
    asked.current = true;
    send({ OpenLibrary: "Discover" });
  }, [active, connected, discover, send]);

  const spotify = playlists?.spotify ?? [];
  const albums = playlists?.saved_albums ?? [];
  const recentShelf = recentAlbums(recent ?? []);
  const openAlbum = (id: string | null) => {
    if (id) open({ page: "album", id });
  };
  const playContext = (uri: string | null) => {
    if (uri) send({ PlayContext: { uri, offset: null } });
  };
  const forYou = madeForYou(discover);

  return (
    <section className="home">
      <div className="home-top">
        <h1>{greeting(new Date().getHours())}</h1>
        <div className="quick-grid">
          <div className="quick-tile">
            <button
              type="button"
              className="quick-open"
              data-focus
              onClick={() => open({ page: "liked" })}
            >
              <span className="quick-liked" aria-hidden="true">
                <Glyph name="heart" size={24} filled />
              </span>
              <span className="quick-name">Liked Songs</span>
            </button>
          </div>
          {spotify.slice(0, QUICK_PLAYLISTS).map((playlist) => (
            <div key={playlist.uri} className="quick-tile">
              <button
                type="button"
                className="quick-open"
                onClick={() => open(playlistView(playlist))}
              >
                <Cover src={playlist.image_url} size={56} />
                <span className="quick-name">{playlist.name}</span>
              </button>
              <span className="quick-play">
                <PlayButton
                  label={`Play ${playlist.name}`}
                  onClick={() => playContext(playlist.uri)}
                />
              </span>
            </div>
          ))}
        </div>
      </div>
      {recentShelf.length > 0 && (
        <Shelf
          oneRow
          title="Recently played"
          onMore={() => open({ page: "recent" })}
        >
          {recentShelf.slice(0, SHELF_MAX).map((track) => (
            <Card
              key={track.album_id}
              image={track.image_url}
              title={track.album}
              subtitle={track.artists.join(", ")}
              onOpen={() => openAlbum(track.album_id)}
              onPlay={() =>
                track.album_id
                  ? playContext(`spotify:album:${track.album_id}`)
                  : track.uri &&
                    send({ PlayUris: { uris: [track.uri], offset: null } })
              }
            />
          ))}
        </Shelf>
      )}
      {spotify.length > 0 && (
        <Shelf oneRow title="Your playlists">
          {spotify.slice(0, SHELF_MAX).map((playlist) => (
            <Card
              key={playlist.uri}
              image={playlist.image_url}
              title={playlist.name}
              subtitle={`By ${playlist.owner}`}
              onOpen={() => open(playlistView(playlist))}
              onPlay={() => playContext(playlist.uri)}
            />
          ))}
        </Shelf>
      )}
      {albums.length > 0 && (
        <Shelf oneRow title="Saved albums">
          {albums.slice(0, SHELF_MAX).map((album, index) => (
            <Card
              key={album.id ?? `${index}-${album.name}`}
              image={album.image_url}
              title={album.name}
              subtitle={album.artists.map((artist) => artist.name).join(", ")}
              onOpen={() => openAlbum(album.id)}
              onPlay={() => playContext(album.uri)}
            />
          ))}
        </Shelf>
      )}
      {forYou.length > 0 && (
        <Shelf
          oneRow
          title="Made For You"
          onMore={() => open({ page: "discover" })}
        >
          {forYou.slice(0, SHELF_MAX).map((playlist) => (
            <Card
              key={playlist.uri}
              image={playlist.image_url}
              title={playlist.name}
              subtitle={playlist.description ?? "By Spotify"}
              onOpen={() => open(discoverView(playlist))}
              onPlay={() => playContext(playlist.uri)}
            />
          ))}
        </Shelf>
      )}
    </section>
  );
}
