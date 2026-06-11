-- now-playing-webhook: POST a JSON payload to a webhook whenever the track changes.
--
-- A small example of the async HTTP + JSON API. Useful for scrobblers, Discord/Slack
-- webhooks, home-automation triggers, or a "what am I listening to" endpoint.
--
-- Install (single file):
--   cp now-playing-webhook.lua ~/.config/spotatui/plugins/
-- Edit WEBHOOK_URL below first.

local WEBHOOK_URL = "https://example.com/webhook"

spotatui.on("track_change", function(pb)
  if not pb or not pb.track then
    return
  end

  local payload = spotatui.json_encode({
    event = "now_playing",
    uri = pb.track.uri,
    name = pb.track.name,
    artists = pb.track.artists,
    album = pb.track.album,
    is_playing = pb.is_playing,
  })

  spotatui.http_post(
    WEBHOOK_URL,
    payload,
    { ["content-type"] = "application/json" },
    function(resp, err)
      if err then
        spotatui.log("now-playing-webhook: request failed: " .. err)
      elseif not resp.ok then
        spotatui.log("now-playing-webhook: server returned " .. resp.status)
      end
    end
  )
end)
