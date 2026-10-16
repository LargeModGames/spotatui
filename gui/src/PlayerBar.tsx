import { EN, type GuiStrings } from "./i18n";
import { useCallback, useRef, useState } from "react";
import type { Action } from "./bindings/Action";
import type { DeviceInfo } from "./bindings/DeviceInfo";
import type { PlaybackPayload } from "./bindings/PlaybackPayload";
import type { Position } from "./connection";
import { DevicesPopover } from "./DevicesPopover";
import { sourceOf } from "./format";
import { Glyph } from "./Icon";
import {
  PlayerBarVisualizer,
  type WatchVisualizer,
} from "./PlayerBarVisualizer";
import { setSetting } from "./settingsModel";
import { SourceBadge } from "./SourceBadge";
import { Meter, Progress, Transport } from "./Transport";
import { Cover } from "./ui";
import { usePosition } from "./usePosition";
import { isEmptySpace, nextMode, VISUALIZER_SETTING } from "./visualizerModel";

/** What the right panel shows; null hides it. */
export type Panel = "now" | "queue" | null;

/** The volume an unmute restores when the mute happened elsewhere. */
const UNMUTE_VOLUME = 50;

const NO_FRAMES: WatchVisualizer = () => () => {};

export function PlayerBar({
  s = EN,
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
  visualizer = null,
  watchVisualizer = NO_FRAMES,
  covered = false,
}: {
  s?: GuiStrings;
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
  /** The visualizer setting's value; null where this build cannot capture. */
  visualizer?: string | null;
  watchVisualizer?: WatchVisualizer;
  /** Something opaque, such as the Room, hides the bar. */
  covered?: boolean;
}) {
  const item = playback?.item ?? null;
  const playing = item?.is_playing ?? false;
  const elapsed = usePosition(position, playing);
  const volume = playback?.volume ?? 0;
  const source = sourceOf(item?.uri ?? null);
  const liked = playback?.liked ?? false;

  const [unmuteTo, setUnmuteTo] = useState(UNMUTE_VOLUME);
  const [devicesOpen, setDevicesOpen] = useState(false);
  const [overEmptySpace, setOverEmptySpace] = useState(false);
  // The bar takes a click only when its press started on the same empty space
  // and closed no popup: a press that slid over a control, or that dismissed a
  // menu, is no click on the bar.
  const pressed = useRef<EventTarget | null>(null);
  const devicesBox = useRef<HTMLDivElement>(null);
  const closeDevices = useCallback(() => {
    setDevicesOpen(false);
    // The keyboard goes back to the button only when it was in the popover.
    const box = devicesBox.current;
    if (box?.contains(document.activeElement))
      box.querySelector<HTMLElement>(".icon-button")?.focus();
  }, []);

  return (
    <footer
      className="playerbar"
      // Only over the empty space: a title would also show over the controls.
      title={overEmptySpace ? s.change_visualizer : undefined}
      onMouseOver={
        visualizer === null
          ? undefined
          : (event) => setOverEmptySpace(isEmptySpace(event.target as Element))
      }
      onMouseLeave={() => setOverEmptySpace(false)}
      onPointerDown={(event) => {
        const popupOpen =
          devicesOpen || document.querySelector('[role="menu"]') !== null;
        pressed.current = popupOpen ? null : event.target;
      }}
      onPointerUp={(event) => {
        // A release on another element sends the click to their common parent.
        if (event.target !== pressed.current) pressed.current = null;
      }}
      onClick={
        visualizer === null
          ? undefined
          : (event) => {
              if (
                event.target === pressed.current &&
                isEmptySpace(event.target as Element)
              )
                send(
                  setSetting(VISUALIZER_SETTING, {
                    Text: nextMode(visualizer),
                  }),
                );
            }
      }
    >
      {visualizer !== null && (
        <PlayerBarVisualizer
          mode={
            visualizer === "Spectrum" || visualizer === "Waveform"
              ? visualizer
              : "Off"
          }
          watch={watchVisualizer}
          covered={covered}
        />
      )}
      <div className="now-track">
        <Cover src={item?.image_url} size={56} />
        <div className="titles">
          <strong title={item?.title}>
            {item?.title ?? s.nothing_playing}
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
            aria-label={s.like}
            aria-pressed={liked}
            disabled={!connected}
            onClick={() => send("ToggleSaveCurrentItem")}
          >
            <Glyph name="heart" filled={liked} />
          </button>
        )}
        {source && source !== "Spotify" && (
          <SourceBadge s={s} source={source} />
        )}
      </div>
      <div className="transport">
        <Transport s={s} item={item} connected={connected} send={send} />
        <Progress
          s={s}
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
          aria-label={s.now_playing_view}
          aria-pressed={panel === "now"}
          onClick={() => onPanel(panel === "now" ? null : "now")}
        >
          <Glyph name="panel" />
        </button>
        <button
          type="button"
          className="icon-button"
          aria-label={s.lyrics}
          aria-keyshortcuts="l"
          aria-pressed={lyricsOpen}
          onClick={onLyrics}
        >
          <Glyph name="lyrics" />
        </button>
        <button
          type="button"
          className="icon-button"
          aria-label={s.queue}
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
            aria-label={s.connect_device}
            aria-haspopup="dialog"
            aria-expanded={devicesOpen}
            disabled={!connected}
            onClick={() => setDevicesOpen((open) => !open)}
          >
            <Glyph name="devices" />
          </button>
          {devicesOpen && (
            <DevicesPopover
              s={s}
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
          aria-label={volume === 0 ? s.unmute : s.mute}
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
          label={s.volume}
          ratio={volume / 100}
          text={`${volume}%`}
          disabled={!connected}
          onPick={(ratio) => send({ SetVolume: Math.round(ratio * 100) })}
          onStep={(up) => send(up ? "VolumeUp" : "VolumeDown")}
        />
        <button
          type="button"
          className="icon-button"
          aria-label={s.open_mini_player}
          data-mini-toggle
          aria-keyshortcuts="Shift+T"
          onClick={() => onMini()}
        >
          <Glyph name="mini" />
        </button>
        <button
          type="button"
          className="icon-button"
          aria-label={s.full_screen}
          aria-keyshortcuts="r"
          onClick={() => onRoom()}
        >
          <Glyph name="fullscreen" />
        </button>
      </div>
    </footer>
  );
}
