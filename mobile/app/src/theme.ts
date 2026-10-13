import { StyleSheet } from 'react-native';

/** The tokens of the "spotatui mobile" design. */
export const colors = {
  background: '#121212',
  tabBar: '#000000',
  surface: '#2a2a2a',
  sheet: '#1f1f1f',
  tile: 'rgba(255,255,255,0.07)',
  border: '#3a3a3a',
  text: '#ffffff',
  muted: '#b3b3b3',
  dim: '#8a8a8a',
  rail: 'rgba(255,255,255,0.3)',
  accent: '#f0a83a',
  onAccent: '#121212',
};

export const fonts = {
  regular: 'HankenGrotesk_400Regular',
  medium: 'HankenGrotesk_500Medium',
  semibold: 'HankenGrotesk_600SemiBold',
  bold: 'HankenGrotesk_700Bold',
  extrabold: 'HankenGrotesk_800ExtraBold',
  mono: 'JetBrainsMono_400Regular',
  monoMedium: 'JetBrainsMono_500Medium',
};

/** Loaded at start; each weight is its own family, so no style sets a fontWeight. */
export const fontFiles = {
  [fonts.regular]: require('@/assets/fonts/HankenGrotesk_400Regular.ttf'),
  [fonts.medium]: require('@/assets/fonts/HankenGrotesk_500Medium.ttf'),
  [fonts.semibold]: require('@/assets/fonts/HankenGrotesk_600SemiBold.ttf'),
  [fonts.bold]: require('@/assets/fonts/HankenGrotesk_700Bold.ttf'),
  [fonts.extrabold]: require('@/assets/fonts/HankenGrotesk_800ExtraBold.ttf'),
  [fonts.mono]: require('@/assets/fonts/JetBrainsMono_400Regular.ttf'),
  [fonts.monoMedium]: require('@/assets/fonts/JetBrainsMono_500Medium.ttf'),
};

export const type = StyleSheet.create({
  title: { fontFamily: fonts.extrabold, fontSize: 28, letterSpacing: -0.3, color: colors.text },
  heading: { fontFamily: fonts.extrabold, fontSize: 20, letterSpacing: -0.2, color: colors.text },
  body: { fontFamily: fonts.regular, fontSize: 14, color: colors.text },
  bodyBold: { fontFamily: fonts.bold, fontSize: 14, color: colors.text },
  small: { fontFamily: fonts.regular, fontSize: 13, color: colors.muted },
  label: { fontFamily: fonts.mono, fontSize: 10, letterSpacing: 1.2, color: colors.muted },
});

/** The smallest touch target. */
export const TOUCH = 44;
