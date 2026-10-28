import { useStringsReader, type StringsReader } from "./useStringsReader";
import { EN, type GuiStrings, fmt } from "./i18n";
import {
  memo,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type MouseEvent,
} from "react";
import type { Action } from "./bindings/Action";
import type { EpisodeInfo } from "./bindings/EpisodeInfo";
import type { PodcastPage as Podcast } from "./bindings/PodcastPage";
import type { ShowPayload } from "./bindings/ShowPayload";
import { Glyph } from "./Icon";
import { clampCursor, step } from "./libraryModel";
import type { OpenView } from "./nav";
import "./Pages.css";
import { copyLink, loadLatch, type LoadLatch } from "./pagesModel";
import "./PodcastPage.css";
import {
  episodeDate,
  episodeLength,
  episodeLink,
  episodePager,
  episodePlay,
  longDescription,
  NEAR_END,
  newestEpisode,
  resumeOf,
  showLink,
} from "./showModel";
import { Cover, PageHeader, PlayButton } from "./ui";

type Send = (action: Action) => void;

const NO_EPISODES: EpisodeInfo[] = [];

export interface PodcastPageProps {
  s?: GuiStrings;
  /** The open view; a new one is a fresh open, even of the same show. */
  view: object;
  id: string;
  name: string;
  /** The last fetched show, which can still be another one. */
  show: ShowPayload | null;
  /** The status revision and whether it is an error: a later error means the fetch failed. */
  statusRev: number | null;
  statusError: boolean;
  playingUri: string | null;
  active: boolean;
  open: OpenView;
  send: (action: Action) => void;
}

/** A podcast show with its episodes. */
export const PodcastPage = memo(function PodcastPage({
  s = EN,
  view,
  id,
  name,
  show,
  statusRev,
  statusError,
  playingUri,
  active,
  send,
}: PodcastPageProps) {
  const readStrings = useStringsReader(s);
  const [want, setWant] = useState(0);
  const [expanded, setExpanded] = useState(false);
  const [shownFor, setShownFor] = useState(view);
  const [latch, setLatch] = useState<LoadLatch>({
    openedAt: statusRev,
    failedId: null,
  });
  const fresh = shownFor !== view;
  if (fresh) {
    setShownFor(view);
    setWant(0);
    setExpanded(false);
  }
  const page = show?.page?.show.id === id ? show.page : null;
  const landed = page !== null && page.loaded;
  const next = loadLatch(latch, {
    id,
    fresh,
    landed,
    statusRev,
    statusError,
  });
  if (next.openedAt !== latch.openedAt || next.failedId !== latch.failedId)
    setLatch(next);
  const loading =
    !landed && next.failedId === id
      ? fmt(s.load_playlist_failed, { name: page?.show.name ?? name })
      : s.loading_episodes;
  const episodes = page?.episodes ?? NO_EPISODES;
  const cursor = clampCursor(want, episodes.length);

  const [pager] = useState(episodePager);
  useEffect(() => {
    pager.land(id, episodes.length);
  }, [pager, id, episodes.length]);
  // A failed page pushes only an error status: the next move or scroll asks again.
  const errorRev = statusError ? statusRev : null;
  useEffect(() => {
    if (errorRev !== null) pager.retry();
  }, [pager, errorRev]);
  const loadMore = useCallback(() => {
    if (active && page && pager.ask(page)) send({ LoadMore: "ShowEpisodes" });
  }, [active, page, pager, send]);

  const list = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    list.current
      ?.querySelector('[aria-selected="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [cursor]);
  useEffect(() => {
    const row = list.current?.children[Math.max(0, episodes.length - NEAR_END)];
    if (!row || !page?.has_more || typeof IntersectionObserver === "undefined")
      return;
    const observer = new IntersectionObserver((entries) => {
      if (entries.some((entry) => entry.isIntersecting)) loadMore();
    });
    observer.observe(row);
    return () => observer.disconnect();
  }, [episodes.length, page?.has_more, loadMore]);

  const play = useCallback(
    (index: number) => {
      const action = episodePlay(episodes, index);
      if (action) send(action);
    },
    [episodes, send],
  );

  if (!page)
    return (
      <section className="podcast-page" aria-label={name}>
        <PageHeader kind={s.podcast} title={name} image={null} />
        <p className="podcast-empty">{loading}</p>
      </section>
    );

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    if (event.key === "Enter") {
      event.preventDefault();
      play(cursor);
      return;
    }
    const move = step(event.key, cursor, episodes.length, page.has_more);
    if (!move) return;
    event.preventDefault();
    setWant(clampCursor(move.next, episodes.length));
    if (move.loadMore) loadMore();
  };

  return (
    <section className="podcast-page" aria-label={page.show.name}>
      <ShowHeader
        readStrings={readStrings}
        s={s}
        page={page}
        expanded={expanded}
        onExpand={() => setExpanded((open) => !open)}
        send={send}
      />
      <div className="page-section">
        <h2>{s.all_episodes}</h2>
        {episodes.length === 0 ? (
          <p className="podcast-empty">
            {page.loaded ? s.show_no_episodes : loading}
          </p>
        ) : (
          <div
            ref={list}
            className="episode-rows"
            role="listbox"
            aria-label={fmt(s.episodes_of, { name: page.show.name })}
            aria-activedescendant={
              cursor >= 0 ? `episode-${cursor}` : undefined
            }
            tabIndex={0}
            data-focus
            onKeyDown={onKeyDown}
          >
            {episodes.map((episode, index) => (
              <EpisodeRow
                readStrings={readStrings}
                s={s}
                key={`${index}-${episode.uri ?? episode.name}`}
                episode={episode}
                index={index}
                selected={index === cursor}
                playing={playingUri !== null && episode.uri === playingUri}
                onPick={setWant}
                onPlay={play}
                send={send}
              />
            ))}
          </div>
        )}
      </div>
    </section>
  );
});

