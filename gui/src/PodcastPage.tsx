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
import { copyLink } from "./pagesModel";
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
  id: string;
  name: string;
  /** The last fetched show, which can still be another one. */
  show: ShowPayload | null;
  playingUri: string | null;
  active: boolean;
  open: OpenView;
  send: (action: Action) => void;
}

/** A podcast show with its episodes. */
export const PodcastPage = memo(function PodcastPage({
  id,
  name,
  show,
  playingUri,
  active,
  send,
}: PodcastPageProps) {
  const [want, setWant] = useState(0);
  const [expanded, setExpanded] = useState(false);
  const [shownId, setShownId] = useState(id);
  if (shownId !== id) {
    setShownId(id);
    setWant(0);
    setExpanded(false);
  }
  const page = show?.page?.show.id === id ? show.page : null;
  const episodes = page?.episodes ?? NO_EPISODES;
  const cursor = clampCursor(want, episodes.length);

  const [pager] = useState(episodePager);
  useEffect(() => {
    pager.land(id, episodes.length);
  }, [pager, id, episodes.length]);
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
        <PageHeader kind="Podcast" title={name} image={null} />
        <p className="podcast-empty">Loading episodes…</p>
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
        page={page}
        expanded={expanded}
        onExpand={() => setExpanded((open) => !open)}
        send={send}
      />
      <div className="page-section">
        <h2>All episodes</h2>
        {episodes.length === 0 ? (
          <p className="podcast-empty">
            {page.loaded
              ? "This show has no episodes yet."
              : "Loading episodes…"}
          </p>
        ) : (
          <div
            ref={list}
            className="episode-rows"
            role="listbox"
            aria-label={`Episodes of ${page.show.name}`}
            aria-activedescendant={
              cursor >= 0 ? `episode-${cursor}` : undefined
            }
            tabIndex={0}
            data-focus
            onKeyDown={onKeyDown}
          >
            {episodes.map((episode, index) => (
              <EpisodeRow
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
  page,
  expanded,
  onExpand,
  send,
}: {
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
        kind="Podcast"
        title={show.name}
        image={show.image_url}
        meta={
          <>
            {show.publisher && <span>{show.publisher}</span>}
            {count > 0 && (
              <span className={show.publisher ? "meta-dot" : undefined}>
                {count === 1 ? "1 episode" : `${count} episodes`}
              </span>
            )}
          </>
        }
      />
      <div className="page-actions">
        <PlayButton
          label="Play the newest episode"
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
          {page.saved ? "Following" : "Follow"}
        </button>
        <button
          type="button"
          className="icon-action"
          aria-label="Copy link"
          title="Copy link"
          disabled={!showId}
          onClick={() =>
            showId &&
            void copyLink(
              showLink(showId),
              "Copied the show link.",
              send,
              navigator.clipboard,
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
              {expanded ? "Show less" : "Show more"}
            </button>
          )}
        </div>
      )}
    </>
  );
}

/** One episode: a click picks it, a double click or its play button plays it. */
export const EpisodeRow = memo(function EpisodeRow({
  episode,
  index,
  selected,
  playing,
  onPick,
  onPlay,
  send,
}: {
  episode: EpisodeInfo;
  index: number;
  selected: boolean;
  playing: boolean;
  onPick: (index: number) => void;
  onPlay: (index: number) => void;
  send: Send;
}) {
  const resume = resumeOf(episode);
  const episodeId = episode.id;
  const date = episodeDate(episode.release_date);
  const length = episodeLength(episode.duration_ms);
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
          {playing && <span className="sr-only">Playing: </span>}
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
            aria-label={`Play ${episode.name}`}
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
          {resume?.played && <span className="episode-played">Played</span>}
          {resume && !resume.played && (
            <>
              <span
                className="episode-progress"
                role="progressbar"
                aria-label="Listened"
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
            aria-label={`Copy episode link for ${episode.name}`}
            title="Copy episode link"
            disabled={!episodeId}
            onClick={inRow(
              () =>
                episodeId &&
                void copyLink(
                  episodeLink(episodeId),
                  "Copied the episode link.",
                  send,
                  navigator.clipboard,
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
