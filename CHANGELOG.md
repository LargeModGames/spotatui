# Changelog

## 0.43.0-radio.1

- Renamed the fork repository to `ethereumdegen/degen-radio`.
- Removed the Spotify playback, Web API, authentication, and login subsystems.
- Removed the YouTube source and downloader integration.
- Retained internet-radio playback, saved stations, directory search, ICY metadata, and MPRIS controls.
- Added `degen-radio radio list --json` and `degen-radio radio play URL` for desktop integrations.
- Restored the Spotatui-style radio page with a persistent directory search box, saved-station and result panels, mouse focus, and left/right panel navigation.
- Added persistent favorite and unfavorite actions from either station panel.
- Added in-app color settings for focused borders, now-playing text, selection highlights, and favorites; theme changes persist in Degen Radio state.
- Added the current ICY song title to the bottom status box.
