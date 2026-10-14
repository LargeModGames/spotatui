import { router, useFocusEffect, useLocalSearchParams } from 'expo-router';
import { Fragment, useCallback } from 'react';
import { FlatList, Pressable, StyleSheet, Text, View } from 'react-native';
import { SafeAreaView, useSafeAreaInsets } from 'react-native-safe-area-context';

import { albumKind, albumPlay, lengthLine, releaseLine, yearOf } from '@gui/pagesModel';

import { Cover } from '@/components/Cover';
import { Icon } from '@/components/Icon';
import { MINI_PLAYER_SPACE, MiniPlayer } from '@/components/MiniPlayer';
import { TrackRow } from '@/components/TrackRow';
import { useCore, useSend } from '@/core-context';
import { usePlayback } from '@/playback';
import { colors, fonts, TOUCH, type } from '@/theme';

/** An album by its Spotify id, laid out as in Spotify's app; `name` and `image` show while it loads. */
export default function AlbumScreen() {
  const { id, name, image } = useLocalSearchParams<{ id: string; name?: string; image?: string }>();
  const { channels } = useCore();
  const send = useSend();
  const { item } = usePlayback();
  const insets = useSafeAreaInsets();
  const payload = channels.album?.payload;
  const album = payload?.album?.id === id ? payload.album : null;
  // The core keeps one album, so a screen under a newer album asks again when it shows.
  useFocusEffect(
    useCallback(() => {
      if (!album) send({ Open: { Album: { id, from_search: false } } });
    }, [album, id, send]),
  );
  const tracks = album?.tracks ?? [];
  const saved = !!payload?.saved && !!album;
  const playsHere = !!album?.uri && item?.context_uri === album.uri;
  const playing = playsHere && !!item?.is_playing;
  const year = yearOf(album?.release_date ?? null);
  const released = releaseLine(album?.release_date ?? null);
  const play = (index: number) => {
    const action = album && albumPlay(album, index);
    if (action) send(action);
  };
  return (
    <View style={styles.screen}>
      <FlatList
        data={tracks}
        keyExtractor={(track, index) => `${index}-${track.uri}`}
        contentContainerStyle={{ paddingBottom: MINI_PLAYER_SPACE + insets.bottom }}
        ListHeaderComponent={
          <SafeAreaView edges={['top']} style={styles.header}>
            <Pressable style={styles.back} onPress={() => router.back()} accessibilityLabel="Back">
              <Icon name="back" size={24} />
            </Pressable>
            <Cover uri={album?.image_url ?? (image || null)} size={240} style={styles.cover} />
            <View style={styles.words}>
              <Text style={styles.name}>{album?.name ?? name ?? 'Loading…'}</Text>
              {album && (
                <Text style={styles.artists} numberOfLines={2}>
                  {album.artists.map((artist, index) => (
                    <Fragment key={`${index}-${artist.name}`}>
                      {index > 0 && ', '}
                      <Text
                        onPress={() =>
                          artist.id &&
                          router.push({ pathname: '/artist/[id]', params: { id: artist.id, name: artist.name } })
                        }>
                        {artist.name}
                      </Text>
                    </Fragment>
                  ))}
                </Text>
              )}
              {album && <Text style={type.small}>{[albumKind(album.album_type), year].filter(Boolean).join(' · ')}</Text>}
            </View>
            {album && (
              <View style={styles.actions}>
                <Pressable
                  style={styles.icon}
                  onPress={() => send(saved ? { UnsaveAlbum: id } : { SaveAlbum: id })}
                  accessibilityLabel={saved ? 'Remove from Your Library' : 'Save to Your Library'}
                  accessibilityState={{ selected: saved }}>
                  <Icon name={saved ? 'saved' : 'addCircle'} size={26} color={saved ? colors.accent : colors.muted} />
                </Pressable>
                <View style={styles.spacer} />
                <Pressable
                  style={styles.icon}
                  onPress={() => send({ SetShuffle: !item?.shuffle })}
                  accessibilityLabel="Shuffle"
                  accessibilityState={{ selected: !!item?.shuffle }}>
                  <Icon name="shuffle" size={24} color={item?.shuffle ? colors.accent : colors.muted} />
                </Pressable>
                <Pressable
                  style={styles.play}
                  disabled={tracks.length === 0}
                  onPress={() => (playsHere ? send('TogglePlayback') : play(0))}
                  accessibilityLabel={playing ? `Pause ${album.name}` : `Play ${album.name}`}>
                  <Icon name={playing ? 'pause' : 'play'} size={28} color={colors.onAccent} />
                </Pressable>
              </View>
            )}
          </SafeAreaView>
        }
        renderItem={({ item: track, index }) => (
          <TrackRow
            image={null}
            cover={false}
            title={track.name}
            subtitle={track.artists.join(', ')}
            playing={!!track.uri && track.uri === item?.uri}
            onPress={() => play(index)}
          />
        )}
        ListFooterComponent={
          album ? (
            <View style={styles.footer}>
              {released && <Text style={styles.released}>{released}</Text>}
              <Text style={type.small}>{lengthLine(tracks)}</Text>
            </View>
          ) : null
        }
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
    experimental_backgroundImage: `linear-gradient(#2a3140, ${colors.background} 380px)`,
  },
  back: { width: TOUCH, height: TOUCH, marginLeft: -12, alignItems: 'center', justifyContent: 'center' },
  cover: { alignSelf: 'center', boxShadow: '0 12px 32px rgba(0,0,0,0.5)' },
  words: { gap: 8 },
  name: { fontFamily: fonts.extrabold, fontSize: 24, letterSpacing: -0.5, color: colors.text },
  artists: { fontFamily: fonts.bold, fontSize: 13, color: colors.text },
  actions: { flexDirection: 'row', alignItems: 'center', gap: 8 },
  spacer: { flex: 1 },
  icon: { width: TOUCH, height: TOUCH, alignItems: 'center', justifyContent: 'center' },
  play: { width: 56, height: 56, borderRadius: 28, alignItems: 'center', justifyContent: 'center', backgroundColor: colors.accent },
  footer: { gap: 4, paddingHorizontal: 16, paddingTop: 16 },
  released: { fontFamily: fonts.medium, fontSize: 13, color: colors.text },
});
