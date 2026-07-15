# Bug: `internet-radio`-only build emits dead-code warnings for the native-queue engine

> **Temporary file — do not merge to `main`.**
> Committed only to carry this note between machines. Once the re-gating fix
> lands, this file and the `temp/` directory should be removed from git history.

## Status
Open. Pre-existing (surfaced, not introduced, by the `Arc`-import gating fix on
`feature/repeat-shuffle-other-sources`). Warnings only — not a compile error, not
a CI target. Low priority.

## Summary
`cargo check --no-default-features --features internet-radio` compiles but emits
9 dead-code warnings. Every one is in the native cross-source queue engine
(`src/infra/queue/`), which is gated on `feature = "audio-decode"`. Internet
radio enables `audio-decode` (it decodes a live stream) but is **not a queueable
source** — it has no track queue — so the entire queue-for-decoded-tracks
machinery is compiled yet never exercised in a radio-only build.

## Reproduce
```bash
cargo check --no-default-features --features internet-radio
```

## Warnings (as of 2026-07-13)
```
unused variable: `guard`                     src/infra/queue/dispatch.rs:536
function `replay_file` is never used         src/infra/queue/mod.rs:280
variant `Decoded` is never constructed       src/infra/queue/mod.rs:330
static `QUEUE_FETCH_SEQ` is never used       src/infra/queue/dispatch.rs:386
function `next_fetch_id` is never used       src/infra/queue/dispatch.rs:389
function `publish_pending_decoded` is never used   src/infra/queue/dispatch.rs:403
function `acquire_queue_player` is never used      src/infra/queue/dispatch.rs:507
function `suspended_context_player` is never used  src/infra/queue/dispatch.rs:535
function `release_librespot` is never used         src/infra/queue/dispatch.rs:558
```

## Root cause
The queue engine's presence is keyed off `audio-decode`, but its *use* requires a
queueable decoded source (Local / Subsonic / YouTube). The two conditions are not
the same set: `internet-radio` satisfies the former without the latter.

## Suggested fix
Re-gate the native-queue engine (the `#[cfg(feature = "audio-decode")]` items in
`src/infra/queue/mod.rs` and `src/infra/queue/dispatch.rs`, and the queue-slot
field/call sites that reference them) on
`any(feature = "local-files", feature = "subsonic", feature = "youtube")`
instead of `feature = "audio-decode"`. That is the set of features that actually
constructs `QueueNowPlaying::Decoded` and drives the fetch/advance/resume paths.

This is a targeted re-gating of pre-existing architecture, so it should be done as
its own change with a check across every single-source and combined feature
combination:
```bash
for f in local-files subsonic youtube internet-radio; do
  cargo check --no-default-features --features $f
done
cargo clippy --no-default-features --features telemetry -- -D warnings   # CI slim
cargo clippy --features all-sources -- -D warnings
```

## Scope note
Not part of the repeat/shuffle feature work and not the `Arc`-gating fix (which
only made these builds *compile* — previously they errored before reaching this
code). The following single-source builds already compile warning-free:
`local-files`, `subsonic`, `youtube`. Only `internet-radio` remains.

## Cleanup
Once the re-gating fix lands, remove this file and the `temp/` directory from
git history.
