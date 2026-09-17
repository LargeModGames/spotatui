# Playlist Sync

Playlist sync keeps one playlist copied onto other sources. You pick a **master**
playlist and one or more **mirrors**, and every run makes the mirrors hold the
same tracks as the master.

Spotify, Qobuz, Subsonic and YouTube can each be a master or a mirror. Local
Files and Internet Radio cannot be linked: a local playlist is a directory on
disk, and radio has stations rather than playlists.

## What a link is

A **link** is one master playlist plus its mirrors:

- The master is the playlist you actually edit, on your phone or in any client.
- Each mirror is a playlist on another source that the sync writes. Put a mirror
  on a different source from the master; mirroring a source onto itself has
  nothing to resolve.
- Each mirror keeps its own match cache, so adding a second mirror never makes
  the first one search again.
- A link with no mirrors is skipped.

Links live in `playlist_sync.yml`, described under [The file](#the-file).

## Master wins

The sync is one way. On every run:

- Tracks added to the master are appended to each mirror, in master order.
- Tracks removed from the master are removed from each mirror.
- A row the sync paired with a master track, or added itself, follows the
  master from then on: when the track leaves the master, that row goes. A row
  that matches nothing in the master is never touched, so add what you like to
  a mirror by hand and the sync leaves it alone.
- Removal takes one row per track. When a track leaves the master, the sync
  removes the last row on the mirror with that id and leaves an earlier copy
  you added by hand. Spotify is the exception: its API removes a track by id
  from every position, so a hand-added copy of the same track goes with it.
- A track the sync did add and you then deleted on the mirror comes back on the
  next run. The master is the truth.

Nothing is ever copied back from a mirror onto the master.

## How tracks are matched

For each master track the sync searches the mirror source and takes the first
candidate that is the same recording:

1. **ISRC exact.** The recording code both tracks carry, compared with case and
   separators ignored. On Spotify the ISRC is searched first, so a match costs
   one call.
2. **Title, first artist and duration.** Title and first artist have to be equal
   once lowercased with punctuation collapsed, and the two durations have to be
   within two seconds. A trailing `(feat. X)`, `[Remastered]` or ` - Radio Edit`
   on the master title is forgiven, both in the search and in the comparison;
   the duration keeps a different edition apart. A candidate that reports no
   duration matches on title and artist alone.
3. **Nothing else.** A live version or a cover carrying the same title is left
   unmatched rather than silently substituted.

YouTube carries no ISRC, so it has its own rule. The video title has to contain
the master title, with a trailing `(feat. X)` or ` - Radio Edit` suffix on the
master forgiven. A video on the artist's own channel (spaces, case and symbols
in the channel name ignored) wins when its known duration is within three
seconds. Failing that, a video on any other channel is taken only when both
durations are known and within two seconds, which is what a re-upload of the
same audio looks like.

Every resolved pair is remembered per mirror, so a re-run costs no searches for
tracks it has already placed. Only what changed on the master costs calls. The
first YouTube run on a long playlist is the slow one: it shells out to `yt-dlp`
once per track, and the status line counts the progress.

## Unmatched tracks

A master track with no mirror track is listed per mirror on the Playlist sync
screen, with one of three reasons:

| Reason | Meaning |
|--------|---------|
| No candidate | The mirror source returned nothing that is the same recording |
| Not syncable | The master track cannot be mirrored at all, such as a local file in a Spotify playlist or a podcast episode |
| Search failed | The search itself failed, and the message is what the source said |

The list is rebuilt on every run, so a track that becomes available simply stops
appearing, and a failed search is tried again next run. A track with no
candidate is searched again only on a run you start yourself (`s` on the sync
screen, or the CLI); the startup run keeps last time's verdict, so a long
unmatched list does not cost a search per track on every launch.

## Making a link

In the sidebar, highlight the playlist that is the master and press `m`. A
picker lists the other sources that can take a mirror: the ones compiled into
this build, with Spotify only while a session exists and Subsonic only with a
server configured. Enter looks for a playlist of yours with the master's name
on that source and adopts it, or creates an empty one when there is none;
then it records the link and starts a run. An adopted playlist keeps every
track it already has: the run pairs them with the master by ISRC, title and
duration before it searches anything, adds what is missing, and leaves every
row that matches nothing in the master alone. A paired row follows the master
from then on, like a row the sync added. Enter also opens the sync screen, where the run's progress shows. Press
`m` on the same master again to add a second mirror to the same link.

