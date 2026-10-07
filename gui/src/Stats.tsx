import { memo, useState, type KeyboardEvent } from "react";
import type { Action } from "./bindings/Action";
import type { StatsPayload } from "./bindings/StatsPayload";
import type { StatsRow } from "./bindings/StatsRow";
import { KeyHints } from "./KeyHints";
import { clampCursor } from "./libraryModel";
import "./Stats.css";
import {
  cyclesTo,
  groupThousands,
  moveText,
  PERIODS,
  playable,
  sentence,
} from "./statsModel";

const SHOWN = 10;
const PLAYS_ROW = ["7d", "30d", "all"];
const PLAYS_LABEL: Record<string, string> = {
  "7d": "plays in the last 7 days",
  "30d": "plays in the last 30 days",
  all: "plays, all time",
};

/** The Stats screen: plays per period, rank movements and the top lists, from local history. */
export const Stats = memo(function Stats({
  stats,
  send,
}: {
  stats: StatsPayload | null;
  send: (action: Action) => void;
}) {
  const [want, setWant] = useState(0);
  const period = stats?.period ?? "30d";
  const tracks = (stats?.top_tracks ?? []).slice(0, SHOWN);
  const cursor = clampCursor(want, tracks.length);

  const pickPeriod = (key: string) => {
    const { forward, count } = cyclesTo(period, key);
    for (let step = 0; step < count; step += 1)
      send({ CycleStatsPeriod: { forward } });
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    // A focused tab or button acts on its own Enter and space.
    if (
      event.target instanceof HTMLButtonElement &&
      (event.key === "Enter" || event.key === " ")
    )
      return;
    const play = (uri: string | null) => {
      if (playable(uri)) send({ PlayUris: { uris: [uri], offset: 0 } });
    };
    switch (event.key) {
      case "[":
      case "]":
        send({ CycleStatsPeriod: { forward: event.key === "]" } });
        break;
      case "e":
        send("GenerateStatsRecap");
        break;
      case "j":
      case "ArrowDown":
        setWant(clampCursor(cursor + 1, tracks.length));
        break;
      case "k":
      case "ArrowUp":
        setWant(clampCursor(cursor - 1, tracks.length));
        break;
      case "Enter":
        play(tracks[cursor]?.uri ?? null);
        break;
      default:
        return;
    }
    event.preventDefault();
  };

  const plays = (key: string) =>
    stats?.plays.find((entry) => entry.period === key)?.plays ?? 0;
  const noHistory = stats?.loaded && plays("all") === 0;
  const emptyPeriod = stats?.loaded && plays(period) === 0;
  return (
    <div
      className="stats"
      tabIndex={-1}
      data-focus={tracks.length === 0 ? true : undefined}
      onKeyDown={onKeyDown}
    >
      <div className="stats-head">
        <h1>Stats</h1>
        <div role="tablist" aria-label="Period">
          {PERIODS.map((entry) => (
            <button
              key={entry.key}
              type="button"
              role="tab"
              aria-selected={entry.key === period}
              onClick={() => pickPeriod(entry.key)}
            >
              {entry.label}
            </button>
          ))}
        </div>
        <kbd>[ ] change period · e export</kbd>
      </div>
      {!stats?.loaded ? (
        <p className="empty">
          {stats?.loading
            ? "Loading…"
            : "Your listening history is not loaded."}
        </p>
      ) : noHistory ? (
        <p className="empty">
          No listening history recorded yet. Play something!
        </p>
      ) : (
        <>
          <div className="stats-plays">
            {PLAYS_ROW.map((key) => (
              <div key={key}>
                <div className={key === period ? "big current" : "big"}>
                  {groupThousands(plays(key))}
                </div>
                <div className="label">{PLAYS_LABEL[key]}</div>
              </div>
            ))}
            <div className="movements">
              {stats.movements.map((movement) => {
                const [before, mark, after] = sentence(movement, period);
                return (
                  <span key={`${movement.kind}-${movement.name}`}>
                    {before}
                    <em>{mark}</em>
                    {after}
                  </span>
                );
              })}
            </div>
          </div>
          {emptyPeriod ? (
            <p className="empty">No plays in this period yet.</p>
          ) : (
            <div className="stats-lists">
              <section aria-label="Top artists">
                <div className="eyebrow list-head">
                  <span>TOP ARTISTS</span>
                  {period !== "all" && <span>VS ALL TIME</span>}
                </div>
                {stats.top_artists.slice(0, SHOWN).map((row, index) => {
                  const move = moveText(row, index + 1);
                  return (
                    <div key={row.title} className="line artist">
                      <span className="n">{index + 1}</span>
                      <b>{row.title}</b>
                      {period !== "all" && (
                        <span className={`move ${move.tone}`}>{move.text}</span>
                      )}
                    </div>
                  );
                })}
              </section>
              <section aria-label="Top albums">
                <div className="eyebrow list-head">
                  <span>TOP ALBUMS</span>
                </div>
                {stats.top_albums.slice(0, SHOWN).map((row, index) => (
                  <div key={row.title} className="line album">
                    <span className="n">{index + 1}</span>
                    <span className="cover" />
                    <b>{row.title}</b>
                  </div>
                ))}
              </section>
              <section aria-label="Top tracks">
                <div className="eyebrow list-head">
                  <span>TOP TRACKS</span>
                </div>
                <div
                  role="listbox"
                  aria-label="Top tracks"
                  tabIndex={0}
                  data-focus
                  aria-activedescendant={`stats-track-${cursor}`}
                >
                  {tracks.map((row, index) => (
                    <Track
                      key={`${index}-${row.uri ?? row.title}`}
                      row={row}
                      index={index}
                      selected={index === cursor}
                      onPick={() => setWant(index)}
                      onPlay={() =>
                        playable(row.uri) &&
                        send({ PlayUris: { uris: [row.uri], offset: 0 } })
                      }
                    />
                  ))}
                </div>
              </section>
            </div>
          )}
          {stats.week_tracks.length > 0 && (
            <section className="week" aria-label="Last 7 days">
              <span className="eyebrow">
                THIS WEEK,
                <br />
                TOP FIVE
              </span>
              {stats.week_tracks.map((row, index) => (
                <div key={`${index}-${row.uri ?? row.title}`}>
                  <span className="rank">{index + 1}</span>
                  <b>{row.title}</b>
                  <span>{row.artist}</span>
                </div>
              ))}
            </section>
          )}
        </>
      )}
      <KeyHints hints={["[ ] period", "e export", "j k move", "enter play"]} />
    </div>
  );
});

function Track({
  row,
  index,
  selected,
  onPick,
  onPlay,
}: {
  row: StatsRow;
  index: number;
  selected: boolean;
  onPick: () => void;
  onPlay: () => void;
}) {
  return (
    <div
      role="option"
      id={`stats-track-${index}`}
      aria-selected={selected}
      aria-disabled={playable(row.uri) ? undefined : true}
      title={
        playable(row.uri) ? undefined : "Only Spotify tracks play from Stats"
      }
      className="line track"
      onClick={onPick}
      onDoubleClick={onPlay}
    >
      <span className="n">{index + 1}</span>
      <span className="name">
        <b>{row.title}</b> {row.artist && <span>{row.artist}</span>}
      </span>
    </div>
  );
}
