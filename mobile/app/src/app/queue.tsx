import { useEffect } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import { PanelHeader, Transport } from '@/components/Controls';
import { Icon } from '@/components/Icon';
import { TrackRow } from '@/components/TrackRow';
import { useChannel, useSend } from '@/core-context';
import { isSpotify, playingFrom, usePlayback } from '@/playback';
import { colors, fonts, TOUCH, type } from '@/theme';

export default function QueueScreen() {
  const queue = useChannel('queue');
  const send = useSend();
  const { item } = usePlayback();
  const uri = item?.uri ?? null;
  useEffect(() => {
    send('RefreshQueue');
  }, [uri, send]);
  const queued = queue?.native ?? [];
  // Spotify's own next tracks only follow while a Spotify track plays.
  const fromSpotify = isSpotify(uri) ? (queue?.spotify.items ?? []).flatMap((entry) => (entry.track ? [entry.track] : [])) : [];
  const from = playingFrom(item);
  const now = queue?.now;
  const clear = () => send('ClearQueue');
  return (
    <View style={styles.screen}>
      <PanelHeader title="Queue" />
      <ScrollView contentContainerStyle={styles.sections}>
        {(now || item) && (
          <View>
            <Text style={[type.label, styles.section]}>NOW PLAYING</Text>
            <TrackRow
              image={now?.image_url ?? item?.image_url}
              title={now?.name ?? item?.title ?? ''}
              subtitle={(now?.artists ?? item?.artists ?? []).join(', ')}
              playing
            />
          </View>
        )}
        {queued.length > 0 && (
          <View>
            <View style={styles.sectionRow}>
              <Text style={[type.label, styles.section]}>QUEUED BY YOU</Text>
              <Pressable style={styles.clear} onPress={clear} accessibilityRole="button">
                <Text style={styles.clearText}>Clear</Text>
              </Pressable>
            </View>
            {queued.map((track, position) => (
              <TrackRow
                key={`${position}-${track.uri}`}
                image={track.image_url}
                title={track.name}
                subtitle={track.artists.join(', ')}
                onPress={track.uri ? () => send({ PlayQueueItem: { uri: track.uri!, position } }) : undefined}>
                {track.uri && (
                  <Pressable
                    style={styles.icon}
                    onPress={() => send({ RemoveFromQueue: { uri: track.uri!, position } })}
                    accessibilityLabel={`Remove ${track.name} from queue`}>
                    <Icon name="close" size={20} color={colors.muted} />
                  </Pressable>
                )}
              </TrackRow>
            ))}
          </View>
        )}
        {fromSpotify.length > 0 && (
          <View>
            <Text style={[type.label, styles.section]}>
              {from.name ? `NEXT FROM ${from.name.toUpperCase()}` : 'NEXT UP'}
            </Text>
            {fromSpotify.map((track, index) => (
              <TrackRow key={`${index}-${track.uri}`} image={track.image_url} title={track.name} subtitle={track.artists.join(', ')} />
            ))}
          </View>
        )}
      </ScrollView>
      <SafeAreaView edges={['bottom']} style={styles.footer}>
        <Transport />
      </SafeAreaView>
    </View>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.background },
  sections: { gap: 20, paddingTop: 16, paddingBottom: 32 },
  section: { paddingHorizontal: 16, paddingVertical: 6 },
  sectionRow: { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', paddingRight: 4 },
  clear: { height: TOUCH, justifyContent: 'center', paddingHorizontal: 12 },
  clearText: { fontFamily: fonts.bold, fontSize: 13, color: colors.muted },
  icon: { width: TOUCH, height: TOUCH, alignItems: 'center', justifyContent: 'center' },
  footer: { paddingHorizontal: 24, paddingTop: 8, paddingBottom: 12, backgroundColor: colors.background },
});
