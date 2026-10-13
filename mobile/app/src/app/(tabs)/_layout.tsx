import { NativeTabs } from 'expo-router/unstable-native-tabs';

import { colors, fonts } from '@/theme';

export default function TabsLayout() {
  return (
    <NativeTabs
      backgroundColor={colors.tabBar}
      indicatorColor={colors.surface}
      iconColor={{ default: colors.dim, selected: colors.text }}
      labelStyle={{
        default: { color: colors.dim, fontFamily: fonts.semibold, fontSize: 11 },
        selected: { color: colors.text, fontFamily: fonts.bold, fontSize: 11 },
      }}>
      <NativeTabs.Trigger name="index">
        <NativeTabs.Trigger.Label>Home</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon sf="house" md="home" />
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="search">
        <NativeTabs.Trigger.Label>Search</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon sf="magnifyingglass" md="search" />
      </NativeTabs.Trigger>
      <NativeTabs.Trigger name="library">
        <NativeTabs.Trigger.Label>Your Library</NativeTabs.Trigger.Label>
        <NativeTabs.Trigger.Icon sf="books.vertical" md="library_music" />
      </NativeTabs.Trigger>
    </NativeTabs>
  );
}
