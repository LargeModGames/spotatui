### The IoEvent pump: source routing, two lanes, an auth gate

`runtime/pump.rs::start_tokio` drains IoEvents serially. Three structural gates, all
worth knowing before adding an event:

- **Source routing**: non-Spotify playback is routed by URI scheme *before* the
  Spotify handler, in this order: `route_queue_event` → `route_local_event`
  (`file:`) → `route_subsonic_event` (`subsonic:`) → `route_qobuz_event`
  (`qobuz:`) → `route_radio_event` (`radio:`) → `route_youtube_event`
  (`youtube:`) → `Network::handle_network_event`.
  This is what keeps `infra/network/` Spotify-only.
- **Claim gate**: before the routers, `start_playback_has_taker` drops a
  `StartPlayback` whose URI scheme (`core::queue::queue_item_source`) names no
  compiled-in source and that no Spotify session can take. The routers'
  foreign-start teardown arms therefore only run for a real source-to-source
  handoff.
- **Service lane**: `Network::runs_on_service_lane` lists events that run on a
  detached task so slow, source-agnostic work cannot head-of-line-block the serial
  pump. The service lane's `Network` is built with **no Spotify client** - adding a
  `self.spotify()` call to a service-lane handler panics.
- **Auth gate**: `Network::event_bypasses_spotify_auth` lists events whose handlers
  never need a Spotify session.
- **Replay**: `Network::event_is_transport` lists events that drive whoever owns
  the sink. One held back by a rate-limit window is stamped with the
  `PlaybackOwner` at deferral time, dropped at the flush when the owner changed,
  and otherwise re-sent on the pump's channel so the routers see it.
  A new IoEvent must be classified against all three lists.

### Listening Party / sync

`src/infra/network/sync.rs` is pure WebSocket transport (`SyncMessage`,
`PartyConnection`); lifecycle logic lives in `src/infra/network/mod.rs`. Handlers
dispatch `StartParty` / `JoinParty` / `SetPartyControlMode` / `LeaveParty`;
`SyncPlayback` is fired by `App::on_tick` every 2s while hosting, not by a handler.
