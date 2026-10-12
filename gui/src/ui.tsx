import { Children, useEffect, useRef, useState, type ReactNode } from "react";
import { Glyph, type GlyphName } from "./Icon";

/** Cover art, or a quiet tile with an icon when there is no image. */
export function Cover({
  src,
  size,
  round = false,
  fallback = "music",
}: {
  src: string | null | undefined;
  /** Pixels; omitted, the cover fills its box. */
  size?: number;
  round?: boolean;
  fallback?: GlyphName;
}) {
  const style = size ? { width: size, height: size } : undefined;
  const className = round ? "cover-art round" : "cover-art";
  if (src)
    return (
      <img
        className={className}
        src={src}
        alt=""
        style={style}
        loading="lazy"
      />
    );
  return (
    <div className={`${className} empty`} style={style} aria-hidden="true">
      <Glyph name={fallback} size={size ? Math.max(16, size / 3) : 32} />
    </div>
  );
}

/** The round accent play button of a page header or a card. */
export function PlayButton({
  label,
  playing = false,
  large = false,
  disabled = false,
  onClick,
}: {
  label: string;
  playing?: boolean;
  large?: boolean;
  disabled?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className={large ? "play-button large" : "play-button"}
      aria-label={label}
      disabled={disabled}
      onClick={(event) => {
        event.stopPropagation();
        onClick();
      }}
    >
      <Glyph name={playing ? "pause" : "play"} size={large ? 24 : 20} />
    </button>
  );
}

/** A grid card: the cover, a title and a subtitle; the play button shows on hover. */
export function Card({
  image,
  title,
  subtitle,
  round = false,
  onOpen,
  onPlay,
}: {
  image: string | null | undefined;
  title: string;
  subtitle: string;
  round?: boolean;
  onOpen: () => void;
  onPlay?: () => void;
}) {
  return (
    <div className="card">
      <button type="button" className="card-open" onClick={onOpen}>
        <span className="card-cover">
          <Cover
            src={image}
            round={round}
            fallback={round ? "user" : "music"}
          />
        </span>
        <span className="card-title">{title}</span>
        <span className="card-subtitle">{subtitle}</span>
      </button>
      {onPlay && (
        <span className="card-play">
          <PlayButton label={`Play ${title}`} onClick={onPlay} />
        </span>
      )}
    </div>
  );
}

/** A titled row of cards. */
export function Shelf({
  title,
  children,
  oneRow = false,
}: {
  title: string;
  children: ReactNode;
  /** Render only the cards the first grid row holds, so no clipped card takes focus. */
  oneRow?: boolean;
}) {
  const cards = useRef<HTMLDivElement>(null);
  const [columns, setColumns] = useState<number | null>(null);
  useEffect(() => {
    const grid = cards.current;
    if (!oneRow || !grid) return;
    const measure = () =>
      setColumns(
        getComputedStyle(grid).gridTemplateColumns.split(" ").filter(Boolean)
          .length,
      );
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(grid);
    return () => observer.disconnect();
  }, [oneRow]);
  const shown =
    oneRow && columns ? Children.toArray(children).slice(0, columns) : children;
  return (
    <section className="shelf" aria-label={title}>
      <h2>{title}</h2>
      <div ref={cards} className="shelf-cards">
        {shown}
      </div>
    </section>
  );
}

/** The gradient header of a list page: the cover, the kind, the title and a meta line. */
export function PageHeader({
  kind,
  title,
  image,
  round = false,
  tone = "neutral",
  meta,
}: {
  kind: string;
  title: string;
  image: string | null | undefined;
  round?: boolean;
  /** Picks the header gradient. */
  tone?: "neutral" | "liked" | "album" | "artist" | "playlist";
  meta?: ReactNode;
}) {
  return (
    <header className={`page-header tone-${tone}`}>
      <Cover
        src={image}
        size={192}
        round={round}
        fallback={round ? "user" : "music"}
      />
      <div className="page-header-text">
        <span className="page-kind">{kind}</span>
        <h1>{title}</h1>
        {meta && <div className="page-meta">{meta}</div>}
      </div>
    </header>
  );
}
