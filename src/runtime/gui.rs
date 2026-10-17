//! The browser frontend's entry point: the shared boot and frontend runtime,
//! the loopback server for the page, and a tick loop without a terminal.

use super::bootstrap::{self, BootOptions};
use super::instance;
use super::startup::{prepare_frontend, shutdown_frontend, FrontendHandles};
use crate::core::action::Action;
use crate::core::app::{App, PlaybackOwner};
use crate::core::driver::{Driver, TickEnv};
use crate::core::i18n::{system_language, Language};
use crate::core::onboarding::Onboarding;
use crate::core::update::{UpdateAsk, UpdateRequest, UpdateStatus};
use crate::core::user_config::{parse_update_delay_secs, BehaviorConfig};
use crate::core::visualizer_look::LookFile;
use crate::gui::bridge::{self, Publisher};
use crate::gui::onboarding::BrowserOnboarding;
use crate::gui::protocol::ClientMessage;
pub use crate::gui::protocol::WindowMode;
use crate::gui::server;
use crate::gui::visualizer::{Feed, FRAME_EVERY};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::Mutex;

/// Shows the page at the launch URL in the shell's window.
pub type ShowPage = Box<dyn FnOnce(&str) -> Result<()> + Send>;

/// What a desktop shell hands `run_gui_hosted` in place of the system browser.
pub struct GuiHost {
  pub show: ShowPage,
  /// Called once boot is done and the page is served.
  pub serving: Box<dyn FnOnce() + Send>,
  /// Turns true when the window closed; a dropped sender counts as closed.
  pub closed: tokio::sync::watch::Receiver<bool>,
  pub updates: Option<UpdateLink>,
  /// Resizes the shell's window when the page asks for the mini player.
  pub window: Option<SetWindow>,
  pub media: Option<MediaLink>,
}

/// Sets the shell's window to a mode the page asked for.
pub type SetWindow = Box<dyn Fn(WindowMode) + Send>;

/// The app's half of a desktop shell's updater.
pub struct UpdateLink {
  pub status: tokio::sync::watch::Receiver<UpdateStatus>,
  pub requests: UnboundedSender<UpdateRequest>,
}

/// The app's half of a shell's system media controls, such as a phone's lock screen.
pub struct MediaLink {
  /// The track this process plays, or `None` while it plays nothing here.
  pub state: tokio::sync::watch::Sender<Option<MediaState>>,
  pub commands: UnboundedReceiver<MediaCommand>,
}

/// What the system media controls show.
#[derive(Clone, Debug, PartialEq)]
pub struct MediaState {
  pub title: String,
  pub artists: Vec<String>,
  pub album: String,
  pub artwork_url: Option<String>,
  pub duration_ms: u64,
  pub position_ms: u64,
  pub playing: bool,
}

/// A press on the system media controls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaCommand {
  Play,
  Pause,
  Next,
  Previous,
  SeekTo(u64),
}

/// How far the position may move from where the controls extrapolate it before it is sent again.
const MEDIA_POSITION_DRIFT_MS: u64 = 2000;

/// How often a running app looks for an update.
const UPDATE_CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// How often `visualizer.yml` is checked for an edit.
const LOOK_CHECK_EVERY: Duration = Duration::from_secs(1);

pub async fn run_gui() -> Result<()> {
  run(None).await
}

/// The GUI inside a desktop shell's window instead of the system browser.
pub async fn run_gui_hosted(host: GuiHost) -> Result<()> {
  run(Some(host)).await
}

async fn run(host: Option<GuiHost>) -> Result<()> {
  // The crash notice's "rerun with --debug" must still work.
  super::start_logging(std::env::args().skip(1).any(|arg| arg == "--debug"))?;
  log::info!("spotatui-gui {} starting up", env!("CARGO_PKG_VERSION"));
  bootstrap::init_audio_backend();
  bootstrap::install_panic_hook();
  let result = launch_gui(host).await;
  if let Err(e) = &result {
    // A windowed binary has no stderr to show this on.
    log::error!("{e:#}");
  }
  result
}

