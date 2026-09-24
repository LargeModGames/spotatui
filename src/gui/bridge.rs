//! Pushes from the tick loop to every page socket, and page messages back.

use crate::core::app::{App, DisplayRevisions};
use crate::gui::onboarding::BrowserOnboarding;
use crate::gui::protocol::{self, ClientMessage, ServerMessage};
use futures::{SinkExt, StreamExt};
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
}

/// The tick loop's side of the bridge.
pub(crate) struct Publisher {
  pushes: broadcast::Sender<String>,
  sent: DisplayRevisions,
  last_tick: Instant,
  alone_since: Instant,
}

pub(crate) fn channel(
  app: SharedApp,
  onboarding: Arc<BrowserOnboarding>,
) -> (Link, Publisher, mpsc::UnboundedReceiver<ClientMessage>) {
  let (pushes, _) = broadcast::channel(64);
  let (inbox, messages) = mpsc::unbounded_channel();
  let now = Instant::now();
  let publisher = Publisher {
    pushes: pushes.clone(),
    sent: DisplayRevisions::default(),
    last_tick: now,
    alone_since: now,
  };
  (
    Link {
      app,
      onboarding,
      pushes,
      inbox,
    },
    publisher,
    messages,
  )
}

impl Publisher {
  /// Broadcast every channel that changed since the last call.
  pub(crate) fn publish(&mut self, app: &App) {
    for message in protocol::diff(&self.sent, app) {
      let _ = self.pushes.send(protocol::encode(&message));
    }
    self.sent = app.display_revisions();
  }

  /// Push the position at most every 250 ms, whatever the tick rate.
  pub(crate) fn tick(&mut self, app: &App, now: Instant) {
    if now.saturating_duration_since(self.last_tick) + TICK_SLACK >= TICK_EVERY {
      self.last_tick = now;
      let _ = self.pushes.send(protocol::encode(&protocol::tick(app)));
    }
  }

  /// True once no page socket has been open for a minute; no page can reach the process then.
  pub(crate) fn abandoned(&mut self, now: Instant) -> bool {
    if self.pushes.receiver_count() > 0 {
      self.alone_since = now;
    }
    now.saturating_duration_since(self.alone_since) >= NO_PAGE_LIMIT
  }
}

async fn resync(app: &SharedApp) -> Vec<ServerMessage> {
  let app = app.borrow().clone();
  match app {
    Some(app) => protocol::resync(&*app.lock().await),
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
  let app = apps.borrow_and_update().clone();
  let revisions = match app {
    Some(app) => app.lock().await.display_revisions(),
    None => DisplayRevisions::default(),
  };
  let mut outgoing = vec![
    protocol::hello(revisions, token),
    protocol::onboarding(views.borrow_and_update().clone()),
  ];
  outgoing.extend(resync(&apps).await);
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
          outgoing = resync(&apps).await;
        }
        Err(RecvError::Closed) => return,
      },
      Ok(()) = views.changed() => {
        outgoing.push(protocol::onboarding(views.borrow_and_update().clone()));
      }
      Ok(()) = apps.changed() => {
        apps.borrow_and_update();
        outgoing = resync(&apps).await;
      }
      incoming = stream.next() => match incoming {
        Some(Ok(WsMessage::Text(text))) => match serde_json::from_str::<ClientMessage>(&text) {
          // Answered here: boot waits on it before the tick loop exists.
          Ok(ClientMessage::Onboarding { reply }) => link.onboarding.answer(reply),
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
    channel(apps, Arc::new(BrowserOnboarding::new()))
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

    assert!(!publisher.abandoned(t0 + Duration::from_secs(30)));
    let page = link.pushes.subscribe();
    assert!(!publisher.abandoned(t0 + Duration::from_secs(120)));
    drop(page);
    assert!(!publisher.abandoned(t0 + Duration::from_secs(150)));
    assert!(publisher.abandoned(t0 + Duration::from_secs(181)));
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
}
