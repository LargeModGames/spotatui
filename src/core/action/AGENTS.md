### The shared Action vocabulary

- Every arm delegates to the same ownership-aware `App` method the
  equivalent keybinding uses. Playback starts go through
  `App::start_playback_uris` / `start_playback_context` /
  `start_playback_track_in_context` - never a hand-built
  `IoEvent::StartPlayback` in an arm.
- No catch-all match arm under `src/core/action/` or in `tui/keymap.rs`:
  both deny `clippy::wildcard_enum_match_arm` and
  `clippy::match_wildcard_for_single_variants` (a named binding like
  `_other =>` counts as a wildcard; `matches!` and `Option`/`Result`
  scrutinees are exempt), and `wildcard_arms_in_action_tree` pins the
  action tree at 0 by a raw text scan that includes tests, comments, and
  string literals. CI clippy never compiles tests, so write a test
  catch-all as `_other =>` and only on a `Result`/`Option` scrutinee.
- Every `Action` variant has an arm in `tui/keymap.rs::default_binding`,
  naming the key or gesture that produces it; a variant no gesture produces
  is `Exposure::Unbound("reason")`. A new variant is a compile error until
  it has an arm, and a test failure until `sample_actions()` in the same
  file has a value for it (an `Unbound` one also goes into the `UNBOUND`
  pin, which a producer scan of `src/tui/` checks). Feature-gate an arm's
  body, never the arm: clippy skips a match when any arm carries a `#[cfg]`.
- `Action` derives serde (the frontend wire shape); a payload type added to it
  must stay serde-derivable and carry the `ts_rs::TS` `cfg_attr` line (see
  Testing conventions), and the change needs regenerated `gui/src/bindings/`.
