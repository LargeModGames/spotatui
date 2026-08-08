//! MCP server: drive spotatui from a coding agent.
//!
//! ```text
//! Claude Code ──stdio(MCP)──> `spotatui mcp` ──loopback TCP──> running TUI
//!                            (relay)                          (server + App)
//! ```
//!
//! The point of this module is that the DJ works **without an API key**: the
//! agent you already have installed and authenticated does the thinking, and
//! spotatui just exposes the player. The tools themselves live in
//! [`crate::infra::dj::tools`], shared with the in-TUI DJ.
//!
//! * [`protocol`] — wire shapes, versions, error codes, dual-era result shaping
//! * [`server`] — the JSON-RPC loop and request dispatch
//! * [`executor`] — how a tool call reaches the live player (or reports that it
//!   cannot)
//! * [`control`] — the TUI-side listener and its token file
//! * [`relay`] — the `spotatui mcp` subcommand
//!
//! Written against protocol revision `2026-07-28`, which removed the
//! `initialize` handshake and made MCP stateless. Because a legacy client
//! against a modern-only server fails outright, the server also answers
//! `initialize` — see [`protocol::Era`].

pub mod control;
pub mod executor;
pub mod protocol;
pub mod relay;
pub mod server;
pub mod status;

pub use control::{clear_handshake, spawn_listener};
pub use relay::run as run_relay;
pub use status::run as run_status;
