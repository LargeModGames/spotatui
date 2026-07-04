//! Playback transport backend abstraction.
//!
//! Playback transport (play/pause/next/seek/shuffle/repeat/volume) can be served
//! two different ways depending on which Spotify device is active:
//!
//! * **Native** — the embedded librespot [`StreamingPlayer`], driving audio
//!   locally and controlling Spotify Connect state directly.
//! * **Connect** — the Spotify Web API, controlling whatever remote device is
//!   currently active.
//!
//! Historically every transport operation branched inline on
//! `is_streaming_active` (via the `network` playback helpers) to pick one of
//! these. [`PlaybackBackend`] consolidates that selection into a single value so
//! each operation dispatches once instead of re-deriving the branch.
//!
//! The selection predicate is intentionally *not* unified across operations:
//! simple symmetric operations select on "native is the active device", while
//! `start_playback` and `transfert_playback_to_device` keep their own
//! asymmetric activation logic in dedicated selector helpers. This type only
//! models the resolved choice plus the player handle needed by the native arm.

use std::sync::Arc;

use crate::infra::player::StreamingPlayer;

/// The resolved transport backend for a single playback operation.
///
/// `Native` carries the player handle so the caller does not need a second
/// lookup; `Connect` indicates the Spotify Web API path should be used.
pub enum PlaybackBackend {
  /// Drive transport through the embedded librespot player.
  Native(Arc<StreamingPlayer>),
  /// Drive transport through the Spotify Web API.
  Connect,
}

/// Decide whether a symmetric transport operation should use the native player.
///
/// This reproduces the original inline guard exactly: native streaming is used
/// only when it is the active device *and* a player handle is present. When the
/// player is absent (even if native streaming is nominally active) the operation
/// falls through to the Web API, matching the historical
/// `if is_native_active { if let Some(player) { .. } }` fallthrough.
pub fn select_native(is_native_active: bool, player_present: bool) -> bool {
  is_native_active && player_present
}

#[cfg(test)]
mod tests {
  use super::select_native;

  #[test]
  fn selects_native_only_when_active_and_player_present() {
    assert!(select_native(true, true));
  }

  #[test]
  fn falls_through_to_connect_when_player_absent() {
    // Mirrors the historical `if let Some(player)` fallthrough: even when native
    // streaming is active, a missing player routes to the Web API.
    assert!(!select_native(true, false));
  }

  #[test]
  fn uses_connect_when_native_not_active() {
    assert!(!select_native(false, true));
  }

  #[test]
  fn uses_connect_when_neither_condition_holds() {
    assert!(!select_native(false, false));
  }
}
