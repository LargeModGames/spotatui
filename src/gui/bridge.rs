//! Pushes from the tick loop to every page socket, and page messages back.

use crate::core::app::{App, DisplayRevisions};
use crate::core::i18n::Language;
use crate::gui::onboarding::BrowserOnboarding;
use crate::gui::protocol::{self, ClientMessage, ServerMessage};
use futures::{SinkExt, StreamExt};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, mpsc, watch, Mutex};
use tokio_tungstenite::{tungstenite::Message as WsMessage, WebSocketStream};

const TICK_EVERY: Duration = Duration::from_millis(250);
/// Timer jitter: a 250 ms tick loop may arrive a few ms early.
const TICK_SLACK: Duration = Duration::from_millis(20);
const NO_PAGE_LIMIT: Duration = Duration::from_secs(60);

/// The app once boot has built it: `None` while the first-launch questions run.
pub(crate) type SharedApp = watch::Receiver<Option<Arc<Mutex<App>>>>;

/// What a page socket needs: the app for its snapshots, the pushes, the questions, and the way back.
#[derive(Clone)]
pub(crate) struct Link {
  app: SharedApp,
  onboarding: Arc<BrowserOnboarding>,
  pushes: broadcast::Sender<String>,
  inbox: mpsc::UnboundedSender<ClientMessage>,
  /// A desktop shell can resize its window.
  window: bool,
  frames: watch::Receiver<Option<Arc<str>>>,
  watchers: Arc<AtomicUsize>,
}

impl Link {
  /// Boot's language is available even when the page assets are missing.
  pub(crate) async fn strings(&self) -> &'static crate::core::i18n::Strings {
    let app = self.app.borrow().clone();
    match app {
      Some(app) => app.lock().await.strings(),
      None => self.onboarding.language().strings(),
    }
  }
}

/// The tick loop's side of the bridge.
pub(crate) struct Publisher {
  pushes: broadcast::Sender<String>,
  sent: DisplayRevisions,
  language: Language,
  last_tick: Instant,
  /// When the last page went away; unset while one is connected or before the first check.
  alone_since: Option<Instant>,
  /// The newest visualizer frame. Kept off `pushes`: a page that falls behind
  /// skips frames instead of filling the backlog and resyncing every channel.
  frames: watch::Sender<Option<Arc<str>>>,
  /// The sockets that show the visualizer.
  watchers: Arc<AtomicUsize>,
  /// The Home page's lines as last pushed: they have no revision of their own.
  welcome: Option<protocol::WelcomePayload>,
}

/// Counts its socket among the visualizer's watchers while it lives.
struct Watching(Arc<AtomicUsize>);

impl Watching {
  fn new(watchers: &Arc<AtomicUsize>) -> Self {
    watchers.fetch_add(1, Ordering::Relaxed);
    Self(Arc::clone(watchers))
  }
}

impl Drop for Watching {
  fn drop(&mut self) {
    self.0.fetch_sub(1, Ordering::Relaxed);
  }
}

pub(crate) fn channel(
  app: SharedApp,
  onboarding: Arc<BrowserOnboarding>,
  window: bool,
) -> (Link, Publisher, mpsc::UnboundedReceiver<ClientMessage>) {
  let (pushes, _) = broadcast::channel(64);
  let (inbox, messages) = mpsc::unbounded_channel();
  let (frames, frame) = watch::channel(None);
  let watchers = Arc::new(AtomicUsize::new(0));
  let now = Instant::now();
  let publisher = Publisher {
    pushes: pushes.clone(),
    sent: DisplayRevisions::default(),
    language: onboarding.language(),
    last_tick: now,
    alone_since: None,
    frames,
    watchers: Arc::clone(&watchers),
    welcome: None,
  };
  (
    Link {
      app,
      onboarding,
      pushes,
      inbox,
      window,
      frames: frame,
      watchers,
    },
    publisher,
    messages,
  )
}

