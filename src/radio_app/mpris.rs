use anyhow::{Context, Result};
use mpris_server::{Metadata, PlaybackStatus, Player};
use std::thread;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum Event {
  PlayPause,
  Play,
  Pause,
  Next,
  Previous,
  Stop,
  OpenUri(String),
  SetVolume(u8),
}

#[derive(Debug, Clone)]
enum Command {
  Metadata { title: String, station: String },
  PlaybackStatus(bool),
  Volume(u8),
  Stopped,
}

pub struct Manager {
  event_rx: std::sync::Mutex<Option<mpsc::UnboundedReceiver<Event>>>,
  command_tx: mpsc::UnboundedSender<Command>,
}

impl Manager {
  pub fn new() -> Result<Self> {
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let (command_tx, mut command_rx) = mpsc::unbounded_channel();

    thread::spawn(move || {
      let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to create MPRIS runtime");
      let local = tokio::task::LocalSet::new();
      local.block_on(&runtime, async move {
        let player = match Player::builder("degen_radio")
          .identity("Degen Radio")
          .desktop_entry("degen-radio")
          .can_play(true)
          .can_pause(true)
          .can_go_next(true)
          .can_go_previous(true)
          .can_seek(false)
          .can_control(true)
          .can_quit(false)
          .can_raise(false)
          .can_set_fullscreen(false)
          .build()
          .await
        {
          Ok(player) => player,
          Err(error) => {
            log::error!("failed to start MPRIS service: {error}");
            return;
          }
        };

        macro_rules! connect {
          ($method:ident, $event:expr) => {{
            let tx = event_tx.clone();
            player.$method(move |_player| {
              let _ = tx.send($event);
            });
          }};
        }
        connect!(connect_play_pause, Event::PlayPause);
        connect!(connect_play, Event::Play);
        connect!(connect_pause, Event::Pause);
        connect!(connect_next, Event::Next);
        connect!(connect_previous, Event::Previous);
        connect!(connect_stop, Event::Stop);

        let tx = event_tx.clone();
        player.connect_open_uri(move |_player, uri| {
          let _ = tx.send(Event::OpenUri(uri.to_string()));
        });
        let tx = event_tx;
        player.connect_set_volume(move |_player, volume| {
          let percent = (volume * 100.0).round().clamp(0.0, 100.0) as u8;
          let _ = tx.send(Event::SetVolume(percent));
        });

        tokio::task::spawn_local(player.run());
        while let Some(command) = command_rx.recv().await {
          match command {
            Command::Metadata { title, station } => {
              let metadata = Metadata::builder()
                .title(&title)
                .album(&station)
                .artist(vec![station.as_str()])
                .build();
              if let Err(error) = player.set_metadata(metadata).await {
                log::warn!("failed to update MPRIS metadata: {error}");
              }
            }
            Command::PlaybackStatus(playing) => {
              let status = if playing {
                PlaybackStatus::Playing
              } else {
                PlaybackStatus::Paused
              };
              if let Err(error) = player.set_playback_status(status).await {
                log::warn!("failed to update MPRIS playback status: {error}");
              }
            }
            Command::Volume(percent) => {
              if let Err(error) = player.set_volume(f64::from(percent) / 100.0).await {
                log::warn!("failed to update MPRIS volume: {error}");
              }
            }
            Command::Stopped => {
              if let Err(error) = player.set_playback_status(PlaybackStatus::Stopped).await {
                log::warn!("failed to stop MPRIS playback: {error}");
              }
            }
          }
        }
      });
    });

    Ok(Self {
      event_rx: std::sync::Mutex::new(Some(event_rx)),
      command_tx,
    })
  }

  pub fn take_events(&self) -> Option<mpsc::UnboundedReceiver<Event>> {
    self.event_rx.lock().ok()?.take()
  }

  pub fn set_metadata(&self, title: impl Into<String>, station: impl Into<String>) {
    let _ = self.command_tx.send(Command::Metadata {
      title: title.into(),
      station: station.into(),
    });
  }

  pub fn set_playing(&self, playing: bool) {
    let _ = self.command_tx.send(Command::PlaybackStatus(playing));
  }

  pub fn set_volume(&self, percent: u8) {
    let _ = self.command_tx.send(Command::Volume(percent));
  }

  pub fn set_stopped(&self) {
    let _ = self.command_tx.send(Command::Stopped);
  }
}

pub async fn open_uri(uri: &str) -> Result<()> {
  let connection = mpris_server::zbus::Connection::session()
    .await
    .context("connecting to the user D-Bus")?;
  let proxy = mpris_server::zbus::Proxy::new(
    &connection,
    "org.mpris.MediaPlayer2.degen_radio",
    "/org/mpris/MediaPlayer2",
    "org.mpris.MediaPlayer2.Player",
  )
  .await
  .context("connecting to the running Degen Radio MPRIS service")?;
  proxy
    .call_method("OpenUri", &(uri))
    .await
    .context("requesting radio playback from Degen Radio")?;
  Ok(())
}
