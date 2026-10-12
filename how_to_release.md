# To create a release

Releases are automated via GitHub Actions using [`.github/workflows/cd.yml`](https://github.com/LargeModGames/spotatui/blob/master/.github/workflows/cd.yml).

The workflow runs when you push a tag.

## Stable release

1. Bump the version in `Cargo.toml` and run the app to update `Cargo.lock`.
   - Set the same version in `desktop/Cargo.toml` (the desktop test `version_matches_root` fails otherwise).
   - Refresh the desktop lock: `cargo metadata --manifest-path desktop/Cargo.toml --format-version 1 > $null` (in bash, `> /dev/null`). Commit `Cargo.lock` and `desktop/Cargo.lock` together.
2. Update the "Unreleased" header for the new version in `CHANGELOG.md` (`### Added/Fixed/Changed` as appropriate).
3. Commit and push your changes.
4. Create an annotated tag with a message (the tag message is shown on the GitHub release page):
   - `git tag -a v0.7.0 -m "Release v0.7.0"`
5. Push that specific tag:
   - `git push origin v0.7.0`
6. Wait for the build on the [Actions page](https://github.com/LargeModGames/spotatui/actions).
7. Stable tags (no suffix) also trigger publish jobs (crates.io, AUR, winget, Homebrew).
8. The `desktop` job then builds the desktop installers (Windows `.exe`, macOS `.dmg`, Linux `.deb`, `.rpm` and `.AppImage`) and attaches them to the release with `.sha256` files. It never blocks the release or the publish jobs; re-run a failed row on its own from the Actions page.
9. The same rows attach the signed update bundles: a `.sig` next to the Windows `-setup.exe` and each `.AppImage`, and `spotatui_<version>_x64.app.tar.gz` / `_aarch64.app.tar.gz` with their `.sig`. Once every row passed, the `desktop-manifest` job uploads `latest.json`, which installed desktop apps read to find the update. A failed row skips it; re-running that row runs it again.

## Desktop app

- Ship a change to `desktop/`, the hosted GUI entry or the desktop job on a pre-release tag first, and promote it once one rc installs and starts on Windows, macOS and Linux.
- The installers are unsigned: the release body carries the SmartScreen and Gatekeeper steps.
- A `desktop/Cargo.lock is stale` warning on a run means step 1's refresh was skipped; the build still used a lock cargo updated on the runner.
- The updater bundles are built in the `desktop` job only (`--config '{"bundle":{"createUpdaterArtifacts":true}}'`), so local and PR builds need no signing key.

### Updater signing key (one-time setup)

The desktop app installs an update only when its signature matches the public key built into the app. Set this up once, before the first release that ships the updater (pwsh):

1. Generate the key pair: `cargo tauri signer generate -w "$HOME\.tauri\spotatui.key"`, and pick a password when it asks.
2. Keep the private key file and its password safe, outside the repository. A lost key means installed apps can never update again; a leaked key lets someone push a signed update to every user.
3. Add both as repository secrets:
   - `Get-Content -Raw "$HOME\.tauri\spotatui.key" | gh secret set TAURI_SIGNING_PRIVATE_KEY`
   - `gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (it asks for the value)
4. Put the content of the public key (`spotatui.key.pub`) in `plugins.updater.pubkey` in `desktop/tauri.conf.json` and commit it.

Without the secrets the `desktop` job fails, because the build refuses to make updater bundles it cannot sign.

## Pre-release (recommended for canary testing)

Use a SemVer pre-release tag like `v0.7.0-rc1` / `v0.7.0-beta.1`.

1. Create an annotated pre-release tag with tester notes:
   - `git tag -a v0.7.0-rc1 -m "RC1: canary build for Spotify API migration"`
2. Push that specific tag:
   - `git push origin v0.7.0-rc1`
3. The workflow will automatically mark the GitHub release as `prerelease: true`.
4. Pre-release tags skip ecosystem publishing jobs (crates.io, AUR, winget, Homebrew). The `desktop` job still runs and attaches the desktop installers, but `desktop-manifest` does not, so installed desktop apps never update to a pre-release.

### Homebrew Packaging

Homebrew publishing is automated via the CD workflow. When you push a new tag:

1. The `publish-homebrew` job downloads the release artifacts
2. Calculates SHA256 checksums for each platform binary
3. Updates the formula in [homebrew-spotatui](https://github.com/LargeModGames/homebrew-spotatui)



TODO: Scoop packaging is not yet set up for spotatui. If you'd like to contribute packaging, PRs are welcome!
