import type { Action } from "./bindings/Action";
import type { PlaylistInfo } from "./bindings/PlaylistInfo";
import type { TrackInfo } from "./bindings/TrackInfo";
import { sourceOf } from "./format";
import type { View } from "./nav";

/** One entry of a track's menu: it sends an action, opens a view, copies text or opens a submenu. */
export type MenuEntry =
  | { label: string; kind: "action"; action: Action }
  | { label: string; kind: "open"; view: View }
  | { label: string; kind: "copy"; text: string; done: string }
  | { label: string; kind: "submenu"; entries: MenuEntry[] };

/** What the list around a row adds to its menu. */
export interface RowContext {
  /** Removes this row from the playlist the list shows. */
  remove?: Action | null;
}

/** A Spotify track the heart and the Spotify-only entries can act on. */
export function spotifyTrack(track: TrackInfo): boolean {
  return sourceOf(track.uri) === "Spotify" && track.id !== null;
}

/** The menu of a track row, in groups the menu separates with a rule. */
export function trackMenu(
  track: TrackInfo,
  liked: boolean,
  playlists: PlaylistInfo[],
  context: RowContext = {},
): MenuEntry[][] {
  const uri = track.uri;
  const spotify = spotifyTrack(track);
  const song = spotify && (uri?.startsWith("spotify:track:") ?? false);
  const edit: MenuEntry[] = [];
  if (spotify && uri && playlists.length > 0)
    edit.push({
      label: "Add to playlist",
      kind: "submenu",
      entries: playlists.map((playlist) => ({
        label: playlist.name,
        kind: "action",
        action: { AddTrackToPlaylist: { playlist: playlist.uri, track: uri } },
      })),
    });
  if (context.remove)
    edit.push({
      label: "Remove from this playlist",
      kind: "action",
      action: context.remove,
    });
  if (spotify && uri)
    edit.push({
      label: liked
        ? "Remove from your Liked Songs"
        : "Save to your Liked Songs",
      kind: "action",
      action: { ToggleSaveTrack: uri },
    });
  const source = sourceOf(uri);
  if (uri && source !== "Radio" && source !== "AppleMusic")
    edit.push({
      label: "Add to queue",
      kind: "action",
      action: { QueueTrack: track },
    });

  const go: MenuEntry[] = [];
  if (song && track.id)
    go.push({
      label: "Go to song radio",
      kind: "open",
      view: { page: "radio", id: track.id, name: track.name },
    });
  if (spotify) {
    const artists: MenuEntry[] = track.artist_refs.flatMap((artist) =>
      artist.id
        ? [
            {
              label: artist.name,
              kind: "open" as const,
              view: { page: "artist", id: artist.id, name: artist.name },
            },
          ]
        : [],
    );
    if (artists.length === 1) go.push({ ...artists[0], label: "Go to artist" });
    else if (artists.length > 1)
      go.push({ label: "Go to artist", kind: "submenu", entries: artists });
    if (track.album_id)
      go.push({
        label: "Go to album",
        kind: "open",
        view: { page: "album", id: track.album_id },
      });
  }

  const links: MenuEntry[] = [];
  if (song) {
    links.push({
      label: "Copy song link",
      kind: "copy",
      text: `https://open.spotify.com/track/${track.id}`,
      done: "Copied the song link.",
    });
    if (track.album_id)
      links.push({
        label: "Copy album link",
        kind: "copy",
        text: `https://open.spotify.com/album/${track.album_id}`,
        done: "Copied the album link.",
      });
  }
  const share: MenuEntry[] =
    links.length > 0
      ? [{ label: "Share", kind: "submenu", entries: links }]
      : [];

  return [edit, go, share].filter((group) => group.length > 0);
}
