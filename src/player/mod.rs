//! Native Spotify playback using librespot
//!
//! This module provides native audio playback capabilities using the librespot library.
//! It registers spotatui as a Spotify Connect device and handles audio streaming.

#[cfg(feature = "streaming")]
mod streaming;

#[cfg(feature = "streaming")]
pub use streaming::*;
