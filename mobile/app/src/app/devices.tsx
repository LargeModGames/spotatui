import { useEffect, useState } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';

import type { DeviceInfo } from '@gui/bindings/DeviceInfo';

import { Icon, type IconName } from '@/components/Icon';
import { useCore, useSend } from '@/core-context';
import { usePlayback } from '@/playback';
import { colors, fonts, TOUCH } from '@/theme';

export default function DevicesSheet() {
  const { channels } = useCore();
  const send = useSend();
  const { playback } = usePlayback();
  useEffect(() => {
    send('RefreshDevices');
  }, [send]);
  const devices = channels.devices?.payload ?? [];
  const active = devices.find((device) => device.is_active);
  const others = devices.filter((device) => !device.is_active);
  const volume = playback?.volume ?? active?.volume_percent ?? 0;
  return (
    <View style={styles.sheet}>
      <Text style={styles.title}>Connect to a device</Text>
      <View style={styles.current}>
        <View style={styles.currentRow}>
          <Icon name={iconFor(active?.kind)} size={28} color={colors.accent} />
          <View style={styles.words}>
            <Text style={styles.currentLabel}>Current device</Text>
            <Text style={styles.currentName} numberOfLines={1}>
              {active?.name ?? playback?.device ?? 'None'}
            </Text>
          </View>
        </View>
        <Volume value={volume} onChange={(percent) => send({ SetVolume: percent })} />
      </View>
      {others.length > 0 && <Text style={styles.subtitle}>Select another device</Text>}
      <ScrollView style={styles.list}>
        {others.map((device, index) => (
          <Pressable
            key={device.id ?? `${index}-${device.name}`}
            style={styles.device}
            disabled={!device.id}
            onPress={() => device.id && send({ TransferPlayback: { device_id: device.id, persist: false } })}
            accessibilityRole="button"
            accessibilityLabel={`Play on ${device.name}`}>
            <Icon name={iconFor(device.kind)} size={24} color={colors.muted} />
            <View style={styles.words}>
              <Text style={styles.deviceName} numberOfLines={1}>
                {device.name}
              </Text>
              <Text style={styles.kind}>{device.kind}</Text>
            </View>
          </Pressable>
        ))}
      </ScrollView>
    </View>
  );
}

/** The volume of the playing device; it follows the finger and is sent on release. */
function Volume({ value, onChange }: { value: number; onChange: (percent: number) => void }) {
  const [width, setWidth] = useState(0);
  const [drag, setDrag] = useState<number | null>(null);
  const shown = drag ?? value;
  const at = (x: number) => Math.round(Math.min(1, Math.max(0, x / Math.max(1, width))) * 100);
  return (
    <View style={styles.volume}>
      <Pressable style={styles.mute} onPress={() => onChange(shown > 0 ? 0 : 50)} accessibilityLabel={shown > 0 ? 'Mute' : 'Unmute'}>
        <Icon name={shown > 0 ? 'volume' : 'mute'} size={20} color={colors.muted} />
      </Pressable>
      <View
        style={styles.slider}
        onLayout={(event) => setWidth(event.nativeEvent.layout.width)}
        onStartShouldSetResponder={() => true}
        onResponderGrant={(event) => setDrag(at(event.nativeEvent.locationX))}
        onResponderMove={(event) => setDrag(at(event.nativeEvent.locationX))}
        onResponderRelease={() => {
          if (drag !== null) onChange(drag);
          setDrag(null);
        }}
        onResponderTerminate={() => setDrag(null)}
        accessibilityRole="adjustable"
        accessibilityLabel="Volume"
        accessibilityValue={{ min: 0, max: 100, now: shown }}>
        <View pointerEvents="none" style={styles.rail} />
        <View pointerEvents="none" style={[styles.fill, { width: `${shown}%` }]} />
        <View pointerEvents="none" style={[styles.thumb, { left: `${shown}%` }]} />
      </View>
      <Text style={styles.percent}>{shown}%</Text>
    </View>
  );
}

function iconFor(kind: DeviceInfo['kind'] | undefined): IconName {
  switch (kind) {
    case 'smartphone':
    case 'tablet':
      return 'phone';
    case 'computer':
      return 'computer';
    case 'tv':
    case 'castvideo':
    case 'stb':
      return 'tv';
    default:
      return 'speaker';
  }
}

const styles = StyleSheet.create({
  sheet: { paddingHorizontal: 16, paddingTop: 20, paddingBottom: 36, backgroundColor: colors.sheet },
  title: { fontFamily: fonts.extrabold, fontSize: 20, letterSpacing: -0.2, color: colors.text },
  current: { gap: 4, marginTop: 16, padding: 14, paddingBottom: 6, borderRadius: 8, backgroundColor: colors.surface },
  currentRow: { flexDirection: 'row', alignItems: 'center', gap: 14 },
  words: { flex: 1, minWidth: 0, gap: 2 },
  currentLabel: { fontFamily: fonts.bold, fontSize: 13, color: colors.accent },
  currentName: { fontFamily: fonts.bold, fontSize: 16, color: colors.text },
  volume: { flexDirection: 'row', alignItems: 'center', gap: 6, marginLeft: -6 },
  mute: { width: TOUCH, height: TOUCH, alignItems: 'center', justifyContent: 'center' },
  slider: { flex: 1, height: 28, justifyContent: 'center' },
  rail: { position: 'absolute', left: 0, right: 0, height: 4, borderRadius: 2, backgroundColor: colors.rail },
  fill: { position: 'absolute', left: 0, height: 4, borderRadius: 2, backgroundColor: colors.text },
  thumb: { position: 'absolute', width: 12, height: 12, marginLeft: -6, borderRadius: 6, backgroundColor: colors.text },
  percent: { width: 40, textAlign: 'right', fontFamily: fonts.regular, fontSize: 12, color: colors.muted },
  // A home full of Cast and Connect speakers scrolls inside the sheet.
  list: { maxHeight: 360, marginHorizontal: -16 },
  subtitle: { marginTop: 24, marginBottom: 4, fontFamily: fonts.bold, fontSize: 14, color: colors.text },
  device: { height: 60, flexDirection: 'row', alignItems: 'center', gap: 14, paddingHorizontal: 24 },
  deviceName: { fontFamily: fonts.semibold, fontSize: 15, color: colors.text },
  kind: { fontFamily: fonts.mono, fontSize: 11, color: colors.muted },
});
