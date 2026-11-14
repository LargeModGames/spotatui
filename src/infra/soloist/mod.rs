//! Spotify Soloist as the engine behind spotatui's own Connect device.
//!
//! spotatui starts the Soloist binary the user downloaded, reads its device id
//! and WebSocket port from its stdout, and drives it over that loopback socket.
//! Rules for this module: `AGENTS.md` next to this file.

pub mod auth;
pub mod download;
pub mod link;
pub mod logline;
pub mod prefs;
pub mod process;
pub mod protocol;
pub mod startup;
pub mod supervisor;

use std::path::PathBuf;

pub use link::{SoloistLink, StreamQuality};

/// What the supervisor tells the user; the App words it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
  PathUnset,
  Missing(PathBuf),
  NotExecutable(PathBuf),
  WrongCpu,
  StartFailed(String),
  KeyMissing(PathBuf),
  NotPaired,
  Offline,
  BadKey,
  Expired,
  ExpiresSoon { count: u32, unit: String },
  DirInUse,
  KeepsExiting,
  NoWebSocket,
  Restarting,
  RestartingForQuality,
}
