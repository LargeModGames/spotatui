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
  installers plus `.sha256` files to that release with `gh release upload
  --clobber`. It installs `tauri-cli` 2.12.2, builds `gui/dist` first (the
  shell's `build.rs` refuses a release build without it) and runs
  `cargo tauri build --ci --bundles <list>` from `desktop/`.
- Only that build turns on `createUpdaterArtifacts` (through `--config`) and
  passes the `TAURI_SIGNING_PRIVATE_KEY` and `_PASSWORD` secrets, so
  `desktop.yml` and local builds need no key. It uploads the `.sig` of the
  `-setup.exe` and each `.AppImage`, and renames macOS's `<productName>.app.tar.gz`
  to `spotatui_<version>_<arch>.app.tar.gz` (both macOS rows build the same
  name) before uploading it with its `.sig`.
- `desktop-manifest` (stable tags only, `needs: desktop`) builds `latest.json`
  from the release's `.sig` files and uploads it; the app's updater endpoint is
  `releases/latest/download/latest.json`. It fails without the
  `windows-x86_64` or `linux-x86_64` entry, and one failed desktop row skips it.
- On macOS it fails when the app binary links anything outside `/System` and
  `/usr/lib` (`otool -L`), so a Homebrew dylib can never ship. Linux rows print
  `ldd` for the rc review.
- Keep the installer names free of OS words (`linux`, `macos`, `windows`):
  self-update falls back to an OS+arch asset match, and the release job's `.deb`
  lookup matches `spotatui_*_<arch>.deb`.
