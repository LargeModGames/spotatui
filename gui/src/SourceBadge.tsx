import type { Source } from "./bindings/Source";

/** The texture swatch of a source. */
export function Swatch({ source }: { source: Source }) {
  return <span className={`swatch ${source.toLowerCase()}`} />;
}

/** The source name with its texture swatch. */
export function SourceBadge({ source }: { source: Source }) {
  return (
    <span className="badge">
      <Swatch source={source} />
      {source.toUpperCase()}
    </span>
  );
}
