import { useEffect, useState, useSyncExternalStore } from 'react';
import { Button, FlatList, Pressable, ScrollView, StyleSheet, TextInput, View } from 'react-native';
import { SafeAreaView } from 'react-native-safe-area-context';

import type { Connection } from '@gui/connection';
import { showsOnboarding } from '@gui/connection';
import type { OnboardingQuestion } from '@gui/bindings/OnboardingQuestion';
import type { OnboardingReply } from '@gui/bindings/OnboardingReply';
import { playRequest } from '@gui/libraryModel';

import { ThemedText } from '@/components/themed-text';
import { ThemedView } from '@/components/themed-view';
import { Spacing } from '@/constants/theme';
import { connect } from '@/core';

export default function HomeScreen() {
  const [connection, setConnection] = useState<Connection | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    connect().then(setConnection, (e) => setError(String(e)));
  }, []);
  return (
    <ThemedView style={styles.container}>
      <SafeAreaView style={styles.safeArea}>
        {error ? (
          <ThemedText>{error}</ThemedText>
        ) : connection ? (
          <Session connection={connection} />
        ) : (
          <ThemedText>Starting spotatui…</ThemedText>
        )}
      </SafeAreaView>
    </ThemedView>
  );
}

/** A test screen over the GUI protocol: the first-launch questions, then the current track. */
function Session({ connection }: { connection: Connection }) {
  const state = useSyncExternalStore(connection.subscribe, connection.getState);
  const reply = (answer: OnboardingReply) => connection.send({ type: 'onboarding', reply: answer });
  if (state.expired) return <ThemedText>The connection to spotatui was refused or closed.</ThemedText>;
  if (!state.connected) return <ThemedText>Connecting…</ThemedText>;
  if (state.onboarding && showsOnboarding(state)) {
    return (
      <ScrollView contentContainerStyle={styles.column}>
        <ThemedText type="code">{state.onboarding.transcript}</ThemedText>
        {state.onboarding.pending && (
          <Question key={state.onboarding.pending.seq} question={state.onboarding.pending} onReply={reply} />
        )}
      </ScrollView>
    );
  }
  const playing = state.channels.playback?.payload.item;
  const status = state.channels.status?.payload.message;
  const tracks = state.channels.search?.payload.tracks ?? [];
  const play = (index: number) => {
    const action = playRequest(tracks, index);
    if (action) connection.send({ type: 'action', action });
  };
  return (
    <View style={[styles.column, styles.fill]}>
      <ThemedText type="small">spotatui {state.version}</ThemedText>
      <ThemedText type="subtitle">{playing?.title ?? 'Nothing is playing'}</ThemedText>
      {playing && <ThemedText>{playing.artists.join(', ')}</ThemedText>}
      <Button
        title={playing?.is_playing ? 'Pause' : 'Play'}
        onPress={() => connection.send({ type: 'action', action: 'TogglePlayback' })}
      />
      {status && <ThemedText type="small">{status}</ThemedText>}
      <TextInput
        placeholder="Search songs"
        placeholderTextColor="#888"
        returnKeyType="search"
        onSubmitEditing={(event) =>
          connection.send({ type: 'action', action: { SearchActiveSource: event.nativeEvent.text } })
        }
        style={styles.input}
      />
      <FlatList
        data={tracks}
        keyExtractor={(track, index) => `${index}-${track.uri ?? track.name}`}
        renderItem={({ item, index }) => (
          <Pressable onPress={() => play(index)} style={styles.row}>
            <ThemedText numberOfLines={1}>{item.name}</ThemedText>
            <ThemedText type="small" numberOfLines={1}>
              {item.artists.join(', ')}
            </ThemedText>
          </Pressable>
        )}
      />
    </View>
  );
}

function Question({
  question: { seq, ask },
  onReply,
}: {
  question: OnboardingQuestion;
  onReply: (reply: OnboardingReply) => void;
}) {
  const [text, setText] = useState('');
  switch (ask.kind) {
    case 'Line':
      return (
        <View style={styles.column}>
          <ThemedText>{ask.prompt}</ThemedText>
          <TextInput value={text} onChangeText={setText} secureTextEntry={ask.masked} style={styles.input} />
          <Button title="Continue" onPress={() => onReply({ seq, answer: { kind: 'Line', text } })} />
        </View>
      );
    case 'Confirm':
      return (
        <View style={styles.column}>
          <ThemedText type="subtitle">{ask.title}</ThemedText>
          <ThemedText>{ask.body}</ThemedText>
          <ThemedText>{ask.question}</ThemedText>
          <Button title="Yes" onPress={() => onReply({ seq, answer: { kind: 'Confirm', yes: true } })} />
          <Button title="No" onPress={() => onReply({ seq, answer: { kind: 'Confirm', yes: false } })} />
        </View>
      );
    case 'PickSources':
      return <ThemedText>This build has only Spotify, so the core does not ask this.</ThemedText>;
  }
}

const styles = StyleSheet.create({
  container: { flex: 1 },
  safeArea: { flex: 1, padding: Spacing.four },
  column: { gap: Spacing.three },
  fill: { flex: 1 },
  row: { paddingVertical: Spacing.two },
  input: { borderWidth: 1, borderColor: '#888', padding: Spacing.two, color: '#888' },
});
