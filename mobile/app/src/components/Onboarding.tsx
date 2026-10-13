import * as WebBrowser from 'expo-web-browser';
import { useState } from 'react';
import { Pressable, ScrollView, StyleSheet, Text, TextInput, View } from 'react-native';

import type { OnboardingQuestion } from '@gui/bindings/OnboardingQuestion';
import type { OnboardingReply } from '@gui/bindings/OnboardingReply';
import type { OnboardingView } from '@gui/bindings/OnboardingView';

import { colors, fonts, type } from '@/theme';

/** The core's first-launch questions; a link in the transcript (the Spotify login) opens on tap. */
export function Onboarding({ view, onReply }: { view: OnboardingView; onReply: (reply: OnboardingReply) => void }) {
  return (
    <ScrollView contentContainerStyle={styles.page}>
      <Text style={type.title}>
        spota<Text style={{ color: colors.accent }}>tui</Text>
      </Text>
      <Transcript text={view.transcript} />
      {view.pending && <Question key={view.pending.seq} question={view.pending} onReply={onReply} />}
    </ScrollView>
  );
}

function Transcript({ text }: { text: string }) {
  const parts = text.split(/(https?:\/\/\S+)/);
  return (
    <Text style={styles.transcript}>
      {parts.map((part, index) =>
        index % 2 === 1 ? (
          <Text key={index} style={styles.link} onPress={() => WebBrowser.openBrowserAsync(part)}>
            {part}
          </Text>
        ) : (
          part
        ),
      )}
    </Text>
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
        <View style={styles.question}>
          <Text style={type.body}>{ask.prompt}</Text>
          <TextInput
            value={text}
            onChangeText={setText}
            secureTextEntry={ask.masked}
            style={styles.input}
            placeholderTextColor={colors.dim}
          />
          <Button label="Continue" onPress={() => onReply({ seq, answer: { kind: 'Line', text } })} />
        </View>
      );
    case 'Confirm':
      return (
        <View style={styles.question}>
          <Text style={type.heading}>{ask.title}</Text>
          <Text style={type.small}>{ask.body}</Text>
          <Text style={type.body}>{ask.question}</Text>
          <Button label="Yes" onPress={() => onReply({ seq, answer: { kind: 'Confirm', yes: true } })} />
          <Button label="No" quiet onPress={() => onReply({ seq, answer: { kind: 'Confirm', yes: false } })} />
        </View>
      );
    case 'PickSources':
      return <Text style={type.small}>This build has only Spotify, so the core does not ask this.</Text>;
  }
}

function Button({ label, quiet, onPress }: { label: string; quiet?: boolean; onPress: () => void }) {
  return (
    <Pressable style={[styles.button, quiet && styles.quiet]} onPress={onPress} accessibilityRole="button">
      <Text style={[styles.buttonText, quiet && styles.quietText]}>{label}</Text>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  page: { gap: 20, padding: 24, paddingTop: 48 },
  transcript: { fontFamily: fonts.mono, fontSize: 12, lineHeight: 18, color: colors.muted },
  link: { color: colors.accent, textDecorationLine: 'underline' },
  question: { gap: 12 },
  input: {
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 4,
    padding: 10,
    color: colors.text,
    fontFamily: fonts.regular,
  },
  button: { height: 48, borderRadius: 24, alignItems: 'center', justifyContent: 'center', backgroundColor: colors.accent },
  buttonText: { fontFamily: fonts.bold, fontSize: 15, color: colors.onAccent },
  quiet: { backgroundColor: 'transparent', borderWidth: 1, borderColor: colors.border },
  quietText: { color: colors.text },
});
