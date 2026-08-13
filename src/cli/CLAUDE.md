### CLI subcommands

Subcommands are assembled and dispatched in `src/runtime.rs::run()`, where
**dispatch order encodes auth requirements**: `history`, `mcp`, and `plugin`
return before config/auth load; playback commands require a Spotify session.
Place new subcommands deliberately. stdout is protocol/output-only (`mcp` writes
JSON-RPC, `history recap` writes HTML) - diagnostics go to stderr; never add a
`println!` on a path a subcommand can reach. Self-update needs `User-Agent` +
`Accept: application/octet-stream` on GitHub asset requests and must not re-exec
during authentication (OAuth port deadlock).
