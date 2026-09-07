use clap::{builder::ArgPredicate, Arg, ArgAction, ArgGroup, Command};

fn device_arg() -> Arg {
  Arg::new("device")
    .short('d')
    .long("device")
    .value_name("DEVICE")
    .help("Specifies the spotify device to use")
}

fn format_arg() -> Arg {
  Arg::new("format")
    .short('f')
    .long("format")
    .value_name("FORMAT")
    .help("Specifies the output format")
    .long_help(
      "There are multiple format specifiers you can use: %a: artist, %b: album, %p: playlist, \
%t: track, %h: show, %f: flags (shuffle, repeat, like), %s: playback status, %v: volume, %d: current device. \
Example: spt pb -s -f 'playing on %d at %v%'",
    )
}

pub fn playback_subcommand() -> Command {
  Command::new("playback")
    .version(env!("CARGO_PKG_VERSION"))
    .author(env!("CARGO_PKG_AUTHORS"))
    .about("Interacts with the playback of a device")
    .long_about(
      "Use `playback` to interact with the playback of the current or any other device. \
You can specify another device with `--device`. If no options were provided, spt \
will default to just displaying the current playback. Actually, after every action \
spt will display the updated playback. The output format is configurable with the \
`--format` flag. Some options can be used together, other options have to be alone.

Here's a list:

* `--next` and `--previous` cannot be used with other options
* `--status`, `--toggle`, `--transfer`, `--volume`, `--like`, `--repeat` and `--shuffle` \
can be used together
* `--share-track` and `--share-album` cannot be used with other options",
    )
    .visible_alias("pb")
    .arg(device_arg())
    .arg(
      format_arg()
        .default_value("%f %s %t - %a")
        .default_value_if("seek", ArgPredicate::IsPresent, "%f %s %t - %a %r")
        .default_value_if("volume", ArgPredicate::IsPresent, "%v% %f %s %t - %a")
        .default_value_if("transfer", ArgPredicate::IsPresent, "%f %s %t - %a on %d"),
    )
    .arg(
      Arg::new("toggle")
        .short('t')
        .long("toggle")
        .action(ArgAction::SetTrue)
        .help("Pauses/resumes the playback of a device"),
    )
    .arg(
      Arg::new("status")
        .short('s')
        .long("status")
        .action(ArgAction::SetTrue)
        .help("Prints out the current status of a device (default)"),
    )
    .arg(
      Arg::new("share-track")
        .long("share-track")
        .action(ArgAction::SetTrue)
        .help("Returns the url to the current track"),
    )
    .arg(
      Arg::new("share-album")
        .long("share-album")
        .action(ArgAction::SetTrue)
        .help("Returns the url to the album of the current track"),
    )
    .arg(
      Arg::new("transfer")
        .long("transfer")
        .value_name("DEVICE")
        .help("Transfers the playback to new DEVICE"),
    )
    .arg(
      Arg::new("like")
        .long("like")
        .action(ArgAction::SetTrue)
        .help("Likes the current song if possible"),
    )
    .arg(
      Arg::new("dislike")
        .long("dislike")
        .action(ArgAction::SetTrue)
        .help("Dislikes the current song if possible"),
    )
    .arg(
      Arg::new("shuffle")
        .long("shuffle")
        .action(ArgAction::SetTrue)
        .help("Toggles shuffle mode"),
    )
    .arg(
      Arg::new("repeat")
        .long("repeat")
        .action(ArgAction::SetTrue)
        .help("Switches between repeat modes"),
    )
    .arg(
      Arg::new("next")
        .short('n')
        .long("next")
        .action(ArgAction::Count)
        .help("Jumps to the next song")
        .long_help(
          "This jumps to the next song if specied once. If you want to jump, let's say 3 songs \
forward, you can use `--next` 3 times: `spt pb -nnn`.",
        ),
    )
    .arg(
      Arg::new("previous")
        .short('p')
        .long("previous")
        .action(ArgAction::Count)
        .help("Jumps to the previous song")
        .long_help(
          "This jumps to the beginning of the current song if specied once. You probably want to \
jump to the previous song though, so you can use the previous flag twice: `spt pb -pp`. To jump \
two songs back, you can use `spt pb -ppp` and so on.",
        ),
    )
    .arg(
      Arg::new("seek")
        .long("seek")
        .value_name("±SECONDS")
        .allow_hyphen_values(true)
        .help("Jumps SECONDS forwards (+) or backwards (-)")
        .long_help(
          "For example: `spt pb --seek +10` jumps ten second forwards, `spt pb --seek -10` ten \
seconds backwards and `spt pb --seek 10` to the tenth second of the track.",
        ),
    )
    .arg(
      Arg::new("volume")
        .short('v')
        .long("volume")
        .value_name("VOLUME")
        .help("Sets the volume of a device to VOLUME (1 - 100)"),
    )
    .group(
      ArgGroup::new("jumps")
        .args(["next", "previous"])
        .multiple(false)
        .conflicts_with_all(["single", "flags", "actions"]),
    )
    .group(
      ArgGroup::new("likes")
        .args(["like", "dislike"])
        .multiple(false),
    )
    .group(
      ArgGroup::new("flags")
        .args(["like", "dislike", "shuffle", "repeat"])
        .multiple(true)
        .conflicts_with_all(["single", "jumps"]),
    )
    .group(
      ArgGroup::new("actions")
        .args(["toggle", "status", "transfer", "volume"])
        .multiple(true)
        .conflicts_with_all(["single", "jumps"]),
    )
    .group(
      ArgGroup::new("single")
        .args(["share-track", "share-album"])
        .multiple(false)
        .conflicts_with_all(["actions", "flags", "jumps"]),
    )
}

