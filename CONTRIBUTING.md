# Contributing to spotatui

Thanks for your interest in spotatui! 💚 We welcome all kinds of contributions, not just code.

## Ways to Contribute

### 🐛 Report Bugs
Found something broken? [Open an issue](https://github.com/LargeModGames/spotatui/issues/new) with:
- What you expected vs what happened
- Steps to reproduce
- Your OS and spotatui version

### 💡 Suggest Features
Have an idea? Start a [Discussion](https://github.com/LargeModGames/spotatui/discussions) or open an issue. We love hearing what would make spotatui better for you.

### 📖 Improve Documentation
- Fix typos or unclear instructions
- Add examples or clarify setup steps
- Translate docs (we'd love to support more languages!)

The wiki is included as a submodule in `spotatui.wiki/`. To contribute:
```bash
git clone --recurse-submodules https://github.com/LargeModGames/spotatui.git
# Make your changes in spotatui.wiki/
# Then commit and open a PR
```

### 🎨 Create Themes
Love customization? Add a new theme preset! Check out `ThemePreset` in `src/core/user_config.rs` for examples, and `docs/themes.md` for the theming docs.

### 🧪 Test on Your Setup
- Try pre-releases and report issues
- Test on unusual setups (BSD, WSL, specific distros)
- Verify audio works with different backends

### ⭐ Spread the Word
- Star the repo
- Share spotatui with music lovers
- Write about your experience

---

## Code Contributions

### Ground Rules
- Be kind and follow our [Code of Conduct](CODE_OF_CONDUCT.md)
- Open an issue first for new features or larger refactors
- Keep PRs focused and scoped for easier review

### Getting Set Up

1. Install a recent stable Rust toolchain (`rustup` recommended)
2. Install platform dependencies from [Development](README.md#development):
   - OpenSSL
   - `xorg-dev` (Linux; clipboard support)
   - PipeWire dev libraries (Linux; audio visualization)
   - `portaudio` via Homebrew (macOS)
3. Clone your fork and create a topic branch from `main`

Run locally:
```bash
cargo run
```

Slim build (no audio/streaming):
```bash
cargo run --no-default-features --features telemetry
```

### Before Opening a PR

Run these checks (the fast local gate):
```bash
cargo fmt --all
cargo clippy --no-default-features --features telemetry -- -D warnings
cargo test --no-default-features --features telemetry
```

CI runs a wider five-leg feature matrix (see `.github/workflows/ci.yml`), including
a plain `cargo test` with default features, and passes `--locked` - regenerate
`Cargo.lock` if you touch `Cargo.toml`. See `AGENTS.md` for the full matrix.

### PR Tips
- Add/adjust tests when changing behavior
- Update `README.md` and `CHANGELOG.md` for user-facing changes
- Include screenshots for UI changes
- Keep commits logical; squashing welcome but not required

---

## Recognition

We use [all-contributors](https://allcontributors.org/) to recognize everyone who helps—code or not! After your contribution is merged, the maintainer will add you to the contributors list.

## Questions?

Start a [Discussion](https://github.com/LargeModGames/spotatui/discussions) or ping us in an issue. We're happy to help!
