//! The browser frontend's entry point: the shared boot and frontend runtime,
//! the loopback server for the page, and a tick loop without a terminal.

use super::bootstrap::{self, BootOptions};
use super::instance;
use super::startup::{prepare_frontend, shutdown_frontend, FrontendHandles};
use crate::core::app::{App, PlaybackOwner};
use crate::core::driver::{Driver, TickEnv};
use crate::core::onboarding::Onboarding;
use crate::gui::bridge::{self, Publisher};
use crate::gui::onboarding::BrowserOnboarding;
use crate::gui::protocol::ClientMessage;
use crate::gui::server;
use anyhow::Result;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedReceiver;
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
}

pub async fn run_gui() -> Result<()> {
  run(None).await
}

/// The GUI inside a desktop shell's window instead of the system browser.
pub async fn run_gui_hosted(host: GuiHost) -> Result<()> {
  run(Some(host)).await
}

async fn run(host: Option<GuiHost>) -> Result<()> {
  // No clap here: the crash notice's "rerun with --debug" must still work.
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

async fn launch_gui(host: Option<GuiHost>) -> Result<()> {
  let hosted = host.is_some();
  #[cfg(all(feature = "macos-media", target_os = "macos"))]
  if hosted {
    crate::infra::macos_media::HOST_OWNS_NSAPP.store(true, std::sync::atomic::Ordering::SeqCst);
  }
  let (show, serving, closed) = match host {
    Some(GuiHost {
      show,
      serving,
      closed,
    }) => (Some(show), Some(serving), Some(closed)),
    None => (None, None, None),
  };
  let mut instance_lock = instance::acquire()?;
  if let Err(e) = crate::core::migrations::apply_legacy_state_file_migrations() {
    log::warn!("[state] failed to migrate legacy app data files: {e}");
  }
  // The page answers the first-launch questions, so it must be up before boot asks them.
  let onboarding = Arc::new(BrowserOnboarding::new());
  let (boot_done, apps) = tokio::sync::watch::channel(None);
  let (link, publisher, inbox) = bridge::channel(apps, Arc::clone(&onboarding));
  let (port, access) = server::spawn_listener(server::ASSETS, link).await?;
  let url = launch_url(port, &access.issue(Instant::now())?);
  match show {
    Some(show) => show(&url)?,
    None => {
      if let Err(e) = open::that_detached(&url) {
        log::warn!("could not open the browser: {e}");
        eprintln!("Open {url} in a browser");
      }
    }
  }
  // No auto-update: it installs the console `spotatui` binary over this one and re-execs it.
  let options = BootOptions {
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
      onboarding.info(&format!("spotatui could not start: {e:#}"));
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
    shared_position,
    mpris,
    discord,
    history_collector,
  } = prepare_frontend(boot).await;
  boot_done.send_replace(Some(Arc::clone(&app)));
  if let Some(serving) = serving {
    serving();
  }
  let tick_rate = Duration::from_millis(user_config.behavior.tick_rate_milliseconds);
  let mut driver = Driver::new(shared_position, mpris, discord);
  serve_until_quit(&app, &mut driver, publisher, inbox, tick_rate, closed).await;
  app.lock().await.close_io_channel();
  shutdown_frontend(&app, &mut driver, history_collector, |_| {}).await;
  drop(driver);
  #[cfg(feature = "mcp-server")]
  crate::infra::mcp::clear_handshake();
  Ok(())
}

/// Ticks the driver, pushes to the page and applies its actions until a quit.
async fn serve_until_quit(
  app: &Arc<Mutex<App>>,
  driver: &mut Driver,
  mut publisher: Publisher,
  mut inbox: UnboundedReceiver<ClientMessage>,
  tick_rate: Duration,
  closed: Option<tokio::sync::watch::Receiver<bool>>,
) {
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
  // Never completes when the handler cannot register: a completed future must not be polled again.
  let ctrl_c = async {
    if let Err(e) = tokio::signal::ctrl_c().await {
      log::warn!("Ctrl-C handler unavailable: {e}");
      std::future::pending::<()>().await;
    }
  };
  tokio::pin!(ctrl_c);
  loop {
    tokio::select! {
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
            viz_bars: None,
            #[cfg(feature = "art-decode")]
            cover_art_full_image_support: true,
          },
        );
        last_tick = now;
        publisher.publish(&app);
        publisher.tick(&app, now);
        if !hosted && publisher.abandoned(now, plays_here(&app)) {
          log::info!("no page connected and nothing played here for a minute; quitting");
          break;
        }
      }
      message = inbox.recv() => match message {
        Some(ClientMessage::Action { action }) => {
          let mut app = app.lock().await;
          app.apply(*action);
          driver.run_pending_script_commands(&mut app);
          publisher.publish(&app);
        }
        // The socket answers these itself; boot is over by now.
        Some(ClientMessage::Onboarding { .. }) => {}
        Some(ClientMessage::Quit) | None => break,
      },
      () = &mut ctrl_c => break,
      () = &mut window_closed => break,
    }
  }
}

/// Whether this process makes the sound, so closing the last tab must not stop it.
fn plays_here(app: &App) -> bool {
  matches!(
    app.playback_owner(),
    PlaybackOwner::Queue | PlaybackOwner::Decoded | PlaybackOwner::NativeSpotify
  ) && app
    .playback_view()
    .0
    .as_ref()
    .is_some_and(|snapshot| snapshot.is_playing)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_launch_url_carries_the_code_in_the_fragment() {
    assert_eq!(launch_url(40513, "abc"), "http://127.0.0.1:40513/#code=abc");
  }
}
