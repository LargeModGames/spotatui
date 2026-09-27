import type { Source } from "./bindings/Source";

/** The source name with its texture swatch. */
export function SourceBadge({ source }: { source: Source }) {
  return (
    <span className="badge">
      <span className={`swatch ${source.toLowerCase()}`} />
      {source.toUpperCase()}
    </span>
  );
}
