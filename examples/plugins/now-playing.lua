-- now-playing: a custom now-playing screen showing the v6 API - the cover_art
-- widget beside synchronized lyrics, laid out in a row container with
-- per-axis size hints.
--
-- Install (single file):
--   cp now-playing.lua "${XDG_CONFIG_HOME:-$HOME/.config}/spotatui/plugins/"
--
-- Suggested binding, in config.yml in the spotatui app config directory:
--   plugin_commands:
--     now_playing: "ctrl-n"
--
-- Keys inside the screen: Esc leaves. Lyrics follow playback automatically.
--
-- Cover art needs a build with the `cover-art` feature and a terminal with a
-- graphics protocol (Kitty, iTerm2, Sixel). Without one the widget draws a
-- status line in its place and the rest of the screen still works.

spotatui.require_api(6)

local SCREEN = "now_playing"

-- How many lyric lines to show around the current one.
local CONTEXT_LINES = 8

local lyrics = {}       -- { { time_ms = ..., text = ... }, ... }
local lyrics_status = "not_started"
local lyrics_track = nil -- uri the loaded lyrics belong to

local function track_uri()
  local pb = spotatui.playback()
  return pb and pb.track and pb.track.uri or nil
end

-- Index of the last lyric line whose timestamp has passed, or nil.
local function current_line_index(progress_ms)
  local found = nil
  for i, line in ipairs(lyrics) do
    if line.time_ms <= progress_ms then
      found = i
    else
      break
    end
  end
  return found
end

-- A window of lyric lines centred on the current one, with it highlighted.
local function lyric_lines(progress_ms)
  if #lyrics == 0 then
    local message = "No lyrics for this track"
    if lyrics_status == "loading" or lyrics_status == "not_started" then
      message = "Loading lyrics..."
    end
    return { { text = message, italic = true } }
  end

  local current = current_line_index(progress_ms)
  local first = math.max(1, (current or 1) - 2)
  local last = math.min(#lyrics, first + CONTEXT_LINES)

  local out = {}
  for i = first, last do
    if i == current then
      out[#out + 1] = { text = lyrics[i].text, bold = true }
    else
      out[#out + 1] = { text = lyrics[i].text, fg = "DarkGray" }
    end
  end
  return out
end

local function render()
  local pb = spotatui.playback()
  local title = "Nothing playing"
  if pb and pb.track then
    title = pb.track.name .. " - " .. table.concat(pb.track.artists, ", ")
  end

  spotatui.set_screen(SCREEN, {
    -- scroll = false keeps the header pinned; PageUp/PageDown move the lyrics.
    { type = "paragraph", height = 2, scroll = false, lines = {
      { text = title, bold = true },
      { text = "Esc leaves", italic = true },
    } },
    {
      type = "row",
      children = {
        { type = "cover_art", source = "current", fit = "contain", width_percent = 45 },
        { type = "paragraph", width_percent = 55, lines = lyric_lines(pb and pb.progress_ms or 0) },
      },
    },
  })
end

local function load_lyrics()
  local uri = track_uri()
  lyrics = {}
  lyrics_status = "loading"
  lyrics_track = uri
  render()

  spotatui.get_lyrics(function(data, err)
    -- The track may have changed while the fetch was in flight.
    if err or not data or track_uri() ~= lyrics_track then
      lyrics_status = "not_found"
      render()
      return
    end
    lyrics_status = data.status
    lyrics = data.lines or {}
    render()
  end)
end

spotatui.register_screen(SCREEN, {
  title = "Now Playing",
  on_key = function()
    -- Esc and the back key are handled globally; nothing else to do here.
  end,
  on_open = function()
    load_lyrics()
  end,
})

-- Advance the highlighted lyric line while the screen is open.
spotatui.set_interval(1000, function()
  if spotatui.current_route() == "plugin:" .. SCREEN then
    render()
  end
end)

spotatui.on("track_change", function()
  if spotatui.current_route() == "plugin:" .. SCREEN then
    load_lyrics()
  end
end)

spotatui.register_command("now_playing", function()
  spotatui.show_screen(SCREEN)
end)
