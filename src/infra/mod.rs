pub mod audio;
#[cfg(feature = "discord-rpc")]
pub mod discord_rpc;
#[cfg(all(feature = "macos-media", target_os = "macos"))]
pub mod macos_media;
pub mod media_metadata;
#[cfg(all(feature = "mpris", target_os = "linux"))]
pub mod mpris;
pub mod network;
#[cfg(feature = "streaming")]
pub mod player;
pub mod redirect_uri;
