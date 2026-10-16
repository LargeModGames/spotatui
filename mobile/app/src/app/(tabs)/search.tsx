import { router } from 'expo-router';
import { useCallback, useEffect, useState } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, TextInput, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import type { AlbumInfo } from '@gui/bindings/AlbumInfo';
import type { ArtistInfo } from '@gui/bindings/ArtistInfo';
import type { SearchPayload } from '@gui/bindings/SearchPayload';
import { sourceOf } from '@gui/format';
import { playRequest } from '@gui/libraryModel';
import { tabHasResults, TABS, type Tab } from '@gui/searchModel';

import { Chip } from '@/components/Controls';
import { Cover } from '@/components/Cover';
import { Icon } from '@/components/Icon';
import { MINI_PLAYER_SPACE, MiniPlayer } from '@/components/MiniPlayer';
import { StatusScrim } from '@/components/StatusScrim';
import { TrackRow } from '@/components/TrackRow';
import { useChannel, useSend } from '@/core-context';
import { usePlayback } from '@/playback';
import { colors, fonts, type } from '@/theme';

/** Songs shown under All; the Songs chip shows every hit. */
const ALL_SONGS = 4;
/** How long the field stays still before a search starts. */
const TYPE_PAUSE_MS = 350;

export default function SearchScreen() {
  const send = useSend();
  const { item } = usePlayback();
  const [text, setText] = useState('');
  const [asked, setAsked] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>('All');
  const search = useChannel('search');
  const query = text.trim();
  // As in Spotify's app, the last results stay while a newer search runs.
  const results = query && search?.query ? search : null;
  const ask = useCallback(
    (next: string) => {
      setAsked(next);
      send({ SearchActiveSource: next });
    },
    [send],
  );
  // A pause in typing starts the search, as in Spotify's app.
  useEffect(() => {
    if (!query || query === asked) return;
    const timer = setTimeout(() => ask(query), TYPE_PAUSE_MS);
    return () => clearTimeout(timer);
  }, [query, asked, ask]);
  const submit = () => {
    if (!query) return;
    setTab('All');
    if (query !== asked) ask(query);
  };
  return (
    <View style={styles.screen}>
      <ScrollView keyboardShouldPersistTaps="handled" contentContainerStyle={{ paddingBottom: MINI_PLAYER_SPACE }}>
        <SafeAreaView edges={['top']} style={styles.top}>
          <Text style={type.title}>Search</Text>
          <View style={styles.field}>
            <Icon name="search" size={20} color={colors.onAccent} />
            <TextInput
              value={text}
              onChangeText={setText}
              onSubmitEditing={submit}
              returnKeyType="search"
              placeholder="What do you want to hear?"
              placeholderTextColor={colors.dim}
              style={styles.input}
              accessibilityLabel="Search songs, artists, albums"
            />
            {text.length > 0 && (
              <Pressable onPress={() => setText('')} accessibilityLabel="Clear search" style={styles.clear}>
                <Icon name="close" size={20} color={colors.onAccent} />
              </Pressable>
            )}
          </View>
          {results && (
            <ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={styles.chips}>
              {TABS.filter((each) => tabHasResults(results, each)).map((each) => (
                <Chip key={each} label={each} on={tab === each} onPress={() => setTab(each)} />
              ))}
            </ScrollView>
          )}
        </SafeAreaView>
        {query && !results && <Text style={[type.small, styles.status]}>Searching…</Text>}
        {results && <Results results={results} tab={tab} playingUri={item?.uri ?? null} />}
      </ScrollView>
      <StatusScrim />
      <MiniPlayer />
    </View>
  );
}

