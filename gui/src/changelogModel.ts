/** A run of text with one style; `url` marks a link the page may open. */
export interface Span {
  text: string;
  bold?: boolean;
  code?: boolean;
  url?: string;
}

export type Block =
  | { kind: "heading"; level: number; spans: Span[] }
  | { kind: "item"; depth: number; spans: Span[] }
  | { kind: "paragraph"; spans: Span[] };

/** The Home page shows this many releases before "Show older". */
export const RELEASES_SHOWN = 3;

const BARE_URL = /^https:\/\/[^\s<>()[\]`*]*[^\s<>()[\]`*.,;:!?'"]/;

/** One release per `## ` heading, newest first; the title above the first one is dropped. */
export function releases(markdown: string): string[] {
  const found: string[] = [];
  for (const line of markdown.split("\n")) {
    if (line.startsWith("## ")) found.push(line);
    else if (found.length > 0) found[found.length - 1] += `\n${line}`;
  }
  return found;
}

/** Headings, bullets (nested by indent) and paragraphs; every other line is plain text. */
export function parseBlocks(markdown: string): Block[] {
  const raw: { block: Block; text: string }[] = [];
  let open: { block: Block; text: string } | null = null;
  for (const line of markdown.split("\n")) {
    if (line.trim() === "") {
      open = null;
      continue;
    }
    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    const item = /^(\s*)[-*+]\s+(.*)$/.exec(line);
    if (heading) {
      const level = heading[1].length;
      raw.push({
        block: { kind: "heading", level, spans: [] },
        text: heading[2],
      });
      open = null;
    } else if (item) {
      const depth = Math.floor(item[1].replace(/\t/g, "  ").length / 2);
      open = { block: { kind: "item", depth, spans: [] }, text: item[2] };
      raw.push(open);
    } else if (open) {
      // A wrapped line continues the bullet or paragraph above it.
      open.text += ` ${line.trim()}`;
    } else {
      open = { block: { kind: "paragraph", spans: [] }, text: line.trim() };
      raw.push(open);
    }
  }
  return raw.map(({ block, text }) => ({
    ...block,
    spans: parseInline(text.trim()),
  }));
}

/** Bold, inline code, `[label](url)` links and bare https links; only https targets become links. */
export function parseInline(text: string, bold = false): Span[] {
  const spans: Span[] = [];
  const push = (span: Span) => {
    const last = spans.at(-1);
    const plain = (one: Span) => !one.code && !one.url;
    if (last && plain(last) && plain(span) && !!last.bold === !!span.bold)
      last.text += span.text;
    else spans.push(span);
  };
  const styled = (span: Span): Span => (bold ? { ...span, bold } : span);
  let rest = text;
  while (rest.length > 0) {
    if (rest.startsWith("**")) {
      bold = !bold;
      rest = rest.slice(2);
      continue;
    }
    const code = /^`([^`]+)`/.exec(rest);
    if (code) {
      push(styled({ text: code[1], code: true }));
      rest = rest.slice(code[0].length);
      continue;
    }
    const link = /^\[([^\]]+)\]\(([^)\s]+)\)/.exec(rest);
    if (link) {
      const url = link[2].startsWith("https://") ? link[2] : undefined;
      for (const span of parseInline(link[1], bold))
        push(url ? { ...span, url } : span);
      rest = rest.slice(link[0].length);
      continue;
    }
    const bare = BARE_URL.exec(rest);
    if (bare) {
      push(styled({ text: bare[0], url: bare[0] }));
      rest = rest.slice(bare[0].length);
      continue;
    }
    const next = rest.slice(1).search(/\*\*|`|\[|https:\/\//);
    const end = next < 0 ? rest.length : next + 1;
    push(styled({ text: rest.slice(0, end) }));
    rest = rest.slice(end);
  }
  return spans;
}
