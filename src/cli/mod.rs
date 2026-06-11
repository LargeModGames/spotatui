mod clap;
mod cli_app;
mod handle;
mod history;
#[cfg(feature = "scripting")]
mod plugin;
#[cfg(feature = "self-update")]
mod update;
mod util;

pub use self::clap::{list_subcommand, play_subcommand, playback_subcommand, search_subcommand};
pub use self::history::{handle_history_matches, history_subcommand};
#[cfg(feature = "scripting")]
pub use self::plugin::{handle_plugin_command, plugin_subcommand};
use cli_app::CliApp;
pub use handle::handle_matches;
#[cfg(feature = "self-update")]
pub use update::{check_for_update, install_update_silent, UpdateOutcome};
