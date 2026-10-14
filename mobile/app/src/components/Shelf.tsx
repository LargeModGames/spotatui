import type { ReactNode } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';

import { Cover } from '@/components/Cover';
import type { IconName } from '@/components/Icon';
import { type } from '@/theme';

/** A titled row of cards that scrolls sideways. */
export function Shelf({ title, children }: { title: string; children: ReactNode }) {
  return (
    <View style={styles.shelf}>
      <Text style={[type.heading, styles.title]}>{title}</Text>
      <ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={styles.cards}>
        {children}
      </ScrollView>
    </View>
  );
}

export function Card({
  image,
  title,
  subtitle,
  round,
  fallback,
  onPress,
}: {
  image: string | null;
  title: string;
  subtitle: string;
  round?: boolean;
  fallback?: IconName;
  onPress: () => void;
}) {
  return (
    <Pressable style={styles.card} onPress={onPress} accessibilityRole="button" accessibilityLabel={title}>
      <Cover uri={image} size={CARD} radius={round ? CARD / 2 : 4} fallback={fallback} />
      <Text style={[type.bodyBold, round && styles.center]} numberOfLines={1}>
        {title}
      </Text>
      <Text style={[type.small, round && styles.center]} numberOfLines={2}>
        {subtitle}
      </Text>
    </Pressable>
  );
}

const CARD = 136;

const styles = StyleSheet.create({
  shelf: { gap: 12, marginTop: 28 },
  title: { paddingHorizontal: 16 },
  cards: { gap: 12, paddingHorizontal: 16 },
  card: { width: CARD, gap: 6 },
  center: { textAlign: 'center' },
});
