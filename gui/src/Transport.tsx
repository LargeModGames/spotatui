import type { Action } from "./bindings/Action";
import type { NowPlaying } from "./bindings/NowPlaying";
import { clock } from "./format";
import { Glyph } from "./Icon";

/** Shuffle, previous, play or pause, next and repeat for the playing item. */
export function Transport({
  item,
  connected,
  send,
}: {
  item: NowPlaying | null;
  connected: boolean;
  send: (action: Action) => void;
}) {
  const playing = item?.is_playing ?? false;
  const repeat = item?.repeat ?? "off";
  return (
    <div className="buttons">
      <button
        type="button"
        className="icon-button"
        aria-label="Shuffle"
        aria-pressed={item?.shuffle ?? false}
        disabled={!connected}
        onClick={() => send("ToggleShuffle")}
      >
        <Glyph name="shuffle" />
      </button>
      <button
        type="button"
        className="icon-button"
        aria-label="Previous"
        disabled={!connected}
        onClick={() => send("PreviousTrack")}
      >
        <Glyph name="previous" />
      </button>
      <button
        type="button"
        className="play-toggle"
        aria-label={playing ? "Pause" : "Play"}
        disabled={!connected}
        onClick={() => send("TogglePlayback")}
      >
        <Glyph name={playing ? "pause" : "play"} />
      </button>
      <button
        type="button"
        className="icon-button"
        aria-label="Next"
        disabled={!connected}
        onClick={() => send("NextTrack")}
      >
        <Glyph name="next" />
      </button>
      <button
        type="button"
        className="icon-button"
        aria-label={repeat === "track" ? "Repeat one" : "Repeat"}
        aria-pressed={repeat !== "off"}
        disabled={!connected}
        onClick={() => send("CycleRepeat")}
      >
        <Glyph name="repeat" />
        {repeat === "track" && <sup>1</sup>}
      </button>
    </div>
  );
}

/** The elapsed time, the seek slider and the length of the playing item. */
export function Progress({
  item,
  elapsed,
  connected,
  send,
}: {
  item: NowPlaying | null;
  elapsed: number;
  connected: boolean;
  send: (action: Action) => void;
}) {
  const duration = item?.duration_ms ?? 0;
  const ms = duration ? Math.min(elapsed, duration) : elapsed;
  const length = item?.is_live ? "live" : clock(duration);
  return (
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
  );
}

const STEP_KEYS: Record<string, boolean> = {
  ArrowRight: true,
  ArrowUp: true,
  ArrowLeft: false,
  ArrowDown: false,
};

/** A thin slider: a click picks the fraction of its width left of the pointer, an arrow key steps. */
export function Meter({
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
