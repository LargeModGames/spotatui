import type { ReactNode } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';

import { Cover } from '@/components/Cover';
import { Icon, type IconName } from '@/components/Icon';
import { colors, fonts } from '@/theme';

/** One list row: an optional rank, the cover, the title (accent while it plays) and a second line. */
export function TrackRow({
  image,
  title,
  subtitle,
  playing,
  rank,
  cover = true,
  round,
  fallback,
  onPress,
  children,
}: {
  image: string | null | undefined;
  title: string;
  subtitle?: string;
  playing?: boolean;
  rank?: number;
  cover?: boolean;
  round?: boolean;
  fallback?: IconName;
  onPress?: () => void;
  children?: ReactNode;
}) {
  return (
    <View style={styles.row}>
      <Pressable style={styles.main} onPress={onPress} disabled={!onPress} accessibilityRole="button" accessibilityLabel={title}>
        {rank !== undefined && <Text style={[styles.rank, playing && styles.accent]}>{rank}</Text>}
        {cover && <Cover uri={image} size={48} radius={round ? 24 : 4} fallback={fallback} />}
        <View style={styles.words}>
          <View style={styles.titleLine}>
            {playing && <Icon name="playing" size={14} color={colors.accent} />}
            <Text style={[styles.title, playing && styles.accent]} numberOfLines={1}>
              {title}
            </Text>
          </View>
          {!!subtitle && (
            <Text style={styles.subtitle} numberOfLines={1}>
              {subtitle}
            </Text>
          )}
        </View>
      </Pressable>
      {children}
    </View>
  );
}

const styles = StyleSheet.create({
  row: { height: 64, flexDirection: 'row', alignItems: 'center', paddingLeft: 16, paddingRight: 4 },
  main: { flex: 1, minWidth: 0, flexDirection: 'row', alignItems: 'center', gap: 12 },
  words: { flex: 1, minWidth: 0, gap: 3 },
  titleLine: { flexDirection: 'row', alignItems: 'center', gap: 6 },
  title: { flexShrink: 1, fontFamily: fonts.semibold, fontSize: 15, color: colors.text },
  accent: { color: colors.accent },
  rank: { width: 20, textAlign: 'center', fontFamily: fonts.medium, fontSize: 15, color: colors.muted },
  subtitle: { fontFamily: fonts.regular, fontSize: 13, color: colors.muted },
});
