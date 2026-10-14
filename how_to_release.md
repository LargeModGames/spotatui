# To create a release

Releases are automated via GitHub Actions using [`.github/workflows/cd.yml`](https://github.com/LargeModGames/spotatui/blob/main/.github/workflows/cd.yml).

The workflow runs when you push a tag.

## Stable release

1. Bump the version in `Cargo.toml` and run the app to update `Cargo.lock`.
   - Set the same version in `desktop/Cargo.toml` (the desktop test `version_matches_root` fails otherwise).
   - Refresh the desktop lock: `cargo metadata --manifest-path desktop/Cargo.toml --format-version 1 > $null` (in bash, `> /dev/null`). Commit `Cargo.lock` and `desktop/Cargo.lock` together.
2. Update the "Unreleased" header for the new version in `CHANGELOG.md` (`### Added/Fixed/Changed` as appropriate).
3. Commit and push your changes.
4. Create an annotated tag:
   - `git tag -a v0.7.0 -m "Release v0.7.0"`
5. Push that specific tag:
   - `git push origin v0.7.0`
6. Wait for the build on the [Actions page](https://github.com/LargeModGames/spotatui/actions). The `release` job creates the release as a draft with the terminal builds and the notes. Nobody sees it yet.
7. The `desktop` job builds the desktop installers (Windows `.exe`, macOS `.dmg`, Linux `.deb`, `.rpm` and `.AppImage`) and the signed update bundles, and attaches them to the draft. Re-run a failed row on its own from the Actions page.
8. When every row passed, `desktop-manifest` writes `latest.json` from the `.sig` files. Installed desktop apps read it to find the update.
9. `publish` writes `checksums.txt`, fails if a download link in the notes has no file, deletes the `.sig` files (latest.json has them), and makes the release public. Only then do the publish jobs run (crates.io, AUR, winget, Homebrew) and Discord gets the announcement.

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

## Pre-release (a maintainer smoke test that stays a draft)

Use a SemVer pre-release tag like `v0.7.0-rc1` / `v0.7.0-beta.1`.

1. Create an annotated pre-release tag:
   - `git tag -a v0.7.0-rc1 -m "RC1: canary build for Spotify API migration"`
2. Push that specific tag:
   - `git push origin v0.7.0-rc1`
3. The workflow marks the release as `prerelease: true` and leaves it a draft. Only people with write access to the repository can see the draft and download its builds.
4. Pre-release tags skip ecosystem publishing jobs (crates.io, AUR, winget, Homebrew). The `desktop` job still runs and attaches the desktop installers, but `desktop-manifest` does not, so installed desktop apps never update to a pre-release.
5. Never publish a pre-release draft by hand: the self-update in v0.45 and older installs a public pre-release without checking it.

### Homebrew Packaging

Homebrew publishing is automated via the CD workflow. When you push a new tag:

1. The `publish-homebrew` job downloads the release artifacts
2. Calculates SHA256 checksums for each platform binary
3. Updates the formula in [homebrew-spotatui](https://github.com/LargeModGames/homebrew-spotatui)



TODO: Scoop packaging is not yet set up for spotatui. If you'd like to contribute packaging, PRs are welcome!
