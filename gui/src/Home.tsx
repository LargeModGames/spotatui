import { EN, type GuiStrings } from "./i18n";
import {
  Fragment,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import type { Action } from "./bindings/Action";
import type { AnnouncementEntry } from "./bindings/AnnouncementEntry";
import type { WelcomePayload } from "./bindings/WelcomePayload";
import {
  parseBlocks,
  parseInline,
  releases,
  RELEASES_SHOWN,
  type Block,
  type Span,
} from "./changelogModel";
import { AnnouncementLevelIcon } from "./AnnouncementDialog";
import "./Pages.css";

/** The Home page shows this many past announcements before "Show older". */
export const ANNOUNCEMENTS_SHOWN = 3;

const REDUCED_MOTION = "(prefers-reduced-motion: reduce)";

/** Whether the OS asks for less motion; false where the page cannot ask. */
function useReducedMotion(): boolean {
  const [reduced, setReduced] = useState(
    () =>
      typeof window !== "undefined" &&
      window.matchMedia?.(REDUCED_MOTION).matches === true,
  );
  useEffect(() => {
    const query = window.matchMedia?.(REDUCED_MOTION);
    if (!query) return;
    const change = () => setReduced(query.matches);
    query.addEventListener("change", change);
    return () => query.removeEventListener("change", change);
  }, []);
  return reduced;
}

/** Home: the spotatui banner, the streak and counter lines, and the changelog of the running app. */
export function Home({
  s = EN,
  welcome,
  announcements,
  changelog,
  connected,
  active,
  askChangelog,
  send,
}: {
  s?: GuiStrings;
  welcome: WelcomePayload | null;
  /** The dismissed announcements still in the feed, newest first. */
  announcements: AnnouncementEntry[];
  /** The changelog text; null until the server answered the ask. */
  changelog: string | null;
  connected: boolean;
  active: boolean;
  askChangelog: () => void;
  send: (action: Action) => void;
}) {
  const asked = useRef(false);
  useEffect(() => {
    // A drop can swallow the ask, so a reconnect may ask once more.
    if (!connected) asked.current = false;
    if (!active || !connected || asked.current || changelog !== null) return;
    asked.current = true;
    askChangelog();
  }, [active, connected, changelog, askChangelog]);
  const [shown, setShown] = useState(RELEASES_SHOWN);
  const [shownAnnouncements, setShownAnnouncements] =
    useState(ANNOUNCEMENTS_SHOWN);
  const reduced = useReducedMotion();
  const all = useMemo(
    () => (changelog === null ? null : releases(changelog)),
    [changelog],
  );
  const openLink = useCallback(
    (url: string) => send({ OpenChangelogLink: { url } }),
    [send],
  );

  return (
    <section className="home welcome" data-focus tabIndex={-1}>
      <WelcomeView
        s={s}
        welcome={welcome}
        animate={!!welcome?.banner_gradient && !reduced}
        releases={all}
        shown={shown}
        onOlder={() => setShown((count) => count + RELEASES_SHOWN)}
        onLink={openLink}
        announcements={
          <AnnouncementHistory
            s={s}
            entries={announcements}
            shown={shownAnnouncements}
            onOlder={() =>
              setShownAnnouncements((count) => count + ANNOUNCEMENTS_SHOWN)
            }
            onOpen={(id) => send({ OpenAnnouncementUrl: { id } })}
          />
        }
      />
    </section>
  );
}

/** The page body without state, so a test can walk it. */
export function WelcomeView({
  s,
  welcome,
  animate,
  releases,
  shown,
  onOlder,
  onLink,
  announcements,
}: {
  s: GuiStrings;
  welcome: WelcomePayload | null;
  animate: boolean;
  releases: string[] | null;
  shown: number;
  onOlder: () => void;
  onLink: (url: string) => void;
  /** Drawn between the lines under the banner and the changelog. */
  announcements?: ReactNode;
}) {
  return (
    <>
      {welcome && (
        <>
          <h1
            className={animate ? "welcome-banner animated" : "welcome-banner"}
            aria-label="spotatui"
          >
            {welcome.banner.replace(/^\n+|\n+$/g, "")}
          </h1>
          <div className="welcome-lines">
            {welcome.streak && (
              <p className="welcome-streak">
                {welcome.streak.days}
                <span className="welcome-note">{welcome.streak.note}</span>
              </p>
            )}
            <p className="welcome-note">{welcome.counter}</p>
            <p className="welcome-discord">
              <Inline spans={parseInline(welcome.discord)} onLink={onLink} />
            </p>
          </div>
        </>
      )}
      {announcements}
      <h2 className="welcome-heading">{s.changelog}</h2>
      {releases === null ? (
        <p className="welcome-note">{s.loading_changelog}</p>
      ) : (
        <>
          {releases.slice(0, shown).map((release, index) => (
            <article key={index} className="release">
              <Blocks blocks={parseBlocks(release)} onLink={onLink} />
            </article>
          ))}
          {shown < releases.length && (
            <button
              type="button"
              className="outline-action welcome-older"
              onClick={onOlder}
            >
              {s.show_older}
            </button>
          )}
        </>
      )}
    </>
  );
}

/** Past announcements with their level, full text and link; hidden without any. */
export function AnnouncementHistory({
  s,
  entries,
  shown,
  onOlder,
  onOpen,
}: {
  s: GuiStrings;
  entries: AnnouncementEntry[];
  shown: number;
  onOlder: () => void;
  onOpen: (id: string) => void;
}) {
  if (entries.length === 0) return null;
  return (
    <>
      <h2 className="welcome-heading">{s.announcements}</h2>
      <ul className="announcement-history">
        {entries.slice(0, shown).map((entry) => (
          <li key={entry.id} className="announcement-entry">
            <AnnouncementLevelIcon level={entry.level} />
            <div className="announcement-text">
              <h3 id={`announcement-history-${entry.id}`}>{entry.title}</h3>
              <p>{entry.body}</p>
            </div>
            {entry.url && (
              <button
                type="button"
                className="outline-action"
                aria-describedby={`announcement-history-${entry.id}`}
                onClick={() => onOpen(entry.id)}
              >
                {s.announcement_open}
              </button>
            )}
          </li>
        ))}
      </ul>
      {shown < entries.length && (
        <button
          type="button"
          className="outline-action welcome-older"
          onClick={onOlder}
        >
          {s.show_older}
        </button>
      )}
    </>
  );
}

/** Consecutive bullets share one list; a nested bullet indents by its depth. */
function Blocks({
  blocks,
  onLink,
}: {
  blocks: Block[];
  onLink: (url: string) => void;
}) {
  const out: ReactNode[] = [];
  let items: ReactNode[] = [];
  const flush = () => {
    if (items.length > 0) out.push(<ul key={out.length}>{items}</ul>);
    items = [];
  };
  blocks.forEach((block, index) => {
    const body = <Inline spans={block.spans} onLink={onLink} />;
    if (block.kind === "item") {
      items.push(
        <li key={index} className={`depth-${Math.min(block.depth, 3)}`}>
          {body}
        </li>,
      );
      return;
    }
    flush();
    if (block.kind === "paragraph") {
      out.push(<p key={out.length}>{body}</p>);
      return;
    }
    const Heading = `h${Math.min(block.level + 1, 6)}` as "h3";
    out.push(<Heading key={out.length}>{body}</Heading>);
  });
  flush();
  return out;
}

/** Links open outside the app through the server, never in the page. */
function Inline({
  spans,
  onLink,
}: {
  spans: Span[];
  onLink: (url: string) => void;
}) {
  return spans.map((span, index) => {
    let body: ReactNode = span.text;
    if (span.code) body = <code>{body}</code>;
    if (span.bold) body = <strong>{body}</strong>;
    const url = span.url;
    if (url)
      body = (
        <a
          href={url}
          onClick={(event) => {
            event.preventDefault();
            onLink(url);
          }}
        >
          {body}
        </a>
      );
    return <Fragment key={index}>{body}</Fragment>;
  });
}