fn launch_url(port: u16, code: &str) -> String {
  format!("http://127.0.0.1:{port}/#code={code}")
}

/// Only the language is deserialized here; boot remains the owner of full config
/// validation and reports errors. This never creates paths or logs config data.
fn preload_language(path: Option<&Path>, system: Language) -> Language {
  #[derive(Default, serde::Deserialize)]
  struct Behavior {
    #[serde(default)]
    language: Language,
  }
  #[derive(serde::Deserialize)]
  struct Config {
    behavior: Option<Behavior>,
  }
  path
    .and_then(|path| std::fs::read_to_string(path).ok())
    .and_then(|text| serde_yaml::from_str::<Config>(&text).ok())
    .and_then(|config| config.behavior)
    .unwrap_or_default()
    .language
    .resolve(system)
}

async fn launch_gui(host: Option<GuiHost>) -> Result<()> {
  let hosted = host.is_some();
  #[cfg(all(feature = "macos-media", target_os = "macos"))]
  if hosted {
    crate::infra::macos_media::HOST_OWNS_NSAPP.store(true, std::sync::atomic::Ordering::SeqCst);
  }
  let (show, serving, closed, updates, window, media) = match host {
    Some(GuiHost {
      show,
      serving,
      closed,
      updates,
      window,
      media,
    }) => (
      Some(show),
      Some(serving),
      Some(closed),
      updates,
      window,
      media,
    ),
    None => (None, None, None, None, None, None),
  };
  let mut instance_lock = instance::acquire()?;
  if let Err(e) = crate::core::migrations::apply_legacy_state_file_migrations() {
    log::warn!("[state] failed to migrate legacy app data files: {e}");
  }
  // The page answers the first-launch questions, so it must be up before boot asks them.
  let default_config = crate::core::paths::app_config_dir().map(|dir| dir.join("config.yml"));
  let system_language = system_language();
  let language = preload_language(default_config.as_deref(), system_language);
  let onboarding = Arc::new(BrowserOnboarding::with_language(language));
  let (boot_done, apps) = tokio::sync::watch::channel(None);
  let (link, publisher, inbox) = bridge::channel(apps, Arc::clone(&onboarding), window.is_some());
  let (port, access) = server::spawn_listener(server::ASSETS, link).await?;
  let url = launch_url(port, &access.issue(Instant::now())?);
  match show {
    Some(show) => show(&url)?,
    None => {
      if let Err(e) = open::that_detached(&url) {
        log::warn!("could not open the browser: {e}");
        eprintln!("{}", language.strings().gui.open_browser(&url));
      }
    }
  }
  // No auto-update: it installs the console `spotatui` binary over this one and re-execs it.
  let options = BootOptions {
    system_language: Some(system_language),
    config_path: None,
    tick_rate: None,
    subcommand: None,
    reconfigure_auth: false,
    play_file: None,
    no_update: true,
  };
  let boot = match bootstrap::boot(options, onboarding.clone(), &mut instance_lock).await {
    Ok(boot) => boot,
    // The shell shows the error in a window of its own.
    Err(e) if hosted => return Err(e),
    Err(e) => {
      onboarding.info(&language.strings().gui.start_failed(&format!("{e:#}")));
      // A browser that is still starting has not connected yet.
      onboarding
        .wait_for_page(crate::gui::onboarding::NO_PAGE_GRACE)
        .await;
      // Long enough for the line to reach the page before the process exits.
      tokio::time::sleep(Duration::from_secs(2)).await;
      return Err(e);
    }
  };
  let FrontendHandles {
    app,
    user_config,
    mpris,
    discord,
    history_collector,
  } = prepare_frontend(boot).await;
  // Read before the page sees the app, so it never draws the default look first.
  let mut look = if crate::gui::visualizer::AVAILABLE {
    crate::core::paths::visualizer_config_path().map(|path| (path, LookFile::default()))
  } else {
    None
  };
  if let Some(loaded) = look.as_mut().and_then(|(path, file)| file.check(path)) {
    app.lock().await.apply_visualizer_look(loaded);
  }
  boot_done.send_replace(Some(Arc::clone(&app)));
  if let Some(serving) = serving {
    serving();
  }
  let tick_rate = Duration::from_millis(user_config.behavior.tick_rate_milliseconds);
  let mut driver = Driver::new(mpris, discord);
  serve_until_quit(
    &app,
    &mut driver,
    publisher,
    inbox,
    tick_rate,
    look,
    ShellLinks {
      closed,
      updates,
      window,
      media,
    },
  )
  .await;
  app.lock().await.close_io_channel();
  shutdown_frontend(&app, &mut driver, history_collector, |_| {}).await;
  drop(driver);
  #[cfg(feature = "mcp-server")]
  crate::infra::mcp::clear_handshake();
  Ok(())
}

