import { useEffect, useRef, useState } from "react";
import type { VisualizerLook } from "./bindings/VisualizerLook";
import {
  fadeStops,
  loadCoverPixels,
  resolveStops,
  swatches,
  type Fade,
} from "./coverPalette";
import {
  barColours,
  restingBars,
  settled,
  spectrumShades,
  stepBars,
  STALE_MS,
  waveShapes,
  type Frame,
  type Shade,
  type VisualizerMode,
} from "./visualizerModel";

/** The server sends Winamp's 75 bands. */
const BANDS = 75;
/** Covers whose pixels are kept, so an album played again needs no new load. */
const KEPT_COVERS = 16;

export type WatchVisualizer = (listener: (frame: Frame) => void) => () => void;

/** The playing item's cover: `key` names the picture, `src` loads it; null without one. */
export interface CoverArt {
  key: string;
  src: string | null;
}

/** Pixels by cover key, of covers that loaded. */
const covers = new Map<string, Uint8ClampedArray>();

function remember(key: string, pixels: Uint8ClampedArray) {
  covers.delete(key);
  covers.set(key, pixels);
  const oldest = covers.keys().next().value;
  if (covers.size > KEPT_COVERS && oldest !== undefined) covers.delete(oldest);
}

/** The spectrum or waveform behind the player bar, from frames the page asks for only while it shows. */
export function PlayerBarVisualizer({
  mode,
  watch,
  covered,
  look,
  art,
}: {
  mode: VisualizerMode;
  watch: WatchVisualizer;
  /** Something opaque, such as the Room, hides the bar. */
  covered: boolean;
  /** From `visualizer.yml`; null until the settings arrive. */
  look: VisualizerLook | null;
  art: CoverArt | null;
}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const size = useRef({ width: 0, height: 0 });
  const [shown, setShown] = useState(false);
  const looks = useRef(look);
  const fade = useRef<Fade | null>(null);
  const [, setLoaded] = useState(0);
  /** The cover URL that would not load, only while it is still the playing one. */
  const [failed, setFailed] = useState<string | null>(null);
  const key = art?.key ?? null;
  const src = art?.src ?? null;
  // Another item forgets the failure, so playing this one again tries again.
  const [failedFor, setFailedFor] = useState(src);
  if (failedFor !== src) {
    setFailedFor(src);
    setFailed(null);
  }
  const wantsCover = mode !== "Off" && look?.source === "cover";
  const pixels =
    key === null
      ? null
      : (covers.get(key) ??
        (src !== null && failed === src ? null : undefined));

  useEffect(() => {
    const element = canvas.current;
    if (!element) return;
    const check = () => setShown(!document.hidden && size.current.width > 0);
    const observer = new ResizeObserver(([entry]) => {
      size.current = {
        width: entry.contentRect.width,
        height: entry.contentRect.height,
      };
      check();
    });
    observer.observe(element);
    document.addEventListener("visibilitychange", check);
    return () => {
      observer.disconnect();
      document.removeEventListener("visibilitychange", check);
    };
  }, []);

  useEffect(() => {
    if (
      !wantsCover ||
      key === null ||
      src === null ||
      covers.has(key) ||
      failed === src
    )
      return;
    let current = true;
    void loadCoverPixels(src).then((loaded) => {
      if (loaded) remember(key, loaded);
      if (!current) return;
      if (loaded) setLoaded((count) => count + 1);
      else setFailed(src);
    });
    return () => {
      current = false;
    };
  }, [wantsCover, key, src, failed]);

  useEffect(() => {
    looks.current = look;
    const element = canvas.current;
    if (!element || !look) return;
    // Until the new cover loads, the old colours stay.
    if (look.source === "cover" && src !== null && pixels === undefined) return;
    const target = resolveStops(
      look,
      pixels ? swatches(pixels, look) : null,
      getComputedStyle(element).getPropertyValue("--accent"),
    );
    const now = performance.now();
    fade.current = {
      from: fade.current ? fadeStops(fade.current, now, look) : target,
      to: target,
      start: now,
    };
  }, [look, src, pixels]);

  const ready = look !== null;
  useEffect(() => {
    const element = canvas.current;
    const context = element?.getContext("2d");
    if (mode === "Off" || !shown || covered || !ready || !element || !context)
      return;
    const accent = getComputedStyle(element).getPropertyValue("--accent");
    const bars = restingBars(BANDS);
    let frame: Frame | null = null;
    let request = 0;
    let last = 0;
    const draw = (now: number) => {
      request = 0;
      const look = looks.current;
      if (!look) return;
      const seconds = last ? (now - last) / 1000 : 1 / 60;
      last = now;
      const { width, height } = size.current;
      const scale = window.devicePixelRatio || 1;
      if (element.width !== Math.round(width * scale)) {
        element.width = Math.round(width * scale);
        element.height = Math.round(height * scale);
      }
      context.setTransform(scale, 0, 0, scale, 0, 0);
      context.clearRect(0, 0, width, height);
      const gradient = {
        stops: fade.current
          ? fadeStops(fade.current, now, look)
          : barColours(accent, look.luminance_ceiling),
        space: look.interpolation,
      };
      const sound = frame && now - frame.at < STALE_MS ? frame.payload : null;
      let moving = false;
      if (mode === "Spectrum") {
        stepBars(
          bars,
          sound?.levels.map((level) => level / 255) ?? [],
          seconds,
          look.spectrum,
        );
        paintShades(
          context,
          spectrumShades(bars, width, height, gradient, look.spectrum),
        );
        moving = sound !== null || !settled(bars);
      } else if (sound && sound.wave.length > 0) {
        paintWave(
          context,
          waveShapes(
            sound.wave.map((value) => value / 127),
            width,
            height,
            gradient,
            look.waveform,
          ),
          width,
        );
        moving = true;
      }
      if (moving) request = requestAnimationFrame(draw);
      else last = 0;
    };
    const stop = watch((next) => {
      frame = next;
      if (!request) request = requestAnimationFrame(draw);
    });
    return () => {
      stop();
      cancelAnimationFrame(request);
      // In bitmap pixels: the drawing transform can be smaller than one.
      context.setTransform(1, 0, 0, 1, 0, 0);
      context.clearRect(0, 0, element.width, element.height);
    };
  }, [mode, shown, covered, ready, watch]);

  return <canvas ref={canvas} className="playerbar-viz" aria-hidden="true" />;
}

