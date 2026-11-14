//! Native Spotify playback using librespot
//!
//! This module provides native audio playback capabilities using the librespot library.
//! It registers spotatui as a Spotify Connect device and handles audio streaming.

#[cfg(feature = "streaming")]
pub mod backend;
#[cfg(feature = "streaming")]
mod events;
#[cfg(all(feature = "streaming", target_os = "windows"))]
mod sink;
// Keep the Windows sink's pure wait policy testable on other platforms.
#[cfg(all(feature = "streaming", any(target_os = "windows", test)))]
mod sink_wait;
#[cfg(feature = "streaming")]
mod streaming;

#[cfg(feature = "streaming")]
pub use backend::{select_native, PlaybackBackend};
#[cfg(feature = "streaming")]
pub use events::*;
#[cfg(feature = "streaming")]
pub use streaming::*;