/// What a desktop shell links to the tick loop; all `None` in a browser.
struct ShellLinks {
  closed: Option<tokio::sync::watch::Receiver<bool>>,
  updates: Option<UpdateLink>,
  window: Option<SetWindow>,
  media: Option<MediaLink>,
}

/// Ticks the driver, pushes to the page and applies its actions until a quit.
async fn serve_until_quit(
  app: &Arc<Mutex<App>>,
  driver: &mut Driver,
  mut publisher: Publisher,
  mut inbox: UnboundedReceiver<ClientMessage>,
  tick_rate: Duration,
  mut look: Option<(PathBuf, LookFile)>,
  shell: ShellLinks,
) {
  let ShellLinks {
    closed,
    updates,
    window,
    media,
  } = shell;
  let (media_state, mut media_commands) = match media {
    Some(MediaLink { state, commands }) => (Some(state), Some(commands)),
    None => (None, None),
  };
  let mut media_sent_at = Instant::now();
  // A shell's window owns the page's life, so a page gone a minute is no reason to quit.
  let hosted = closed.is_some();
  let window_closed = async {
    match closed {
      Some(mut closed) => {
        let _ = closed.wait_for(|closed| *closed).await;
      }
      None => std::future::pending::<()>().await,
    }
  };
  tokio::pin!(window_closed);
  driver.dispatch_startup(&mut *app.lock().await);
  let mut ticks = tokio::time::interval(tick_rate);
  ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
  let mut last_tick = Instant::now();
  let mut visualizer = Feed::new();
  let mut frame_clock = tokio::time::interval(FRAME_EVERY);
  frame_clock.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
  // Never completes when the handler cannot register: a completed future must not be polled again.
  let ctrl_c = async {
    if let Err(e) = tokio::signal::ctrl_c().await {
      log::warn!("Ctrl-C handler unavailable: {e}");
      std::future::pending::<()>().await;
    }
  };
  tokio::pin!(ctrl_c);
  let (mut update_status, update_requests) = match updates {
    Some(UpdateLink { status, requests }) => (Some(status), Some(requests)),
    None => (None, None),
  };
  if let Some(status) = &mut update_status {
    app
      .lock()
      .await
      .set_update_status(status.borrow_and_update().clone());
  }
  let ask_updater = |request: UpdateRequest| {
    if let Some(requests) = &update_requests {
      let _ = requests.send(request);
    }
  };
  // The first tick is at once: a check at launch.
  let mut update_checks = tokio::time::interval(UPDATE_CHECK_EVERY);
  update_checks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
  let mut last_check = None;
  let mut look_checks = tokio::time::interval_at(
    tokio::time::Instant::now() + LOOK_CHECK_EVERY,
    LOOK_CHECK_EVERY,
  );
  look_checks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
  loop {
    tokio::select! {
      _ = look_checks.tick(), if look.is_some() => {
        if let Some(loaded) = look.as_mut().and_then(|(path, file)| file.check(path)) {
          let mut app = app.lock().await;
          app.apply_visualizer_look(loaded);
          publisher.publish(&app);
        }
      }
      _ = ticks.tick() => {
        // A shell runs the main run loop itself.
        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        if !hosted {
          use objc2_foundation::{NSDate, NSRunLoop};
          NSRunLoop::currentRunLoop().runUntilDate(&NSDate::dateWithTimeIntervalSinceNow(0.001));
        }
        let now = Instant::now();
        let mut app = app.lock().await;
        driver.tick(
          &mut app,
          now - last_tick,
          TickEnv {
            now,
            #[cfg(any(feature = "audio-viz", feature = "audio-viz-cpal"))]
            viz: visualizer
              .wants_capture(now)
              .then_some(crate::core::driver::VizNeed::Samples),
            #[cfg(feature = "art-decode")]
            cover_art_full_image_support: true,
          },
        );
        // After the tick, which opens, polls or closes the capture.
        visualizer.note(&app, plays_here(&app), publisher.visualizer_watched(), now);
        last_tick = now;
        publisher.publish(&app);
        publisher.tick(&app, now);
        send_media(media_state.as_ref(), &app, &mut media_sent_at);
        if !hosted && publisher.abandoned(now, plays_here(&app)) {
          log::info!("no page connected and nothing played here for a minute; quitting");
          break;
        }
      }
      _ = frame_clock.tick(), if visualizer.streams(publisher.visualizer_watched(), Instant::now()) => {
        if let Some(frame) = visualizer.frame(driver, Instant::now()) {
          publisher.frame(&frame);
        }
      }
      _ = update_checks.tick(), if update_requests.is_some() => {
        if let Some(check) = scheduled_check(&app.lock().await.user_config.behavior) {
          last_check = Some(check);
          ask_updater(check);
        }
      }
      status = async {
        match update_status.as_mut() {
          Some(status) => status.changed().await.ok().map(|()| status.borrow_and_update().clone()),
          None => std::future::pending().await,
        }
      } => match status {
        Some(status) => {
          let mut app = app.lock().await;
          app.set_update_status(status);
          publisher.publish(&app);
        }
        // The shell dropped its updater.
        None => update_status = None,
      },
      command = async {
        match media_commands.as_mut() {
          Some(commands) => commands.recv().await,
          None => std::future::pending().await,
        }
      } => match command {
        Some(command) => {
          let mut app = app.lock().await;
          if app.user_config.behavior.enable_media_keys {
            app.apply(media_action(command));
            publisher.publish(&app);
            send_media(media_state.as_ref(), &app, &mut media_sent_at);
          }
        }
        None => media_commands = None,
      },
      // A page that asked for something sees the answer when the pump has it.
      _ = super::pump::events_handled().notified() => {
        let app = app.lock().await;
        publisher.publish(&app);
        send_media(media_state.as_ref(), &app, &mut media_sent_at);
      }
      message = inbox.recv() => match message {
        Some(ClientMessage::Action { action }) => {
          let mut app = app.lock().await;
          app.apply(*action);
          driver.run_pending_script_commands(&mut app);
          publisher.publish(&app);
          // A click on the bar changes the visualizer's mode at once.
          visualizer.note(&app, plays_here(&app), publisher.visualizer_watched(), Instant::now());
          // A changed auto-update setting reaches the updater at once.
          if let Some(check) = changed_check(last_check, &app.user_config.behavior) {
            last_check = Some(check);
            ask_updater(check);
          }
        }
        // The socket answers these itself; boot is over by now.
        Some(
          ClientMessage::Onboarding { .. }
          | ClientMessage::Visualizer { .. }
          | ClientMessage::Changelog,
        ) => {}
        Some(ClientMessage::Update { ask }) => match ask {
          UpdateAsk::Check => {
            let check = update_check(&app.lock().await.user_config.behavior);
            last_check = Some(check);
            ask_updater(check);
          }
          UpdateAsk::Download => ask_updater(UpdateRequest::Download),
          UpdateAsk::Install => {
            if matches!(app.lock().await.update_status(), Some(UpdateStatus::Ready { .. })) {
              ask_updater(UpdateRequest::Install);
              break;
            }
          }
        },
        Some(ClientMessage::Window { mode }) => {
          if let Some(set) = &window {
            set(mode);
          }
        }
        Some(ClientMessage::Quit) | None => break,
      },
      () = &mut ctrl_c => break,
      () = &mut window_closed => break,
    }
  }
}