pub fn play_subcommand() -> Command {
  Command::new("play")
    .version(env!("CARGO_PKG_VERSION"))
    .author(env!("CARGO_PKG_AUTHORS"))
    .about("Plays a uri or another spotify item by name")
    .long_about(
      "If you specify a uri, the type can be inferred. If you want to play something by \
name, you have to specify the type: `--track`, `--album`, `--artist`, `--playlist` \
or `--show`. The first item which was found will be played without confirmation. \
To add a track to the queue, use `--queue`. To play a random song from a playlist, \
use `--random`. Again, with `--format` you can specify how the output will look. \
The same function as found in `playback` will be called.",
    )
    .visible_alias("p")
    .arg(device_arg())
    .arg(format_arg().default_value("%f %s %t - %a"))
    .arg(
      Arg::new("uri")
        .short('u')
        .long("uri")
        .value_name("URI")
        .help("Plays the URI"),
    )
    .arg(
      Arg::new("name")
        .short('n')
        .long("name")
        .value_name("NAME")
        .requires("contexts")
        .help("Plays the first match with NAME from the specified category"),
    )
    .arg(
      Arg::new("queue")
        .short('q')
        .long("queue")
        .action(ArgAction::SetTrue)
        // Only works with tracks
        .conflicts_with_all(["album", "artist", "playlist", "show"])
        .help("Adds track to queue instead of playing it directly"),
    )
    .arg(
      Arg::new("random")
        .short('r')
        .long("random")
        .action(ArgAction::SetTrue)
        // Only works with playlists
        .conflicts_with_all(["track", "album", "artist", "show"])
        .help("Plays a random track (only works with playlists)"),
    )
    .arg(
      Arg::new("album")
        .short('b')
        .long("album")
        .action(ArgAction::SetTrue)
        .help("Looks for an album"),
    )
    .arg(
      Arg::new("artist")
        .short('a')
        .long("artist")
        .action(ArgAction::SetTrue)
        .help("Looks for an artist"),
    )
    .arg(
      Arg::new("track")
        .short('t')
        .long("track")
        .action(ArgAction::SetTrue)
        .help("Looks for a track"),
    )
    .arg(
      Arg::new("show")
        .short('w')
        .long("show")
        .action(ArgAction::SetTrue)
        .help("Looks for a show"),
    )
    .arg(
      Arg::new("playlist")
        .short('p')
        .long("playlist")
        .action(ArgAction::SetTrue)
        .help("Looks for a playlist"),
    )
    .group(
      ArgGroup::new("contexts")
        .args(["track", "artist", "playlist", "album", "show"])
        .multiple(false),
    )
    .group(
      ArgGroup::new("actions")
        .args(["uri", "name"])
        .multiple(false)
        .required(true),
    )
}

