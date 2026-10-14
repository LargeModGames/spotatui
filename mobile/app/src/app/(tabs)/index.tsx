import { router } from 'expo-router';
import { useEffect, useRef } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { asksDiscover, greeting, madeForYou, recentAlbums, SHELF_MAX } from '@gui/pagesModel';

import { Cover } from '@/components/Cover';
import { Icon } from '@/components/Icon';
import { MINI_PLAYER_SPACE, MiniPlayer } from '@/components/MiniPlayer';
import { Card, Shelf } from '@/components/Shelf';
import { StatusScrim } from '@/components/StatusScrim';
import { useCore, useSend } from '@/core-context';
import { colors, fonts, type } from '@/theme';

/** Liked Songs plus five playlists fill the 2x3 grid. */
const GRID_PLAYLISTS = 5;

export default function HomeScreen() {
  const { channels, connected } = useCore();
  const send = useSend();
  const spotify = channels.liked?.payload.available ?? false;
  const discover = channels.discover?.payload ?? null;
  // Home asks for its data once a Spotify session is known, as the browser does.
  const asked = useRef(false);
  useEffect(() => {
    if (!spotify || asked.current) return;
    asked.current = true;
    send({ Navigate: 'RecentlyPlayed' });
  }, [spotify, send]);
  // Made For You is asked for once; a reconnect may ask again, as on the browser's Home.
  const askedDiscover = useRef(false);
  useEffect(() => {
    if (!connected) askedDiscover.current = false;
    if (askedDiscover.current || !asksDiscover(discover, connected)) return;
    askedDiscover.current = true;
    send({ OpenLibrary: 'Discover' });
  }, [connected, discover, send]);
  const playlists = (channels.library?.payload.spotify ?? []).slice(0, GRID_PLAYLISTS);
  const recent = recentAlbums(channels.recently_played?.payload ?? []).slice(0, SHELF_MAX);
  const forYou = madeForYou(discover).slice(0, SHELF_MAX);
  return (
    <View style={styles.screen}>
      <ScrollView contentContainerStyle={{ paddingBottom: MINI_PLAYER_SPACE }}>
        <SafeAreaView edges={['top']} style={styles.top}>
          <Text style={styles.wordmark}>
            spota<Text style={styles.accent}>tui</Text>
          </Text>
          <View style={styles.titleRow}>
            <Text style={type.title}>{greeting(new Date().getHours())}</Text>
            <View style={styles.source}>
              <View style={styles.sourceDot} />
              <Text style={styles.sourceText}>SPOTIFY</Text>
            </View>
          </View>
          <View style={styles.grid}>
            <Tile title="Liked Songs" onPress={() => router.push({ pathname: '/playlist/[id]', params: { id: 'liked' } })}>
              <View style={styles.likedArt}>
                <Icon name="heart" size={22} />
              </View>
            </Tile>
            {playlists.map((playlist) => (
              <Tile
                key={playlist.uri}
                title={playlist.name}
                onPress={() => router.push({ pathname: '/playlist/[id]', params: { id: playlist.uri } })}>
                <Cover uri={playlist.image_url} size={56} radius={0} />
              </Tile>
            ))}
          </View>
        </SafeAreaView>
        {recent.length > 0 && (
          <Shelf title="Recently played">
            {recent.map((track) => (
              <Card
                key={track.album_id}
                image={track.image_url}
                title={track.album}
                subtitle={track.artists.join(', ')}
                onPress={() => router.push({ pathname: '/album/[id]', params: { id: track.album_id!, name: track.album, image: track.image_url ?? '' } })}
              />
            ))}
          </Shelf>
        )}
        {forYou.length > 0 && (
          <Shelf title="Made For You">
            {forYou.map((playlist) => (
              <Card
                key={playlist.uri}
                image={playlist.image_url}
                title={playlist.name}
                subtitle={playlist.description ?? 'By Spotify'}
                onPress={() =>
                  router.push({
                    pathname: '/playlist/[id]',
                    params: { id: playlist.uri, name: playlist.name, image: playlist.image_url ?? '' },
                  })
                }
              />
            ))}
          </Shelf>
        )}
      </ScrollView>
      <StatusScrim />
      <MiniPlayer />
    </View>
  );
}

function Tile({ title, onPress, children }: { title: string; onPress: () => void; children: React.ReactNode }) {
  return (
    <Pressable style={styles.tile} onPress={onPress} accessibilityRole="button" accessibilityLabel={title}>
      {children}
      <Text style={styles.tileTitle} numberOfLines={2}>
        {title}
      </Text>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.background },
  top: {
    gap: 16,
    paddingHorizontal: 16,
    paddingTop: 12,
    experimental_backgroundImage: `linear-gradient(#3a2e1f, ${colors.background})`,
  },
  wordmark: { fontFamily: fonts.extrabold, fontSize: 22, letterSpacing: -0.6, color: colors.text },
  accent: { color: colors.accent, textShadowColor: 'rgba(240,168,58,0.4)', textShadowRadius: 10 },
  titleRow: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', gap: 12 },
  source: {
    height: 32,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
    paddingHorizontal: 10,
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 4,
  },
  sourceDot: { width: 9, height: 9, backgroundColor: colors.text },
  sourceText: { fontFamily: fonts.mono, fontSize: 11, letterSpacing: 0.7, color: colors.text },
  grid: { flexDirection: 'row', flexWrap: 'wrap', gap: 8 },
  tile: {
    width: '48.8%',
    height: 56,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
    borderRadius: 4,
    overflow: 'hidden',
    backgroundColor: colors.tile,
  },
  likedArt: {
    width: 56,
    height: 56,
    alignItems: 'center',
    justifyContent: 'center',
    experimental_backgroundImage: 'linear-gradient(135deg, #4b3a8f, #a7c4c8)',
  },
  tileTitle: { flex: 1, paddingRight: 8, fontFamily: fonts.bold, fontSize: 13, color: colors.text },
});