/// Whether this process makes the sound, so closing the last tab must not stop it.
fn plays_here(app: &App) -> bool {
  owns_sound(app)
    && app
      .playback_view()
      .0
      .as_ref()
      .is_some_and(|snapshot| snapshot.is_playing)
}

/// Whether a player in this process owns playback, not another Connect device.
fn owns_sound(app: &App) -> bool {
  matches!(
    app.playback_owner(),
    PlaybackOwner::Queue | PlaybackOwner::Decoded | PlaybackOwner::NativeSpotify
  )
}

/// Send the media controls `app`'s state when they must hear of it.
fn send_media(
  state: Option<&tokio::sync::watch::Sender<Option<MediaState>>>,
  app: &App,
  sent_at: &mut Instant,
) {
  let Some(state) = state else {
    return;
  };
  let now = Instant::now();
  let next = media_state_of(app);
  state.send_if_modified(|shown| {
    let changed = media_changed(shown.as_ref(), next.as_ref(), now - *sent_at);
    if changed {
      *shown = next;
      *sent_at = now;
    }
    changed
  });
}

/// The media controls' view: only a track this process plays, so a phone that is a remote shows none,
/// and none while the media keys setting is off.
fn media_state_of(app: &App) -> Option<MediaState> {
  if !app.user_config.behavior.enable_media_keys || !owns_sound(app) {
    return None;
  }
  let snapshot = app.playback_view().0.as_ref()?;
  let metadata = &snapshot.metadata;
  Some(MediaState {
    title: metadata.title.clone(),
    artists: metadata.artists.clone(),
    album: metadata.album.clone(),
    artwork_url: metadata.image_url.clone(),
    duration_ms: u64::from(metadata.duration_ms),
    position_ms: app.playback_position_ms().unwrap_or(snapshot.progress_ms) as u64,
    playing: snapshot.is_playing,
  })
}

