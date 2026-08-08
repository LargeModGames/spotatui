# spotatui agent plugin

One directory that registers spotatui's MCP server with an AI coding agent *and*
installs a DJ skill alongside it, in two formats at once:

- **[Agent Plugins 1.0](https://agent-plugins.org)** (`plugin.json`, `mcp.json`) —
  the vendor-neutral format, understood by VS Code, Cursor, GitHub Copilot, OpenAI
  Codex, and Kiro.
- **Claude Code plugin** (`.claude-plugin/plugin.json`, `.mcp.json`) — Claude Code
  does not read the Agent Plugins format yet, so the same identity and the same
  server registration are mirrored in its own layout.

> Not to be confused with spotatui's **Lua plugins**, which are in-app scripts that
> react to playback events and draw UI. Those are documented in
> [`PLUGINS.md`](../PLUGINS.md).

## What is inside

```
agent-plugin/
├── plugin.json                    Agent Plugins 1.0 manifest
├── mcp.json                       Agent Plugins 1.0 MCP server config
├── .claude-plugin/plugin.json     Claude Code manifest
├── .mcp.json                      Claude Code MCP server config
└── skills/spotatui-dj/SKILL.md    the DJ skill, shared by both formats
```

Both MCP configs register the same thing: a **stdio** server run as
`spotatui mcp`. The skill teaches the agent the DJ workflow — read the listening
history first, search before queueing by name, queue rather than interrupt, and
what `owned`, `exclude_owned`, and `set_dj_vibe` actually mean.

## Prerequisites

The plugin registers the server; it cannot install the binary. Before installing:

1. spotatui built with the feature: `cargo install --locked spotatui --features mcp-server`
   (it is not in `default`), or, from a checkout: `cargo install --path . --features mcp-server`.
2. `behavior.mcp_enabled: true` in `~/.config/spotatui/config.yml`.
3. spotatui running — the socket opens at startup, so restart it after step 2.
4. `spotatui` on the `PATH` the agent client spawns processes with.

Check all of it with `spotatui mcp status` (safe; exits non-zero on the failing
step). Never run `spotatui mcp` to test — that command *is* the server and blocks
on stdin.

## Install

**Claude Code**

```
/plugin marketplace add LargeModGames/spotatui
/plugin install spotatui@spotatui
```

**Agent Plugins 1.0 clients** (VS Code, Cursor, GitHub Copilot, Codex, Kiro)

Point the client at this `agent-plugin/` directory using its own plugin install
mechanism. The spec defines the package layout and leaves distribution to each
client, so check your client's docs for how it adds a local or repository-hosted
plugin.

## Full setup and troubleshooting

[`docs/mcp-setup.md`](../docs/mcp-setup.md) is the complete guide, written as
instructions you can hand straight to an agent, with a troubleshooting table for
every failure mode.
