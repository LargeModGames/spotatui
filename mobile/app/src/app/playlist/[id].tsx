import { router, useLocalSearchParams } from 'expo-router';
import { useEffect } from 'react';
import { FlatList, Pressable, StyleSheet, Text, View } from 'react-native';
import { SafeAreaView, useSafeAreaInsets } from 'react-native-safe-area-context';

import { listPlayRequest, playRequest } from '@gui/libraryModel';
import { lengthLine } from '@gui/pagesModel';

import { Cover } from '@/components/Cover';
import { Icon } from '@/components/Icon';
import { MINI_PLAYER_SPACE, MiniPlayer } from '@/components/MiniPlayer';
import { TrackRow } from '@/components/TrackRow';
import { useChannel, useSend } from '@/core-context';
import { usePlayback } from '@/playback';
import { colors, fonts, TOUCH, type } from '@/theme';

/**
 * A playlist by its URI, or Liked Songs for the id "liked". `name` and `image` come
 * along for a playlist outside the library, such as a Made For You one.
 */
export default function PlaylistScreen() {
  const params = useLocalSearchParams<{ id: string; name?: string; image?: string }>();
  const id = params.id;
  const send = useSend();
  const { item } = usePlayback();
  const insets = useSafeAreaInsets();
  const liked = id === 'liked';
  const uri = liked ? null : id;
  useEffect(() => {
    send(liked ? { OpenLibrary: 'LikedSongs' } : { Open: { Playlist: { id: uri!, from_search: false } } });
  }, [liked, uri, send]);
  const library = useChannel('library');
  const info = library?.spotify.find((playlist) => playlist.uri === uri);
  const table = useChannel('track_table');
  const likedSongs = useChannel('liked');
  const tracks = liked ? (likedSongs?.tracks ?? []) : table?.uri === uri ? table.tracks : [];
  const hasMore = liked ? !!likedSongs?.has_more : table?.uri === uri && table.has_more;
  const total = liked ? likedSongs?.total : info?.track_count;
  const name = liked ? 'Liked Songs' : (info?.name ?? params.name ?? 'Playlist');
  const image = info?.image_url ?? (params.image || null);
  const mine = !!uri && !!library?.editable.includes(uri) && !info?.collaborative;
  const playsHere = liked ? !!item?.context_uri?.includes(':collection') : !!uri && item?.context_uri === uri;
  const playing = playsHere && !!item?.is_playing;
  const playAt = (index: number) => {
    const action = liked ? playRequest(tracks, index) : listPlayRequest('Spotify', uri!, tracks, index);
    if (action) send(action);
  };
  const playAll = () => {
    if (playsHere) return send('TogglePlayback');
    if (liked) return playAt(0);
    send({ PlayContext: { uri: uri!, offset: null } });
  };
  return (
    <View style={styles.screen}>
      <FlatList
        data={tracks}
        keyExtractor={(track, index) => `${index}-${track.uri}`}
        onEndReached={() => hasMore && send({ LoadMore: liked ? 'SavedTracks' : 'PlaylistTracks' })}
        contentContainerStyle={{ paddingBottom: MINI_PLAYER_SPACE + insets.bottom }}
        ListHeaderComponent={
          <SafeAreaView edges={['top']} style={styles.header}>
            <Pressable style={styles.back} onPress={() => router.back()} accessibilityLabel="Back">
              <Icon name="back" size={24} />
            </Pressable>
            {liked ? (
              <View style={styles.likedArt}>
                <Icon name="heart" size={72} />
              </View>
            ) : (
              <Cover uri={image} size={232} style={styles.cover} />
            )}
            <View style={styles.words}>
              <Text style={styles.name}>{name}</Text>
              {!liked && (mine || info?.owner) && <Text style={styles.owner}>{mine ? 'By you' : info?.owner}</Text>}
              <Text style={type.small}>
                {hasMore || tracks.length === 0 ? (total ? `${total} songs` : 'Loading…') : lengthLine(tracks)}
              </Text>
            </View>
            <View style={styles.actions}>
              <Pressable
                style={styles.shuffle}
                onPress={() => send({ SetShuffle: !item?.shuffle })}
                accessibilityLabel="Shuffle"
                accessibilityState={{ selected: !!item?.shuffle }}>
                <Icon name="shuffle" size={24} color={item?.shuffle ? colors.accent : colors.muted} />
              </Pressable>
              <Pressable style={styles.play} onPress={playAll} accessibilityLabel={playing ? `Pause ${name}` : `Play ${name}`}>
                <Icon name={playing ? 'pause' : 'play'} size={28} color={colors.onAccent} />
              </Pressable>
            </View>
          </SafeAreaView>
        }
        renderItem={({ item: track, index }) => (
          <TrackRow
            image={track.image_url}
            title={track.name}
            subtitle={track.artists.join(', ')}
            playing={!!track.uri && track.uri === item?.uri}
            onPress={() => playAt(index)}
          />
        )}
      />
      <MiniPlayer edge />
    </View>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.background },
  header: {
    gap: 16,
    paddingHorizontal: 16,
    paddingBottom: 8,
    experimental_backgroundImage: `linear-gradient(#2c3a24, ${colors.background} 360px)`,
  },
  back: { width: TOUCH, height: TOUCH, marginLeft: -12, alignItems: 'center', justifyContent: 'center' },
  cover: { alignSelf: 'center', boxShadow: '0 12px 32px rgba(0,0,0,0.5)' },
  likedArt: {
    alignSelf: 'center',
    width: 232,
    height: 232,
    borderRadius: 4,
    alignItems: 'center',
    justifyContent: 'center',
    experimental_backgroundImage: 'linear-gradient(135deg, #4b3a8f, #a7c4c8)',
  },
  words: { gap: 8 },
  name: { fontFamily: fonts.extrabold, fontSize: 26, letterSpacing: -0.5, color: colors.text },
  owner: { fontFamily: fonts.bold, fontSize: 13, color: colors.text },
  actions: { flexDirection: 'row', alignItems: 'center', justifyContent: 'flex-end', gap: 8 },
  shuffle: { width: TOUCH, height: TOUCH, alignItems: 'center', justifyContent: 'center' },
  play: { width: 56, height: 56, borderRadius: 28, alignItems: 'center', justifyContent: 'center', backgroundColor: colors.accent },
});
