import { router } from 'expo-router';
import { useEffect } from 'react';
import { Pressable, StyleSheet, Text, useWindowDimensions, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { upNext } from '@gui/libraryModel';

import { SeekBar, Transport } from '@/components/Controls';
import { Cover } from '@/components/Cover';
import { Icon } from '@/components/Icon';
import { useChannel, useSend } from '@/core-context';
import { isSpotify, playingFrom, usePlayback } from '@/playback';
import { colors, fonts, TOUCH, type } from '@/theme';

export default function NowPlayingScreen() {
  const queue = useChannel('queue');
  const send = useSend();
  const { playback, item } = usePlayback();
  const { width } = useWindowDimensions();
  const uri = item?.uri ?? null;
  // The queue is not pushed on its own: ask on open and on every track change.
  useEffect(() => {
    send('RefreshQueue');
  }, [uri, send]);
  const next = upNext(queue, isSpotify(uri))[0];
  const from = playingFrom(item);
  return (
    <SafeAreaView style={styles.screen} edges={['top', 'bottom']}>
      <View style={styles.header}>
        <Pressable style={styles.icon} onPress={() => router.back()} accessibilityLabel="Close now playing">
          <Icon name="down" size={26} />
        </Pressable>
        <View style={styles.from}>
          <Text style={type.label}>{from.kind}</Text>
          {!!from.name && (
            <Text style={styles.fromName} numberOfLines={1}>
              {from.name}
            </Text>
          )}
        </View>
        <View style={styles.icon} />
      </View>
      {item ? (
        <>
          <Cover uri={item.image_url} size={width - 48} radius={8} style={styles.cover} />
          <View style={styles.titleRow}>
            <View style={styles.words}>
              <Text style={styles.title} numberOfLines={1}>
                {item.title}
              </Text>
              <Text style={styles.artist} numberOfLines={1}>
                {item.artists.join(', ')}
              </Text>
            </View>
            {isSpotify(item.uri) && (
              <Pressable
                style={styles.icon}
                onPress={() => send('ToggleSaveCurrentItem')}
                accessibilityLabel={playback?.liked ? 'Remove from Liked Songs' : 'Save to Liked Songs'}>
                <Icon name="heart" size={24} color={playback?.liked ? colors.accent : colors.muted} />
              </Pressable>
            )}
          </View>
          <SeekBar />
          <Transport />
          <View style={styles.tools}>
            <Pressable style={styles.device} onPress={() => router.push('/devices')} accessibilityLabel="Connect to a device">
              <Icon name="devices" size={20} color={colors.accent} />
              <Text style={styles.deviceName} numberOfLines={1}>
                {(playback?.device ?? 'No device').toUpperCase()}
              </Text>
            </Pressable>
            <View style={styles.toolButtons}>
              <Pressable style={styles.icon} onPress={() => router.push('/lyrics')} accessibilityLabel="Lyrics">
                <Icon name="lyrics" color={colors.muted} />
              </Pressable>
              <Pressable style={styles.icon} onPress={() => router.push('/queue')} accessibilityLabel="Queue">
                <Icon name="queue" color={colors.muted} />
              </Pressable>
            </View>
          </View>
          {next && (
            <Pressable style={styles.next} onPress={() => router.push('/queue')} accessibilityRole="button">
              <Cover uri={next.image_url} size={40} />
              <View style={styles.words}>
                <Text style={type.label}>UP NEXT</Text>
                <Text style={styles.nextTitle} numberOfLines={1}>
                  {next.name}
                  <Text style={styles.nextArtist}> · {next.artists.join(', ')}</Text>
                </Text>
              </View>
              <Icon name="forward" size={18} color={colors.muted} />
            </Pressable>
          )}
        </>
      ) : (
        <Text style={[type.body, styles.nothing]}>Nothing is playing</Text>
      )}
    </SafeAreaView>
  );
}

const styles = StyleSheet.create({
  screen: {
    flex: 1,
    paddingHorizontal: 24,
    experimental_backgroundImage: 'linear-gradient(180deg, #24474c 0%, #000000 72%)',
  },
  header: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', marginHorizontal: -12, marginTop: 8 },
  icon: { width: TOUCH, height: TOUCH, alignItems: 'center', justifyContent: 'center' },
  from: { flex: 1, alignItems: 'center', gap: 2 },
  fromName: { fontFamily: fonts.bold, fontSize: 13, color: colors.text },
  cover: { marginTop: 20, boxShadow: '0 16px 48px rgba(0,0,0,0.6)' },
  titleRow: { flexDirection: 'row', alignItems: 'center', gap: 12, marginTop: 28, marginBottom: 12, marginRight: -10 },
  words: { flex: 1, minWidth: 0, gap: 4 },
  title: { fontFamily: fonts.extrabold, fontSize: 24, letterSpacing: -0.5, color: colors.text },
  artist: { fontFamily: fonts.regular, fontSize: 16, color: colors.muted },
  tools: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', marginTop: 14, marginHorizontal: -10 },
  device: { height: TOUCH, flexShrink: 1, flexDirection: 'row', alignItems: 'center', gap: 8, paddingHorizontal: 10 },
  deviceName: { flexShrink: 1, fontFamily: fonts.mono, fontSize: 11, letterSpacing: 0.9, color: colors.accent },
  toolButtons: { flexDirection: 'row' },
  next: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    marginTop: 16,
    padding: 12,
    borderRadius: 8,
    backgroundColor: colors.tile,
  },
  nextTitle: { fontFamily: fonts.bold, fontSize: 14, color: colors.text },
  nextArtist: { fontFamily: fonts.regular, color: colors.muted },
  nothing: { marginTop: 40, textAlign: 'center' },
});
