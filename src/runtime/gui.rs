//! The browser frontend's entry point: the shared boot and frontend runtime,
//! the loopback server for the page, and a tick loop without a terminal.

use super::bootstrap::{self, BootOptions};
use super::instance;
use super::startup::{prepare_frontend, shutdown_frontend, FrontendHandles};
use crate::core::app::App;
use crate::core::driver::{Driver, TickEnv};
use crate::core::onboarding::{Onboarding, OnboardingAnswer, OnboardingPrompt};
use crate::core::source::Source;
use crate::gui::bridge::{self, Publisher};
use crate::gui::protocol::ClientMessage;
use crate::gui::server;
use anyhow::{anyhow, Result};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::Mutex;

const SETUP_IN_TERMINAL: &str = "spotatui-gui cannot run the first-launch setup yet: finish it once in the terminal `spotatui` (a build with the terminal UI), then start spotatui-gui again";

/// The first-launch surface until the page can answer: every question fails boot with the terminal hint.
pub(super) struct TerminalSetupRequired;

impl Onboarding for TerminalSetupRequired {
  fn info(&self, text: &str) {
    log::info!("{text}");
  }

  fn progress(&self, text: &str) {
    log::info!("{text}");
  }

  fn prompt_line(&self, _prompt: &str) -> Result<String> {
    Err(anyhow!(SETUP_IN_TERMINAL))
  }

  // True so the song-counter question fails boot instead of silently opting the user out.
  fn is_interactive(&self) -> bool {
    true
  }

  fn ask(&self, _prompt: &OnboardingPrompt) -> Result<OnboardingAnswer> {
    Err(anyhow!(SETUP_IN_TERMINAL))
  }

  fn pick_sources(&self, _options: &[Source]) -> Result<Option<Vec<Source>>> {
    Err(anyhow!(SETUP_IN_TERMINAL))
  }
}

pub async fn run_gui() -> Result<()> {
  bootstrap::setup_logging()?;
  log::info!("spotatui-gui {} starting up", env!("CARGO_PKG_VERSION"));
  bootstrap::init_audio_backend();
  bootstrap::install_panic_hook();
  let result = launch_gui().await;
  if let Err(e) = &result {
    // A windowed binary has no stderr to show this on.
    log::error!("{e:#}");
  }
  result
}

async fn launch_gui() -> Result<()> {
  let mut instance_lock = instance::acquire()?;
  if let Err(e) = crate::core::migrations::apply_legacy_state_file_migrations() {
    log::warn!("[state] failed to migrate legacy app data files: {e}");
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
  let boot = bootstrap::boot(options, Arc::new(TerminalSetupRequired), &mut instance_lock).await?;
  let FrontendHandles {
    app,
    user_config,
    shared_position,
    mpris,
    discord,
    history_collector,
  } = prepare_frontend(boot).await;
  let tick_rate = Duration::from_millis(user_config.behavior.tick_rate_milliseconds);
  let mut driver = Driver::new(shared_position, mpris, discord);
  let (link, publisher, inbox) = bridge::channel(Arc::clone(&app));
  let result = match server::spawn_listener(server::ASSETS, link).await {
    Ok((port, access)) => match access.issue(Instant::now()) {
      Ok(code) => {
        let url = format!("http://127.0.0.1:{port}/#code={code}");
        if let Err(e) = open::that_detached(&url) {
          log::warn!("could not open the browser: {e}");
          eprintln!("Open {url} in a browser");
        }
        serve_until_quit(&app, &mut driver, publisher, inbox, tick_rate).await;
        Ok(())
      }
      Err(e) => Err(e),
    },
    Err(e) => Err(e),
  };
  // Also on a serve error: prepare_frontend already started librespot, the pump and the MCP listener.
  app.lock().await.close_io_channel();
  shutdown_frontend(&app, &mut driver, history_collector, |_| {}).await;
  drop(driver);
  #[cfg(feature = "mcp-server")]
  crate::infra::mcp::clear_handshake();
  result
}

/// Ticks the driver, pushes to the page and applies its actions until a quit.
async fn serve_until_quit(
  app: &Arc<Mutex<App>>,
  driver: &mut Driver,
  mut publisher: Publisher,
  mut inbox: UnboundedReceiver<ClientMessage>,
  tick_rate: Duration,
) {
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
        #[cfg(all(feature = "macos-media", target_os = "macos"))]
        {
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
        if publisher.abandoned(now) {
          log::info!("no page connected for a minute; quitting");
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
        Some(ClientMessage::Quit) | None => break,
      },
      () = &mut ctrl_c => break,
    }
  }
}