The **Playlist sync** row in the Library block of the sidebar opens the sync
screen at any time; the row sits below Stats, so move the cursor down inside
the Library block to reach it. The screen shows one row per link, and for the
highlighted link each mirror's counts, its last run, and the unmatched tracks
with their reason. With the screen focused, `s` runs every link now and `D`
removes the highlighted link after a confirmation; the mirror playlists stay
where they are. The help menu (`?`) lists both keys under "Playlist sync".

## Running a sync

Three triggers, and no timer:

- **TUI startup.** A run starts in the background when spotatui launches, so the
  interface stays usable while it works.
- **The Playlist sync screen.** `s` runs every link on demand.
- **The CLI.** `spotatui sync`, below.

One run at a time: a second trigger while a run is in flight answers
`Playlist sync already running`. A finished run posts one line, for example
`Playlist sync: 3 added, 1 removed, 2 unmatched`.

A mirror receives its tracks in batches of ten as they resolve, and the file
is saved after every batch, so a run cut short by quitting continues where it
stopped on the next start. A YouTube mirror is the slow one: every unresolved
track costs one `yt-dlp` search of several seconds, so a long playlist takes
minutes on its first run. The status bar counts the progress every ten tracks.

## The CLI

```bash
spotatui sync                      # every link
spotatui sync --link "Road Trip"   # one link
spotatui sync --dry-run            # report the changes, write nothing
```

`--link` takes the master playlist name, trimmed and ignoring case, or the link
id exactly.

`--dry-run` does every read and every search and prints what it would do. It
writes nothing at all, not even the match cache, so the next real run repeats
those searches.

The command needs no Spotify login when no link uses Spotify, so a Qobuz to
Subsonic link syncs on a machine with no Spotify session (a fresh install
still asks for the Spotify app credentials once, like every command). A link
whose source is not connected, not logged in, not configured, or not compiled
into your build is **skipped** with a message, and a skip is not a failure.

Exit code: zero when every link ran or was skipped, non-zero when a link failed,
for instance when a write was rejected or a rate limit stopped the run.

## The file

Links and their match caches live in `playlist_sync.yml` in the app state
directory: `$XDG_STATE_HOME/spotatui/playlist_sync.yml` when `XDG_STATE_HOME` is
set to an absolute path, or `~/.local/state/spotatui/playlist_sync.yml` when it
is unset or not absolute. `SPOTATUI_PLAYLIST_SYNC_PATH` overrides the whole path.

A missing file means no links. A malformed file is reported and never
overwritten, so a hand edit that went wrong stays yours to fix.

You can write a link by hand. Everything a run fills in (`matches`, `unmatched`,
`last_run`) is optional:

```yaml
version: 1
links:
  - id: road-trip
    master:
      source: Spotify
      playlist_uri: "spotify:playlist:37i9dQZF1DX0XUsuxWHRQd"
      name: Road Trip
    mirrors:
      - endpoint:
          source: Qobuz
          playlist_uri: "qobuz:playlist:24601"
          name: Road Trip
      - endpoint:
          source: Subsonic
          playlist_uri: "subsonic:playlist:42"
          name: Road Trip
```

- `id` is any string, unique in the file. It is what `--link` matches exactly.
- `source` is one of `Spotify`, `Qobuz`, `Subsonic`, `YouTube`, spelled exactly
  like that.
- `playlist_uri` is the URI spotatui uses for that playlist:
  `spotify:playlist:<id>`, `qobuz:playlist:<id>`, `subsonic:playlist:<id>`, or
  `youtube:playlist:<id>` from `youtube_playlists.yml`.
- `name` is a label only, used in the report and by `--link`.

## Notes

- **One writer at a time.** `spotatui sync` and a running spotatui are separate
  processes, and the file is saved whole. The last one to save wins and there is
  no lock file, so do not run the CLI while the app is syncing.
- **A YouTube mirror is local only.** It is the `youtube_playlists.yml` file on
  this machine, not a playlist in a YouTube account, so your phone never sees it.
