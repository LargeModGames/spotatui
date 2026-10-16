import { useEffect, useRef, useState } from "react";
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

export type WatchVisualizer = (listener: (frame: Frame) => void) => () => void;

/** The spectrum or waveform behind the player bar, from frames the page asks for only while it shows. */
export function PlayerBarVisualizer({
  mode,
  watch,
  covered,
}: {
  mode: VisualizerMode;
  watch: WatchVisualizer;
  /** Something opaque, such as the Room, hides the bar. */
  covered: boolean;
}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const size = useRef({ width: 0, height: 0 });
  const [shown, setShown] = useState(false);

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
    const element = canvas.current;
    const context = element?.getContext("2d");
    if (mode === "Off" || !shown || covered || !element || !context) return;
    const colours = barColours(
      getComputedStyle(element).getPropertyValue("--accent"),
    );
    const bars = restingBars(BANDS);
    let frame: Frame | null = null;
    let request = 0;
    let last = 0;
    const draw = (now: number) => {
      request = 0;
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
      const sound = frame && now - frame.at < STALE_MS ? frame.payload : null;
      let moving = false;
      if (mode === "Spectrum") {
        stepBars(
          bars,
          sound?.levels.map((level) => level / 255) ?? [],
          seconds,
        );
        paintShades(context, spectrumShades(bars, width, height, colours));
        moving = sound !== null || !settled(bars);
      } else if (sound && sound.wave.length > 0) {
        paintWave(
          context,
          waveShapes(
            sound.wave.map((value) => value / 127),
            width,
            height,
            colours,
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
  }, [mode, shown, covered, watch]);

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
    sweep.addColorStop(0, line.low);
    sweep.addColorStop(1, line.high);
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
