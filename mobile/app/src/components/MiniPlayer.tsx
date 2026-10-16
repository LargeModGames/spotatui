import { router } from 'expo-router';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { Cover } from '@/components/Cover';
import { Icon } from '@/components/Icon';
import { useSend } from '@/core-context';
import { isSpotify, useElapsed, usePlayback } from '@/playback';
import { colors, fonts, TOUCH } from '@/theme';

/**
 * The bar above the tabs: the playing track, devices, like and play/pause. A screen
 * without tabs passes `edge`, so the bar clears the system navigation instead.
 */
export function MiniPlayer({ edge }: { edge?: boolean }) {
  const send = useSend();
  const insets = useSafeAreaInsets();
  const { playback, item } = usePlayback();
  if (!item) return null;
  return (
    <View style={[styles.bar, edge && { bottom: 8 + insets.bottom }]}>
      <Pressable
        style={styles.track}
        onPress={() => router.push('/now-playing')}
        accessibilityRole="button"
        accessibilityLabel={`Now playing: ${item.title}`}>
        <Cover uri={item.image_url} size={40} />
        <View style={styles.words}>
          <Text style={styles.title} numberOfLines={1}>
            {item.title}
          </Text>
          <Text style={styles.artist} numberOfLines={1}>
            {item.artists.join(', ')}
          </Text>
        </View>
      </Pressable>
      <Pressable style={styles.button} onPress={() => router.push('/devices')} accessibilityLabel="Connect to a device">
        <Icon name="devices" size={20} color={colors.muted} />
      </Pressable>
      {isSpotify(item.uri) && (
        <Pressable
          style={styles.button}
          onPress={() => send('ToggleSaveCurrentItem')}
          accessibilityLabel={playback?.liked ? 'Remove from Liked Songs' : 'Save to Liked Songs'}
          accessibilityState={{ selected: !!playback?.liked }}>
          <Icon name="heart" size={20} color={playback?.liked ? colors.accent : colors.muted} />
        </Pressable>
      )}
      <Pressable
        style={styles.button}
        onPress={() => send('TogglePlayback')}
        accessibilityLabel={item.is_playing ? 'Pause' : 'Play'}>
        <Icon name={item.is_playing ? 'pause' : 'play'} size={24} />
      </Pressable>
      <MiniProgress duration={item.duration_ms} />
    </View>
  );
}

/** The rail under the bar: the only part that follows the position. */
function MiniProgress({ duration }: { duration: number }) {
  const ms = useElapsed();
  const progress = ms !== null && duration > 0 ? Math.min(1, ms / duration) : 0;
  return (
    <View style={styles.rail}>
      <View style={[styles.fill, { width: `${progress * 100}%` }]} />
    </View>
  );
}

const styles = StyleSheet.create({
  bar: {
    position: 'absolute',
    left: 8,
    right: 8,
    bottom: 8,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 2,
    paddingVertical: 8,
    paddingLeft: 8,
    paddingRight: 4,
    borderRadius: 8,
    backgroundColor: colors.surface,
    boxShadow: '0 8px 24px rgba(0,0,0,0.5)',
  },
  track: { flex: 1, minWidth: 0, flexDirection: 'row', alignItems: 'center', gap: 10 },
  words: { flex: 1, minWidth: 0, gap: 1 },
  title: { fontFamily: fonts.bold, fontSize: 13, color: colors.text },
  artist: { fontFamily: fonts.regular, fontSize: 12, color: colors.muted },
  button: { width: TOUCH, height: TOUCH, alignItems: 'center', justifyContent: 'center' },
  rail: {
    position: 'absolute',
    left: 8,
    right: 8,
    bottom: 0,
    height: 2,
    borderRadius: 1,
    overflow: 'hidden',
    backgroundColor: 'rgba(255,255,255,0.25)',
  },
  fill: { height: '100%', backgroundColor: colors.text },
});

/** The space a scroll view leaves free under its content for the mini player. */
export const MINI_PLAYER_SPACE = 80;
