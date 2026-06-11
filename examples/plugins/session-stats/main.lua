-- session-stats: counts tracks played this session and shows them in a popup.
--
-- This is a *directory* plugin: it ships as a folder with an entry point (main.lua) plus a
-- helper module (stats.lua). This is the layout `spotatui plugin add owner/repo` produces.
--
-- Install (directory):
--   cp -r session-stats ~/.config/spotatui/plugins/
--
-- Suggested binding, in ~/.config/spotatui/config.yml:
--   plugin_commands:
--     session_stats: "ctrl-s"

local stats = require("stats")

spotatui.on("track_change", function(pb)
  if pb and pb.track then
    stats.record(pb.track.name)
  end
end)

spotatui.register_command("session_stats", function()
  local lines = {
    { text = "Tracks played this session: " .. stats.count, bold = true, fg = "Green" },
    "",
  }
  if #stats.recent == 0 then
    table.insert(lines, "No tracks played yet.")
  else
    table.insert(lines, { text = "Most recent:", fg = "Cyan" })
    for _, name in ipairs(stats.recent) do
      table.insert(lines, "  " .. name)
    end
  end
  table.insert(lines, "")
  table.insert(lines, "Press Esc to close")
  spotatui.popup("Session stats", lines)
end)