/// Whether the controls must hear of `next`: anything but the position changed, or the
/// position left the line they extrapolate from what they were sent `since` ago.
fn media_changed(shown: Option<&MediaState>, next: Option<&MediaState>, since: Duration) -> bool {
  let (Some(shown), Some(next)) = (shown, next) else {
    return shown.is_some() != next.is_some();
  };
  let moved = if shown.playing {
    since.as_millis() as u64
  } else {
    0
  };
  shown.title != next.title
    || shown.artists != next.artists
    || shown.album != next.album
    || shown.artwork_url != next.artwork_url
    || shown.duration_ms != next.duration_ms
    || shown.playing != next.playing
    || next.position_ms.abs_diff(shown.position_ms + moved) > MEDIA_POSITION_DRIFT_MS
}

fn media_action(command: MediaCommand) -> Action {
  match command {
    MediaCommand::Play => Action::Play,
    MediaCommand::Pause => Action::Pause,
    MediaCommand::Next => Action::NextTrack,
    MediaCommand::Previous => Action::PreviousTrack,
    MediaCommand::SeekTo(ms) => Action::SeekTo(u32::try_from(ms).unwrap_or(u32::MAX)),
  }
}

/// The check the user's auto-update settings ask for.
fn update_check(behavior: &BehaviorConfig) -> UpdateRequest {
  UpdateRequest::Check {
    download: !behavior.disable_auto_update,
    delay_secs: parse_update_delay_secs(&behavior.auto_update_delay).unwrap_or(0),
  }
}

/// The check at launch and every 6 hours; none while auto-update is off.
fn scheduled_check(behavior: &BehaviorConfig) -> Option<UpdateRequest> {
  (!behavior.disable_auto_update).then(|| update_check(behavior))
}