function paintShades(context: CanvasRenderingContext2D, shades: Shade[]) {
  for (const shade of shades) {
    if (shade.top === shade.bottom) {
      context.fillStyle = shade.top;
    } else {
      const gradient = context.createLinearGradient(
        0,
        shade.y,
        0,
        shade.y + shade.height,
      );
      gradient.addColorStop(0, shade.top);
      gradient.addColorStop(1, shade.bottom);
      context.fillStyle = gradient;
    }
    context.fillRect(shade.x, shade.y, shade.width, shade.height);
  }
}

function paintWave(
  context: CanvasRenderingContext2D,
  { fills, lines }: ReturnType<typeof waveShapes>,
  width: number,
) {
  for (const { corners, peak, midline, colour, clear } of fills) {
    const gradient = context.createLinearGradient(0, peak, 0, midline);
    gradient.addColorStop(0, colour);
    gradient.addColorStop(1, clear);
    context.fillStyle = gradient;
    tracePath(context, corners);
    context.fill();
  }
  context.lineJoin = "round";
  for (const line of lines) {
    const sweep = context.createLinearGradient(0, 0, width, 0);
    const last = line.colours.length - 1;
    line.colours.forEach((colour, index) =>
      sweep.addColorStop(last > 0 ? index / last : 0, colour),
    );
    context.strokeStyle = sweep;
    context.lineWidth = line.width;
    tracePath(context, line.points);
    context.stroke();
  }
}

function tracePath(
  context: CanvasRenderingContext2D,
  points: [number, number][],
) {
  context.beginPath();
  context.moveTo(...points[0]);
  for (const point of points.slice(1)) context.lineTo(...point);
}
