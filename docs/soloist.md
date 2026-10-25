# Soloist

[Spotify Soloist](https://developer.spotify.com/dashboard/soloist) is Spotify's own headless player. spotatui can run it as the engine behind its Spotify Connect device, in place of librespot, the engine that [native streaming](native-streaming.md) uses by default. spotatui then sends play, pause, skip, seek, volume, shuffle and repeat to Soloist, and Soloist plays the audio.

## When to use it

- **Native streaming cannot play your account.** Tracks fail one after another and the [log](configuration.md#logging) shows `error audio key 0 1`. Spotify refuses librespot the audio keys for some accounts. After three failed tracks in a row, spotatui offers to set up Soloist.
- **You want lossless audio.** Soloist streams FLAC where your plan and country have Spotify lossless.

## What you need

- **A build with the Soloist engine.** The prebuilt binaries and the desktop app have it. With `cargo install`, add `--features soloist` (see [Which build has which features](installation.md#which-build-has-which-features)).
- **Spotify Premium.**
- **A supported system:** Linux (x86_64, arm64, arm32), macOS on Apple Silicon, or Windows x86_64. Spotify has no Soloist build for Intel Macs, so the Soloist settings do not show there.
- **A Soloist API key** from Spotify's developer dashboard.
- **The Spotify app** on a computer or a phone on the same network, once, to pair.

## Setup

Every step is in Settings:

- **Terminal:** open Settings and stay on the Behavior tab. The Soloist rows come right after Playback Engine. When you select a row that has a button, the row shows the button's key, `[o]`. Press `o` to use the button.
- **Browser and desktop app:** open Settings. The Soloist rows are in the Streaming group, with their buttons next to them.

1. **Download Soloist.** Select **Soloist Path** and press `o` (or click **Download from Spotify**). spotatui downloads the official build for your system from Spotify, unpacks it and saves its path. The status bar shows the progress. By using Soloist, you accept the Spotify Terms and Conditions of Use.
   On Windows, Windows Firewall asks if Soloist can use the network. Allow it, or a phone or another computer cannot pair with it.
2. **Get an API key.** Select **Soloist API Key** and press `o` (or click **Get a key**). Spotify's developer dashboard opens in your browser. Make a Soloist API key there and copy it.
3. **Enter the key.** Edit the **Soloist API Key** row (Enter in the terminal), paste the key, confirm it, and save. spotatui keeps the key in `soloist_credentials.yml` next to `config.yml` and shows only dots in Settings.
4. **Switch the engine.** Set **Playback Engine** to **Soloist** and save. spotatui stops librespot and starts Soloist. You do not have to restart spotatui.
5. **Pair once.** A window asks you to pick spotatui in Spotify. Open Spotify on a computer or phone on the same network, open its device menu, and pick **spotatui** (or the name you set in `streaming_device_name`). The window closes by itself when Soloist logs in, and spotatui shows "Paired: spotatui plays through Soloist."
   Right after the first pairing, Soloist restarts once to apply the audio quality. Soloist keeps the login, so you pair only once.

Now play something. The playbar shows what Soloist streams, for example `FLAC 44.1`.

## Audio quality

**Soloist Audio Quality** sets what Soloist asks for: Normal, High, Very high or Lossless (the default). A change takes effect when Soloist starts again, for example at the next launch.

Lossless needs a Premium plan with lossless in your country. If the playbar still shows a lossy stream, open the device menu in the Spotify mobile app, pick spotatui, and set its quality to Lossless.

spotatui turns Soloist's volume normalization off.

## Switch back

Set **Playback Engine** back to **librespot** and save. spotatui stops Soloist and starts native streaming again, without a restart. The Soloist download, the key and the pairing stay, so a later switch back to Soloist is immediate.

## In the config file

The same settings in `config.yml`:

```yaml
behavior:
  playback_engine: soloist                # librespot (the default) or soloist
  soloist_path: /path/to/soloist          # the download button fills this in
  soloist_audio_quality: lossless         # normal, high, very_high or lossless
```

The key is in `soloist_credentials.yml`, next to `config.yml`:

```yaml
api_key: your-key
```

If the environment variable `SPOTATUI_SOLOIST_API_KEY` is set, spotatui uses it in place of the file.

Where the files go:

- Downloaded builds: `soloist-bin/` in the [state directory](configuration.md), one folder per download.
- Soloist's own data (the pairing and the device id): `soloist/` in the state directory, and `soloist/` in the cache directory.

## Good to know

- **Builds expire.** A Soloist build stops working 90 days after its release. spotatui warns when less than a week is left. Download a new build in Settings (step 1); the key and the pairing stay.
- **The key is visible to your other programs.** Soloist takes the key on its command line, so other programs that run as your user can read it. spotatui never writes the key to its log.
- **No downloads for offline play.** Soloist has no offline mode; it streams, like native streaming.
- **No other sources queued behind Spotify.** With Soloist, you cannot queue a track from another source behind a Spotify track, because Soloist does not tell spotatui when a track ends. Native streaming can.

## Troubleshooting

| Message | What to do |
|---------|------------|
| Soloist is not installed. | Download it (step 1). |
| Soloist not found at ... | The file at Soloist Path is gone. Download it again (step 1). |
| This Soloist build does not run on this CPU. | Download it again (step 1); spotatui picks the build for your system. |
| Soloist needs an API key. | Get and enter a key (steps 2 and 3). |
| Soloist rejected the API key. | Enter a new key (steps 2 and 3). |
| Pair spotatui: open Spotify on a computer on this network and pick 'spotatui'. | Pair it (step 5). If a phone does not list spotatui, use the Spotify app on a computer. On Windows, check that Windows Firewall allows Soloist. |
| Soloist build expired. | Download a new build (step 1). |
| Soloist cannot reach Spotify. | Check your internet connection. |
| Another Soloist uses spotatui's Soloist data folder. | Close the other Soloist, or the other spotatui that runs one. |
| Soloist keeps exiting; see the log. | Run spotatui with `--debug` and look at the [log](configuration.md#logging). |
