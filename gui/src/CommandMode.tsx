import {
  useEffect,
  useEffectEvent,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import type { Action } from "./bindings/Action";
import type { DeviceInfo } from "./bindings/DeviceInfo";
import type { SearchPayload } from "./bindings/SearchPayload";
import type { SourcePayload } from "./bindings/SourcePayload";
import {
  actionFor,
  COMMANDS,
  complete,
  directAction,
  loadRecent,
  parse,
  pushRecent,
  rowsFor,
  saveRecent,
  spotifyLink,
  type Row,
  type Verb,
} from "./commandModel";
import "./CommandMode.css";
import { sourceOf } from "./format";
import { KeyHints } from "./KeyHints";
import { SourceBadge } from "./SourceBadge";

/** The `:` palette: a command line over the current screen that runs existing Actions. */
export function CommandMode({
  search,
  searchRev,
  source,
  devices,
  storage,
  send,
  onSearch,
  onClose,
}: {
  search: SearchPayload | null;
  searchRev: number | null;
  source: SourcePayload | null;
  devices: DeviceInfo[];
  storage: Pick<Storage, "getItem" | "setItem">;
  send: (action: Action) => void;
  onSearch: (query: string) => void;
  onClose: () => void;
}) {
  const [input, setInput] = useState("");
  const [cursor, setCursor] = useState(0);
  // Digits type into the argument until ArrowDown moves into the results.
  const [picking, setPicking] = useState(false);
  // The pending search; Shift+Enter on it plays the first playable answer.
  const [sent, setSent] = useState<{
    query: string;
    rev: number;
    playNow: boolean;
  } | null>(null);
  const [recent] = useState(() => loadRecent(storage));
  const [recall, setRecall] = useState(-1);
  // The typed command a history walk started from; walking back past the newest entry restores it.
  const [draft, setDraft] = useState("");

  const compiled = source?.compiled ?? [];
  const verbs = (Object.keys(COMMANDS) as Verb[]).filter(
    (verb) => verb !== "source" || compiled.length > 1,
  );
  const parsed = parse(input);
  const link = spotifyLink(input);
  // Rows show only for results that answer this query, not for any later search update.
  const answered =
    sent !== null &&
    sent.query === parsed.arg &&
    search?.query === sent.query &&
    searchRev !== null &&
    searchRev > sent.rev;
  const rows =
    parsed.verb === "device" || answered
      ? rowsFor(parsed, search, devices)
      : [];
  const pick = Math.min(cursor, rows.length - 1);

  // The device list is fetched once, when the verb first appears.
  const fetchedDevices = useRef(false);
  useEffect(() => {
    if (parsed.verb !== "device" || fetchedDevices.current) return;
    fetchedDevices.current = true;
    send("RefreshDevices");
  }, [parsed.verb, send]);

  const finish = (action: Action | null) => {
    if (action) send(action);
    saveRecent(storage, pushRecent(recent, input));
    onClose();
  };

  const runRow = (row: Row | undefined, playNow: boolean) => {
    const action = row && actionFor(parsed, row, search, playNow);
    if (action) finish(action);
  };

  const run = (playNow: boolean) => {
    if (link) return finish(link);
    if (parsed.verb === null || parsed.verb === "search") {
      if (!parsed.arg) return;
      // The Search screen runs it, so its own waiting state and cursor follow.
      onSearch(parsed.arg);
      return finish(null);
    }
    if (parsed.verb === "source" || parsed.verb === "party") {
      const action = directAction(parsed, compiled);
      if (action) finish(action);
      return;
    }
    if (rows.length > 0) return runRow(rows[Math.max(pick, 0)], playNow);
    if (parsed.verb !== "device" && parsed.arg) {
      send({ SearchActiveSource: parsed.arg });
      setSent({ query: parsed.arg, rev: searchRev ?? 0, playNow });
      setCursor(0);
    }
  };

  // A Shift+Enter search plays its first playable answer once the answer lands.
  const playAnswer = useEffectEvent(() => {
    const action = rows
      .map((row) => actionFor(parsed, row, search, true))
      .find((each) => each != null);
    if (action) finish(action);
  });
  const playsAnswer = answered && sent?.playNow === true;
  useEffect(() => {
    if (playsAnswer) playAnswer();
  }, [playsAnswer]);

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.nativeEvent.isComposing) return;
    const { key } = event;
    // Shift keeps the digit in `code` while `key` turns into a symbol.
    const digit = /^Digit([1-9])$/.exec(event.code)?.[1];
    if (key === "Escape") {
      event.preventDefault();
      onClose();
    } else if (key === "Enter") {
      event.preventDefault();
      run(event.shiftKey);
    } else if (key === "Tab") {
      event.preventDefault();
      setInput(complete(input, compiled, devices));
    } else if (key === "ArrowDown" && rows.length > 0) {
      event.preventDefault();
      setCursor(picking ? Math.min(pick + 1, rows.length - 1) : 0);
      setPicking(true);
    } else if (key === "ArrowUp" && picking) {
      event.preventDefault();
      if (pick > 0) setCursor(pick - 1);
      else setPicking(false);
    } else if (
      recent.length > 0 &&
      (key === "ArrowUp" || (key === "ArrowDown" && recall >= 0))
    ) {
      event.preventDefault();
      if (recall < 0) setDraft(input);
      const next = Math.min(
        Math.max(recall + (key === "ArrowUp" ? 1 : -1), -1),
        recent.length - 1,
      );
      setRecall(next);
      setInput(next < 0 ? draft : recent[next]);
    } else if (picking && rows.length > 0 && digit) {
      event.preventDefault();
      runRow(rows[Number(digit) - 1], event.shiftKey);
    } else if (key.length === 1) {
      setPicking(false);
    }
  };

  const help = link
    ? "Play this Spotify link"
    : parsed.verb
      ? `${COMMANDS[parsed.verb].usage} · ${COMMANDS[parsed.verb].what}`
      : parsed.arg
        ? `search “${parsed.arg}” in ${source?.active ?? "your music"}`
        : "Type a command, or words to search";
  const waiting =
    sent !== null && sent.query === parsed.arg && !answered && !link;
  return (
    <div className="palette-scrim" onClick={onClose}>
      <div
        className="palette"
        role="dialog"
        aria-modal="true"
        aria-label="Command mode"
        onClick={(event) => event.stopPropagation()}
      >
        <label className="palette-input">
          <span aria-hidden="true">:</span>
          <input
            type="text"
            role="combobox"
            aria-label="Command"
            aria-expanded={rows.length > 0}
            aria-controls="palette-rows"
            aria-activedescendant={
              picking && pick >= 0 ? `palette-${pick}` : undefined
            }
            autoFocus
            value={input}
            onChange={(event) => {
              setInput(event.target.value);
              setRecall(-1);
              setCursor(0);
              setPicking(false);
            }}
            onKeyDown={onKeyDown}
          />
          <span className="tag">
            {parsed.verb || link ? "COMMAND" : "SEARCH"}
          </span>
        </label>
        <div className="palette-help">
          {help}
          {waiting && " · searching…"}
          {!waiting &&
            (parsed.verb === "play" || parsed.verb === "queue") &&
            parsed.arg &&
            !answered &&
            " · enter to find"}
        </div>
        {rows.length > 0 && (
          <ul id="palette-rows" role="listbox" aria-label="Results">
            {rows.map((row, index) => {
              const rowSource = sourceOf(row.uri);
              return (
                <li
                  key={`${index}-${row.uri ?? row.deviceId}`}
                  id={`palette-${index}`}
                  role="option"
                  aria-selected={picking && index === pick}
                  onClick={() => runRow(row, false)}
                >
                  <span className="n">{index + 1}</span>
                  <span className="kind">{row.kind}</span>
                  <span className="what">
                    <b>{row.title}</b> <span>{row.meta}</span>
                  </span>
                  {rowSource && <SourceBadge source={rowSource} />}
                </li>
              );
            })}
          </ul>
        )}
        <div className="palette-lists">
          <div className="recent">
            <span className="eyebrow">RECENT · ↑</span>
            {recent.slice(0, 5).map((entry) => (
              <span key={entry}>{entry}</span>
            ))}
          </div>
          <div className="verbs">
            <span className="eyebrow">COMMANDS · tab to complete</span>
            {verbs.map((verb) => (
              <span key={verb}>
                <b>{verb}</b> <span>{COMMANDS[verb].what}</span>
              </span>
            ))}
          </div>
        </div>
        <KeyHints
          hints={[
            "↓ then 1–9 pick",
            "enter run",
            "shift+enter play now",
            "tab complete",
            "esc close",
          ]}
        />
      </div>
    </div>
  );
}