/** The show's header, its follow, play and copy actions, and its description. */
export function ShowHeader({
  s = EN,
  readStrings = () => s,
  page,
  expanded,
  onExpand,
  send,
}: {
  s?: GuiStrings;
  readStrings?: StringsReader;
  page: Podcast;
  expanded: boolean;
  onExpand: () => void;
  send: Send;
}) {
  const show = page.show;
  const showId = show.id;
  const newest = newestEpisode(page.episodes);
  const count = Math.max(page.total, page.episodes.length);
  const long = longDescription(show.description);
  return (
    <>
      <PageHeader
        kind={s.podcast}
        title={show.name}
        image={show.image_url}
        meta={
          <>
            {show.publisher && <span>{show.publisher}</span>}
            {count > 0 && (
              <span className={show.publisher ? "meta-dot" : undefined}>
                {count === 1 ? s.one_episode : fmt(s.episodes_count, { count })}
              </span>
            )}
          </>
        }
      />
      <div className="page-actions">
        <PlayButton
          label={s.play_newest_episode}
          large
          disabled={newest < 0}
          onClick={() => {
            const action = episodePlay(page.episodes, newest);
            if (action) send(action);
          }}
        />
        <button
          type="button"
          className="outline-action"
          aria-pressed={page.saved}
          disabled={!showId}
          onClick={() =>
            showId &&
            send(page.saved ? { UnsaveShow: showId } : { SaveShow: showId })
          }
        >
          {page.saved ? s.following : s.follow}
        </button>
        <button
          type="button"
          className="icon-action"
          aria-label={s.copy_link}
          title={s.copy_link}
          disabled={!showId}
          onClick={() =>
            showId &&
            void copyLink(
              showLink(showId),
              "copied_show_link",
              send,
              navigator.clipboard,
              readStrings,
            )
          }
        >
          <Glyph name="link" size={24} />
        </button>
      </div>
      {show.description && (
        <div className="page-section show-about">
          <p
            className={expanded ? "show-description open" : "show-description"}
          >
            {show.description}
          </p>
          {long && (
            <button
              type="button"
              className="text-action"
              aria-expanded={expanded}
              onClick={onExpand}
            >
              {expanded ? s.show_less : s.show_more}
            </button>
          )}
        </div>
      )}
    </>
  );
}

/** One episode: a click picks it, a double click or its play button plays it. */
export const EpisodeRow = memo(function EpisodeRow({
  s = EN,
  readStrings = () => s,
  episode,
  index,
  selected,
  playing,
  onPick,
  onPlay,
  send,
}: {
  s?: GuiStrings;
  readStrings?: StringsReader;
  episode: EpisodeInfo;
  index: number;
  selected: boolean;
  playing: boolean;
  onPick: (index: number) => void;
  onPlay: (index: number) => void;
  send: Send;
}) {
  const resume = resumeOf(episode, s);
  const episodeId = episode.id;
  const date = episodeDate(episode.release_date, s);
  const length = episodeLength(episode.duration_ms, s);
  return (
    <div
      id={`episode-${index}`}
      role="option"
      aria-selected={selected}
      className={playing ? "episode-row now" : "episode-row"}
      onClick={() => onPick(index)}
      onDoubleClick={() => onPlay(index)}
    >
      <Cover src={episode.image_url} size={80} />
      <span className="episode-text">
        <span className="episode-name">
          {playing && <span className="sr-only">{s.playing_prefix}</span>}
          {episode.name}
        </span>
        {episode.description && (
          <span className="episode-description">{episode.description}</span>
        )}
        <span className="episode-meta">
          <button
            type="button"
            className="episode-play"
            tabIndex={-1}
            aria-label={fmt(s.play_named, { name: episode.name })}
            disabled={!episode.uri}
            onClick={inRow(() => onPlay(index))}
            onDoubleClick={stop}
          >
            <Glyph name="play" size={14} />
          </button>
          {date && <span>{date}</span>}
          {length && (
            <span className={date ? "meta-dot" : undefined}>{length}</span>
          )}
          {resume?.played && <span className="episode-played">{s.played}</span>}
          {resume && !resume.played && (
            <>
              <span
                className="episode-progress"
                role="progressbar"
                aria-label={s.listened}
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={resume.percent}
              >
                <span style={{ width: `${resume.percent}%` }} />
              </span>
              {resume.left && <span>{resume.left}</span>}
            </>
          )}
          <button
            type="button"
            className="episode-copy"
            tabIndex={-1}
            aria-label={fmt(s.copy_episode_link_named, { name: episode.name })}
            title={s.copy_episode_link}
            disabled={!episodeId}
            onClick={inRow(
              () =>
                episodeId &&
                void copyLink(
                  episodeLink(episodeId),
                  "copied_episode_link",
                  send,
                  navigator.clipboard,
                  readStrings,
                ),
            )}
            onDoubleClick={stop}
          >
            <Glyph name="link" size={16} />
          </button>
        </span>
      </span>
    </div>
  );
});

function inRow(action: () => void) {
  return (event: MouseEvent<HTMLButtonElement>) => {
    event.stopPropagation();
    action();
  };
}

function stop(event: MouseEvent) {
  event.stopPropagation();
}
