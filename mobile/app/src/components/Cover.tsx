import { Image, type ImageStyle } from 'expo-image';
import { StyleSheet, View, type StyleProp } from 'react-native';

import { Icon, type IconName } from '@/components/Icon';
import { colors } from '@/theme';

/** A cover image, or a plain tile with a glyph when there is none. */
export function Cover({
  uri,
  size,
  radius = 4,
  fallback = 'library',
  style,
}: {
  uri: string | null | undefined;
  size: number;
  radius?: number;
  fallback?: IconName;
  style?: StyleProp<ImageStyle>;
}) {
  const box = { width: size, height: size, borderRadius: radius };
  if (!uri) {
    return (
      <View style={[styles.empty, box, style]}>
        <Icon name={fallback} size={size * 0.4} color={colors.dim} />
      </View>
    );
  }
  return <Image source={{ uri }} style={[box, style]} contentFit="cover" transition={150} />;
}

const styles = StyleSheet.create({
  empty: { backgroundColor: colors.surface, alignItems: 'center', justifyContent: 'center' },
});
