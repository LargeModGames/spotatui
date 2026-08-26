//! The serial IoEvent pump: source routing by URI scheme, the service lane,
//! and the party-relay drain. Every frontend drives `App` through this.

use crate::infra::network::{IoEvent, Network};

// CLI mode never starts the pump; only a frontend launch does.
#[cfg_attr(not(feature = "tui"), allow(dead_code))]
pub(super) async fn start_tokio(io_rx: std::sync::mpsc::Receiver<IoEvent>, network: &mut Network) {
  // Bridge the sync dispatch channel onto the async runtime: a dedicated
  // thread does blocking recv() and forwards into a tokio channel the pump can
  // await, replacing the old try_recv() + 5ms sleep poll (which added 0-5ms
  // latency to every event and busy-woke the task while idle).
  let (bridge_tx, mut bridge_rx) = tokio::sync::mpsc::unbounded_channel::<IoEvent>();
  std::thread::spawn(move || {
    while let Ok(io_event) = io_rx.recv() {
      if bridge_tx.send(io_event).is_err() {
        break;
      }
    }
  });

  // Party relay messages used to piggyback on the 5ms poll loop; with the pump
  // parked on recv(), drain them on an interval instead.
  let mut party_poll = tokio::time::interval(std::time::Duration::from_millis(25));
  party_poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

  loop {
    let io_event = tokio::select! {
      maybe_event = bridge_rx.recv() => match maybe_event {
        Some(io_event) => io_event,
        None => break,
      },
      _ = party_poll.tick(), if network.party_incoming_rx.is_some() => {
        network.process_party_messages().await;
        continue;
      }
    };

    // Source-agnostic service events (lyrics, cover art, telemetry,
    // announcements, friends, stats/recap) only touch `App` and their own HTTP
    // clients — never the Spotify client, pacing, or `Network` state — so they
    // run concurrently on a detached task instead of queueing behind
    // rate-limited Spotify calls on this serial pump (and vice versa).
    if Network::runs_on_service_lane(&io_event) {
      let mut service_network = Network::new(
        None,
        network.client_config.clone(),
        &network.app,
        network.token_cache_path.clone(),
      );
      tokio::spawn(async move {
        service_network.handle_network_event(io_event).await;
      });
      continue;
    }

    {
      // The native queue router runs first: it owns `AdvanceNativeQueue` and
      // the queue slot's transport controls, and relinquishes the slot on an
      // unrelated `StartPlayback` (returning false so the per-source
      // teardowns/starts still run). Compiled unconditionally.
      let handled_queue =
        crate::infra::queue::dispatch::route_queue_event(&network.app, &io_event).await;
      // Local-file playback is intercepted before the Spotify network so the
      // network stays Spotify-only (see infra::local::dispatch).
      let handled_locally = !handled_queue && {
        #[cfg(feature = "local-files")]
        {
          crate::infra::local::dispatch::route_local_event(&network.app, &io_event).await
        }
        #[cfg(not(feature = "local-files"))]
        {
          false
        }
      };
      // Subsonic is intercepted after local and before the Spotify network. A
      // `subsonic:` URI falls through the local dispatch (its `is_file_uri` is
      // false) and is caught here (see infra::subsonic::dispatch). Skipped when
      // local already consumed the event.
      #[cfg(feature = "subsonic")]
      let handled_subsonic = !handled_queue
        && !handled_locally
        && crate::infra::subsonic::dispatch::route_subsonic_event(&network.app, &io_event).await;
      #[cfg(not(feature = "subsonic"))]
      let handled_subsonic = false;
      // Qobuz is intercepted after Subsonic. A `qobuz:` URI falls through the
      // earlier dispatches and is caught here (see infra::qobuz::dispatch).
      #[cfg(feature = "qobuz")]
      let handled_qobuz = !handled_queue
        && !handled_locally
        && !handled_subsonic
        && crate::infra::qobuz::dispatch::route_qobuz_event(&network.app, &io_event).await;
      #[cfg(not(feature = "qobuz"))]
      let handled_qobuz = false;
      // Internet radio is intercepted last before the Spotify network. A
      // `radio:` URI falls through both earlier dispatches and is caught here
      // (see infra::radio::dispatch). Skipped when already consumed.
      #[cfg(feature = "internet-radio")]
      let handled_radio = !handled_queue
        && !handled_locally
        && !handled_subsonic
        && !handled_qobuz
        && crate::infra::radio::dispatch::route_radio_event(&network.app, &io_event).await;
      #[cfg(not(feature = "internet-radio"))]
      let handled_radio = false;
      // YouTube is intercepted last before the Spotify network. A `youtube:`
      // URI falls through the three earlier dispatches and is caught here
      // (see infra::youtube::dispatch). Skipped when already consumed.
      #[cfg(feature = "youtube")]
      let handled_youtube = !handled_queue
        && !handled_locally
        && !handled_subsonic
        && !handled_qobuz
        && !handled_radio
        && crate::infra::youtube::dispatch::route_youtube_event(&network.app, &io_event).await;
      #[cfg(not(feature = "youtube"))]
      let handled_youtube = false;
      if !handled_queue
        && !handled_locally
        && !handled_subsonic
        && !handled_qobuz
        && !handled_radio
        && !handled_youtube
      {
        network.handle_network_event(io_event).await;
      } else {
        // A source router consumed the event and returned without touching
        // `is_loading`, which `App::dispatch` set to true. Only
        // `handle_network_event` resets it, and we skipped that path, so clear
        // it here — otherwise selecting/loading Local, Subsonic, Qobuz, Radio,
        // or YouTube content leaves the UI stuck on the loading indicator.
        network.app.lock().await.is_loading = false;
      }
    }
    network.process_party_messages().await;
  }
}
