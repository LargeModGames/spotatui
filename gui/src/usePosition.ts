import { useEffect, useState } from "react";
import type { Position } from "./connection";

/** The position interpolated per animation frame from the last tick while playing. */
export function usePosition(
  position: Position | null,
  playing: boolean,
): number {
  const [now, setNow] = useState(() => performance.now());
  useEffect(() => {
    if (!playing) return;
    let frame = requestAnimationFrame(function step(time) {
      setNow(time);
      frame = requestAnimationFrame(step);
    });
    return () => cancelAnimationFrame(frame);
  }, [playing]);
  if (!position || position.ms === null) return 0;
  return playing ? position.ms + Math.max(0, now - position.at) : position.ms;
}
