import type { ReactNode } from "react";

/** A line icon on the 24-unit grid of the design canvas. */
export function Icon({
  size = 16,
  stroke = 1.75,
  children,
}: {
  size?: number;
  stroke?: number;
  children: ReactNode;
}) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={stroke}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

const PATHS = {
  home: (
    <path d="M3 10.5 12 3l9 7.5V20a1 1 0 0 1-1 1h-5v-6h-6v6H4a1 1 0 0 1-1-1z" />
  ),
  search: (
    <>
      <circle cx="11" cy="11" r="7" />
      <path d="m20 20-4-4" />
    </>
  ),
  library: <path d="M4 4v16M9 4v16M14 4l6 16" />,
  back: <path d="m15 5-7 7 7 7" />,
  forward: <path d="m9 5 7 7-7 7" />,
  play: <path d="M7 4.5v15l12-7.5z" fill="currentColor" stroke="none" />,
  pause: (
    <path d="M6 4h4v16H6zM14 4h4v16h-4z" fill="currentColor" stroke="none" />
  ),
  next: <path d="M5 5v14l10-7zM19 5v14" />,
  previous: <path d="M19 5v14L9 12zM5 5v14" />,
  shuffle: (
    <path d="M3 7h3.5c5 0 6 10 11 10H21m0 0-3-3m3 3-3 3M3 17h3.5c1.6 0 2.7-1 3.6-2.4M21 7h-3.5c-1.6 0-2.7 1-3.6 2.4M21 7l-3-3m3 3-3 3" />
  ),
  repeat: (
    <path d="M4 11V9a3 3 0 0 1 3-3h13m0 0-3-3m3 3-3 3M20 13v2a3 3 0 0 1-3 3H4m0 0 3 3m-3-3 3-3" />
  ),
  heart: (
    <path d="M12 20s-7.5-4.6-9-9.3C2 7.4 4.2 4.5 7.3 4.5c2 0 3.5 1.1 4.7 2.7 1.2-1.6 2.7-2.7 4.7-2.7 3.1 0 5.3 2.9 4.3 6.2C19.5 15.4 12 20 12 20z" />
  ),
  queue: <path d="M4 6h16M4 12h10M4 18h10M18 14v6m-3-3h6" />,
  lyrics: (
    <>
      <rect x="9" y="3" width="6" height="11" rx="3" />
      <path d="M5 11a7 7 0 0 0 14 0M12 18v3" />
    </>
  ),
  devices: (
    <>
      <rect x="6" y="3" width="12" height="18" rx="2" />
      <circle cx="12" cy="14" r="3" />
      <path d="M12 7h.01" />
    </>
  ),
  volume: (
    <path d="M4 9v6h4l5 4V5L8 9zM16.5 8.5a5 5 0 0 1 0 7M19 6a8.5 8.5 0 0 1 0 12" />
  ),
  mute: <path d="M4 9v6h4l5 4V5L8 9zM17 9l5 6m0-6-5 6" />,
  fullscreen: <path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5" />,
  panel: (
    <>
      <rect x="3" y="4" width="18" height="16" rx="2" />
      <path d="M15 4v16" />
    </>
  ),
  close: <path d="M6 6l12 12M18 6 6 18" />,
  plus: <path d="M12 5v14M5 12h14" />,
  clock: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7v5l3 2" />
    </>
  ),
  discover: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="m15.5 8.5-2 5-5 2 2-5z" />
    </>
  ),
  session: <path d="M3 12h4l3-7 4 14 3-7h4" />,
  stats: <path d="M5 20V10M12 20V4M19 20v-7" />,
  party: (
    <>
      <circle cx="9" cy="8" r="3.5" />
      <path d="M2.5 20a6.5 6.5 0 0 1 13 0M16 4.5a3.5 3.5 0 0 1 0 7M18 14a6.5 6.5 0 0 1 3.5 6" />
    </>
  ),
  link: (
    <path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7L12 6.3M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1" />
  ),
  music: (
    <>
      <path d="M9 18V5l11-2v13" />
      <circle cx="6" cy="18" r="3" />
      <circle cx="17" cy="16" r="3" />
    </>
  ),
  user: (
    <>
      <circle cx="12" cy="8" r="4" />
      <path d="M4 21a8 8 0 0 1 16 0" />
    </>
  ),
  more: <path d="M5 12h.01M12 12h.01M19 12h.01" strokeWidth="3" />,
  gear: (
    <>
      <path d="M18.8 10.3L21.4 10.7 21.4 13.3 18.8 13.7A7 7 0 0 1 18 15.6L19.6 17.7 17.7 19.6 15.6 18A7 7 0 0 1 13.7 18.8L13.3 21.4 10.7 21.4 10.3 18.8A7 7 0 0 1 8.4 18L6.3 19.6 4.4 17.7 6 15.6A7 7 0 0 1 5.2 13.7L2.6 13.3 2.6 10.7 5.2 10.3A7 7 0 0 1 6 8.4L4.4 6.3 6.3 4.4 8.4 6A7 7 0 0 1 10.3 5.2L10.7 2.6 13.3 2.6 13.7 5.2A7 7 0 0 1 15.6 6L17.7 4.4 19.6 6.3 18 8.4A7 7 0 0 1 18.8 10.3z" />
      <circle cx="12" cy="12" r="3" />
    </>
  ),
} satisfies Record<string, ReactNode>;

export type GlyphName = keyof typeof PATHS;

/** A named icon from the app's own set. */
export function Glyph({
  name,
  size = 16,
  filled = false,
}: {
  name: GlyphName;
  size?: number;
  /** Fills the outline, for a pressed heart. */
  filled?: boolean;
}) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill={filled ? "currentColor" : "none"}
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {PATHS[name]}
    </svg>
  );
}
