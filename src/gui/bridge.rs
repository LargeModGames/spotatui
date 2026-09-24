//! Pushes from the tick loop to every page socket, and page messages back.

use crate::core::app::{App, DisplayRevisions};
use crate::gui::protocol::{self, ClientMessage};
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, mpsc, Mutex};
use tokio_tungstenite::{tungstenite::Message as WsMessage, WebSocketStream};

const TICK_EVERY: Duration = Duration::from_millis(250);
const NO_PAGE_LIMIT: Duration = Duration::from_secs(60);

/// What a page socket needs: the app for its first snapshot, the pushes, and the way back.
#[derive(Clone)]
pub(crate) struct Link {
  app: Arc<Mutex<App>>,
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
  app: Arc<Mutex<App>>,
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
  (Link { app, pushes, inbox }, publisher, messages)
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
    if now.saturating_duration_since(self.last_tick) >= TICK_EVERY {
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

/// Serve one upgraded page socket until either side closes it.
pub(crate) async fn run_socket<S>(socket: WebSocketStream<S>, link: Link, token: Option<String>)
where
  S: AsyncRead + AsyncWrite + Unpin,
{
  let (mut sink, mut stream) = socket.split();
  // Subscribe before the snapshot: a push racing it is older and the page drops it.
  let mut pushes = link.pushes.subscribe();
  let mut outgoing = {
    let app = link.app.lock().await;
    let mut first = vec![protocol::hello(app.display_revisions(), token)];
    first.extend(protocol::resync(&app));
    first
  };
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
          outgoing = protocol::resync(&*link.app.lock().await);
        }
        Err(RecvError::Closed) => return,
      },
      incoming = stream.next() => match incoming {
        Some(Ok(WsMessage::Text(text))) => match serde_json::from_str::<ClientMessage>(&text) {
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
mod tests {
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

  #[tokio::test]
  async fn a_tick_push_waits_a_quarter_second_whatever_the_tick_rate() {
    let t0 = Instant::now();
    let app = shared_app();
    let (link, mut publisher, _inbox) = channel(Arc::clone(&app));
    let mut rx = link.pushes.subscribe();
    let app = app.lock().await;

    publisher.tick(&app, t0 + Duration::from_millis(100));
    assert!(rx.try_recv().is_err());
    publisher.tick(&app, t0 + Duration::from_millis(300));
    assert!(rx.try_recv().unwrap().contains("\"kind\":\"tick\""));
    assert!(rx.try_recv().is_err());
  }

  #[test]
  fn a_bridge_with_no_page_for_a_minute_is_abandoned() {
    let t0 = Instant::now();
    let (link, mut publisher, _inbox) = channel(shared_app());

    assert!(!publisher.abandoned(t0 + Duration::from_secs(30)));
    let page = link.pushes.subscribe();
    assert!(!publisher.abandoned(t0 + Duration::from_secs(120)));
    drop(page);
    assert!(!publisher.abandoned(t0 + Duration::from_secs(150)));
    assert!(publisher.abandoned(t0 + Duration::from_secs(181)));
  }
}
