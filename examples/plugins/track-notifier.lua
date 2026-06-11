-- track-notifier: show a "Now playing" toast and a playbar segment on every track change.
--
-- Install (single file):
--   cp track-notifier.lua ~/.config/spotatui/plugins/
-- Then restart spotatui.

spotatui.on("track_change", function(pb)
  if pb and pb.track then
    local artists = table.concat(pb.track.artists, ", ")
    spotatui.notify("Now playing: " .. pb.track.name .. " - " .. artists, 4)
    spotatui.set_playbar(pb.track.name)
  else
    -- Nothing playing: clear our playbar segment.
    spotatui.set_playbar(nil)
  end
end)

spotatui.on("start", function()
  spotatui.log("track-notifier loaded (api version " .. spotatui.api_version .. ")")
end)
