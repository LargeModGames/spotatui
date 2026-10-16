import { useEffect, useState } from 'react';

import type { NowPlaying } from '@gui/bindings/NowPlaying';
import type { State } from '@gui/connection';

import { useChannel, useConnection, useCoreSelect } from '@/core-context';

/** The playback channel; a position tick does not re-render its caller. */
export function usePlayback() {
  const playback = useChannel('playback') ?? null;
  return { playback, item: playback?.item ?? null };
}

function isPlaying(state: State): boolean {
  return state.channels.playback?.payload.item?.is_playing ?? false;
}

/** The position at `now` (a performance.now() value), interpolated while playing; null while the tick loop is stale. */
function elapsedAt(state: State, now: number): number | null {
  const position = state.position;
  if (!position || position.ms === null) return null;
  return isPlaying(state) ? position.ms + Math.max(0, now - position.at) : position.ms;
}

/** `select` over the interpolated position; the caller re-renders only when its result changes, so pass a stable `select`. */
export function usePositionSelect<T extends number | string | boolean | null>(select: (ms: number | null) => T): T {
  const connection = useConnection();
  const playing = useCoreSelect(isPlaying);
  const [value, setValue] = useState(() => select(elapsedAt(connection.getState(), performance.now())));
  useEffect(() => {
    const read = () => setValue(select(elapsedAt(connection.getState(), performance.now())));
    read();
    const unsubscribe = connection.subscribe(read);
    if (!playing) return unsubscribe;
    let frame = requestAnimationFrame(function step() {
      read();
      frame = requestAnimationFrame(step);
    });
    return () => {
      unsubscribe();
      cancelAnimationFrame(frame);
    };
  }, [connection, select, playing]);
  return value;
}

const identity = (ms: number | null) => ms;

/** The position every frame while playing, null while stale; call it only in a small leaf. */
export function useElapsed(): number | null {
  return usePositionSelect(identity);
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

/** "PLAYING FROM PLAYLIST" from the context URI, and the core's name for it. */
export function playingFrom(item: NowPlaying | null): { kind: string; name: string | null } {
  const context = item?.context_uri;
  if (!item || !context) return { kind: 'NOW PLAYING', name: null };
  const name = item.context_name;
  if (context.includes(':collection')) return { kind: 'PLAYING FROM YOUR LIBRARY', name };
  switch (context.split(':')[1]) {
    case 'playlist':
      return { kind: 'PLAYING FROM PLAYLIST', name };
    case 'album':
      return { kind: 'PLAYING FROM ALBUM', name };
    case 'artist':
      return { kind: 'PLAYING FROM ARTIST', name };
    default:
      return { kind: 'PLAYING FROM', name };
  }
}
