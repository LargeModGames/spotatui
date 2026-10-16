import { Image } from 'expo-image';
import { router, useFocusEffect, useLocalSearchParams } from 'expo-router';
import { useCallback, useState } from 'react';
import { Animated, Pressable, StyleSheet, Text, useWindowDimensions, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { playRequest } from '@gui/libraryModel';
import { albumKind, yearOf } from '@gui/pagesModel';

import { Icon } from '@/components/Icon';
import { MINI_PLAYER_SPACE, MiniPlayer } from '@/components/MiniPlayer';
import { Card, Shelf } from '@/components/Shelf';
import { TrackRow } from '@/components/TrackRow';
import { useChannel, useSend } from '@/core-context';
import { usePlayback } from '@/playback';
import { colors, fonts, TOUCH, type } from '@/theme';

const POPULAR = 5;
const POPULAR_MORE = 10;
const RELEASES = 4;

/** An artist by its Spotify id, laid out as in Spotify's app; `name` and `image` show while it loads. */
export default function ArtistScreen() {
  const { id, name, image } = useLocalSearchParams<{ id: string; name?: string; image?: string }>();
  const artist = useChannel('artist');
  const send = useSend();
  const { item } = usePlayback();
  const insets = useSafeAreaInsets();
  const { width } = useWindowDimensions();
  const [more, setMore] = useState(false);
  const [allReleases, setAllReleases] = useState(false);
  const fetched = artist?.page;
  const page = fetched?.id === id ? fetched : null;
  // The core keeps one artist, so a screen under a newer artist asks again when it shows.
  useFocusEffect(
    useCallback(() => {
      if (!page) send({ Open: { Artist: { id, name: name ?? '' } } });
    }, [page, id, name, send]),
  );
  const uri = `spotify:artist:${id}`;
  const playsHere = item?.context_uri === uri;
  const playing = playsHere && !!item?.is_playing;
  const top = page?.top_tracks ?? [];
  const popular = top.slice(0, more ? POPULAR_MORE : POPULAR);
  const albums = page?.albums ?? [];
  const releases = allReleases ? albums : albums.slice(0, RELEASES);
  const title = page?.name ?? name ?? 'Artist';
  const photo = page?.image_url ?? (image || null);
  const heroHeight = Math.round(width * 0.8);
  // As in Spotify's app, a bar with the name fades in when the photo scrolls away.
  const [scrollY] = useState(() => new Animated.Value(0));
  const barHeight = insets.top + 52;
  const barOpacity = scrollY.interpolate({
    inputRange: [heroHeight - barHeight - 60, heroHeight - barHeight],
    outputRange: [0, 1],
    extrapolate: 'clamp',
  });
  return (
    <View style={styles.screen}>
      <Animated.ScrollView
        contentContainerStyle={{ paddingBottom: MINI_PLAYER_SPACE + insets.bottom }}
        scrollEventThrottle={16}
        onScroll={Animated.event([{ nativeEvent: { contentOffset: { y: scrollY } } }], { useNativeDriver: true })}>
        <View style={[styles.hero, { height: heroHeight }]}>
          {photo && <Image source={{ uri: photo }} style={StyleSheet.absoluteFill} contentFit="cover" />}
          <View style={styles.fade} />
          <Text style={styles.name} numberOfLines={2}>
            {title}
          </Text>
        </View>
        {page ? (
          <View style={styles.actions}>
            <Pressable
              style={[styles.follow, page.following && styles.followOn]}
              onPress={() => send(page.following ? { UnfollowArtist: id } : { FollowArtist: id })}
              accessibilityRole="button"
              accessibilityState={{ selected: page.following }}>
              <Text style={styles.followText}>{page.following ? 'Following' : 'Follow'}</Text>
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
              onPress={() => (playsHere ? send('TogglePlayback') : send({ PlayContext: { uri, offset: null } }))}
              accessibilityLabel={playing ? `Pause ${title}` : `Play ${title}`}>
              <Icon name={playing ? 'pause' : 'play'} size={28} color={colors.onAccent} />
            </Pressable>
          </View>
        ) : (
          <Text style={[type.small, styles.loading]}>Loading…</Text>
        )}
        {popular.length > 0 && (
          <View>
            <Text style={[type.heading, styles.heading]}>Popular</Text>
            {popular.map((track, index) => (
              <TrackRow
                key={`${index}-${track.uri}`}
                rank={index + 1}
                image={track.image_url}
                title={track.name}
                playing={!!track.uri && track.uri === item?.uri}
                onPress={() => {
                  const action = playRequest(top, index);
                  if (action) send(action);
                }}
              />
            ))}
            {top.length > POPULAR && (
              <Pressable style={styles.textAction} onPress={() => setMore((shows) => !shows)} accessibilityRole="button">
                <Text style={styles.textActionLabel}>{more ? 'Show less' : 'See more'}</Text>
              </Pressable>
            )}
          </View>
        )}
        {releases.length > 0 && (
          <View>
            <Text style={[type.heading, styles.heading]}>Popular releases</Text>
            {releases.map((album, index) => {
              const year = yearOf(album.release_date);
              const kind = albumKind(album.album_type);
              return (
                <TrackRow
                  key={album.id ?? `${index}-${album.name}`}
                  image={album.image_url}
                  title={album.name}
                  subtitle={year ? `${year} · ${kind}` : kind}
                  onPress={() => album.id && router.push({ pathname: '/album/[id]', params: { id: album.id, name: album.name, image: album.image_url ?? '' } })}
                />
              );
            })}
            {albums.length > RELEASES && (
              <Pressable style={styles.pill} onPress={() => setAllReleases((shows) => !shows)} accessibilityRole="button">
                <Text style={styles.followText}>{allReleases ? 'Show less' : 'See discography'}</Text>
              </Pressable>
            )}
          </View>
        )}
        {!!page?.related_artists.length && (
          <Shelf title="Fans also like">
            {page.related_artists.map((artist, index) => (
              <Card
                key={artist.id ?? `${index}-${artist.name}`}
                image={artist.image_url}
                title={artist.name}
                subtitle="Artist"
                round
                fallback="speaker"
                onPress={() =>
                  artist.id && router.push({ pathname: '/artist/[id]', params: { id: artist.id, name: artist.name, image: artist.image_url ?? '' } })
                }
              />
            ))}
          </Shelf>
        )}
      </Animated.ScrollView>
      <Animated.View pointerEvents="none" style={[styles.bar, { height: barHeight, paddingTop: insets.top, opacity: barOpacity }]}>
        <Text style={styles.barTitle} numberOfLines={1}>
          {title}
        </Text>
      </Animated.View>
      <Pressable style={[styles.back, { top: insets.top + 8 }]} onPress={() => router.back()} accessibilityLabel="Back">
        <Icon name="back" size={22} />
      </Pressable>
      <MiniPlayer edge />
    </View>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.background },
  hero: { justifyContent: 'flex-end', backgroundColor: '#3a2a2a' },
  fade: {
    position: 'absolute',
    top: 0,
    right: 0,
    bottom: 0,
    left: 0,
    experimental_backgroundImage: `linear-gradient(transparent 45%, ${colors.background})`,
  },
  name: { margin: 16, fontFamily: fonts.extrabold, fontSize: 44, lineHeight: 48, letterSpacing: -1, color: colors.text },
  back: {
    position: 'absolute',
    left: 8,
    width: 36,
    height: 36,
    borderRadius: 18,
    alignItems: 'center',
    justifyContent: 'center',
    backgroundColor: 'rgba(0,0,0,0.5)',
  },
  bar: {
    position: 'absolute',
    top: 0,
    left: 0,
    right: 0,
    justifyContent: 'center',
    paddingLeft: 56,
    paddingRight: 16,
    backgroundColor: '#3a2a2a',
  },
  barTitle: { fontFamily: fonts.bold, fontSize: 16, color: colors.text },
  loading: { paddingHorizontal: 16 },
  actions: { flexDirection: 'row', alignItems: 'center', gap: 8, paddingHorizontal: 16 },
  spacer: { flex: 1 },
  follow: {
    height: 32,
    justifyContent: 'center',
    paddingHorizontal: 16,
    borderWidth: 1,
    borderColor: colors.muted,
    borderRadius: 16,
  },
  followOn: { borderColor: colors.text },
  followText: { fontFamily: fonts.bold, fontSize: 13, color: colors.text },
  icon: { width: TOUCH, height: TOUCH, alignItems: 'center', justifyContent: 'center' },
  play: { width: 56, height: 56, borderRadius: 28, alignItems: 'center', justifyContent: 'center', backgroundColor: colors.accent },
  heading: { paddingHorizontal: 16, paddingTop: 24, paddingBottom: 4 },
  textAction: { alignSelf: 'flex-start', minHeight: TOUCH, justifyContent: 'center', paddingHorizontal: 16 },
  textActionLabel: { fontFamily: fonts.bold, fontSize: 13, color: colors.muted },
  pill: {
    alignSelf: 'center',
    height: 32,
    justifyContent: 'center',
    marginTop: 12,
    paddingHorizontal: 16,
    borderWidth: 1,
    borderColor: colors.muted,
    borderRadius: 16,
  },
});