impl Publisher {
  /// Broadcast every channel that changed since the last call.
  pub(crate) fn publish(&mut self, app: &App) {
    for message in protocol::diff(&self.sent, app, self.language) {
      let _ = self.pushes.send(protocol::encode(&message));
    }
    self.sent = app.display_revisions();
    self.language = app.language();
    let welcome = protocol::welcome_payload(app);
    if self.welcome.as_ref() != Some(&welcome) {
      let _ = self
        .pushes
        .send(protocol::encode(&protocol::welcome(welcome.clone())));
      self.welcome = Some(welcome);
    }
  }

  /// Push the position at most every 250 ms, whatever the tick rate.
  pub(crate) fn tick(&mut self, app: &App, now: Instant) {
    if now.saturating_duration_since(self.last_tick) + TICK_SLACK >= TICK_EVERY {
      self.last_tick = now;
      let _ = self.pushes.send(protocol::encode(&protocol::tick(app)));
    }
  }

  /// True once, for a minute, no page socket has been open and this process played nothing.
  pub(crate) fn abandoned(&mut self, now: Instant, playing: bool) -> bool {
    if playing || self.pushes.receiver_count() > 0 {
      self.alone_since = None;
      return false;
    }
    now.saturating_duration_since(*self.alone_since.get_or_insert(now)) >= NO_PAGE_LIMIT
  }

  /// Whether a page shows the visualizer.
  pub(crate) fn visualizer_watched(&self) -> bool {
    self.watchers.load(Ordering::Relaxed) > 0
  }

  /// Offer a visualizer frame to the pages that show the visualizer.
  pub(crate) fn frame(&self, message: &ServerMessage) {
    self
      .frames
      .send_replace(Some(protocol::encode(message).into()));
  }
}

async fn resync(app: &SharedApp, language: Option<Language>) -> Vec<ServerMessage> {
  let app = app.borrow().clone();
  match app {
    Some(app) => protocol::resync(&*app.lock().await, language),
    None => Vec::new(),
  }
}

/// Serve one upgraded page socket until either side closes it.
pub(crate) async fn run_socket<S>(socket: WebSocketStream<S>, link: Link, token: Option<String>)
where
  S: AsyncRead + AsyncWrite + Unpin,
{
  let (mut sink, mut stream) = socket.split();
  let _page = link.onboarding.page_connected();
  // Subscribe before the snapshot: a push racing it is older and the page drops it.
  let mut pushes = link.pushes.subscribe();
  let mut views = link.onboarding.subscribe();
  let mut apps = link.app.clone();
  let mut frames = link.frames.clone();
  let mut watching = None;
  let app = apps.borrow_and_update().clone();
  let (revisions, language) = match app {
    Some(app) => {
      let app = app.lock().await;
      (app.display_revisions(), app.language())
    }
    None => (DisplayRevisions::default(), link.onboarding.language()),
  };
  let mut outgoing = vec![
    protocol::hello(revisions, token, link.window, language),
    protocol::onboarding(views.borrow_and_update().clone()),
  ];
  outgoing.extend(resync(&apps, Some(language)).await);
  loop {
    for message in outgoing.drain(..) {
      if sink
        .send(WsMessage::text(protocol::encode(&message)))
        .await
        .is_err()
      {
        return;
      }
    }
    tokio::select! {
      push = pushes.recv() => match push {
        Ok(text) => {
          if sink.send(WsMessage::text(text)).await.is_err() {
            return;
          }
        }
        Err(RecvError::Lagged(_)) => {
          // Stale ticks carry no revision; drop the backlog and start over.
          pushes = pushes.resubscribe();
          outgoing = resync(&apps, None).await;
        }
        Err(RecvError::Closed) => return,
      },
      Ok(()) = views.changed() => {
        outgoing.push(protocol::onboarding(views.borrow_and_update().clone()));
      }
      Ok(()) = apps.changed() => {
        apps.borrow_and_update();
        outgoing = resync(&apps, Some(language)).await;
      }
      Ok(()) = frames.changed(), if watching.is_some() => {
        let frame = frames.borrow_and_update().clone();
        if let Some(frame) = frame {
          if sink.send(WsMessage::text(frame.to_string())).await.is_err() {
            return;
          }
        }
      }
      incoming = stream.next() => match incoming {
        Some(Ok(WsMessage::Text(text))) => match serde_json::from_str::<ClientMessage>(&text) {
          // Answered here: boot waits on it before the tick loop exists.
          Ok(ClientMessage::Onboarding { reply }) => link.onboarding.answer(reply),
          // Per socket: the inbox does not know which page asked.
          Ok(ClientMessage::Visualizer { on }) => {
            if !on {
              watching = None;
            } else if watching.is_none() {
              // A frame from before the page asked is stale.
              frames.borrow_and_update();
              watching = Some(Watching::new(&link.watchers));
            }
          }
          // Only the page that asked gets the 240 KB answer.
          Ok(ClientMessage::Changelog) => outgoing.push(protocol::changelog()),
          Ok(message) => {
            let _ = link.inbox.send(message);
          }
          Err(error) => log::warn!("gui: unreadable page message: {error}"),
        },
        Some(Ok(_)) => {}
        Some(Err(_)) | None => return,
      },
    }
  }
}

