import { useFonts } from 'expo-font';
import { DarkTheme, Stack, ThemeProvider } from 'expo-router';
import * as SplashScreen from 'expo-splash-screen';
import { StatusBar } from 'expo-status-bar';
import { useEffect, useState, type ReactNode } from 'react';
import { StyleSheet, Text, View } from 'react-native';

import type { Connection } from '@gui/connection';
import { showsOnboarding } from '@gui/connection';

import { Onboarding } from '@/components/Onboarding';
import { connect } from '@/core';
import { CoreProvider, useConnectionSend, useCoreSelect } from '@/core-context';
import { colors, fontFiles, type } from '@/theme';

SplashScreen.preventAutoHideAsync();

const navTheme = {
  ...DarkTheme,
  colors: { ...DarkTheme.colors, background: colors.background, card: colors.tabBar, primary: colors.accent },
};

export default function RootLayout() {
  const [fontsLoaded] = useFonts(fontFiles);
  const [connection, setConnection] = useState<Connection | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    connect().then(setConnection, (e) => setError(String(e)));
  }, []);
  useEffect(() => {
    if (fontsLoaded) SplashScreen.hideAsync();
  }, [fontsLoaded]);
  if (!fontsLoaded) return null;
  return (
    <ThemeProvider value={navTheme}>
      <StatusBar style="light" />
      {error ? (
        <Message text={error} />
      ) : !connection ? (
        <Message text="Starting spotatui…" />
      ) : (
        <CoreProvider connection={connection}>
          <Gate>
            <Stack screenOptions={{ headerShown: false, contentStyle: { backgroundColor: colors.background } }}>
              <Stack.Screen name="(tabs)" />
              <Stack.Screen name="playlist/[id]" />
              <Stack.Screen name="artist/[id]" />
              <Stack.Screen name="album/[id]" />
              <Stack.Screen name="settings" />
              <Stack.Screen name="now-playing" options={{ presentation: 'fullScreenModal', animation: 'slide_from_bottom' }} />
              <Stack.Screen name="lyrics" options={{ presentation: 'fullScreenModal', animation: 'slide_from_bottom' }} />
              <Stack.Screen name="queue" options={{ presentation: 'fullScreenModal', animation: 'slide_from_bottom' }} />
              <Stack.Screen
                name="devices"
                options={{
                  presentation: 'formSheet',
                  sheetAllowedDetents: 'fitToContents',
                  sheetCornerRadius: 16,
                  contentStyle: { backgroundColor: colors.sheet },
                }}
              />
            </Stack>
          </Gate>
        </CoreProvider>
      )}
    </ThemeProvider>
  );
}

/** The app once the core has booted; before that its connection state or first-launch questions. */
function Gate({ children }: { children: ReactNode }) {
  const expired = useCoreSelect((state) => state.expired);
  const connected = useCoreSelect((state) => state.connected);
  const onboarding = useCoreSelect((state) => (state.onboarding && showsOnboarding(state) ? state.onboarding : null));
  const send = useConnectionSend();
  if (expired) return <Message text="The connection to spotatui was refused or closed." />;
  if (!connected) return <Message text="Connecting…" />;
  if (onboarding) return <Onboarding view={onboarding} onReply={(reply) => send({ type: 'onboarding', reply })} />;
  return children;
}

function Message({ text }: { text: string }) {
  return (
    <View style={styles.message}>
      <Text style={type.body}>{text}</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  message: { flex: 1, alignItems: 'center', justifyContent: 'center', padding: 24, backgroundColor: colors.background },
});
