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

/** A Spotify track the heart and the Spotify-only entries can act on. */
export function spotifyTrack(track: TrackInfo): boolean {
  return sourceOf(track.uri) === "Spotify" && track.id !== null;
}

/** The menu of a track row, in groups the menu separates with a rule. */
export function trackMenu(
  track: TrackInfo,
  liked: boolean,
  playlists: PlaylistInfo[],
): MenuEntry[][] {
  const uri = track.uri;
  const spotify = spotifyTrack(track);
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

  const share: MenuEntry[] = spotify
    ? [
        {
          label: "Share",
          kind: "submenu",
          entries: [
            {
              label: "Copy song link",
              kind: "copy",
              text: `https://open.spotify.com/track/${track.id}`,
              done: "Copied the song link.",
            },
          ],
        },
      ]
    : [];

  return [edit, go, share].filter((group) => group.length > 0);
}
