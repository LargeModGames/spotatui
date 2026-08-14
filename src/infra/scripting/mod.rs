//! Lua scripting engine (Phase 1).
//!
//! Embeds `mlua` and exposes a curated `spotatui.*` API to user plugins. Lua never sees
//! `&mut App` or rspotify types: reads come from a cached snapshot of the [`plugin_api`]
//! facade, and actions are queued as [`ScriptEffect`]s that the driver drains while holding
//! `&mut App`. Every Rust->Lua callback is wrapped in `catch_unwind`; a misbehaving plugin
//! logs an error, surfaces a status message, and is disabled (one strike) rather than
//! crashing the TUI.

mod api;
mod effects;
mod engine;
mod events;
mod shared;

pub use engine::ScriptEngine;

#[cfg(test)]
mod tests;
