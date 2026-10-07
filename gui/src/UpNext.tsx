import type { TrackInfo } from "./bindings/TrackInfo";
import { clock } from "./format";

/** The queue after the playing track, as the aside beside the library. */
export function UpNext({ tracks }: { tracks: TrackInfo[] }) {
  return (
    <aside className="upnext" aria-label="Up next">
      <h2>Up next</h2>
      {tracks.length === 0 ? (
        <p className="empty">Nothing is queued.</p>
      ) : (
        <ol>
          {tracks.map((track, index) => (
            <li key={`${index}-${track.uri ?? track.name}`}>
              <span className="n">{index + 1}</span>
              <span className="name" title={track.artists.join(", ")}>
                {track.name}
              </span>
              <span className="time">
                {track.duration_ms > 0 && clock(track.duration_ms)}
              </span>
            </li>
          ))}
        </ol>
      )}
    </aside>
  );
}
