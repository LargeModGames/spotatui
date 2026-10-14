## CI Checks (run before opening a PR)

- `mcp-only` and `ai-dj-only` matter more than their size suggests: both enable
  `dj-core` **without** `streaming` (a combination nothing else covers), and each
  front door has to build without the other.
- Reproducing legs locally: `cargo test` reproduces `default`. For `all-sources`,
  copy the exact `--no-default-features --features …` string out of `ci.yml` -
  `cargo test --features all-sources` is **not** it (the alias only adds the five
  sources on top of default), and the leg includes `audio-viz` (PipeWire), so it
  only compiles on Linux.
- The `all-sources` leg must stay in sync with `cd.yml`'s Linux release row
  (macOS releases ship the same five sources but a different
  backend/OS-integration set).

## Desktop app

- `desktop.yml` checks the Tauri shell in `desktop/`. Pull requests run
  `ubuntu-24.04` only; `push: main` and `workflow_dispatch` add `macos-latest`
  and `windows-latest`, so a macOS or Windows break shows up after the merge.
  Linux rows add the WebKitGTK packages to ci.yml's apt list and set ci.yml's
  `RUSTFLAGS`; macOS installs `portaudio` and probes it statically.
- Its `FEATURES` env must match the `spotatui` dependency's features in
  `desktop/Cargo.toml`: the root clippy step lints `src/` under exactly that
  list, the only place the hosted GUI entry is linted on macOS and Windows.
- Desktop cargo commands run without `--locked`. A stale `desktop/Cargo.lock`
  gives a `::warning::`, never a failure; the release commit refreshes it.
- The cd.yml `desktop` job runs on tags only, after `release`, and uploads the
  installers to that draft release with `gh release upload --clobber`. It runs the prebuilt `@tauri-apps/cli` 2.12.2 from npm, builds
  `gui/dist` first (the shell's `build.rs` refuses a release build without
  it) and runs
  `npx @tauri-apps/cli@2.12.2 build --ci --bundles <list>` from `desktop/`.
- Only that build turns on `createUpdaterArtifacts` (through `--config`) and
  passes the `TAURI_SIGNING_PRIVATE_KEY` and `_PASSWORD` secrets, so
  `desktop.yml` and local builds need no key. It uploads the `.sig` of the
  `-setup.exe` and each `.AppImage`, and renames macOS's `<productName>.app.tar.gz`
  to `spotatui_<version>_<arch>.app.tar.gz` (both macOS rows build the same
  name) before uploading it with its `.sig`.
- `desktop-manifest` (stable tags only, `needs: desktop`) builds `latest.json`
  from the release's `.sig` files and uploads it (`publish` deletes the `.sig`
  files later, since latest.json embeds them); the app's updater endpoint is
  `releases/latest/download/latest.json`. It fails without the
  `windows-x86_64` or `linux-x86_64` entry, and one failed desktop row skips it.
- On macOS it fails when the app binary links anything outside `/System` and
  `/usr/lib` (`otool -L`), so a Homebrew dylib can never ship. Linux rows print
  `ldd` for the rc review.
- Keep the installer names free of OS words (`linux`, `macos`, `windows`):
  self-update falls back to an OS+arch asset match, and the release job's `.deb`
  lookup matches `spotatui_*_<arch>.deb`.

## Release page

- `release` creates a draft; `publish` (after `desktop` and `desktop-manifest`)
  writes `checksums.txt`, fails on a notes link with no matching asset, and
  makes a stable tag public. The publish jobs and the Discord post need
  `publish`. A `-` tag stays a draft for good: self-update in v0.45 and older
  installs a public prerelease without checking it.
- The notes link every file by name, the desktop ones from the
  `desktop/Cargo.toml` version, so a Tauri naming change shows up as a failed
  link check.
- Never remove the five TUI archive sidecars (`spotatui-<platform>.tar.gz.sha256`,
  `spotatui-windows-x86_64.zip.sha256`): self-update in v0.39.1 to v0.45 looks
  each one up by exact name and stops updating without it. From v0.46 the
  updater reads `checksums.txt` and pins the install to the checked tag.
