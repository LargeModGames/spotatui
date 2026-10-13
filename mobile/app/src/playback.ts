import type { NowPlaying } from '@gui/bindings/NowPlaying';
import type { PlaylistInfo } from '@gui/bindings/PlaylistInfo';
import { usePosition } from '@gui/usePosition';

import { useCore } from '@/core-context';

/** The playback channel plus the position, interpolated per frame while playing. */
export function usePlayback() {
  const state = useCore();
  const playback = state.channels.playback?.payload ?? null;
  const item = playback?.item ?? null;
  const ms = usePosition(state.position, item?.is_playing ?? false);
  // usePosition reads a stale tick as 0; the screens say so instead.
  const stale = !state.position || state.position.ms === null;
  return { playback, item, ms, stale };
}

/** "3:07" for a millisecond count. */
export function clock(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
}

/** Whether a URI is Spotify's, so the like button applies to it. */
export function isSpotify(uri: string | null | undefined): boolean {
  return !!uri?.startsWith('spotify:');
}

/** "PLAYING FROM PLAYLIST" and the context's name, as far as the page can tell from its URI. */
export function playingFrom(item: NowPlaying | null, playlists: PlaylistInfo[]): { kind: string; name: string | null } {
  const context = item?.context_uri;
  if (!item || !context) return { kind: 'NOW PLAYING', name: null };
  if (context.includes(':collection')) return { kind: 'PLAYING FROM YOUR LIBRARY', name: 'Liked Songs' };
  switch (context.split(':')[1]) {
    case 'playlist':
      return { kind: 'PLAYING FROM PLAYLIST', name: playlists.find((p) => p.uri === context)?.name ?? null };
    case 'album':
      return { kind: 'PLAYING FROM ALBUM', name: item.album };
    case 'artist':
      return { kind: 'PLAYING FROM ARTIST', name: item.artists[0] ?? null };
    default:
      return { kind: 'PLAYING FROM', name: null };
  }
}
