import type { Action } from "./bindings/Action";
import type { LyricLine } from "./bindings/LyricLine";
import "./LyricText.css";

/** A lyric line: a button that seeks to its start when synced, plain text when not. */
export function LyricText({
  line,
  synced,
  tabStop,
  connected,
  send,
}: {
  line: LyricLine;
  synced: boolean;
  tabStop: boolean;
  connected: boolean;
  send: (action: Action) => void;
}) {
  const text = line.text || "♪";
  if (!synced) return text;
  return (
    <button
      type="button"
      className="lyric-seek"
      tabIndex={tabStop ? 0 : -1}
      onClick={() => {
        if (connected) send({ SeekTo: line.at_ms });
      }}
    >
      {text}
    </button>
  );
}
