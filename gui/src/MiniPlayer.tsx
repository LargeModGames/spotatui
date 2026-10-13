import { useEffect, useRef, type Ref } from "react";
import type { ShellCommand } from "./areas";
import type { Action } from "./bindings/Action";
import type { PlaybackPayload } from "./bindings/PlaybackPayload";
import type { Position } from "./connection";
import { sourceOf } from "./format";
import { Glyph } from "./Icon";
import { SourceBadge } from "./SourceBadge";
import { Progress, Transport } from "./Transport";
import { Cover } from "./ui";
import { usePosition } from "./usePosition";
import "./MiniPlayer.css";

export interface MiniPlayerProps {
  playback: PlaybackPayload | null;
  position: Position | null;
  connected: boolean;
  send: (action: Action) => void;
  /** Back to the full window. */
  onExit: () => void;
}

/** A shell command as the mini player sees it: the rest of the page is hidden. */
export function miniCommand(command: ShellCommand | null): ShellCommand | null {
  return command === "toggle" || command === "mini" || command === "escape"
    ? command
    : null;
}

/** The playing item and its controls alone, in a small window. */
export function MiniPlayer(props: MiniPlayerProps) {
  const elapsed = usePosition(
    props.position,
    props.playback?.item?.is_playing ?? false,
  );
  // The control that had the keyboard is hidden with the full layout.
  const section = useRef<HTMLElement>(null);
  useEffect(() => {
    const root = section.current;
    const target =
      root?.querySelector<HTMLElement>(".play-toggle:not(:disabled)") ??
      root?.querySelector<HTMLElement>(".mini-exit");
    target?.focus();
  }, []);
  return <MiniCard {...props} elapsed={elapsed} section={section} />;
}

/** The mini player at a known position, free of hooks. */
export function MiniCard({
  playback,
  elapsed,
  connected,
  send,
  onExit,
  section,
}: Omit<MiniPlayerProps, "position"> & {
  elapsed: number;
  section?: Ref<HTMLElement>;
}) {
  const item = playback?.item ?? null;
  const source = sourceOf(item?.uri ?? null);
  const liked = playback?.liked ?? false;
  return (
    <section ref={section} className="mini-player" aria-label="Mini player">
      <div className={item ? "mini-card" : "mini-card idle"}>
        <button
          type="button"
          className="icon-button mini-exit"
          aria-label="Exit mini player"
          aria-keyshortcuts="Shift+T Escape"
          onClick={() => onExit()}
        >
          <Glyph name="close" />
        </button>
        {item ? (
          <>
            <div className="mini-cover">
              <Cover src={item.image_url} />
            </div>
            <div className="mini-meta">
              <div className="titles">
                <strong title={item.title}>{item.title}</strong>
                <span title={item.artists.join(", ")}>
                  {item.artists.join(", ")}
                </span>
              </div>
              {source === "Spotify" && (
                <button
                  type="button"
                  className="like"
                  aria-label="Like"
                  aria-pressed={liked}
                  disabled={!connected}
                  onClick={() => send("ToggleSaveCurrentItem")}
                >
                  <Glyph name="heart" filled={liked} />
                </button>
              )}
              {source && source !== "Spotify" && (
                <SourceBadge source={source} />
              )}
            </div>
            <div className="transport">
              <Progress
                item={item}
                elapsed={elapsed}
                connected={connected}
                send={send}
              />
              <Transport item={item} connected={connected} send={send} />
            </div>
          </>
        ) : (
          <div className="mini-empty">
            <Glyph name="music" size={32} />
            <p>Nothing playing</p>
          </div>
        )}
      </div>
    </section>
  );
}
