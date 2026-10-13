import { router } from 'expo-router';
import { useState } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { Icon } from '@/components/Icon';
import { useSend } from '@/core-context';
import { clock, usePlayback } from '@/playback';
import { colors, fonts, TOUCH } from '@/theme';

/** A filter chip. */
export function Chip({ label, on, onPress }: { label: string; on: boolean; onPress: () => void }) {
  return (
    <Pressable
      style={[styles.chip, on && styles.chipOn]}
      onPress={onPress}
      accessibilityRole="button"
      accessibilityState={{ selected: on }}>
      <Text style={[styles.chipText, on && styles.chipTextOn]}>{label}</Text>
    </Pressable>
  );
}

/** The header of a full-screen panel: a close chevron and a centered title. */
export function PanelHeader({ title, subtitle }: { title: string; subtitle?: string }) {
  return (
    <SafeAreaView edges={['top']} style={styles.header}>
      <Pressable style={styles.icon} onPress={() => router.back()} accessibilityLabel={`Close ${title}`}>
        <Icon name="down" size={26} />
      </Pressable>
      <View style={styles.headerWords}>
        <Text style={styles.headerTitle} numberOfLines={1}>
          {title}
        </Text>
        {!!subtitle && (
          <Text style={styles.headerSubtitle} numberOfLines={1}>
            {subtitle}
          </Text>
        )}
      </View>
      <View style={styles.icon} />
    </SafeAreaView>
  );
}

/** Previous, play/pause and next, with shuffle and repeat around them unless `compact`. */
export function Transport({ compact, light }: { compact?: boolean; light?: boolean }) {
  const send = useSend();
  const { item } = usePlayback();
  const playing = item?.is_playing ?? false;
  const repeat = item?.repeat ?? 'off';
  return (
    <View style={[styles.transport, compact && styles.compact]}>
      {!compact && (
        <Pressable
          style={styles.icon}
          onPress={() => send('ToggleShuffle')}
          accessibilityLabel="Shuffle"
          accessibilityState={{ selected: !!item?.shuffle }}>
          <Icon name="shuffle" color={item?.shuffle ? colors.accent : colors.muted} />
        </Pressable>
      )}
      <Pressable style={styles.skip} onPress={() => send('PreviousTrack')} accessibilityLabel="Previous">
        <Icon name="previous" size={30} />
      </Pressable>
      <Pressable
        style={[styles.play, light && styles.playLight]}
        onPress={() => send('TogglePlayback')}
        accessibilityLabel={playing ? 'Pause' : 'Play'}>
        <Icon name={playing ? 'pause' : 'play'} size={30} color={light ? '#2a213d' : colors.onAccent} />
      </Pressable>
      <Pressable style={styles.skip} onPress={() => send('NextTrack')} accessibilityLabel="Next">
        <Icon name="next" size={30} />
      </Pressable>
      {!compact && (
        <Pressable
          style={styles.icon}
          onPress={() => send('CycleRepeat')}
          accessibilityLabel="Repeat"
          accessibilityState={{ selected: repeat !== 'off' }}>
          <Icon name={repeat === 'track' ? 'repeatOne' : 'repeat'} color={repeat !== 'off' ? colors.accent : colors.muted} />
        </Pressable>
      )}
    </View>
  );
}

/** The seek bar: it follows the finger and seeks on release. */
export function SeekBar() {
  const send = useSend();
  const { item, ms, stale } = usePlayback();
  const [width, setWidth] = useState(0);
  const [drag, setDrag] = useState<number | null>(null);
  const duration = item?.duration_ms ?? 0;
  const seekable = !!item && !item.is_live && duration > 0;
  const shown = drag ?? (stale ? 0 : Math.min(ms, duration));
  const ratio = duration > 0 ? shown / duration : 0;
  const at = (x: number) => Math.round(Math.min(1, Math.max(0, x / Math.max(1, width))) * duration);
  return (
    <View style={styles.seek}>
      <View
        style={styles.track}
        onLayout={(event) => setWidth(event.nativeEvent.layout.width)}
        onStartShouldSetResponder={() => seekable}
        onResponderGrant={(event) => setDrag(at(event.nativeEvent.locationX))}
        onResponderMove={(event) => setDrag(at(event.nativeEvent.locationX))}
        onResponderRelease={() => {
          if (drag !== null) send({ SeekTo: drag });
          setDrag(null);
        }}
        onResponderTerminate={() => setDrag(null)}
        accessibilityRole="adjustable"
        accessibilityLabel="Seek"
        accessibilityValue={{ text: `${clock(shown)} of ${clock(duration)}` }}>
        {/* Only the track takes touches, so locationX is always relative to it. */}
        <View pointerEvents="none" style={styles.rail} />
        <View pointerEvents="none" style={[styles.fill, { width: `${ratio * 100}%` }]} />
        <View pointerEvents="none" style={[styles.thumb, { left: `${ratio * 100}%` }]} />
      </View>
      <View style={styles.times}>
        <Text style={styles.time}>{stale ? '--:--' : clock(shown)}</Text>
        <Text style={styles.time}>{item?.is_live ? 'LIVE' : clock(duration)}</Text>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  chip: { height: 32, justifyContent: 'center', paddingHorizontal: 14, borderRadius: 16, backgroundColor: colors.sheet },
  chipOn: { backgroundColor: colors.accent },
  chipText: { fontFamily: fonts.medium, fontSize: 14, color: colors.text },
  chipTextOn: { color: colors.onAccent, fontFamily: fonts.bold },
  header: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', paddingHorizontal: 12, paddingTop: 8 },
  headerWords: { flex: 1, alignItems: 'center', gap: 2 },
  headerTitle: { fontFamily: fonts.bold, fontSize: 14, color: colors.text },
  headerSubtitle: { fontFamily: fonts.regular, fontSize: 12, color: colors.muted },
  icon: { width: TOUCH, height: TOUCH, alignItems: 'center', justifyContent: 'center' },
  transport: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between' },
  compact: { justifyContent: 'center', gap: 32 },
  skip: { width: 56, height: 56, alignItems: 'center', justifyContent: 'center' },
  play: {
    width: 68,
    height: 68,
    borderRadius: 34,
    alignItems: 'center',
    justifyContent: 'center',
    backgroundColor: colors.accent,
    boxShadow: '0 8px 16px rgba(0,0,0,0.3)',
  },
  playLight: { width: 60, height: 60, borderRadius: 30, backgroundColor: colors.text },
  seek: { gap: 4 },
  track: { height: 28, justifyContent: 'center' },
  rail: { position: 'absolute', left: 0, right: 0, height: 4, borderRadius: 2, backgroundColor: colors.rail },
  fill: { position: 'absolute', left: 0, height: 4, borderRadius: 2, backgroundColor: colors.text },
  thumb: {
    position: 'absolute',
    width: 12,
    height: 12,
    marginLeft: -6,
    borderRadius: 6,
    backgroundColor: colors.text,
    boxShadow: '0 2px 4px rgba(0,0,0,0.5)',
  },
  times: { flexDirection: 'row', justifyContent: 'space-between' },
  time: { fontFamily: fonts.regular, fontSize: 12, color: colors.muted, fontVariant: ['tabular-nums'] },
});
