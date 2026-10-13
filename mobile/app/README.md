# spotatui mobile

The Android app: an Expo app that runs the spotatui core in its own process
(`../core`, loaded through the local module `modules/spotatui-core`) and talks
to it over the same loopback protocol as the browser GUI (`gui/src`).

## Build and run on a device

You need the Android SDK and NDK 27.1.12297006, JDK 17, Node, and
`cargo install cargo-ndk` with the `aarch64-linux-android` (and, for the
emulator, `x86_64-linux-android`) Rust targets.

1. Build the core into the module, from `mobile/core`:

   ```sh
   cargo ndk -t arm64-v8a -P 26 -o ../app/modules/spotatui-core/android/src/main/jniLibs build
   ```

2. From `mobile/app`, install the packages and build the development app:

   ```sh
   npm ci
   npx expo run:android --device
   ```

3. Start Metro with `npx expo start --dev-client`. Over adb, run
   `adb reverse tcp:8081 tcp:8081` so the phone reaches it on localhost.

A change in Rust or Kotlin needs steps 1 and 2 again; a change in TypeScript
reloads through Metro.
