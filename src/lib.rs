//! spotatui as a library: one core shared by every frontend.
//!
//! The modules stay private; the public API is exactly the entry points the
//! bin shims in `src/bin/` need, nothing else. The terminal frontend is the
//! only frontend module so far: a build without `tui` (the headless CI leg)
//! turns any import of it from core/infra/cli into a compile error, which is
//! what keeps a second frontend from re-coupling to this one.

mod cli;
mod core;
#[cfg(test)]
mod gates;
mod infra;
mod runtime;
#[cfg(feature = "tui")]
mod tui;

pub use runtime::run_cli;
