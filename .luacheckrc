-- Luacheck config for the example plugins under examples/plugins/.
-- The host injects a single global, `spotatui` (see src/infra/scripting/api.rs).
std = "lua54"
read_globals = { "spotatui" }
