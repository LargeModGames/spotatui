import { NativeModule, requireNativeModule } from 'expo';

declare class SpotatuiCoreModule extends NativeModule<{}> {
  /** Starts the core once and resolves to its launch URL. */
  start(): Promise<string>;
}

export default requireNativeModule<SpotatuiCoreModule>('SpotatuiCore');
