# Contributing to spotatui

Thanks for taking the time to improve spotatui! This guide explains how to set up a dev environment, run checks, and open a helpful pull request.

## Ground rules
- Be kind and follow our [Code of Conduct](CODE_OF_CONDUCT.md).
- Prefer an issue first for new features or larger refactors; bug reports can use the existing template.
- Keep pull requests focused and scoped so they are easy to review.

## Getting set up
1. Install a recent stable Rust toolchain (`rustup` recommended).
2. Install platform dependencies listed in the [Development](README.md#development) section:
   - OpenSSL
   - `xorg-dev` (Linux; needed for clipboard support)
   - PipeWire dev libraries on Linux if you plan to use audio visualization
   - `portaudio` via Homebrew on macOS
3. Clone your fork and create a topic branch from `master`.

You can run the app locally with the default feature set:
```bash
cargo run
```
If you prefer to avoid optional audio/streaming dependencies, use the slimmer feature set we use in CI:
```bash
cargo run --no-default-features --features telemetry
```

## Checks to run before opening a PR
Run the same commands CI uses so reviewers can focus on the change itself:
```bash
cargo fmt --all
cargo clippy --no-default-features --features telemetry -- -D warnings
cargo test --no-default-features --features telemetry
```
If you have the required system dependencies installed and want to cover everything, you can also run with `--all-features`.

## Pull request tips
- Add or adjust tests when you change behavior.
- Update `README.md` and/or `CHANGELOG.md` when user-facing behavior shifts.
- Include a brief description of the problem and solution in the PR body, along with any screenshots for UI changes.
- Keep commits logical; squashing is welcome but not required.

## Releases
Release steps for maintainers live in `how_to_release.md`. Contributors do not need to publish artifacts.
