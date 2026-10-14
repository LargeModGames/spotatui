# Contributing to spotatui

Thanks for your interest in spotatui! You do not need to write code to help: bug reports, docs, themes, packaging and testing on your setup all count, and all of them get you on the [contributor list](#recognition).

- [Ways to help](#ways-to-help)
- [Report a bug](#report-a-bug)
- [Set up](#set-up)
- [Where things live](#where-things-live)
- [Run it](#run-it)
- [Before you open a PR](#before-you-open-a-pr)
- [If the gates test fails](#if-the-gates-test-fails)
- [After you open a PR](#after-you-open-a-pr)
- [Using AI Tools](#using-ai-tools)
- [Contributing to the librespot fork](#contributing-to-the-librespot-fork)
- [Recognition](#recognition)
- [Questions](#questions)

## Ways to help

- **Report a bug** or ask for a feature with the [issue forms](https://github.com/LargeModGames/spotatui/issues/new/choose).
- **Fix the docs.** The user docs live in [`docs/`](docs/README.md); change them with a normal PR. (The GitHub wiki is out of date and takes no PRs; `docs/` replaces it.)
- **Add a theme preset.** See `ThemePreset` in `src/core/theme.rs` and [`docs/themes.md`](docs/themes.md).
- **Test on your setup.** Try a release on an unusual distro, BSD, WSL or audio setup and tell us what breaks.
- **Package spotatui** for a system it is not on yet. See the [help wanted](https://github.com/LargeModGames/spotatui/issues?q=is%3Aissue%20state%3Aopen%20label%3A%22help%20wanted%22) issues.
- **Write code.** Start with a [good first issue](https://github.com/LargeModGames/spotatui/issues?q=is%3Aissue%20state%3Aopen%20label%3A%22good%20first%20issue%22), and open an issue first for a new feature or a large refactor.

Be kind, and follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## Report a bug

Run spotatui again with `--debug`, make the bug happen, and attach the log:

```bash
spotatui --debug
```

spotatui prints the log path when it starts and when it exits: `spotatui_logs/spotatuilog<pid>` in your temp directory. If it crashed, `spotatui_panic.log` in the state directory has the panic, a backtrace and the end of the log. Skim the log before you post it: tokens are never written, but a log can contain your display name. See [Logging](docs/configuration.md#logging).

Tell us the frontend (terminal or desktop app), the source, your OS, how you installed spotatui, and the version. Report what you see on the spotatui tracker, even when the cause may be in the librespot fork; we move it if it is.

Security problems go to the [security policy](SECURITY.md), not to a public issue.

## Set up

1. Install Rust with [rustup](https://rustup.rs). The minimum version is the `rust-version` in `Cargo.toml`.
2. Install the system packages for your OS from [docs/installation.md#prerequisites](docs/installation.md#prerequisites).
3. For the browser GUI and the desktop app, install Node 24.
4. For the YouTube source, install [yt-dlp](https://github.com/yt-dlp/yt-dlp).
5. Fork the repo, clone your fork, and make a branch from `main`.

## Where things live

| Path | What it is |
|---|---|
| `src/core/` | App state, the tick scheduler, the shared `Action` vocabulary, config and state files |
| `src/infra/` | Spotify (through the librespot fork), the other sources, audio, Lua scripting, the AI DJ and MCP, OS integrations |
| `src/tui/` | The terminal UI: event loop, key handlers, draw functions |
| `src/gui/` | The server and protocol behind the browser GUI and the desktop app |
| `src/cli/` | The `spotatui <subcommand>` commands |
| `src/runtime/` | Startup and the entry points of each frontend |
| `gui/` | The web page of the browser GUI and the desktop app (TypeScript) |
| `desktop/` | The Tauri desktop app, a separate crate with its own `Cargo.lock` |
| `agent-plugin/` | The agent plugin that makes a coding agent the DJ |
| `examples/plugins/` | Example Lua plugins |
| `docs/` | The user docs |

The design rules of each area are in the `AGENTS.md` file of its folder ([`AGENTS.md`](AGENTS.md) at the root links them all). They are written for AI agents, but they are the best map of the code for people too. To add a terminal screen, follow the checklist in [`src/tui/AGENTS.md`](src/tui/AGENTS.md).

## Run it

```bash
# The full terminal app: Spotify only
cargo run

# With the other sources
cargo run --features all-sources

# The slim build: no librespot, audio or scripting. Fastest to compile.
cargo run --no-default-features --features telemetry,tui
```

The browser GUI needs its page built first; without it, the page is a 503 error:

```bash
cd gui
npm ci
npm run build
cd ..
cargo run --bin spotatui-gui --features gui
```

The desktop app needs the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) and the built page:

```bash
cargo run --manifest-path desktop/Cargo.toml
```

## Before you open a PR

Run the fast local gate. CI runs the same three commands:

```bash
cargo fmt --all
cargo clippy --no-default-features --features telemetry,tui -- -D warnings
cargo test --no-default-features --features telemetry,tui
```

Also run, when they apply:

- `cargo test` with the features your code is behind, if you changed code behind a feature flag, for example `cargo test --features gui` or `cargo test --features all-sources`. The slim gate does not compile that code, and a default `cargo test` compiles only the default features.
- in `gui/`: `npm ci`, `npm run lint`, `npm run format:check`, `npm run typecheck`, `npm test` and `npm run build`, if you changed `gui/`.
- `cargo clippy --manifest-path desktop/Cargo.toml -- -D warnings` and `cargo test --manifest-path desktop/Cargo.toml`, if you changed `desktop/`, `src/gui/` or `src/runtime/gui.rs`.
- any cargo command after you edit `Cargo.toml`, so `Cargo.lock` updates. CI builds with `--locked` and fails on a stale lock.

CI then checks eight feature combinations on Linux plus one macOS job ([`ci.yml`](.github/workflows/ci.yml)). The desktop app builds on macOS and Windows only after the merge. Code coverage is reported but never blocks a PR.

Add or update tests when you change behavior, add a line under `## [Unreleased]` in [`CHANGELOG.md`](CHANGELOG.md) for a change users notice, and add screenshots for a UI change.

## If the gates test fails

`cargo test` includes a gates test. It counts how often some code reaches across the layers of the app, for example a key handler that writes a field of `App` directly, and it keeps those numbers in [`tools/gates.count`](tools/gates.count). The numbers may only go down.

When your change makes one go up, change the code instead of the number: use an `App` method, `app.apply(Action::...)`, or `app.view` for presentation state. The failure message names the counter, and [`src/gates.rs`](src/gates.rs) says what each counter measures. Never edit the two adoption floors at the end of the file; a bot raises them after the merge. If `tools/gates.count` conflicts with `main`, leave it to a maintainer.

## After you open a PR

- **Title:** `type(scope): subject`, with a type in use here (`feat`, `fix`, `perf`, `refactor`, `docs`, `chore`, `ci`, `test`), for example `fix(queue): keep the order after a skip`.
- **Description:** fill in the template: what changed and why, and the commands you ran under Testing.
- **`Fixes #N`:** only when the PR fixes the whole issue, because the issue closes on merge. For part of an issue, write `Part of #N`.
- **CodeRabbit** reviews every PR, drafts too, and asks for changes until its threads are resolved. Answer it like a person: fix what is right, and say why when it is wrong.
- A [maintainer](README.md#maintainers) reviews and squash-merges the PR. Releases come out every few weeks.

### Using AI Tools

AI tools are welcome. How you wrote the code matters less than whether you checked it. You are the author of your PR, whatever wrote the first draft:

- Read your whole diff before you open the PR, and be ready to explain any line of it.
- Run the checks yourself and paste their output under Testing. Do not claim a check you did not run.
- For a `good first issue`, do what the issue asks and run every command under "Done when".
- Change only what the issue is about. No unrelated rewrites or deletions.
- Write the description yourself: what changed and why. A copy of the issue text or generic filler tells a reviewer nothing.
- Take one `good first issue` at a time, and wait for a review before you open the next PR.

PRs that ignore the issue, delete unrelated content, or claim checks that were never run are closed without a detailed review. Accounts that open PRs in bulk and do not read the reviews are blocked.

These rules apply to the librespot fork too.

## Contributing to the librespot fork

Spotify playback, Spotify Connect, the Spotify session and the calls spotatui makes to Spotify run on our librespot fork, [spotatui-librespot](https://github.com/LargeModGames/spotatui-librespot). It is published on crates.io as seven `spotatui-librespot-*` crates with one version, and spotatui pins the ones it uses to that exact version.

The fork is part of spotatui: **a merged PR there counts as a spotatui contribution**, and it gets you on the spotatui [contributor list](#recognition).

- **Bugs:** report what you see on the spotatui tracker. Open an issue on the [fork's tracker](https://github.com/LargeModGames/spotatui-librespot/issues) when you know the bug is in the library code.
- **PRs** go to the fork's `spotatui` branch. Its [`.github/CONTRIBUTING.md`](https://github.com/LargeModGames/spotatui-librespot/blob/spotatui/.github/CONTRIBUTING.md) has the checks and how to test your change inside spotatui.
- **Releases:** after the merge, a maintainer publishes a new fork version and updates the pins here in a separate PR.
- A fix that is not specific to spotatui is worth offering to [librespot](https://github.com/librespot-org/librespot) too.

## Recognition

We use [all-contributors](https://allcontributors.org) to thank everyone who helps, with code or without. After your PR is merged, or after a bug report or another contribution, comment on that PR or issue:

```text
@all-contributors please add @your-name for code
```

Use `code`, `doc`, `bug`, `ideas`, `design`, `platform` (packaging), `test`, or any other [contribution type](https://allcontributors.org/docs/en/emoji-key). The bot opens a PR that adds you to the [contributor list](README.md#spotatui-contributors). For a fork PR, comment on a spotatui issue or PR and link the fork PR. A maintainer can also add you.

## Questions

Ask in the [Discord](https://spotatui.com/discord) or in [Discussions](https://github.com/LargeModGames/spotatui/discussions). We are happy to help.
