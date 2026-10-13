import { router } from 'expo-router';
import { useState, type ReactElement } from 'react';
import { FlatList, ScrollView, StyleSheet, Text, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import type { ListTarget } from '@gui/bindings/ListTarget';
import { songCount } from '@gui/libraryModel';

import { Chip } from '@/components/Controls';
import { MINI_PLAYER_SPACE, MiniPlayer } from '@/components/MiniPlayer';
import { StatusScrim } from '@/components/StatusScrim';
import { TrackRow } from '@/components/TrackRow';
import { useCore, useSend } from '@/core-context';
import { usePlayback } from '@/playback';
import { colors, type } from '@/theme';

const FILTERS = ['Playlists', 'Albums', 'Artists', 'Podcasts'] as const;
type Filter = (typeof FILTERS)[number];

export default function LibraryScreen() {
  const { channels } = useCore();
  const send = useSend();
  const { item } = usePlayback();
  const [filter, setFilter] = useState<Filter>('Playlists');
  const library = channels.library?.payload;
  const liked = channels.liked?.payload;
  const play = (uri: string | null) => uri && send({ PlayContext: { uri, offset: null } });
  const pick = (next: Filter) => {
    setFilter(next);
    // Albums, artists and shows are fetched when their list is first shown.
    if (next !== 'Playlists') send({ OpenLibrary: next });
  };
  const rows: ReactElement[] = [];
  let more: ListTarget | null = null;
  switch (filter) {
    case 'Playlists':
      rows.push(
        <TrackRow
          key="liked"
          image={null}
          fallback="heart"
          title="Liked Songs"
          subtitle={liked?.total ? `Playlist · ${songCount(liked.total)}` : 'Playlist'}
          playing={!!item?.context_uri?.includes(':collection')}
          onPress={() => router.push({ pathname: '/playlist/[id]', params: { id: 'liked' } })}
        />,
        ...(library?.spotify ?? []).map((playlist) => (
          <TrackRow
            key={playlist.uri}
            image={playlist.image_url}
            title={playlist.name}
            subtitle={`Playlist · ${playlist.track_count ? songCount(playlist.track_count) : playlist.owner}`}
            playing={item?.context_uri === playlist.uri}
            onPress={() => router.push({ pathname: '/playlist/[id]', params: { id: playlist.uri } })}
          />
        )),
      );
      break;
    case 'Albums':
      rows.push(
        ...(library?.saved_albums ?? []).map((album, index) => (
          <TrackRow
            key={`${index}-${album.uri}`}
            image={album.image_url}
            title={album.name}
            subtitle={`Album · ${album.artists.map((artist) => artist.name).join(', ')}`}
            playing={!!album.uri && item?.context_uri === album.uri}
            onPress={() => play(album.uri)}
          />
        )),
      );
      if (library?.saved_albums_has_more) more = 'SavedAlbums';
      break;
    case 'Artists':
      rows.push(
        ...(library?.followed_artists ?? []).map((artist, index) => (
          <TrackRow
            key={`${index}-${artist.uri}`}
            image={artist.image_url}
            title={artist.name}
            subtitle="Artist"
            round
            fallback="speaker"
            onPress={() => play(artist.uri)}
          />
        )),
      );
      break;
    case 'Podcasts':
      rows.push(
        ...(library?.saved_shows ?? []).map((show, index) => (
          <TrackRow key={`${index}-${show.uri}`} image={show.image_url} title={show.name} subtitle={`Podcast · ${show.publisher}`} onPress={() => play(show.uri)} />
        )),
      );
      if (library?.saved_shows_has_more) more = 'SavedShows';
      break;
  }
  return (
    <View style={styles.screen}>
      <SafeAreaView edges={['top']} style={styles.top}>
        <Text style={styles.title}>Your Library</Text>
        <ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={styles.chips}>
          {FILTERS.map((each) => (
            <Chip key={each} label={each} on={filter === each} onPress={() => pick(each)} />
          ))}
        </ScrollView>
      </SafeAreaView>
      <FlatList
        data={rows}
        renderItem={({ item: row }) => row}
        keyExtractor={(row) => String(row.key)}
        contentContainerStyle={{ paddingTop: 8, paddingBottom: MINI_PLAYER_SPACE }}
        onEndReached={() => more && send({ LoadMore: more })}
        ListEmptyComponent={<Text style={[type.small, styles.empty]}>Nothing here yet.</Text>}
      />
      <StatusScrim />
      <MiniPlayer />
    </View>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.background },
  top: { gap: 12, paddingHorizontal: 16, paddingTop: 12, paddingBottom: 4 },
  title: { ...type.title, fontSize: 24 },
  chips: { gap: 8 },
  empty: { paddingHorizontal: 16, paddingTop: 16 },
});
