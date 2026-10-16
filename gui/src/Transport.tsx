import type { PointerEvent } from "react";
import { EN, type GuiStrings, fmt } from "./i18n";
import type { Action } from "./bindings/Action";
import type { NowPlaying } from "./bindings/NowPlaying";
import { clock } from "./format";
import { Glyph } from "./Icon";

/** Shuffle, previous, play or pause, next and repeat for the playing item. */
export function Transport({
  s = EN,
  item,
  connected,
  send,
}: {
  s?: GuiStrings;
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
        aria-label={s.shuffle}
        aria-pressed={item?.shuffle ?? false}
        disabled={!connected}
        onClick={() => send("ToggleShuffle")}
      >
        <Glyph name="shuffle" />
      </button>
      <button
        type="button"
        className="icon-button"
        aria-label={s.previous}
        disabled={!connected}
        onClick={() => send("PreviousTrack")}
      >
        <Glyph name="previous" />
      </button>
      <button
        type="button"
        className="play-toggle"
        aria-label={playing ? s.pause : s.play}
        disabled={!connected}
        onClick={() => send("TogglePlayback")}
      >
        <Glyph name={playing ? "pause" : "play"} />
      </button>
      <button
        type="button"
        className="icon-button"
        aria-label={s.next}
        disabled={!connected}
        onClick={() => send("NextTrack")}
      >
        <Glyph name="next" />
      </button>
      <button
        type="button"
        className="icon-button"
        aria-label={repeat === "track" ? s.repeat_one : s.repeat}
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
  s = EN,
  item,
  elapsed,
  connected,
  send,
}: {
  s?: GuiStrings;
  item: NowPlaying | null;
  elapsed: number;
  connected: boolean;
  send: (action: Action) => void;
}) {
  const duration = item?.duration_ms ?? 0;
  const ms = duration ? Math.min(elapsed, duration) : elapsed;
  const length = item?.is_live ? s.live : clock(duration);
  return (
    <div className="progress">
      <span>{clock(ms)}</span>
      <Meter
        label={s.seek}
        ratio={duration ? ms / duration : 0}
        text={fmt(s.seek_position, { position: clock(ms), length })}
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

/** A thin slider: a press or a drag picks the fraction of its width left of the pointer on release, an arrow key steps. */
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
      onPointerDown={(event) => {
        if (disabled || event.button !== 0) return;
        event.currentTarget.setPointerCapture(event.pointerId);
        event.currentTarget.dataset.drag = String(event.pointerId);
        follow(event.currentTarget, pointerRatio(event));
      }}
      onPointerMove={(event) => {
        if (!disabled && dragging(event))
          follow(event.currentTarget, pointerRatio(event));
      }}
      onPointerUp={(event) => {
        if (!disabled && dragging(event)) onPick(pointerRatio(event));
      }}
      onLostPointerCapture={(event) => {
        if (dragging(event)) delete event.currentTarget.dataset.drag;
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

function pointerRatio(event: PointerEvent<HTMLDivElement>): number {
  const box = event.currentTarget.getBoundingClientRect();
  return clamp((event.clientX - box.left) / box.width);
}

/** The event comes from the pointer whose press the meter took; a touch the browser captured on its own does not. */
function dragging(event: PointerEvent<HTMLDivElement>): boolean {
  return event.currentTarget.dataset.drag === String(event.pointerId);
}

/** Draws the bar at the pointer outside React, so a playback push during the drag cannot move it back. */
function follow(meter: HTMLDivElement, ratio: number) {
  meter.style.setProperty("--drag", `${ratio * 100}%`);
}

function clamp(ratio: number): number {
  return Math.min(1, Math.max(0, ratio));
}
