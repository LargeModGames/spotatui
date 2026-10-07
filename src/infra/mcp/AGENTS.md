### MCP server (feature `mcp-server`)

`spotatui mcp` is a stdio relay to the running TUI over loopback TCP: the TUI
listens on 127.0.0.1 with a token published in a 0600
`~/.config/spotatui/mcp.json`, gated on `behavior.mcp_enabled`. The relay
reconnects per line because the two processes have independent lifetimes.
`spotatui mcp` blocks - use `spotatui mcp status` to test. The protocol
(`src/infra/mcp/protocol.rs`) is hand-rolled: keep MCP *protocol* errors (unknown
tool, bad args) distinct from tool *execution* errors (`isError: true`) - clients
feed only the second back to the model.
