import { NativeModule, requireNativeModule } from 'expo';

declare class SpotatuiCoreModule extends NativeModule<{}> {
  /** Starts the core once and resolves to its launch URL. */
  start(): Promise<string>;
  /** Storage that lives as long as the core, not as long as the JavaScript runtime. */
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export default requireNativeModule<SpotatuiCoreModule>('SpotatuiCore');
