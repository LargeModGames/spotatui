-- queue-browser: a custom plugin screen that lists the play queue and lets you
-- queue more music from your library, showing off the v5 API (custom screens,
-- async data reads, timers, storage, navigation).
--
-- Install (single file):
--   cp queue-browser.lua ~/.config/spotatui/plugins/
--
-- Suggested binding, in ~/.config/spotatui/config.yml:
--   plugin_commands:
--     queue_browser: "ctrl-b"
--
-- Keys inside the screen: j/k move, enter queues the selected liked song,
-- r refreshes, Esc leaves.

spotatui.require_api(5)

local SCREEN = "queue_browser"

local queue_lines = {}
local liked = {} -- { { name = ..., uri = ... }, ... }
local selected = 1
local status = "loading..."

local function render()
  local items = {}
  for _, entry in ipairs(liked) do
    items[#items + 1] = entry.name
  end
  if #items == 0 then
    items = { { text = "(no liked songs loaded)", italic = true } }
  end

  spotatui.set_screen(SCREEN, {
    {
      type = "paragraph",
      height = 2,
      lines = {
        { text = "j/k move - enter queues the selected liked song - r refreshes - Esc leaves", italic = true },
        { text = status, fg = "Yellow" },
      },
    },
    { type = "paragraph", lines = queue_lines, height = #queue_lines + 1 },
    { type = "list", title = "Liked songs", items = items, selected = math.min(selected, math.max(#items, 1)) },
  })
end

local function refresh()
  status = "refreshing..."
  render()

  spotatui.get_queue(function(queue, err)
    queue_lines = {}
    if err then
      queue_lines = { { text = "queue: " .. err, fg = "Red" } }
    else
      local now = queue.currently_playing
      local now_name = now and (now.track and now.track.name or (now.episode and now.episode.name)) or "nothing"
      queue_lines[1] = { text = "Now playing: " .. now_name, bold = true, fg = "Green" }
      for i, item in ipairs(queue.items) do
        if i > 5 then
          queue_lines[#queue_lines + 1] = { text = ("  ... and %d more"):format(#queue.items - 5), italic = true }
          break
        end
        local name = item.track and item.track.name or (item.episode and item.episode.name) or "?"
        queue_lines[#queue_lines + 1] = "  " .. i .. ". " .. name
      end
    end
    status = ""
    render()
  end)

  spotatui.get_saved_tracks(function(tracks, err)
    if err then
      status = "liked songs: " .. err
      render()
      return
    end
    liked = {}
    for _, t in ipairs(tracks) do
      if t.uri then
        liked[#liked + 1] = { name = t.name .. " - " .. table.concat(t.artists, ", "), uri = t.uri }
      end
    end
    status = #liked .. " liked songs (queued opens count: " .. (spotatui.storage_get("opens") or 0) .. ")"
    render()
  end)
end

spotatui.register_screen(SCREEN, {
  title = "Queue Browser",
  on_key = function(key)
    if key == "j" and selected < #liked then
      selected = selected + 1
      render()
    elseif key == "k" and selected > 1 then
      selected = selected - 1
      render()
    elseif key == "enter" and liked[selected] then
      spotatui.add_to_queue(liked[selected].uri)
      spotatui.notify("Queued: " .. liked[selected].name, 3)
      -- The queue changes server-side; re-read it shortly after.
      spotatui.set_timeout(1000, refresh)
    elseif key == "r" then
      refresh()
    end
  end,
  on_open = function()
    spotatui.storage_set("opens", (spotatui.storage_get("opens") or 0) + 1)
    refresh()
  end,
})

spotatui.register_command("queue_browser", function()
  spotatui.show_screen(SCREEN)
end)
