import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { Action } from "./bindings/Action";
import type { EpisodeInfo } from "./bindings/EpisodeInfo";
import type { SourcePlaylists } from "./bindings/SourcePlaylists";
import { EN, ZH_HANS, ZH_HANT, deviceKind, fmt, tabLabel } from "./i18n";
import { ConfirmDialog } from "./ConfirmDialog";
import { LibraryPanel, SpotifyLogin } from "./LibraryPanel";
import { albumSorts, artistSorts, playlistRemoval } from "./libraryModel";
import { MiniCard } from "./MiniPlayer";
import { copyLink } from "./pagesModel";
import { PlaylistNameDialog } from "./PlaylistNameDialog";
import { PodcastPage } from "./PodcastPage";
import { itemLine } from "./queueModel";
import { RadioPage } from "./RadioPage";
import { RecentlyPlayed } from "./RecentlyPlayed";
import { episodeDate, episodeLength, resumeOf } from "./showModel";
import { sortOptions } from "./sortModel";
import { byName } from "./testTree";

const episode: EpisodeInfo = {
  id: "ep",
  uri: "spotify:episode:ep",
  name: "單集 {name}",
  show_name: "",
  description: "原文 {count}",
  release_date: "2026-10-01",
  image_url: null,
  duration_ms: 3900000,
  is_playable: true,
  resume_point: { fully_played: false, resume_position_ms: 60000 },
};
const lists: SourcePlaylists = {
  spotify: [],
  local: [],
  subsonic: [],
  qobuz: [],
  youtube: [],
  apple_music: [],
  apple_music_uncounted: [],
  radio: [],
  saved_albums: [],
  saved_albums_has_more: false,
  followed_artists: [],
  saved_shows: [],
  saved_shows_has_more: false,
  saved_show_ids: [],
  editable: [],
  folders: [
    { kind: "folder", name: "我的 {name}", uri: "folder:1", children: [] },
  ],
};
const noop = () => {};

