// The `cfg_attr(not(feature = "tui"), allow(dead_code))` modules below are
// the core surface whose only callers today live in `tui/`, so a headless
// build counts much of them as dead. Allowed per module rather than per item:
// every tui-enabled CI leg still lints them in full (anything dead in *all*
// builds still warns there), and the shared action layer is what reclaims
// this surface for other frontends as the GUI substrate migration proceeds.
// `action` is allowed dead without `scripting` (not `tui`): until the handler
// conversion sub-PRs land, the scripting engine and the DJ tools are its only
// producers, so a scripting-less leg counts the un-adopted surface as dead.
// The `default` and `all-sources` legs (scripting on) still lint it in full.
#[cfg_attr(not(feature = "scripting"), allow(dead_code))]
pub mod action;
#[cfg_attr(not(feature = "tui"), allow(dead_code))]
pub mod app;
#[cfg(feature = "art-decode")]
pub mod art;
pub mod auth;
pub mod banner;
pub mod config;
#[cfg(feature = "art-decode")]
pub mod cover_theme;
#[cfg_attr(not(feature = "tui"), allow(dead_code))]
pub mod driver;
pub mod first_run;
#[cfg_attr(not(feature = "tui"), allow(dead_code))]
pub mod format;
pub mod geometry;
#[cfg_attr(not(feature = "tui"), allow(dead_code))]
pub mod input;
pub mod limits;
pub mod migrations;
pub mod onboarding;
pub mod pagination;
pub mod paths;
#[cfg_attr(not(feature = "tui"), allow(dead_code))]
pub mod persisted_playback;
pub mod plugin_api;
pub mod queue;
#[cfg_attr(not(feature = "tui"), allow(dead_code))]
pub mod sort;
pub mod source;
#[cfg_attr(not(feature = "tui"), allow(dead_code))]
pub mod state;
#[cfg(test)]
pub mod test_helpers;
#[cfg_attr(not(feature = "tui"), allow(dead_code))]
pub mod theme;
#[cfg_attr(not(feature = "tui"), allow(dead_code))]
pub mod user_config;