function Results({ results, tab, playingUri }: { results: SearchPayload; tab: Tab; playingUri: string | null }) {
  const send = useSend();
  const play = (uri: string | null) => uri && send({ PlayContext: { uri, offset: null } });
  const songs = tab === 'All' ? results.tracks.slice(0, ALL_SONGS) : results.tracks;
  const top = results.artists[0];
  if (tab === 'All' && results.tracks.length === 0 && !top) {
    return <Text style={[type.small, styles.status]}>No results.</Text>;
  }
  return (
    <View style={styles.results}>
      {tab === 'All' && top && (
        <View style={styles.section}>
          <Text style={[type.heading, styles.heading]}>Top result</Text>
          <Pressable style={styles.topCard} onPress={() => openArtist(top, play)} accessibilityRole="button" accessibilityLabel={top.name}>
            <Cover uri={top.image_url} size={92} radius={46} fallback="speaker" />
            <View style={styles.topWords}>
              <Text style={styles.topName} numberOfLines={2}>
                {top.name}
              </Text>
              <Text style={type.small}>Artist</Text>
            </View>
            <Pressable style={styles.playRound} onPress={() => play(top.uri)} accessibilityLabel={`Play ${top.name}`}>
              <Icon name="play" size={24} color={colors.onAccent} />
            </Pressable>
          </Pressable>
        </View>
      )}
      {(tab === 'All' || tab === 'Songs') && songs.length > 0 && (
        <View>
          {tab === 'All' && <Text style={[type.heading, styles.heading, styles.songsHeading]}>Songs</Text>}
          {songs.map((track, index) => (
            <TrackRow
              key={`${index}-${track.uri}`}
              image={track.image_url}
              title={track.name}
              subtitle={track.artists.join(', ')}
              playing={!!track.uri && track.uri === playingUri}
              onPress={() => {
                const action = playRequest(results.tracks, results.tracks.indexOf(track));
                if (action) send(action);
              }}
            />
          ))}
        </View>
      )}
      {tab === 'All' && results.albums.length > 0 && (
        <View style={styles.section}>
          <Text style={[type.heading, styles.heading]}>Albums</Text>
          <ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={styles.cards}>
            {results.albums.map((album, index) => (
              <Pressable key={`${index}-${album.uri}`} style={styles.card} onPress={() => openAlbum(album, play)} accessibilityLabel={album.name}>
                <Cover uri={album.image_url} size={136} />
                <Text style={type.bodyBold} numberOfLines={1}>
                  {album.name}
                </Text>
                <Text style={type.small} numberOfLines={1}>
                  {album.artists.map((artist) => artist.name).join(', ')}
                </Text>
              </Pressable>
            ))}
          </ScrollView>
        </View>
      )}
      {tab === 'Artists' &&
        results.artists.map((artist, index) => (
          <TrackRow key={`${index}-${artist.uri}`} image={artist.image_url} title={artist.name} subtitle="Artist" round fallback="speaker" onPress={() => openArtist(artist, play)} />
        ))}
      {tab === 'Albums' &&
        results.albums.map((album, index) => (
          <TrackRow
            key={`${index}-${album.uri}`}
            image={album.image_url}
            title={album.name}
            subtitle={`Album · ${album.artists.map((artist) => artist.name).join(', ')}`}
            onPress={() => openAlbum(album, play)}
          />
        ))}
      {tab === 'Playlists' &&
        results.playlists.map((playlist, index) => (
          <TrackRow
            key={`${index}-${playlist.uri}`}
            image={playlist.image_url}
            title={playlist.name}
            subtitle={`Playlist · ${playlist.owner}`}
            onPress={() => router.push({ pathname: '/playlist/[id]', params: { id: playlist.uri } })}
          />
        ))}
      {tab === 'Podcasts' &&
        results.shows.map((show, index) => (
          <TrackRow key={`${index}-${show.uri}`} image={show.image_url} title={show.name} subtitle={show.publisher} onPress={() => play(show.uri)} />
        ))}
    </View>
  );
}

/** The page id of a Spotify hit; other sources have no artist or album page, so their hits play. */
function spotifyId(id: string | null, uri: string | null): string | null {
  return id !== null && sourceOf(uri) === 'Spotify' ? id : null;
}

function openArtist(artist: ArtistInfo, play: (uri: string | null) => void) {
  const id = spotifyId(artist.id, artist.uri);
  if (id) router.push({ pathname: '/artist/[id]', params: { id, name: artist.name, image: artist.image_url ?? '' } });
  else play(artist.uri);
}

function openAlbum(album: AlbumInfo, play: (uri: string | null) => void) {
  const id = spotifyId(album.id, album.uri);
  if (id) router.push({ pathname: '/album/[id]', params: { id, name: album.name, image: album.image_url ?? '' } });
  else play(album.uri);
}
const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.background },
  top: { gap: 14, paddingHorizontal: 16, paddingTop: 12 },
  field: {
    height: 48,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
    paddingLeft: 12,
    borderRadius: 6,
    backgroundColor: colors.text,
  },
  input: { flex: 1, minWidth: 0, height: 48, fontFamily: fonts.medium, fontSize: 15, color: colors.onAccent },
  clear: { width: 44, height: 44, alignItems: 'center', justifyContent: 'center' },
  chips: { gap: 8 },
  status: { paddingHorizontal: 16, paddingTop: 24 },
  results: { gap: 24, paddingTop: 24 },
  section: { gap: 12 },
  heading: { paddingHorizontal: 16 },
  songsHeading: { marginBottom: 4 },
  topCard: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 16,
    marginHorizontal: 16,
    padding: 16,
    borderRadius: 8,
    backgroundColor: colors.tile,
  },
  topWords: { flex: 1, gap: 4 },
  topName: { fontFamily: fonts.extrabold, fontSize: 26, letterSpacing: -0.5, color: colors.text },
  playRound: { width: 48, height: 48, borderRadius: 24, alignItems: 'center', justifyContent: 'center', backgroundColor: colors.accent },
  cards: { gap: 12, paddingHorizontal: 16 },
  card: { width: 136, gap: 6 },
});
