# CLI

Beyond the terminal UI, spotatui has a command-line interface for playback, search, playlists, sync and plugins.

```bash
spotatui [OPTIONS] [SUBCOMMAND]
```

Running `spotatui` with no subcommand starts the terminal UI. `spotatui --help` and `spotatui <subcommand> --help` list every flag.

## Top-level flags

| Flag | Purpose |
| --- | --- |
| `-t, --tick-rate <MS>` | UI tick rate, 1 to 999 ms |
| `-c, --config <PATH>` | Use another config file |
| `--reconfigure-auth` | Forget the cached Spotify login and log in again |
| `--completions <SHELL>` | Print shell completions: `bash`, `zsh`, `fish`, `powershell` or `elvish` |
| `--play-file <PATH>` | Play a local audio file at startup (local-files builds) |
| `--debug` | Verbose logging for a bug report, see [Logging](configuration.md#logging) |
| `-U, --no-update` | Skip the auto-update check for this launch (self-update builds) |

## Output format

`--format` (on `playback`, `play`, `list` and `search`) takes these placeholders: `%a` artist, `%b` album, `%p` playlist, `%t` track, `%h` show, `%u` URI, `%f` flags, `%s` status, `%v` volume, `%d` device, `%r` progress.

## `playback` (alias `pb`)

Control playback on the active device. With no flag it prints the status.

| Flag | Purpose |
| --- | --- |
| `-d, --device <NAME>` | Target a device by name |
| `-t, --toggle` | Pause or resume |
| `-s, --status` | Print the playback status |
| `--share-track` / `--share-album` | Print the URL of the current track or album (use alone) |
| `--transfer <NAME>` | Move playback to another device |
| `--like` / `--dislike` | Like or unlike the current track |
| `--shuffle` | Toggle shuffle |
| `--repeat` | Cycle the repeat mode |
| `-n, --next` | Next track (repeat the flag to skip more) |
| `-p, --previous` | Previous track (twice goes to the actual previous track) |
| `--seek <±SECONDS>` | Seek relative (`+10`, `-10`) or absolute (`10`) |
| `-v, --volume <0-100>` | Set the volume |

`--next` and `--previous` cannot be combined with other flags.

```bash
spotatui pb --toggle
spotatui pb --volume 60
```

## `play` (alias `p`)

Play a Spotify URI, or search by name and play the first match. Give exactly one of `--uri` or `--name`.

| Flag | Purpose |
| --- | --- |
| `-u, --uri <URI>` | A `spotify:track:`, `album:`, `artist:`, `playlist:` or `show:` URI |
| `-n, --name <NAME>` | Search and play the first match; needs one type flag below |
| `-b, --album` / `-a, --artist` / `-t, --track` / `-w, --show` / `-p, --playlist` | What to search for |
| `-q, --queue` | Queue the track instead of playing it (tracks only) |
| `-r, --random` | Play a random track (playlists only) |
| `-d, --device <NAME>` | Target a device by name |

```bash
spotatui play --track --name "bohemian rhapsody"
spotatui play --playlist --name "Your Playlist" --random
```

## `list` (alias `l`)

| Flag | Purpose |
| --- | --- |
| `-d, --devices` | List devices |
| `-p, --playlists` | List your playlists |
| `--liked` | List your liked songs |
| `--limit <1-50>` | Maximum results |

```bash
spotatui list --liked --limit 50
```

## `search` (alias `s`)

```bash
spotatui search "daft punk" --artists --limit 10
```

| Flag | Purpose |
| --- | --- |
| `<SEARCH>` | The query |
| `-b, --albums` / `-a, --artists` / `-p, --playlists` / `-t, --tracks` / `-w, --shows` | What to search |
| `--limit <1-10>` | Maximum results |

## `sync`

Sync the linked playlists across sources (see [Playlist sync](playlist-sync.md)). It needs no Spotify device and exits non-zero when the sync fails.

| Flag | Purpose |
| --- | --- |
| `--link <NAME>` | Only the link with this master playlist name or id |
| `--dry-run` | Report what would change, write nothing |

## `history recap`

Make a shareable HTML recap from your local listening history.

```bash
spotatui history recap --period 30d --output recap.html
```

| Flag | Purpose |
| --- | --- |
| `--period <PERIOD>` | `7d`, `30d` (default), `month`, `year` or `all` |
| `-o, --output <PATH>` | Output file (default `spotatui-recap.html`) |

## `mcp`

The MCP server for coding agents (mcp-server builds, which include every prebuilt terminal binary; the desktop app has no `spotatui mcp`). `spotatui mcp` is the stdio server an agent starts; `spotatui mcp status` checks the setup and exits non-zero when it is not ready (`--json` for machine-readable output). See [MCP setup](mcp-setup.md).

## `plugin`

Install and manage Lua plugins (needs `git`). See [Scripting](scripting.md).

```bash
spotatui plugin add owner/repo [--force]   # or a full git URL
spotatui plugin list                       # alias: ls
spotatui plugin remove <NAME>              # alias: rm
spotatui plugin update [NAME]
spotatui plugin new <NAME> [--force]
```

## `update`

Self-update builds only.

```bash
spotatui update            # print the current and the latest version
spotatui update --install  # download, check, and install the latest version (-i)
```
