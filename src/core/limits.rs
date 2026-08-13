//! Frontend-neutral UI limits shared by core state and the frontends.

/// Upper bound for the persisted playbar height, enforced wherever
/// `playbar_height_rows` enters runtime state.
pub const MAX_PLAYBAR_ROWS: u16 = 50;
