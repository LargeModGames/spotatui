pub mod audio;
#[cfg(feature = "discord-rpc")]
pub mod discord_rpc;
#[cfg(feature = "dj-core")]
pub mod dj;
// The collector half (spawned by the frontend launch) has no caller in a
// headless build; the stats half still serves the CLI. Every tui-enabled CI
// leg lints this module in full.
#[cfg_attr(not(feature = "tui"), allow(dead_code))]
pub mod history;
#[cfg(feature = "local-files")]
pub mod local;
#[cfg(all(feature = "macos-media", target_os = "macos"))]
pub mod macos_media;
#[cfg(feature = "mcp-server")]
pub mod mcp;
pub mod media_metadata;
#[cfg(all(feature = "mpris", target_os = "linux"))]
pub mod mpris;
pub mod network;
#[cfg(feature = "streaming")]
pub mod player;
#[cfg(feature = "qobuz")]
pub mod qobuz;
pub mod queue;
#[cfg(feature = "internet-radio")]
pub mod radio;
pub mod redirect_uri;
#[cfg(feature = "scripting")]
pub mod scripting;
#[cfg(feature = "subsonic")]
pub mod subsonic;
#[cfg(feature = "youtube")]
pub mod youtube;