#[cfg(test)]
pub(crate) mod tests {
  use super::*;
  use crate::core::user_config::UserConfig;
  use std::time::SystemTime;

  fn shared_app() -> Arc<Mutex<App>> {
    let (tx, _rx) = std::sync::mpsc::channel();
    Arc::new(Mutex::new(App::new(
      tx,
      UserConfig::new(),
      Some(SystemTime::now()),
    )))
  }

  /// A bridge over an app that has finished booting.
  pub(crate) fn booted(
    app: Arc<Mutex<App>>,
  ) -> (Link, Publisher, mpsc::UnboundedReceiver<ClientMessage>) {
    let (_boot, apps) = watch::channel(Some(app));
    channel(apps, Arc::new(BrowserOnboarding::new()), false)
  }

  #[tokio::test]
  async fn a_tick_push_waits_a_quarter_second_whatever_the_tick_rate() {
    let app = shared_app();
    let (link, mut publisher, _inbox) = booted(Arc::clone(&app));
    let t0 = publisher.last_tick;
    let mut rx = link.pushes.subscribe();
    let app = app.lock().await;

    publisher.tick(&app, t0 + Duration::from_millis(100));
    assert!(rx.try_recv().is_err());
    publisher.tick(&app, t0 + Duration::from_millis(249));
    assert!(rx.try_recv().unwrap().contains("\"kind\":\"tick\""));
    assert!(rx.try_recv().is_err());
    publisher.tick(&app, t0 + Duration::from_millis(498));
    assert!(rx.try_recv().is_ok());
  }

  #[test]
  fn a_bridge_with_no_page_for_a_minute_is_abandoned() {
    let t0 = Instant::now();
    let (link, mut publisher, _inbox) = booted(shared_app());

    assert!(!publisher.abandoned(t0 + Duration::from_secs(30), false));
    let page = link.pushes.subscribe();
    assert!(!publisher.abandoned(t0 + Duration::from_secs(120), false));
    drop(page);
    assert!(!publisher.abandoned(t0 + Duration::from_secs(150), false));
    assert!(!publisher.abandoned(t0 + Duration::from_secs(209), false));
    assert!(publisher.abandoned(t0 + Duration::from_secs(210), false));
  }

