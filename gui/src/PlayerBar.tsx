import type { Action } from "./bindings/Action";
import type { PlaybackPayload } from "./bindings/PlaybackPayload";
import type { Position } from "./connection";
import { clock, sourceOf } from "./format";
import { Icon } from "./Icon";
import { SourceBadge } from "./SourceBadge";
import { usePosition } from "./usePosition";

export function PlayerBar({
  playback,
  position,
  connected,
  send,
  onRoom,
}: {
  playback: PlaybackPayload | null;
  position: Position | null;
  connected: boolean;
  send: (action: Action) => void;
  onRoom: (lyrics: boolean) => void;
}) {
  const item = playback?.item ?? null;
  const playing = item?.is_playing ?? false;
  const duration = item?.duration_ms ?? 0;
  const elapsed = usePosition(position, playing);
  const ms = duration ? Math.min(elapsed, duration) : elapsed;
  const length = item?.is_live ? "live" : clock(duration);
  const volume = playback?.volume ?? 0;
  const source = sourceOf(item?.uri ?? null);
  const repeat = item?.repeat ?? "off";
  return (
    <footer className="playerbar">
      <div className="track">
        {item?.image_url ? (
          <img className="cover" src={item.image_url} alt="" />
        ) : (
          <span className="cover" />
        )}
        <div className="titles">
          <strong>{item?.title ?? "Nothing playing"}</strong>
          {item && (
            <span>
              {[item.artists.join(", "), item.album]
                .filter(Boolean)
                .join(" · ")}
            </span>
          )}
        </div>
        {source && <SourceBadge source={source} />}
      </div>
      <div className="transport">
        <div className="buttons">
          <button
            className="toggle"
            aria-label="Shuffle"
            aria-pressed={item?.shuffle ?? false}
            disabled={!connected}
            onClick={() => send("ToggleShuffle")}
          >
            <Icon>
              <path d="M16 3h5v5" />
              <path d="M4 20 21 3" />
              <path d="M21 16v5h-5" />
              <path d="m15 15 6 6" />
              <path d="M4 4l5 5" />
            </Icon>
          </button>
          <button
            aria-label="Previous"
            disabled={!connected}
            onClick={() => send("PreviousTrack")}
          >
            <Icon size={18}>
              <path d="M19 5 9 12l10 7V5z" />
              <path d="M5 5v14" />
            </Icon>
          </button>
          <button
            className="play"
            aria-label={playing ? "Pause" : "Play"}
            disabled={!connected}
            onClick={() => send("TogglePlayback")}
          >
            {playing ? (
              <Icon stroke={3}>
                <path d="M8 5v14M16 5v14" />
              </Icon>
            ) : (
              <Icon stroke={2}>
                <path d="M7 4.5v15l12-7.5z" fill="currentColor" />
              </Icon>
            )}
          </button>
          <button
            aria-label="Next"
            disabled={!connected}
            onClick={() => send("NextTrack")}
          >
            <Icon size={18}>
              <path d="m5 5 10 7-10 7V5z" />
              <path d="M19 5v14" />
            </Icon>
          </button>
          <button
            className="toggle"
            aria-label={repeat === "track" ? "Repeat one" : "Repeat"}
            aria-pressed={repeat !== "off"}
            disabled={!connected}
            onClick={() => send("CycleRepeat")}
          >
            <Icon>
              <path d="m17 2 4 4-4 4" />
              <path d="M3 11v-1a4 4 0 0 1 4-4h14" />
              <path d="m7 22-4-4 4-4" />
              <path d="M21 13v1a4 4 0 0 1-4 4H3" />
            </Icon>
            {repeat === "track" && <sup>1</sup>}
          </button>
        </div>
        <div className="progress">
          <span>{clock(ms)}</span>
          <Meter
            label="Seek"
            ratio={duration ? ms / duration : 0}
            text={`${clock(ms)} of ${length}`}
            disabled={!connected || !duration}
            onPick={(ratio) => send({ SeekTo: Math.round(ratio * duration) })}
            onStep={(up) => send(up ? "SeekForward" : "SeekBackward")}
          />
          <span>{length}</span>
        </div>
      </div>
      <div className="side">
        <button type="button" className="link" onClick={() => onRoom(false)}>
          Room <kbd>r</kbd>
        </button>
        <button type="button" className="link" onClick={() => onRoom(true)}>
          Lyrics <kbd>l</kbd>
        </button>
        <Icon>
          <path d="M11 5 6 9H2v6h4l5 4V5z" />
          <path d="M15.5 8.5a5 5 0 0 1 0 7" />
        </Icon>
        <Meter
          label="Volume"
          ratio={volume / 100}
          text={`${volume}%`}
          disabled={!connected}
          onPick={(ratio) => send({ SetVolume: Math.round(ratio * 100) })}
          onStep={(up) => send(up ? "VolumeUp" : "VolumeDown")}
        />
      </div>
    </footer>
  );
}

const STEP_KEYS: Record<string, boolean> = {
  ArrowRight: true,
  ArrowUp: true,
  ArrowLeft: false,
  ArrowDown: false,
};

/** A thin slider: a click picks the fraction of its width left of the pointer, an arrow key steps. */
function Meter({
  label,
  ratio,
  text,
  disabled,
  onPick,
  onStep,
}: {
  label: string;
  ratio: number;
  text: string;
  disabled: boolean;
  onPick: (ratio: number) => void;
  onStep: (up: boolean) => void;
}) {
  return (
    <div
      className="meter"
      role="slider"
      tabIndex={disabled ? -1 : 0}
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(clamp(ratio) * 100)}
      aria-valuetext={text}
      aria-disabled={disabled}
      onClick={(event) => {
        if (disabled) return;
        const box = event.currentTarget.getBoundingClientRect();
        onPick(clamp((event.clientX - box.left) / box.width));
      }}
      onKeyDown={(event) => {
        const up = STEP_KEYS[event.key];
        if (disabled || up === undefined) return;
        event.preventDefault();
        onStep(up);
      }}
    >
      <div style={{ width: `${clamp(ratio) * 100}%` }} />
    </div>
  );
}

function clamp(ratio: number): number {
  return Math.min(1, Math.max(0, ratio));
}
