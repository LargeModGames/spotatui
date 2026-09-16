mod cli;
mod config;
mod directory;
mod mpris;
mod player;
mod stream;
#[cfg(feature = "tui")]
mod tui;

use anyhow::{anyhow, Context, Result};
use config::Station;
use player::{LocalPlayer, PreparedStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

pub async fn run_cli() -> Result<()> {
  setup_logging()?;
  let matches = cli::build().get_matches();
  if cli::handle(&matches).await? {
    return Ok(());
  }

  #[cfg(feature = "tui")]
  {
    let config_path = matches.get_one::<String>("config").map(PathBuf::from);
    return run(config_path).await;
  }
  #[cfg(not(feature = "tui"))]
  Err(anyhow!(
    "this build has no terminal UI; use `spotatui radio`"
  ))
}

fn setup_logging() -> Result<()> {
  let log_dir = dirs::state_dir()
    .context("cannot resolve the user state directory")?
    .join("spotatui");
  std::fs::create_dir_all(&log_dir)?;
  let log_file = fern::log_file(log_dir.join("spotatui.log"))?;
  fern::Dispatch::new()
    .level(log::LevelFilter::Info)
    .chain(log_file)
    .apply()
    .map_err(|error| anyhow!(error))?;
  Ok(())
}

#[cfg(feature = "tui")]
struct Session {
  station: Station,
  now_playing: Arc<Mutex<Option<String>>>,
  cancel: Arc<dyn Fn() + Send + Sync>,
}

#[cfg(feature = "tui")]
struct PreparedTune {
  generation: u64,
  station: Station,
  stream_name: Option<String>,
  now_playing: Arc<Mutex<Option<String>>>,
  cancel: Arc<dyn Fn() + Send + Sync>,
  prepared: PreparedStream,
}

#[cfg(feature = "tui")]
enum TuneResult {
  Ready(PreparedTune),
  Failed { generation: u64, message: String },
}

#[cfg(feature = "tui")]
async fn run(config_path: Option<PathBuf>) -> Result<()> {
  let loaded = config::load(config_path.as_deref())?;
  let player = Arc::new(LocalPlayer::new().context("opening the default audio output")?);
  player.set_volume(loaded.volume_percent);

  let mpris = Arc::new(mpris::Manager::new()?);
  let mpris_rx = mpris
    .take_events()
    .context("MPRIS event receiver already taken")?;

  let (tune_tx, tune_rx) = mpsc::unbounded_channel();
  let state = tui::State::new(loaded.stations, loaded.volume_percent);
  tui::run(state, player, tune_tx, tune_rx, mpris, mpris_rx).await
}

#[cfg(feature = "tui")]
fn spawn_tune(generation: u64, station: Station, tx: mpsc::UnboundedSender<TuneResult>) {
  tokio::spawn(async move {
    let result = async {
      let opened = stream::open_radio_stream(&station.url).await?;
      let stream::OpenedStream {
        reader,
        cancel,
        now_playing,
        content_type,
        station_name,
      } = opened;
      let cancel: Arc<dyn Fn() + Send + Sync> = Arc::from(cancel);
      let decode = tokio::task::spawn_blocking(move || {
        LocalPlayer::prepare_stream(reader, content_type.as_deref(), None)
      });
      let prepared = match tokio::time::timeout(stream::PROBE_TIMEOUT, decode).await {
        Ok(result) => result.context("radio decoder task failed")??,
        Err(_) => {
          cancel();
          anyhow::bail!("timed out while detecting the stream audio format");
        }
      };
      Ok::<PreparedTune, anyhow::Error>(PreparedTune {
        generation,
        station,
        stream_name: station_name,
        now_playing,
        cancel,
        prepared,
      })
    }
    .await;

    let message = match result {
      Ok(prepared) => TuneResult::Ready(prepared),
      Err(error) => TuneResult::Failed {
        generation,
        message: format!("Could not play station: {error:#}"),
      },
    };
    let _ = tx.send(message);
  });
}
