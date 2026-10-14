import { SymbolView } from 'expo-symbols';
import type { ComponentProps } from 'react';

import { colors } from '@/theme';

type SymbolName = ComponentProps<typeof SymbolView>['name'];

/** The app's icons: SF Symbols on iOS, Material Symbols on Android. */
const ICONS = {
  home: { ios: 'house', android: 'home' },
  search: { ios: 'magnifyingglass', android: 'search' },
  library: { ios: 'books.vertical', android: 'library_music' },
  settings: { ios: 'gearshape', android: 'settings' },
  history: { ios: 'clock', android: 'history' },
  devices: { ios: 'hifispeaker.and.homepod', android: 'devices' },
  heart: { ios: 'heart', android: 'favorite' },
  play: { ios: 'play.fill', android: 'play_arrow' },
  pause: { ios: 'pause.fill', android: 'pause' },
  next: { ios: 'forward.end.fill', android: 'skip_next' },
  previous: { ios: 'backward.end.fill', android: 'skip_previous' },
  shuffle: { ios: 'shuffle', android: 'shuffle' },
  repeat: { ios: 'repeat', android: 'repeat' },
  repeatOne: { ios: 'repeat.1', android: 'repeat_one' },
  lyrics: { ios: 'quote.bubble', android: 'lyrics' },
  queue: { ios: 'list.bullet', android: 'queue_music' },
  down: { ios: 'chevron.down', android: 'expand_more' },
  more: { ios: 'ellipsis', android: 'more_horiz' },
  close: { ios: 'xmark', android: 'close' },
  phone: { ios: 'iphone', android: 'smartphone' },
  computer: { ios: 'desktopcomputer', android: 'computer' },
  speaker: { ios: 'hifispeaker', android: 'speaker' },
  tv: { ios: 'tv', android: 'tv' },
  volume: { ios: 'speaker.wave.2', android: 'volume_up' },
  mute: { ios: 'speaker.slash', android: 'volume_off' },
  add: { ios: 'plus', android: 'add' },
  addCircle: { ios: 'plus.circle', android: 'add_circle' },
  saved: { ios: 'checkmark.circle.fill', android: 'check_circle' },
  forward: { ios: 'chevron.right', android: 'chevron_right' },
  back: { ios: 'chevron.left', android: 'arrow_back' },
  playing: { ios: 'waveform', android: 'graphic_eq' },
} satisfies Record<string, SymbolName>;

export type IconName = keyof typeof ICONS;

export function Icon({ name, size = 22, color = colors.text }: { name: IconName; size?: number; color?: string }) {
  return <SymbolView name={ICONS[name]} size={size} tintColor={color} />;
}