describe("main additions in the shared languages", () => {
  it("localizes login and dialogs while keeping names and raw actions literal", () => {
    for (const s of [ZH_HANS, ZH_HANT]) {
      const actions: Action[] = [];
      const login = SpotifyLogin({ s, send: (action) => actions.push(action) });
      (byName(login, s.spotify_login).props.onClick as () => void)();
      expect(actions).toEqual([{ SelectSource: "Spotify" }]);
      const onCreate = vi.fn();
      const dialog = PlaylistNameDialog({
        s,
        name: " {name} 歌 ",
        onName: noop,
        onCreate,
        onCancel: noop,
      });
      expect(renderToStaticMarkup(dialog)).toContain(s.create_playlist);
      (byName(dialog, s.create).props.onClick as () => void)();
      expect(onCreate).toHaveBeenCalledWith("{name} 歌");
      const removal = playlistRemoval(
        "Spotify",
        { uri: "spotify:playlist:p", name: "{name} 歌" },
        lists,
        s,
      )!;
      expect(removal.action).toEqual({
        UnfollowPlaylist: "spotify:playlist:p",
      });
      expect(removal.body).toBe(
        fmt(s.remove_library_body, { name: "{name} 歌" }),
      );
      expect(
        renderToStaticMarkup(
          <ConfirmDialog
            s={s}
            title={removal.title}
            body={removal.body}
            confirm={removal.confirm}
            onConfirm={noop}
            onCancel={noop}
          />,
        ),
      ).toContain(s.cancel);
      const html = renderToStaticMarkup(
        <LibraryPanel
          s={s}
          playlists={lists}
          artists={[]}
          source={{ active: "Spotify", compiled: ["Spotify"] }}
          sync={null}
          view={{ page: "home" }}
          open={noop}
          send={noop}
          loggedOut
        />,
      );
      for (const text of [
        s.spotify_login,
        s.create_playlist,
        s.podcasts,
        s.folder,
        "我的 {name}",
      ])
        expect(html).toContain(text);
    }
  });

  it("localizes podcast headers, dates, lengths, resume state and queue fallback", () => {
    for (const s of [ZH_HANS, ZH_HANT]) {
      const html = renderToStaticMarkup(
        <PodcastPage
          s={s}
          id="show"
          name="節目 {count}"
          show={{
            page: {
              show: {
                id: "show",
                uri: "spotify:show:show",
                name: "節目 {count}",
                publisher: "作者",
                description: "原始说明",
                image_url: null,
              },
              episodes: [episode],
              loaded: true,
              saved: false,
              total: 1,
              has_more: false,
            },
          }}
          playingUri={episode.uri}
          active
          open={noop}
          send={noop}
        />,
      );
      for (const text of [
        s.podcast,
        s.all_episodes,
        s.copy_link,
        s.playing_prefix,
        episode.name,
        episode.description,
        "節目 {count}",
      ])
        expect(html).toContain(text);
      expect(html).toContain(episodeDate(episode.release_date, s));
      expect(html).toContain(episodeLength(episode.duration_ms, s));
      const resume = resumeOf(episode, s);
      expect(resume && !resume.played && resume.left).toBe(
        fmt(s.time_left, {
          time: episodeLength(episode.duration_ms - 60000, s),
        }),
      );
      expect(
        itemLine({ kind: "episode", track: null, episode }, s)?.subtitle,
      ).toBe(s.podcast_episode);
      expect(tabLabel("Podcasts", s)).toBe(s.podcasts);
    }
  });

  it("uses the same localized transport in the mini player and keeps its action vocabulary", () => {
    for (const s of [ZH_HANS, ZH_HANT]) {
      const actions: Action[] = [];
      const card = (
        <MiniCard
          s={s}
          elapsed={61000}
          connected
          playback={{
            item: {
              title: "歌 {time}",
              artists: ["藝人"],
              album: "專輯",
              image_url: null,
              duration_ms: 120000,
              uri: "spotify:track:t",
              is_playing: false,
              is_live: false,
              shuffle: false,
              repeat: "off",
              context_uri: null,
              context_name: null,
              quality: null,
            },
            volume: 50,
            device: "Cast {name}",
            liked: false,
          }}
          send={(action) => actions.push(action)}
          onExit={noop}
        />
      );
      const html = renderToStaticMarkup(card);
      for (const label of [s.mini_player, s.exit_mini_player, s.play, s.seek])
        expect(html).toContain(`aria-label="${label}"`);
      expect(html).toContain(
        fmt(s.seek_position, { position: "1:01", length: "2:00" }),
      );
      (byName(card, s.play).props.onClick as () => void)();
      expect(actions).toEqual(["TogglePlayback"]);
      expect(deviceKind("castaudio", s)).toBe(s.device_cast_audio);
      expect(deviceKind("vendor-kind", s)).toBe("vendor-kind");
    }
  });

  it("localizes radio and recently played loading states without translating user names", () => {
    for (const s of [ZH_HANS, ZH_HANT]) {
      const html = renderToStaticMarkup(
        <RadioPage
          s={s}
          id="seed"
          name="{name} 歌"
          table={null}
          available
          statusRev={0}
          statusError={false}
          playingUri={null}
          active
          open={noop}
          send={noop}
        />,
      );
      expect(html).toContain(fmt(s.song_radio_named, { name: "{name} 歌" }));
      expect(html).toContain(s.song_radio);

      const recent = renderToStaticMarkup(
        <RecentlyPlayed
          s={s}
          recent={null}
          available
          playingUri={null}
          active
          open={noop}
          send={noop}
        />,
      );
      expect(recent).toContain(s.loading_recent);
      expect(recent).toContain(s.history);
    }
  });

  it("keeps every sort token while translating its display label", () => {
    for (const factory of [sortOptions, albumSorts, artistSorts]) {
      const english = factory(EN);
      for (const s of [ZH_HANS, ZH_HANT]) {
        const localized = factory(s);
        expect(localized.map((x) => x.value)).toEqual(
          english.map((x) => x.value),
        );
      }
    }
    expect(sortOptions(EN)[2]).toEqual({
      value: "name:desc",
      label: "Title (Z-A)",
    });
  });

  it("reports a clipboard failure in the selected language", async () => {
    for (const s of [ZH_HANS, ZH_HANT]) {
      const actions: Action[] = [];
      await copyLink(
        "https://open.spotify.com/album/a",
        "copied_album_link",
        (action) => actions.push(action),
        { writeText: () => Promise.reject(new Error("raw")) },
        () => s,
      );
      expect(actions).toEqual([{ NotifyError: [s.copy_link_failed, 4] }]);
    }
  });

  it("fills every GUI template with distinct literal sentinel values in each table", () => {
    for (const s of [EN, ZH_HANS, ZH_HANT]) {
      for (const [key, template] of Object.entries(s)) {
        const placeholders = [...template.matchAll(/\{([^{}]+)\}/g)].map(
          (m) => m[1],
        );
        const values = Object.fromEntries(
          placeholders.map((p) => [p, `SENTINEL_${p}_中`]),
        );
        const text = fmt(template, values);
        expect(text, key).not.toMatch(/\{[^{}]+\}/);
        for (const value of Object.values(values))
          expect(text, key).toContain(value);
      }
    }
  });
});
