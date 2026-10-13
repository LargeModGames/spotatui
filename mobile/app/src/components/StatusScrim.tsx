import { StyleSheet, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

/** A backdrop behind the status bar, so scrolled content does not run under the clock. */
export function StatusScrim() {
  const insets = useSafeAreaInsets();
  return <View pointerEvents="none" style={[styles.scrim, { height: insets.top }]} />;
}

const styles = StyleSheet.create({
  scrim: { position: 'absolute', top: 0, left: 0, right: 0, backgroundColor: 'rgba(18,18,18,0.92)' },
});
