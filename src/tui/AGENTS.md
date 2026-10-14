### Adding a new screen

Six places, not four:

1. `RouteId` + `ActiveBlock` variants in `src/core/app/route.rs`.
2. `src/tui/handlers/<screen>.rs` with `pub fn handler(key: Key, app: &mut App)`,
   registered with **both** a `mod <screen>;` line and a `handle_block_events`
   match arm in `handlers/mod.rs` - a file missing its `mod` line silently is not
   compiled. A handler writes shared `App` state only through an `App` method or
   `app.apply(Action::…)`; its cursors and focus are `app.view` fields.
3. A draw fn in `src/tui/ui/<screen>.rs`, wired into `ui::draw_route_content`
   (content-pane screens) **or** the `terminal.draw` match in `tui/runner.rs`
   (full-screen/overlay screens), which also needs an explicit no-op arm in
   `draw_route_content`.
4. An arm in `handlers::handle_escape`.
5. `common_key_events::content_active_block_for_route` if right-arrow from the
   sidebar should reach it.
6. A row in `help_entries` (`src/tui/keymap.rs`) for any new key, with the
   `Requirement` the key needs (`Requirement::None` for a source-agnostic key),
   and, for any new `Action` variant, a `default_binding` arm plus a
   `sample_actions()` value in the same file.

Any new typing surface must also be added to
`runner.rs::key_reaches_handlers_before_back`, or the default back key `q` closes
it mid-typing. Keep feature-gated screens' match arms ungated and `#[cfg]` only
the body, so the exhaustive `ActiveBlock` match compiles in slim builds. New
Spotify calls become `IoEvent` variants in `src/infra/network/mod.rs`, implemented
in the matching `src/infra/network/<concern>.rs`. A new `Action` variant is a
compile error in `src/tui/keymap.rs::default_binding` until its arm names a key,
a gesture, or `Exposure::Unbound`.

### Adding a feature-gated sidebar row

`library_row_requirements()` (`src/core/app/library.rs`) is the full table of
`(LibraryTarget, Requirement)` rows; `App::library_rows()` is the list the
active source and the session can serve, and the only list the handlers and
the draw function index. A new row needs a `LibraryTarget` variant (with its
`name()`), a table entry, and an `open_library_section` arm. Look a row up
**by target** (`app.library_rows().iter().position(...)`), never by index: the
index depends on the features built in and on the session.