/// The check a settings change sends: the new settings after an earlier check,
/// or the first check when auto-update is turned on.
fn changed_check(last: Option<UpdateRequest>, behavior: &BehaviorConfig) -> Option<UpdateRequest> {
  match last {
    Some(last) => Some(update_check(behavior)).filter(|check| *check != last),
    None => scheduled_check(behavior),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_first_page_uses_the_saved_language_before_app_boot() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("custom.yml");
    std::fs::write(&path, "behavior:\n  language: zh-Hant\n").unwrap();
    assert_eq!(
      preload_language(Some(&path), Language::En),
      Language::ZhHant
    );
    std::fs::write(&path, "behavior:\n  language: auto\n").unwrap();
    assert_eq!(
      preload_language(Some(&path), Language::ZhHans),
      Language::ZhHans
    );
    assert_eq!(preload_language(None, Language::En), Language::En);
  }

  fn behavior(disable_auto_update: bool) -> BehaviorConfig {
    let mut behavior = crate::core::user_config::UserConfig::new().behavior;
    behavior.disable_auto_update = disable_auto_update;
    behavior
  }

  #[test]
  fn no_scheduled_update_check_runs_while_auto_update_is_off() {
    assert_eq!(scheduled_check(&behavior(true)), None);
    assert_eq!(
      scheduled_check(&behavior(false)),
      Some(update_check(&behavior(false)))
    );
  }

  #[test]
  fn turning_auto_update_on_before_any_check_asks_for_one() {
    assert_eq!(changed_check(None, &behavior(true)), None);
    assert_eq!(
      changed_check(None, &behavior(false)),
      Some(update_check(&behavior(false)))
    );
  }

  #[test]
  fn a_settings_change_after_a_check_sends_only_a_different_check() {
    let on = update_check(&behavior(false));
    assert_eq!(changed_check(Some(on), &behavior(false)), None);
    assert_eq!(
      changed_check(Some(on), &behavior(true)),
      Some(update_check(&behavior(true)))
    );
  }

  #[test]
  fn the_launch_url_carries_the_code_in_the_fragment() {
    assert_eq!(launch_url(40513, "abc"), "http://127.0.0.1:40513/#code=abc");
  }

  fn media(position_ms: u64, playing: bool) -> MediaState {
    MediaState {
      title: "Song".into(),
      artists: vec!["Artist".into()],
      album: "Album".into(),
      artwork_url: None,
      duration_ms: 200_000,
      position_ms,
      playing,
    }
  }

  #[test]
  fn a_playing_position_that_moves_with_the_clock_is_not_sent_again() {
    let shown = media(10_000, true);
    let next = media(15_000, true);
    assert!(!media_changed(
      Some(&shown),
      Some(&next),
      Duration::from_secs(5)
    ));
  }

  #[test]
  fn a_seek_or_a_paused_position_change_is_sent() {
    let shown = media(10_000, true);
    assert!(media_changed(
      Some(&shown),
      Some(&media(60_000, true)),
      Duration::from_secs(5)
    ));
    let paused = media(10_000, false);
    assert!(media_changed(
      Some(&paused),
      Some(&media(15_000, false)),
      Duration::from_secs(5)
    ));
  }

  #[test]
  fn a_new_track_a_pause_or_the_end_of_local_playback_is_sent() {
    let shown = media(10_000, true);
    let other = MediaState {
      title: "Other".into(),
      ..media(10_000, true)
    };
    assert!(media_changed(Some(&shown), Some(&other), Duration::ZERO));
    assert!(media_changed(
      Some(&shown),
      Some(&media(10_000, false)),
      Duration::ZERO
    ));
    assert!(media_changed(Some(&shown), None, Duration::ZERO));
    assert!(!media_changed(None, None, Duration::ZERO));
  }

  #[test]
  fn media_buttons_become_the_page_actions() {
    assert!(matches!(media_action(MediaCommand::Play), Action::Play));
    assert!(matches!(media_action(MediaCommand::Pause), Action::Pause));
    assert!(matches!(
      media_action(MediaCommand::Next),
      Action::NextTrack
    ));
    assert!(matches!(
      media_action(MediaCommand::Previous),
      Action::PreviousTrack
    ));
    assert!(matches!(
      media_action(MediaCommand::SeekTo(u64::MAX)),
      Action::SeekTo(u32::MAX)
    ));
  }
}
