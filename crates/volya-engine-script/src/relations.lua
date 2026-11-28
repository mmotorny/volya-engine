-- SPDX-FileCopyrightText: 2026 Maksym Motorny
-- SPDX-License-Identifier: BSD-3-Clause

-- The relations' Lua side (relations.rs;
-- docs/behavior/script-api-relations-community-and-ranking.md, whose
-- section numbers these are): the members of `game_object` and the
-- namespaces `relation_registry` and `actor_stats` (1.2) over the Rust
-- side's operations. Run once per state, after the game object basics and
-- the simulation registry and before any script.

-- The Rust side's operations.
local raw = __volya_relations
__volya_relations = nil

local game_object = rawget(_G, "game_object")

for _, name in ipairs({
  "team", "squad", "group", "change_team", "relation", "set_relation",
  "goodwill", "set_goodwill", "change_goodwill", "general_goodwill", "rank",
  "character_rank", "set_character_rank", "character_reputation",
  "change_character_reputation", "character_community",
  "set_character_community", "get_actor_relation_flags",
  "set_actor_relation_flags",
}) do
  game_object[name] = function(...) return raw(name, ...) end
end

-- A namespace of the functions `names`, each the operation `<prefix>.<name>`.
local function namespace(prefix, names)
  local t = {}
  for _, name in ipairs(names) do
    local op = prefix .. "." .. name
    t[name] = function(...) return raw(op, ...) end
  end
  rawset(_G, prefix, t)
end

namespace("relation_registry", {
  "change_community_goodwill", "set_community_goodwill", "community_goodwill",
})
namespace("actor_stats", {
  "get_actor_ranking", "remove_from_ranking", "add_to_ranking", "get_points",
  "add_points_str", "add_points",
})
