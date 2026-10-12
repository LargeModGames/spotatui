import { useCallback, useRef, useState } from "react";
import type { Action } from "./bindings/Action";
import type { DeviceInfo } from "./bindings/DeviceInfo";
import type { PlaybackPayload } from "./bindings/PlaybackPayload";
import type { Position } from "./connection";
import { DevicesPopover } from "./DevicesPopover";
import { clock, sourceOf } from "./format";
import { Glyph } from "./Icon";
import { SourceBadge } from "./SourceBadge";
import { Cover } from "./ui";
import { usePosition } from "./usePosition";

/** What the right panel shows; null hides it. */
export type Panel = "now" | "queue" | null;

/** The volume an unmute restores when the mute happened elsewhere. */
const UNMUTE_VOLUME = 50;

export function PlayerBar({
  playback,
  position,
  connected,
  devices,
  panel,
  onPanel,
  lyricsOpen,
  onLyrics,
  onRoom,
  send,
}: {
  playback: PlaybackPayload | null;
  position: Position | null;
  connected: boolean;
  devices: DeviceInfo[];
  panel: Panel;
  onPanel: (panel: Panel) => void;
  lyricsOpen: boolean;
  onLyrics: () => void;
  onRoom: () => void;
  send: (action: Action) => void;
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
  const liked = playback?.liked ?? false;

  const [unmuteTo, setUnmuteTo] = useState(UNMUTE_VOLUME);
  const [devicesOpen, setDevicesOpen] = useState(false);
  const devicesBox = useRef<HTMLDivElement>(null);
  const closeDevices = useCallback(() => {
    setDevicesOpen(false);
    // The keyboard goes back to the button only when it was in the popover.
    const box = devicesBox.current;
    if (box?.contains(document.activeElement))
      box.querySelector<HTMLElement>(".icon-button")?.focus();
  }, []);

  return (
    <footer className="playerbar">
      <div className="now-track">
        <Cover src={item?.image_url} size={56} />
        <div className="titles">
          <strong title={item?.title}>
            {item?.title ?? "Nothing playing"}
          </strong>
          {item && <span>{item.artists.join(", ")}</span>}
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
        {source && source !== "Spotify" && <SourceBadge source={source} />}
      </div>
      <div className="transport">
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
        <button
          type="button"
          className="icon-button"
          aria-label="Now playing view"
          aria-pressed={panel === "now"}
          onClick={() => onPanel(panel === "now" ? null : "now")}
        >
          <Glyph name="panel" />
        </button>
        <button
          type="button"
          className="icon-button"
          aria-label="Lyrics"
          aria-keyshortcuts="l"
          aria-pressed={lyricsOpen}
          onClick={onLyrics}
        >
          <Glyph name="lyrics" />
        </button>
        <button
          type="button"
          className="icon-button"
          aria-label="Queue"
          aria-keyshortcuts="Shift+Q"
          aria-pressed={panel === "queue"}
          onClick={() => onPanel(panel === "queue" ? null : "queue")}
        >
          <Glyph name="queue" />
        </button>
        <div className="devices-box" ref={devicesBox}>
          <button
            type="button"
            className="icon-button"
            aria-label="Connect to a device"
            aria-haspopup="dialog"
            aria-expanded={devicesOpen}
            disabled={!connected}
            onClick={() => setDevicesOpen((open) => !open)}
          >
            <Glyph name="devices" />
          </button>
          {devicesOpen && (
            <DevicesPopover
              devices={devices}
              current={playback?.device ?? null}
              send={send}
              onClose={closeDevices}
            />
          )}
        </div>
        <button
          type="button"
          className="icon-button"
          aria-label={volume === 0 ? "Unmute" : "Mute"}
          disabled={!connected}
          onClick={() => {
            if (volume === 0) {
              send({ SetVolume: unmuteTo });
            } else {
              setUnmuteTo(volume);
              send({ SetVolume: 0 });
            }
          }}
        >
          <Glyph name={volume === 0 ? "mute" : "volume"} />
        </button>
        <Meter
          label="Volume"
          ratio={volume / 100}
          text={`${volume}%`}
          disabled={!connected}
          onPick={(ratio) => send({ SetVolume: Math.round(ratio * 100) })}
          onStep={(up) => send(up ? "VolumeUp" : "VolumeDown")}
        />
        <button
          type="button"
          className="icon-button"
          aria-label="Full screen"
          aria-keyshortcuts="r"
          onClick={() => onRoom()}
        >
          <Glyph name="fullscreen" />
        </button>
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
