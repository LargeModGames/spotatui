import { router } from 'expo-router';
import { useState } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, TextInput, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import type { SearchPayload } from '@gui/bindings/SearchPayload';
import { playRequest } from '@gui/libraryModel';
import { tabHasResults, TABS, type Tab } from '@gui/searchModel';

import { Chip } from '@/components/Controls';
import { Cover } from '@/components/Cover';
import { Icon } from '@/components/Icon';
import { MINI_PLAYER_SPACE, MiniPlayer } from '@/components/MiniPlayer';
import { StatusScrim } from '@/components/StatusScrim';
import { TrackRow } from '@/components/TrackRow';
import { useCore, useSend } from '@/core-context';
import { usePlayback } from '@/playback';
import { colors, fonts, type } from '@/theme';

/** Songs shown under All; the Songs chip shows every hit. */
const ALL_SONGS = 4;

export default function SearchScreen() {
  const { channels } = useCore();
  const send = useSend();
  const { item } = usePlayback();
  const [text, setText] = useState('');
  const [asked, setAsked] = useState<string | null>(null);
  const [tab, setTab] = useState<Tab>('All');
  const search = channels.search?.payload;
  // Results of an older query are not shown while a new one runs.
  const results = search && search.query === asked ? search : null;
  const submit = () => {
    const query = text.trim();
    if (!query) return;
    setAsked(query);
    setTab('All');
    send({ SearchActiveSource: query });
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
        {asked && !results && <Text style={[type.small, styles.status]}>Searching…</Text>}
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
          <View style={styles.topCard}>
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
          </View>
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
              <Pressable key={`${index}-${album.uri}`} style={styles.card} onPress={() => play(album.uri)} accessibilityLabel={`Play ${album.name}`}>
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
          <TrackRow key={`${index}-${artist.uri}`} image={artist.image_url} title={artist.name} subtitle="Artist" round fallback="speaker" onPress={() => play(artist.uri)} />
        ))}
      {tab === 'Albums' &&
        results.albums.map((album, index) => (
          <TrackRow
            key={`${index}-${album.uri}`}
            image={album.image_url}
            title={album.name}
            subtitle={`Album · ${album.artists.map((artist) => artist.name).join(', ')}`}
            onPress={() => play(album.uri)}
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
