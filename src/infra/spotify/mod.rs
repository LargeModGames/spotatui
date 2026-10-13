//! Spotify through the official client's internal API, over a librespot session.

mod cast;
mod catalog;
pub mod credentials;
mod json;
mod lan;
mod library;
mod listening;
#[cfg(test)]
mod live_tests;
pub mod lyrics;
mod metadata;
mod player;
mod playlists;
mod router;
mod search;
pub mod session;
mod zeroconf;

pub use router::route;
