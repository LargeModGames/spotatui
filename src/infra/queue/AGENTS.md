### Alternative sources (Local / Subsonic / Radio / YouTube / Qobuz)

- Repeat/shuffle for decoded sources live in the pure module
  `src/infra/queue/mod.rs` (`advance_decision`, `resume_index_after_queue`, …);
  state is player-global on `App` (`decoded_repeat`, `decoded_shuffle`).
  Repeat-one affects auto-advance only, never a manual skip. Note
  `resume_index_after_queue` returning `None` means "context exhausted, tear
  down" - the *opposite* of `advance_index`'s `None` ("clamp, no-op").
