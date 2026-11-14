import type { GuiStrings } from "./bindings/GuiStrings";
import type { ScreenStrings } from "./bindings/ScreenStrings";
import type { Source } from "./bindings/Source";
import tables from "./bindings/strings.json";
export const EN: GuiStrings = tables.EN.gui;
export const ZH_HANS: GuiStrings = tables.ZH_HANS.gui;
export const ZH_HANT: GuiStrings = tables.ZH_HANT.gui;
export const SCREENS_EN: ScreenStrings = tables.EN.screens;
export type { GuiStrings };

/** One pass: braces in user-supplied names are never interpreted as placeholders. */
export function fmt(
  template: string,
  values: Record<string, string | number>,
): string {
  return template.replace(/\{([^{}]+)\}/g, (whole, key: string) =>
    Object.hasOwn(values, key) ? String(values[key]) : whole,
  );
}

/** Source ids stay unchanged on the wire and in commands. */
export function sourceLabel(source: Source, s: GuiStrings = EN): string {
  if (source === "Local") return s.source_local;
  if (source === "Radio") return s.source_radio;
  return source;
}

export function sourceBadge(source: Source, s: GuiStrings = EN): string {
  if (source === "Local") return s.source_local_badge;
  if (source === "Radio") return s.source_radio_badge;
  return source.toUpperCase();
}

export function tabLabel(tab: string, s: GuiStrings = EN): string {
  switch (tab) {
    case "All":
      return s.all;
    case "Songs":
      return s.songs;
    case "Artists":
      return s.artists;
    case "Albums":
      return s.albums;
    case "Podcasts":
      return s.podcasts;
    case "Playlists":
      return s.playlists;
    default:
      return tab;
  }
}

export function commandKind(kind: string, s: GuiStrings = EN): string {
  switch (kind) {
    case "song":
      return s.song.toLowerCase();
    case "album":
      return s.album.toLowerCase();
    case "artist":
      return s.artist_meta;
    case "device":
      return s.current_device_kind;
    default:
      return kind;
  }
}

/** Device type ids are protocol data; unknown types keep their original label. */
export function deviceKind(kind: string, s: GuiStrings = EN): string {
  if (kind === "castaudio") return s.device_cast_audio;
  if (kind === "castvideo") return s.device_cast_video;
  if (kind === "computer") return s.device_computer;
  if (kind === "tablet") return s.device_tablet;
  if (kind === "smartphone") return s.device_smartphone;
  if (kind === "smartwatch") return s.device_smartwatch;
  if (kind === "speaker") return s.device_speaker;
  if (kind === "tv") return s.device_tv;
  if (kind === "avr") return s.device_avr;
  if (kind === "stb") return s.device_stb;
  if (kind === "audiodongle") return s.device_audio_dongle;
  if (kind === "gameconsole") return s.device_game_console;
  if (kind === "automobile") return s.device_automobile;
  if (kind === "unknown") return s.device_unknown;
  return kind;
}
