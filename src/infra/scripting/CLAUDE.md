### Lua plugins (feature `scripting`)

Plugins never see `&mut App` or rspotify types: reads are cached serde snapshots
from `src/core/plugin_api.rs`; writes are `ScriptEffect`s the engine drains while
holding `&mut App`, each routed through the same `App` method the equivalent
keybinding uses. Snapshot changes must be additive (`#[serde(default)]`, new keys
only) - removing/renaming a key breaks installed plugins and requires bumping
`API_VERSION` and updating `docs/scripting.md`. Validation lives in
`scripting/api.rs`; a failing callback is disabled on one strike.
