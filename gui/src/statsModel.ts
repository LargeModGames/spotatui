import type { StatsMovement } from "./bindings/StatsMovement";
import type { StatsRow } from "./bindings/StatsRow";

/** The period ring of the app, in the order `[` and `]` step through it. */
export const PERIODS: { key: string; label: string; phrase: string }[] = [
  { key: "7d", label: "7 days", phrase: "in the last 7 days" },
  { key: "30d", label: "30 days", phrase: "in the last 30 days" },
  { key: "month", label: "This month", phrase: "this month" },
  { key: "year", label: "This year", phrase: "this year" },
  { key: "all", label: "All time", phrase: "of all time" },
];

/** How many steps, and which way, from one period to another on the ring. */
export function cyclesTo(
  from: string,
  to: string,
): { forward: boolean; count: number } {
  const size = PERIODS.length;
  const start = PERIODS.findIndex((period) => period.key === from);
  const end = PERIODS.findIndex((period) => period.key === to);
  if (start < 0 || end < 0) return { forward: true, count: 0 };
  const ahead = (end - start + size) % size;
  return ahead <= size - ahead
    ? { forward: true, count: ahead }
    : { forward: false, count: size - ahead };
}

export type Tone = "up" | "down" | "same";

/** An artist's place against its all-time rank. `rank` is 1-based within the period. */
export function moveText(
  row: StatsRow,
  rank: number,
): { text: string; tone: Tone } {
  if (row.new) return { text: "new", tone: "up" };
  const before = row.all_time_rank;
  if (before === null || before === rank) return { text: "=", tone: "same" };
  return before > rank
    ? { text: `▲ ${before - rank} from #${before}`, tone: "up" }
    : { text: `▼ ${rank - before} from #${before}`, tone: "down" };
}

/** A movement as a sentence in three parts; the middle one is set in amber. */
export function sentence(
  movement: StatsMovement,
  periodKey: string,
): [string, string, string] {
  const phrase =
    PERIODS.find((period) => period.key === periodKey)?.phrase ?? "";
  const { name, from, to } = movement;
  switch (movement.kind) {
    case "climb":
      return [
        `${name} climbed from #${from} all time to `,
        `#${to}`,
        ` ${phrase}.`,
      ];
    case "fall":
      return to === null
        ? [`${name}, your #${from} all time, has `, "no plays", ` ${phrase}.`]
        : [`${name} fell from #${from} to `, `#${to}`, ` ${phrase}.`];
    default:
      return [`${name} is `, "new", ` in your top 10 ${phrase}.`];
  }
}

/** Thousands apart with a no-break space, as the design sets big numbers: `1 043`. */
export function groupThousands(value: number): string {
  return String(value).replace(/\B(?=(\d{3})+(?!\d))/g, "\u00a0");
}

/** The terminal plays only Spotify tracks from Stats. */
export function playable(uri: string | null): uri is string {
  return uri?.startsWith("spotify:track:") ?? false;
}
