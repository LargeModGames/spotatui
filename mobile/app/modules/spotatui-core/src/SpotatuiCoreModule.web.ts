import { registerWebModule, NativeModule } from 'expo';

// SpotatuiCoreModule is not available on the web platform.
class SpotatuiCoreModule extends NativeModule<{}> {}

export default registerWebModule(SpotatuiCoreModule, 'SpotatuiCoreModule');
