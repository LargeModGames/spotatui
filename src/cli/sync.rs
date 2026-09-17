use clap::{Arg, ArgAction, ArgMatches, Command};

/// What `spotatui sync` was asked to do.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncArgs {
  /// A master playlist name or a link id; `None` runs every link.
  pub link: Option<String>,
  pub dry_run: bool,
}

pub fn sync_subcommand() -> Command {
  Command::new("sync")
    .about("Sync your linked playlists across sources")
    .arg(
      Arg::new("link")
        .long("link")
        .value_name("NAME")
        .help("Sync only the link with this master playlist name or id"),
    )
    .arg(
      Arg::new("dry-run")
        .long("dry-run")
        .action(ArgAction::SetTrue)
        .help("Report what would change without writing anything"),
    )
}

/// Read the parsed arguments off the matches.
pub fn sync_args(matches: &ArgMatches) -> SyncArgs {
  SyncArgs {
    link: matches.get_one::<String>("link").cloned(),
    dry_run: matches.get_flag("dry-run"),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn parse(args: &[&str]) -> Result<SyncArgs, clap::Error> {
    let matches = Command::new("spotatui")
      .subcommand(sync_subcommand())
      .try_get_matches_from(args)?;
    Ok(sync_args(
      matches
        .subcommand_matches("sync")
        .expect("the sync subcommand"),
    ))
  }

  #[test]
  fn sync_takes_an_optional_link_and_a_dry_run_flag() {
    assert_eq!(
      parse(&["spotatui", "sync"]).expect("a plain sync parses"),
      SyncArgs {
        link: None,
        dry_run: false,
      }
    );
    assert_eq!(
      parse(&["spotatui", "sync", "--link", "Late Night", "--dry-run"])
        .expect("both arguments parse"),
      SyncArgs {
        link: Some("Late Night".to_string()),
        dry_run: true,
      }
    );
  }

  #[test]
  fn sync_rejects_an_unknown_flag() {
    assert!(parse(&["spotatui", "sync", "--force"]).is_err());
  }
}
