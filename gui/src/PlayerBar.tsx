import { useCallback, useRef, useState } from "react";
import type { Action } from "./bindings/Action";
import type { DeviceInfo } from "./bindings/DeviceInfo";
import type { PlaybackPayload } from "./bindings/PlaybackPayload";
import type { Position } from "./connection";
import { DevicesPopover } from "./DevicesPopover";
import { sourceOf } from "./format";
import { Glyph } from "./Icon";
import { SourceBadge } from "./SourceBadge";
import { Meter, Progress, Transport } from "./Transport";
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
  onMini,
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
  /** Switches to the mini player. */
  onMini: () => void;
  send: (action: Action) => void;
}) {
  const item = playback?.item ?? null;
  const playing = item?.is_playing ?? false;
  const elapsed = usePosition(position, playing);
  const volume = playback?.volume ?? 0;
  const source = sourceOf(item?.uri ?? null);
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
          {item && (
            <span className="artist-line">
              <span>{item.artists.join(", ")}</span>
              {item.quality && (
                <span className="quality">{` • ${item.quality}`}</span>
              )}
            </span>
          )}
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
        <Transport item={item} connected={connected} send={send} />
        <Progress
          item={item}
          elapsed={elapsed}
          connected={connected}
          send={send}
        />
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
          aria-label="Open mini player"
          aria-keyshortcuts="Shift+T"
          onClick={() => onMini()}
        >
          <Glyph name="mini" />
        </button>
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
