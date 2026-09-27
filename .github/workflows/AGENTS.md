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