pub fn list_subcommand() -> Command {
  Command::new("list")
    .version(env!("CARGO_PKG_VERSION"))
    .author(env!("CARGO_PKG_AUTHORS"))
    .about("Lists devices, liked songs and playlists")
    .long_about(
      "This will list devices, liked songs or playlists. With the `--limit` flag you are \
able to specify the amount of results (between 1 and 50). Here, the `--format` is \
even more awesome, get your output exactly the way you want. The format option will \
be applied to every item found.",
    )
    .visible_alias("l")
    .arg(
      format_arg()
        .default_value_if("devices", ArgPredicate::IsPresent, "%v% %d")
        .default_value_if("liked", ArgPredicate::IsPresent, "%t - %a (%u)")
        .default_value_if("playlists", ArgPredicate::IsPresent, "%p (%u)"),
    )
    .arg(
      Arg::new("devices")
        .short('d')
        .long("devices")
        .action(ArgAction::SetTrue)
        .help("Lists devices"),
    )
    .arg(
      Arg::new("playlists")
        .short('p')
        .long("playlists")
        .action(ArgAction::SetTrue)
        .help("Lists playlists"),
    )
    .arg(
      Arg::new("liked")
        .long("liked")
        .action(ArgAction::SetTrue)
        .help("Lists liked songs"),
    )
    .arg(
      Arg::new("limit")
        .long("limit")
        .help("Specifies the maximum number of results (1 - 50)"),
    )
    .group(
      ArgGroup::new("listable")
        .args(["devices", "playlists", "liked"])
        .required(true)
        .multiple(false),
    )
}

pub fn search_subcommand() -> Command {
  Command::new("search")
    .version(env!("CARGO_PKG_VERSION"))
    .author(env!("CARGO_PKG_AUTHORS"))
    .about("Searches for tracks, albums and more")
    .long_about(
      "This will search for something on spotify and displays you the items. The output \
format can be changed with the `--format` flag and the limit can be changed with \
the `--limit` flag (between 1 and 10). The type can't be inferred, so you have to \
specify it.",
    )
    .visible_alias("s")
    .arg(
      format_arg()
        .default_value_if("tracks", ArgPredicate::IsPresent, "%t - %a (%u)")
        .default_value_if("playlists", ArgPredicate::IsPresent, "%p (%u)")
        .default_value_if("artists", ArgPredicate::IsPresent, "%a (%u)")
        .default_value_if("albums", ArgPredicate::IsPresent, "%b - %a (%u)")
        .default_value_if("shows", ArgPredicate::IsPresent, "%h - %a (%u)"),
    )
    .arg(
      Arg::new("search")
        .required(true)
        .value_name("SEARCH")
        .help("Specifies the search query"),
    )
    .arg(
      Arg::new("albums")
        .short('b')
        .long("albums")
        .action(ArgAction::SetTrue)
        .help("Looks for albums"),
    )
    .arg(
      Arg::new("artists")
        .short('a')
        .long("artists")
        .action(ArgAction::SetTrue)
        .help("Looks for artists"),
    )
    .arg(
      Arg::new("playlists")
        .short('p')
        .long("playlists")
        .action(ArgAction::SetTrue)
        .help("Looks for playlists"),
    )
    .arg(
      Arg::new("tracks")
        .short('t')
        .long("tracks")
        .action(ArgAction::SetTrue)
        .help("Looks for tracks"),
    )
    .arg(
      Arg::new("shows")
        .short('w')
        .long("shows")
        .action(ArgAction::SetTrue)
        .help("Looks for shows"),
    )
    .arg(
      Arg::new("limit")
        .long("limit")
        .help("Specifies the maximum number of results (1 - 10)"),
    )
    .group(
      ArgGroup::new("searchable")
        .args(["playlists", "tracks", "albums", "artists", "shows"])
        .required(true)
        .multiple(false),
    )
}