  #[test]
  fn a_bridge_with_no_page_stays_while_it_plays_and_quits_a_minute_after() {
    let t0 = Instant::now();
    let (_link, mut publisher, _inbox) = booted(shared_app());

    assert!(!publisher.abandoned(t0, false));
    assert!(!publisher.abandoned(t0 + Duration::from_secs(120), true));
    assert!(!publisher.abandoned(t0 + Duration::from_secs(150), false));
    assert!(!publisher.abandoned(t0 + Duration::from_secs(209), false));
    assert!(publisher.abandoned(t0 + Duration::from_secs(210), false));
  }
  #[tokio::test]
  async fn a_lagging_socket_drops_the_backlog_and_gets_a_fresh_resync() {
    use tokio_tungstenite::tungstenite::protocol::Role;
    let (link, _publisher, _inbox) = booted(shared_app());
    let (server_end, client_end) = tokio::io::duplex(64);
    let server = WebSocketStream::from_raw_socket(server_end, Role::Server, None).await;
    let mut client = WebSocketStream::from_raw_socket(client_end, Role::Client, None).await;
    tokio::spawn(run_socket(server, link.clone(), None));

    // The first frame proves the socket subscribed; the tiny pipe then stalls it.
    let first = client.next().await.unwrap().unwrap();
    assert!(first.to_text().unwrap().contains("\"kind\":\"hello\""));
    for _ in 0..70 {
      link.pushes.send("stale".to_string()).unwrap();
    }

    let mut frames = Vec::new();
    while let Ok(Some(Ok(frame))) =
      tokio::time::timeout(Duration::from_millis(500), client.next()).await
    {
      frames.push(frame.to_text().unwrap().to_string());
    }
    assert!(frames.iter().all(|frame| frame != "stale"));
    let routes = frames
      .iter()
      .filter(|frame| frame.contains("\"kind\":\"route\""))
      .count();
    assert_eq!(routes, 2, "{frames:?}");
  }

  #[tokio::test]
  async fn a_socket_gets_visualizer_frames_only_while_its_page_shows_the_visualizer() {
    use tokio_tungstenite::tungstenite::protocol::Role;
    let (link, publisher, _inbox) = booted(shared_app());
    let (server_end, client_end) = tokio::io::duplex(1 << 20);
    let server = WebSocketStream::from_raw_socket(server_end, Role::Server, None).await;
    let mut client = WebSocketStream::from_raw_socket(client_end, Role::Client, None).await;
    tokio::spawn(run_socket(server, link, None));
    client.next().await.unwrap().unwrap();
    async fn frames_after(
      client: &mut WebSocketStream<tokio::io::DuplexStream>,
      publisher: &Publisher,
      ask: &str,
      watched: bool,
    ) -> Vec<String> {
      client.send(WsMessage::text(ask)).await.unwrap();
      let deadline = Instant::now() + Duration::from_secs(2);
      while publisher.visualizer_watched() != watched {
        assert!(Instant::now() < deadline, "the socket never took {ask}");
        tokio::time::sleep(Duration::from_millis(1)).await;
      }
      publisher.frame(&protocol::visualizer(&[1.0], &[]));
      let mut frames = Vec::new();
      while let Ok(Some(Ok(frame))) =
        tokio::time::timeout(Duration::from_millis(300), client.next()).await
      {
        let text = frame.to_text().unwrap().to_string();
        if text.contains("\"kind\":\"visualizer\"") {
          frames.push(text);
        }
      }
      frames
    }

    // Offered before the page asked: stale by the time it does.
    publisher.frame(&protocol::visualizer(&[0.5], &[]));
    let shown = frames_after(
      &mut client,
      &publisher,
      r#"{"type":"visualizer","on":true}"#,
      true,
    )
    .await;
    let hidden = frames_after(
      &mut client,
      &publisher,
      r#"{"type":"visualizer","on":false}"#,
      false,
    )
    .await;

    assert_eq!(
      shown,
      [protocol::encode(&protocol::visualizer(&[1.0], &[]))]
    );
    assert!(hidden.is_empty(), "{hidden:?}");
  }
}
