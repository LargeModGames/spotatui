# Contributing to degen-radio

Report bugs and propose changes at <https://github.com/ethereumdegen/degen-radio/issues>.

## Development

Install a recent stable Rust toolchain plus the platform audio and D-Bus development packages. On Debian/Ubuntu:

```bash
sudo apt-get install libasound2-dev libdbus-1-dev pkg-config
```

Run the application and checks:

```bash
cargo run
cargo fmt --all --check
cargo check --locked
cargo test --locked
```

Keep changes focused. Update `README.md` and `CHANGELOG.md` for user-visible behavior.
