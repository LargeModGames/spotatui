import { useEffect, useRef, useState } from "react";
import type { NowPlaying } from "./bindings/NowPlaying";
import type { ScreenStrings } from "./bindings/ScreenStrings";
import { pageStorage } from "./commandModel";
import { type Position, shownAt } from "./connection";
import { EN, type GuiStrings, SCREENS_EN } from "./i18n";
import { font, freshState } from "./screenDraw";
import {
  type Audio,
  audioOf,
  barCount,
  fitBands,
  nextStyle,
  NO_TRACK,
  readStyle,
  saveStyle,
  settleBands,
  SILENT,
  stepSeconds,
  STYLE_KEY,
  STYLES,
  type Style,
  type TrackView,
  type ScreenFeed,
  styleName,
} from "./screenModel";
import { PAINTERS } from "./screenPaint";
import { parseHex, rgba, STALE_MS } from "./visualizerModel";
import "./VisualizerScreen.css";

/** A window resize settles this long before the band count follows it. */
const RESIZE_SETTLE_MS = 250;
/** With no frame at all this long after the ask, the screen says it waits for audio. */
const WAITING_MS = 1500;
const CYCLE_KEY = "V";

function trackAt(
  item: NowPlaying | null,
  position: Position | null,
  now: number,
): TrackView {
  if (!item) return NO_TRACK;
  return {
    title: item.title,
    progressMs: shownAt(position, item.is_playing, now),
    durationMs: item.duration_ms > 0 ? item.duration_ms : null,
    playing: item.is_playing,
    shuffle: item.shuffle,
    repeat: item.repeat !== "off",
    live: item.is_live,
  };
}

/** The terminal's visualizer styles on a page of their own. */
export function VisualizerScreen({
  s = EN,
  screens = SCREENS_EN,
  item,
  position,
  active,
  available,
  feed,
}: {
  s?: GuiStrings;
  screens?: ScreenStrings;
  item: NowPlaying | null;
  position: Position | null;
  active: boolean;
  /** This build captures what the computer plays. */
  available: boolean;
  feed?: ScreenFeed;
}) {
  const [style, setStyle] = useState<Style>(() =>
    readStyle(pageStorage().getItem(STYLE_KEY)),
  );
  const canvas = useRef<HTMLCanvasElement>(null);
  const [width, setWidth] = useState(0);
  const [shown, setShown] = useState(false);
  const frame = useRef<{ audio: Audio; at: number } | null>(null);
  /** When the page last asked for frames: the wait for audio counts from there. */
  const asked = useRef(0);
  const latestProps = useRef({
    item,
    position,
    waiting: screens.audio_waiting,
  });
  const bars = barCount(style, width);

  // The draw loop reads the newest props without starting again.
  useEffect(() => {
    latestProps.current = { item, position, waiting: screens.audio_waiting };
  });

  const choose = (next: Style) => {
    setStyle(next);
    saveStyle(pageStorage(), next);
  };

  useEffect(() => {
    const element = canvas.current;
    if (!element) return;
    let settle = 0;
    const check = () => setShown(!document.hidden && element.clientWidth > 0);
    const observer = new ResizeObserver(([entry]) => {
      check();
      const next = entry.contentRect.width;
      // A hidden page keeps its width; the first width needs no wait.
      if (next === 0) return;
      setWidth((current) => (current === 0 ? next : current));
      clearTimeout(settle);
      settle = window.setTimeout(() => setWidth(next), RESIZE_SETTLE_MS);
    });
    observer.observe(element);
    document.addEventListener("visibilitychange", check);
    return () => {
      clearTimeout(settle);
      observer.disconnect();
      document.removeEventListener("visibilitychange", check);
    };
  }, [available]);

  useEffect(() => {
    if (!active || !available || !shown || !feed) return;
    asked.current = performance.now();
    return feed.watch(({ payload, at }) => {
      frame.current = { audio: audioOf(payload), at };
    });
  }, [active, available, shown, feed]);

  useEffect(() => {
    if (width > 0) feed?.bars(bars);
  }, [feed, bars, width]);

  useEffect(() => {
    const element = canvas.current;
    const ctx = element?.getContext("2d");
    if (!active || !available || !shown || !element || !ctx) return;
    const accent = parseHex(
      getComputedStyle(element).getPropertyValue("--accent"),
    );
    const state = freshState();
    const paint = PAINTERS[style];
    let last: number | null = null;
    let held: number[] = [];
    let request = 0;
    const draw = (now: number) => {
      const w = element.clientWidth;
      const h = element.clientHeight;
      const scale = window.devicePixelRatio || 1;
      const bw = Math.round(w * scale);
      const bh = Math.round(h * scale);
      if (element.width !== bw || element.height !== bh) {
        element.width = bw;
        element.height = bh;
      }
      ctx.setTransform(scale, 0, 0, scale, 0, 0);
      ctx.clearRect(0, 0, w, h);
      const latest = frame.current;
      const sound =
        latest !== null && now - latest.at < STALE_MS ? latest.audio : SILENT;
      const dt = stepSeconds(last, now);
      const bars = barCount(style, w);
      held = settleBands(held, sound.bands, dt);
      const { item, position, waiting } = latestProps.current;
      paint({
        ctx,
        w,
        h,
        audio: { bands: fitBands(held, bars), wave: sound.wave },
        track: trackAt(item, position, now),
        now,
        dt,
        accent,
        state,
      });
      last = now;
      // Silence or a pause is a frame too; only a capture that never answered waits.
      const answered = latest !== null && latest.at >= asked.current;
      if (!answered && now - asked.current > WAITING_MS) {
        ctx.font = font(13);
        ctx.textAlign = "center";
        ctx.textBaseline = "bottom";
        ctx.fillStyle = rgba([179, 179, 179], 0.8);
        ctx.fillText(waiting, w / 2, h - 8);
      }
      request = requestAnimationFrame(draw);
    };
    request = requestAnimationFrame(draw);
    return () => {
      cancelAnimationFrame(request);
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.clearRect(0, 0, element.width, element.height);
    };
  }, [active, available, shown, style]);

  return (
    <div
      className="viz-screen"
      data-focus
      tabIndex={-1}
      onKeyDown={(event) => {
        if (
          event.key === CYCLE_KEY &&
          !event.ctrlKey &&
          !event.metaKey &&
          !event.altKey &&
          !(event.target instanceof HTMLSelectElement)
        ) {
          event.preventDefault();
          choose(nextStyle(style));
        }
      }}
    >
      <header className="viz-screen-bar">
        <h1>{s.visualizer}</h1>
        <select
          className="list-sort"
          aria-label={s.visualizer_style}
          value={style}
          onChange={(event) => choose(readStyle(event.target.value))}
        >
          {STYLES.map((option) => (
            <option key={option} value={option}>
              {styleName(option, screens)}
            </option>
          ))}
        </select>
        <button
          type="button"
          className="list-sort"
          title={screens.visualizer_style_hint}
          aria-keyshortcuts={CYCLE_KEY}
          onClick={() => choose(nextStyle(style))}
        >
          {s.next_visualizer_style}
        </button>
      </header>
      {available ? (
        <canvas ref={canvas} className="viz-screen-canvas" aria-hidden="true" />
      ) : (
        <p className="viz-screen-note" role="status">
          {screens.no_capture}
        </p>
      )}
    </div>
  );
}
