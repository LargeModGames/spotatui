-- track-info-popup: a `track_info` command that opens a popup with the current track details.
--
-- Install (single file):
--   case "${XDG_CONFIG_HOME:-}" in
--     /*) config_home="$XDG_CONFIG_HOME" ;;
--     *) config_home="$HOME/.config" ;;
--   esac
--   mkdir -p "$config_home/spotatui/plugins"
--   cp track-info-popup.lua "$config_home/spotatui/plugins/"
--
-- Suggested binding, in config.yml in the spotatui app config directory:
--   plugin_commands:
--     track_info: "ctrl-i"

spotatui.register_command("track_info", function()
  local pb = spotatui.playback()
  if not pb or not pb.track then
    spotatui.notify("Nothing is playing", 3)
    return
  end

  local t = pb.track
  local minutes = math.floor(t.duration_ms / 60000)
  local seconds = math.floor((t.duration_ms % 60000) / 1000)

  spotatui.popup("Track info", {
    { text = t.name, bold = true, fg = "Green" },
    { text = "by " .. table.concat(t.artists, ", "), fg = "Cyan" },
    { text = "on " .. t.album },
    "",
    string.format("Length: %d:%02d", minutes, seconds),
    { text = pb.is_playing and "Playing" or "Paused", fg = pb.is_playing and "Green" or "Yellow" },
    "",
    "Press Esc to close",
  })
end)
