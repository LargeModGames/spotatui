-- accent-cycler: a `cycle_accent` command that rotates the theme accent color at runtime.
--
-- Theme overrides from set_theme are runtime-only; they reset when spotatui restarts.
--
-- Install (single file):
--   cp accent-cycler.lua ~/.config/spotatui/plugins/
--
-- Suggested binding, in ~/.config/spotatui/config.yml:
--   plugin_commands:
--     cycle_accent: "ctrl-y"

local accents = { "Magenta", "Cyan", "Green", "Yellow", "Red", "Blue" }
local index = 0

spotatui.register_command("cycle_accent", function()
  index = (index % #accents) + 1
  local color = accents[index]
  spotatui.set_theme({
    playbar_progress = color,
    hint = color,
    selected = color,
  })
  spotatui.notify("Accent: " .. color, 2)
end)
