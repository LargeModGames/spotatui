import { useCallback, useEffect, useRef } from 'react';
import { ScrollView, StyleSheet, Text, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { activeLyric } from '@gui/roomModel';

import { PanelHeader, SeekBar, Transport } from '@/components/Controls';
import { useChannel } from '@/core-context';
import { usePlayback, usePositionSelect } from '@/playback';
import { colors, fonts, type } from '@/theme';

const BACKGROUND = '#4a3b6b';

export default function LyricsScreen() {
  const lyrics = useChannel('lyrics');
  const { item } = usePlayback();
  const lines = lyrics?.status === 'found' ? lyrics.lines : [];
  const line = useCallback(
    (ms: number | null) => (lyrics?.synced && lyrics.status === 'found' ? activeLyric(lyrics.lines, ms ?? 0) : -1),
    [lyrics],
  );
  const current = usePositionSelect(line);
  const scroll = useRef<ScrollView>(null);
  const offsets = useRef<number[]>([]);
  // Keep the current line a third of the way down the view.
  useEffect(() => {
    const y = offsets.current[current];
    if (y !== undefined) scroll.current?.scrollTo({ y: Math.max(0, y - 160), animated: true });
  }, [current]);
  return (
    <View style={styles.screen}>
      <PanelHeader title={item?.title ?? 'Lyrics'} subtitle={item?.artists.join(', ')} />
      <ScrollView ref={scroll} contentContainerStyle={styles.lines}>
        {note(lyrics?.status, lyrics?.synced ?? false, lines.length) && (
          <Text style={[type.body, styles.note]}>{note(lyrics?.status, lyrics?.synced ?? false, lines.length)}</Text>
        )}
        {lines.map((line, index) => (
          <Text
            key={index}
            onLayout={(event) => {
              offsets.current[index] = event.nativeEvent.layout.y;
            }}
            style={[
              styles.line,
              current >= 0 && (index < current ? styles.past : index === current ? styles.now : styles.future),
            ]}>
            {line.text || '♪'}
          </Text>
        ))}
      </ScrollView>
      <SafeAreaView edges={['bottom']} style={styles.footer}>
        <SeekBar />
        <Transport compact light />
      </SafeAreaView>
    </View>
  );
}

/** What the page says instead of, or above, the lines. */
function note(status: string | undefined, synced: boolean, count: number): string | null {
  if (status === 'loading' || status === 'not_started' || status === undefined) return 'Looking for lyrics…';
  if (status === 'not_found' || count === 0) return 'No lyrics for this song.';
  if (!synced) return 'These lyrics are not synced to the song.';
  return null;
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: BACKGROUND },
  lines: { gap: 14, paddingHorizontal: 24, paddingTop: 28, paddingBottom: 160 },
  note: { color: 'rgba(255,255,255,0.75)' },
  line: { fontFamily: fonts.bold, fontSize: 24, lineHeight: 32, letterSpacing: -0.2, color: colors.text },
  past: { color: 'rgba(255,255,255,0.7)' },
  now: { color: colors.text },
  future: { color: 'rgba(0,0,0,0.55)' },
  footer: {
    gap: 10,
    paddingHorizontal: 24,
    paddingTop: 16,
    paddingBottom: 12,
    backgroundColor: BACKGROUND,
    boxShadow: `0 -16px 24px ${BACKGROUND}`,
  },
});
