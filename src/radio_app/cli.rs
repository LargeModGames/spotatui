use super::config::{self, Station};
use super::mpris;
use anyhow::{bail, Context, Result};
use clap::{Arg, ArgAction, ArgMatches, Command};
use std::path::PathBuf;

pub fn build() -> Command {
  Command::new("spotatui")
    .version(env!("CARGO_PKG_VERSION"))
    .author(env!("CARGO_PKG_AUTHORS"))
    .about("Internet radio for the terminal")
    .arg(
      Arg::new("config")
        .long("config")
        .value_name("PATH")
        .global(true)
        .help("Use a custom config.yml"),
    )
    .subcommand(
      Command::new("radio")
        .about("Lists stations or controls a running Spotatui instance")
        .subcommand_required(true)
        .subcommand(
          Command::new("list")
            .about("Lists saved radio stations")
            .arg(
              Arg::new("json")
                .long("json")
                .action(ArgAction::SetTrue)
                .help("Print JSON"),
            ),
        )
        .subcommand(
          Command::new("play")
            .about("Plays a stream in the running Spotatui instance")
            .arg(Arg::new("url").value_name("URL").required(true)),
        ),
    )
}

pub async fn handle(matches: &ArgMatches) -> Result<bool> {
  let Some(radio) = matches.subcommand_matches("radio") else {
    return Ok(false);
  };
  match radio.subcommand() {
    Some(("list", args)) => {
      let config_path = matches.get_one::<String>("config").map(PathBuf::from);
      let loaded = config::load(config_path.as_deref())?;
      println!(
        "{}",
        render_stations(&loaded.stations, args.get_flag("json"))?
      );
    }
    Some(("play", args)) => {
      let value = args.get_one::<String>("url").expect("required by clap");
      let uri = normalized_uri(value)?;
      mpris::open_uri(&uri).await?;
      println!("Radio playback requested.");
    }
    _ => unreachable!("radio requires a subcommand"),
  }
  Ok(true)
}

fn render_stations(stations: &[Station], json: bool) -> Result<String> {
  if json {
    return serde_json::to_string(stations).context("serializing station list");
  }
  if stations.is_empty() {
    return Ok("No saved radio stations.".to_owned());
  }
  Ok(
    stations
      .iter()
      .map(|station| format!("{}\t{}", station.name, station.url))
      .collect::<Vec<_>>()
      .join("\n"),
  )
}

pub fn normalized_uri(value: &str) -> Result<String> {
  let value = value.trim();
  let url = value.strip_prefix("radio:").unwrap_or(value);
  let parsed = url::Url::parse(url).context("invalid radio stream URL")?;
  if !matches!(parsed.scheme(), "http" | "https") {
    bail!("radio stream URL must use http or https");
  }
  Ok(format!("radio:{url}"))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn normalizes_radio_urls() {
    assert_eq!(
      normalized_uri("https://example.com/live").unwrap(),
      "radio:https://example.com/live"
    );
    assert_eq!(
      normalized_uri("radio:http://example.com/live").unwrap(),
      "radio:http://example.com/live"
    );
    assert!(normalized_uri("file:///tmp/test.mp3").is_err());
  }

  #[test]
  fn renders_station_json_for_widget_consumers() {
    let stations = vec![Station {
      name: "Test FM".to_owned(),
      url: "https://example.com/live".to_owned(),
    }];
    assert_eq!(
      render_stations(&stations, true).unwrap(),
      r#"[{"name":"Test FM","url":"https://example.com/live"}]"#
    );
  }
}
