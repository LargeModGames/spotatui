import { router } from 'expo-router';
import { useState } from 'react';
import { Modal, Pressable, ScrollView, StyleSheet, Switch, Text, TextInput, View } from 'react-native';
import { SafeAreaView, useSafeAreaInsets } from 'react-native-safe-area-context';

import type { Action } from '@gui/bindings/Action';
import type { SettingRow } from '@gui/bindings/SettingRow';
import { commitValue, optionLabel, sections, setSetting } from '@gui/settingsModel';

import { Icon } from '@/components/Icon';
import { MINI_PLAYER_SPACE, MiniPlayer } from '@/components/MiniPlayer';
import { useChannel, useSend } from '@/core-context';
import { colors, fonts, TOUCH, type } from '@/theme';

type Send = (action: Action) => void;

/** The settings the core offers a phone, grouped as on the browser's Settings page. */
export default function SettingsScreen() {
  const rows = useChannel('settings') ?? null;
  const send = useSend();
  const insets = useSafeAreaInsets();
  return (
    <View style={styles.screen}>
      <SafeAreaView edges={['top']} style={styles.header}>
        <Pressable style={styles.back} onPress={() => router.back()} accessibilityLabel="Back">
          <Icon name="back" size={24} />
        </Pressable>
        <Text style={styles.title}>Settings</Text>
        <View style={styles.spacer} />
      </SafeAreaView>
      <ScrollView contentContainerStyle={{ paddingHorizontal: 16, paddingBottom: MINI_PLAYER_SPACE + insets.bottom }}>
        {rows === null && <Text style={type.small}>Loading…</Text>}
        {sections(rows ?? [], false).map((section) => (
          <View key={section.group}>
            <Text style={[type.heading, { marginTop: 24, marginBottom: 4 }]}>{section.heading}</Text>
            {section.rows.map((row) => (
              <SettingItem key={row.id} row={row} send={send} />
            ))}
          </View>
        ))}
      </ScrollView>
      <MiniPlayer edge />
    </View>
  );
}

function SettingItem({ row, send }: { row: SettingRow; send: Send }) {
  const [open, setOpen] = useState(false);
  const v = row.value;
  const words = (inline: boolean) => (
    <View style={inline && styles.words}>
      <Text style={styles.name}>{row.name}</Text>
      <Text style={type.small}>{row.description}</Text>
    </View>
  );
  switch (v.kind) {
    case 'bool':
      return (
        <View style={[styles.item, styles.inline]}>
          {words(true)}
          <Switch
            value={v.value}
            onValueChange={(on) => send(setSetting(row.id, { Bool: on }))}
            trackColor={{ false: colors.surface, true: colors.accent }}
            thumbColor={colors.text}
            accessibilityLabel={row.name}
          />
        </View>
      );
    case 'choice':
      return (
        <>
          <Pressable style={[styles.item, styles.inline]} onPress={() => setOpen(true)} accessibilityRole="button">
            {words(true)}
            <Text style={type.small}>{optionLabel(row.id, v.value)}</Text>
            <Icon name="forward" size={18} color={colors.dim} />
          </Pressable>
          {open && (
            <ChoiceSheet
              row={row}
              value={v.value}
              options={v.options}
              onPick={(option) => {
                if (option !== v.value) send(setSetting(row.id, { Text: option }));
              }}
              onClose={() => setOpen(false)}
            />
          )}
        </>
      );
    case 'number':
    case 'text':
      return (
        <View style={styles.item}>
          {words(false)}
          <SettingField row={row} send={send} />
        </View>
      );
    case 'secret':
      return (
        <View style={styles.item}>
          {words(false)}
          <SecretField row={row} set={v.set} send={send} />
        </View>
      );
  }
}

function ChoiceSheet({
  row,
  value,
  options,
  onPick,
  onClose,
}: {
  row: SettingRow;
  value: string;
  options: string[];
  onPick: (option: string) => void;
  onClose: () => void;
}) {
  const insets = useSafeAreaInsets();
  return (
    <Modal
      visible
      transparent
      animationType="slide"
      statusBarTranslucent
      navigationBarTranslucent
      onRequestClose={onClose}>
      <Pressable style={styles.backdrop} onPress={onClose} />
      <View style={[styles.sheet, { paddingBottom: insets.bottom + 16 }]}>
        <Text style={[type.bodyBold, styles.sheetTitle]}>{row.name}</Text>
        <ScrollView>
          {options.map((option) => {
            const checked = option === value;
            return (
              <Pressable
                key={option}
                style={styles.option}
                onPress={() => {
                  onPick(option);
                  onClose();
                }}
                accessibilityRole="radio"
                accessibilityState={{ checked }}>
                <Text style={[styles.name, checked && { color: colors.accent }]}>{optionLabel(row.id, option)}</Text>
                {checked && <Icon name="saved" size={20} color={colors.accent} />}
              </Pressable>
            );
          })}
        </ScrollView>
      </View>
    </Modal>
  );
}

function SettingField({ row, send }: { row: SettingRow; send: Send }) {
  const [draft, setDraft] = useState<string | null>(null);
  if (row.value.kind !== 'number' && row.value.kind !== 'text') return null;
  const commit = () => {
    if (draft !== null) {
      const input = commitValue(row, draft);
      if (input) send(setSetting(row.id, input));
    }
    setDraft(null);
  };
  return (
    <TextInput
      style={styles.field}
      value={draft ?? String(row.value.value)}
      onChangeText={setDraft}
      onEndEditing={commit}
      returnKeyType="done"
      keyboardType={row.value.kind === 'number' ? 'number-pad' : 'default'}
      autoCorrect={false}
      autoCapitalize="none"
      accessibilityLabel={row.name}
    />
  );
}

function SecretField({ row, set, send }: { row: SettingRow; set: boolean; send: Send }) {
  const [draft, setDraft] = useState('');
  return (
    <TextInput
      style={styles.field}
      value={draft}
      onChangeText={setDraft}
      onEndEditing={() => {
        const input = commitValue(row, draft);
        if (input) send(setSetting(row.id, input));
        setDraft('');
      }}
      secureTextEntry
      placeholder={set ? 'Saved' : 'Not set'}
      placeholderTextColor={colors.dim}
      autoComplete="new-password"
      accessibilityLabel={row.name}
    />
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: colors.background },
  header: { flexDirection: 'row', alignItems: 'center' },
  back: { width: TOUCH, height: TOUCH, alignItems: 'center', justifyContent: 'center' },
  title: { flex: 1, textAlign: 'center', fontFamily: fonts.bold, fontSize: 16, color: colors.text },
  spacer: { width: TOUCH },
  item: { paddingVertical: 12, minHeight: TOUCH },
  inline: { flexDirection: 'row', alignItems: 'center', gap: 12 },
  words: { flex: 1 },
  name: { fontFamily: fonts.semibold, fontSize: 15, color: colors.text },
  field: {
    fontFamily: fonts.regular,
    fontSize: 15,
    color: colors.text,
    borderBottomWidth: 1,
    borderColor: colors.border,
    paddingVertical: 8,
    marginTop: 8,
  },
  backdrop: { flex: 1, backgroundColor: 'rgba(0,0,0,0.6)' },
  sheet: {
    backgroundColor: colors.sheet,
    borderTopLeftRadius: 16,
    borderTopRightRadius: 16,
    maxHeight: '70%',
  },
  sheetTitle: { padding: 16 },
  option: {
    minHeight: TOUCH,
    paddingHorizontal: 16,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
  },
});
