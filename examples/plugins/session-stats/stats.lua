-- Helper module for the session-stats plugin. Loaded via `require("stats")`, which resolves
-- against the plugin's own directory (spotatui adds it to package.path for directory plugins).

local M = {
  count = 0,
  recent = {},
}

function M.record(name)
  M.count = M.count + 1
  table.insert(M.recent, 1, name)
  -- Keep only the last 10.
  while #M.recent > 10 do
    table.remove(M.recent)
  end
end

return M
