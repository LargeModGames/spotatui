//! Spotify through the official client's internal API, over a librespot session.

mod catalog;
pub mod credentials;
mod json;
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

pub use router::route;
