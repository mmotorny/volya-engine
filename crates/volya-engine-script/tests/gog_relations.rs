// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's relations, community and ranking, checked against
//! docs/behavior/script-api-relations-community-and-ranking.md (cited by
//! section): the 250 loadable scripts (docs/behavior/script-loading.md 3,
//! recomputed here) and their uses of this spec's names (1.2, 10), the
//! shipped `game_relations.ltx` as the system configuration holds it
//! (2.1 – 2.3), and the members on a game state started with the shipped
//! scripts over a New Game's simulation: an `esc_lager` campfire stalker,
//! the trader, a dog and the player online from their records, as the
//! campfire stalkers' registration and the shipped scripts call them (4,
//! 5, 7, 8, 9). Skips when `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;
#[path = "support/shipped_scripts.rs"]
mod shipped_scripts;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::rc::Rc;

use shipped_scripts::{FSGAME, GameTrees, Token, tokens};
use volya_engine_core::config::{self, Config, PathAliases, value};
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::simulation::graph::Graph;
use volya_engine_core::simulation::{AlifeConfig, Simulation};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{EntryKind, StdVfs, Vfs, read};
use volya_engine_formats::game_graph::GameGraph;
use volya_engine_formats::object::Kind;
use volya_engine_formats::patrol_path::PatrolPaths;
use volya_engine_script::Value;
use volya_engine_script::entry_points::{GameScripts, Startup};
use volya_engine_script::files::FileSystem;
use volya_engine_script::game_object::{GameObjects, ObjectClass, SpawnData};
use volya_engine_script::loader::Prefix;
use volya_engine_script::services::EngineServices;
use volya_engine_script::simulation::AlifeSimulator;
use volya_engine_script::story::Story;

/// Every file under `dir`, `/`-separated.
fn walk(vfs: &dyn Vfs, dir: &str, out: &mut Vec<String>) {
    for e in vfs.read_dir(dir).unwrap() {
        let path = if dir.is_empty() {
            e.name.clone()
        } else {
            format!("{dir}/{}", e.name)
        };
        match e.kind {
            EntryKind::Dir => walk(vfs, &path, out),
            EntryKind::File => out.push(path),
        }
    }
}

/// The words of `bytes`: runs of letters, digits and `_` not starting with
/// a digit, lowercase (script-loading.md 3).
fn words(bytes: &[u8], out: &mut BTreeSet<String>) {
    for run in bytes.split(|b| !(b.is_ascii_alphanumeric() || *b == b'_')) {
        if run.first().is_some_and(|b| !b.is_ascii_digit()) {
            out.insert(String::from_utf8_lossy(run).to_ascii_lowercase());
        }
    }
}

/// A script's words after comment removal, string contents kept.
fn script_words(tokens: &[Token]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for t in tokens {
        match t {
            Token::Name(n) => {
                out.insert(n.to_ascii_lowercase());
            }
            Token::String(s) => words(s, &mut out),
            _ => {}
        }
    }
    out
}

/// The 250 loadable scripts of script-loading.md 3 and every script's
/// tokens by name.
fn loadable(base: &dyn Vfs) -> (BTreeSet<String>, BTreeMap<String, Vec<Token>>) {
    let mut files = Vec::new();
    walk(base, "", &mut files);
    let lower: Vec<(String, &String)> = files.iter().map(|f| (f.to_ascii_lowercase(), f)).collect();
    let ext = |f: &str, exts: &[&str]| exts.iter().any(|e| f.ends_with(e));
    let roots_files: Vec<&String> = lower
        .iter()
        .filter(|(l, _)| {
            (l.starts_with("gamedata/config/") && ext(l, &[".ltx", ".xml"]))
                || (!l.starts_with("gamedata/config/")
                    && !l.starts_with("gamedata/scripts/")
                    && ext(l, &[".ltx", ".ini", ".spawn", ".game"]))
        })
        .map(|(_, p)| *p)
        .collect();
    assert_eq!(roots_files.len(), 893 + 195, "root files");
    let mut roots = BTreeSet::from(["_g".to_owned()]);
    for path in roots_files {
        words(&read(base, path).unwrap(), &mut roots);
    }
    let binaries = gog_archives::game_files("binaries", |p| {
        let p = p.to_ascii_lowercase();
        p == "bin/xrgame.dll" || p == "bin/xr_3da.exe"
    })
    .unwrap();
    assert_eq!(binaries.len(), 2, "xrGame.dll and XR_3DA.exe");
    for binary in &binaries {
        words(&fs::read(&binary.file).unwrap(), &mut roots);
    }
    let mut scripts = BTreeMap::new();
    for (l, path) in &lower {
        if let Some(name) = l
            .strip_prefix("gamedata/scripts/")
            .and_then(|n| n.strip_suffix(".script"))
        {
            scripts.insert(name.to_owned(), tokens(&read(base, path).unwrap()));
        }
    }
    let mut reached = BTreeSet::new();
    let mut pending: Vec<String> = scripts
        .keys()
        .filter(|n| roots.contains(*n))
        .cloned()
        .collect();
    while let Some(name) = pending.pop() {
        if !reached.insert(name.clone()) {
            continue;
        }
        let found = script_words(&scripts[&name]);
        pending.extend(
            scripts
                .keys()
                .filter(|n| found.contains(*n) && !reached.contains(*n))
                .cloned(),
        );
    }
    assert_eq!(reached.len(), 250, "loadable scripts");
    (reached, scripts)
}

/// The names a script calls after `:`.
fn methods(tokens: &[Token]) -> BTreeSet<String> {
    tokens
        .windows(2)
        .filter_map(|w| match w {
            [Token::Punct(b':'), Token::Name(n)] => Some(n.clone()),
            _ => None,
        })
        .collect()
}

/// The functions a script calls of namespace `namespace`.
fn functions(tokens: &[Token], namespace: &str) -> BTreeSet<String> {
    tokens
        .windows(3)
        .filter_map(|w| match w {
            [Token::Name(ns), Token::Punct(b'.'), Token::Name(n)] if ns == namespace => {
                Some(n.clone())
            }
            _ => None,
        })
        .collect()
}

/// Section 10's most used names: loadable scripts calling each.
const USED: [(&str, usize); 4] = [
    ("character_community", 14),
    ("relation", 11),
    ("character_rank", 6),
    ("set_relation", 6),
];

/// Section 10: the members no script calls.
const UNUSED: [&str; 6] = [
    "team",
    "general_goodwill",
    "change_goodwill",
    "get_actor_relation_flags",
    "set_actor_relation_flags",
    "set_character_reputation",
];

#[test]
fn shipped_uses_of_the_relations_names() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let base = DbVfs::open_game(root, None).unwrap();
    let (reached, scripts) = loadable(&base);
    let scripts: BTreeMap<&str, &Vec<Token>> = (scripts.iter())
        .filter(|(n, _)| n.as_str() != "lua_help")
        .map(|(n, t)| (n.as_str(), t))
        .collect();
    let calling = |n: &str| -> Vec<&str> {
        (scripts.iter())
            .filter(|(_, t)| methods(t).contains(n))
            .map(|(s, _)| *s)
            .collect()
    };
    let loadable_calling = |n: &str| calling(n).iter().filter(|s| reached.contains(**s)).count();
    let got: Vec<(&str, usize)> = USED
        .iter()
        .map(|(n, _)| (*n, loadable_calling(n)))
        .collect();
    assert_eq!(got, USED);
    for n in UNUSED {
        assert_eq!(calling(n), Vec::<&str>::new(), "{n}");
    }
    let stats_calls: BTreeSet<String> = (scripts.values())
        .flat_map(|t| functions(t, "actor_stats"))
        .collect();
    for n in ["get_points", "add_points_str", "get_actor_ranking"] {
        assert!(!stats_calls.contains(n), "actor_stats.{n}");
    }
    // 1.2: the goodwill methods game objects lack, as the shipped scripts
    // call them.
    assert_eq!(calling("set_community_goodwill"), ["enemy", "xr_gulag"]);
    assert_eq!(calling("community_goodwill"), ["enemy"]);
}

/// The system configuration of the shipped data.
fn system(tree: &dyn Vfs, aliases: &PathAliases) -> Config {
    config::load_aliased(tree, aliases, "$game_config$", "system.ltx").unwrap()
}

/// `(section, key)`'s comma-separated integers.
fn numbers(system: &Config, section: &str, key: &str) -> Vec<i64> {
    let v = system.read(section, key).unwrap();
    v.split(',').map(value::int).collect()
}

/// A list's (name, number) pairs, a missing number 0 (2.3).
fn list(system: &Config, section: &str, key: &str) -> Vec<(String, i64)> {
    let items: Vec<&str> = system.read(section, key).unwrap().split(',').collect();
    (items.chunks(2))
        .map(|p| {
            (
                p[0].trim().to_owned(),
                p.get(1).map_or(0, |n| value::int(n)),
            )
        })
        .collect()
}

/// 2.1 – 2.3 and Q-21: the shipped `game_relations.ltx` in the system
/// configuration.
#[test]
fn shipped_relations_configuration() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();
    let system = system(&tree, &aliases);
    assert_eq!(
        numbers(&system, "action_points", "personal_goodwill_limits"),
        [-1000, 1000]
    );
    assert_eq!(
        numbers(&system, "action_points", "community_goodwill_limits"),
        [-3000, 1000]
    );
    let one = |key: &str| numbers(&system, "game_relations", key)[0];
    assert_eq!(
        [
            one("goodwill_friend"),
            one("goodwill_neutal"),
            one("goodwill_enemy")
        ],
        [1000, 0, -1000]
    );
    assert_eq!(
        [
            one("attitude_neutal_threshold"),
            one("attitude_friend_threshold")
        ],
        [-400, 500]
    );
    let rating = list(&system, "game_relations", "rating");
    let names: Vec<&str> = rating.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["novice", "experienced", "veteran", "master"]);
    assert_eq!(
        rating[..3].iter().map(|r| r.1).collect::<Vec<_>>(),
        [300, 600, 900]
    );
    let reputation = list(&system, "game_relations", "reputation");
    let names: Vec<&str> = reputation.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(
        names,
        [
            "terrible",
            "very_bad",
            "bad",
            "neutral",
            "good",
            "very_good",
            "excellent"
        ]
    );
    for table in ["rank_relations", "reputation_relations"] {
        let s = system.section(table).unwrap();
        for e in s.entries() {
            let row: Vec<i64> = e.value().unwrap_or("").split(',').map(value::int).collect();
            assert!(row.iter().all(|&v| v == 0), "[{table}] {}", e.key());
        }
    }
    let monsters = list(&system, "monster_communities", "communities");
    assert_eq!(monsters.len(), 20, "monster communities");
    let number = |n: &str| monsters.iter().find(|(m, _)| m == n).map(|m| m.1);
    assert_eq!(
        [
            number("actor"),
            number("human"),
            number("arena_monstr"),
            number("boar")
        ],
        [Some(255), Some(255), Some(39), Some(40)]
    );
    assert_eq!(
        monsters.last().map(|m| (m.0.as_str(), m.1)),
        Some(("tushkano", 56))
    );
    // The `human` row's missing comma (2.3): its first item holds the
    // first two written values, so as read it is one item short and every
    // later column takes the next written value.
    let file = read(&tree, "gamedata/config/creatures/game_relations.ltx").unwrap();
    let file = String::from_utf8_lossy(&file);
    let line = (file.lines())
        .find_map(|l| l.trim_start().strip_prefix("human"))
        .and_then(|l| l.trim_start().strip_prefix('='))
        .unwrap();
    let line = line.split(';').next().unwrap();
    let written: Vec<i64> = (line.split([',', ' ', '\t']))
        .filter(|w| !w.trim().is_empty())
        .map(value::int)
        .collect();
    assert_eq!(written.len(), monsters.len(), "the values written");
    let human = system.get("monster_relations", "human").flatten().unwrap();
    let read: Vec<i64> = human.split(',').map(value::int).collect();
    assert_eq!(read.len(), monsters.len() - 1, "the items read");
    assert_eq!(read[1..], written[2..], "the later columns");
    assert_eq!(written[..2], [0, 1], "the first item's two values");
    // TODO(#1201): 2.3 has `human` -> `actor` 0; the configuration's
    // whitespace removal (ltx-config.md) makes the first item `01`.
    assert_eq!(read[0], 1, "the first item as the configuration holds it");
}

const STALKER_CLASS: ObjectClass = ObjectClass::Named("CAI_Stalker");

/// A started game state, its simulation, the system configuration and the
/// ids of the player, the trader, the stalker and the dog.
type Started = (GameScripts, Rc<RefCell<AlifeSimulator>>, Config, [u16; 4]);

/// A started game state (spec D) over a New Game's simulation, with the
/// player, the trader, an `esc_lager` stalker and a dog online from their
/// records, unbound.
fn started() -> Option<Started> {
    let archives = gog_archives::archives()?;
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();
    let trees = Rc::new(GameTrees { tree, root });
    let vfs = &trees.tree;
    let system = system(vfs, &aliases);
    let game = config::load_aliased(vfs, &aliases, "$game_config$", "game.ltx").unwrap();
    let alife_ltx = config::load_aliased(vfs, &aliases, "$game_config$", "alife.ltx").unwrap();
    let spawn_file = read(vfs, "gamedata/spawns/all.spawn").unwrap();
    let graph_file = read(vfs, "gamedata/game.graph").unwrap();
    let mut sim = Simulation::new(
        Graph::new(GameGraph::parse(&graph_file).unwrap()),
        PatrolPaths::parse(&spawn_file).unwrap(),
        AlifeConfig::read(&alife_ltx, &system).unwrap(),
    );
    sim.new_game(&spawn_file, &system, &mut |_, _| Ok(5), None)
        .unwrap();
    let console = Console::new(ConsoleFiles {
        default_controls: Vec::new(),
        presets: Default::default(),
    });
    let services = Rc::new(RefCell::new(EngineServices::new(console, system.clone())));
    let files = Rc::new(RefCell::new(FileSystem::new(
        trees.clone(),
        aliases.clone(),
        Rc::new(system.clone()),
        Rc::new(game),
    )));
    let alife = Rc::new(RefCell::new(AlifeSimulator::new()));
    let mut scripts = GameScripts::start(Startup {
        trees,
        aliases,
        prefix: Prefix::Default,
        services,
        objects: Rc::new(RefCell::new(GameObjects::new())),
        files,
        story: Rc::new(RefCell::new(Story::new())),
        dialogs: Rc::default(),
        times: Rc::default(),
        sounds: Rc::default(),
        simulation: Rc::clone(&alife),
        window_files: Rc::default(),
        level: Rc::default(),
        effects_and_weather: Rc::default(),
    })
    .unwrap();
    let find = |f: &dyn Fn(&volya_engine_formats::object::ObjectRecord) -> bool| {
        sim.objects().find(|o| f(&o.record)).map(|o| o.id).unwrap()
    };
    let player = find(&|r| r.kind() == Kind::Player);
    let trader = find(&|r| r.kind() == Kind::Trader);
    let stalker =
        find(&|r| r.kind() == Kind::Stalker && r.header.name.as_str().starts_with("esc_lager"));
    let dog = find(&|r| {
        r.kind() == Kind::Monster
            && system
                .get(r.header.section.as_str(), "species")
                .flatten()
                .map(str::trim)
                == Some("dog")
    });
    let data: Vec<(ObjectClass, SpawnData)> = [
        (ObjectClass::Player, player),
        (ObjectClass::Named("CAI_Trader"), trader),
        (STALKER_CLASS, stalker),
        (ObjectClass::Named("CAI_Dog"), dog),
    ]
    .into_iter()
    .map(|(class, id)| {
        let record = &sim.object(id).unwrap().record;
        (class, SpawnData::from_record(id, record, &system).unwrap())
    })
    .collect();
    {
        let mut a = alife.borrow_mut();
        a.simulation = Some(sim);
        a.graph_file = Some(graph_file.into());
    }
    let s = scripts.scripts();
    for (class, data) in data {
        // Unbound: no function of this name (spec L 3.1).
        (s.spawn_engine_object_with_binding(class, data, Some(b"volya_test_unbound"))).unwrap();
    }
    s.exec(
        "=volya_test",
        b"function volya_test_keep(...) volya_test_handles = {...} end",
    )
    .unwrap();
    let ids = [player, trader, stalker, dog];
    let handles: Vec<Value> = (ids.iter())
        .map(|&id| Value::Object(s.game_object(id).unwrap()))
        .collect();
    s.call("volya_test_keep", &handles).unwrap();
    Some((scripts, alife, system, ids))
}

fn run(scripts: &mut GameScripts, code: &str) -> Vec<Value> {
    let code = format!("local actor, trader, stalker, dog = unpack(volya_test_handles)\n{code}");
    (scripts.scripts().exec("=check", code.as_bytes())).unwrap_or_else(|e| panic!("{code}: {e}"))
}

fn numbers_of(values: &[Value]) -> Vec<f64> {
    values
        .iter()
        .map(|v| v.as_number().unwrap_or(f64::NAN))
        .collect()
}

/// 2.1, 5.3, 8.1: the campfire stalker's registration (`se_stalker`'s
/// ranking, `xr_motivator`'s community) on its New Game record; 5.4 sets
/// its simulation object's community too.
#[test]
fn a_campfire_stalker_registers() {
    let Some((mut scripts, alife, system, [player, _, stalker, _])) = started() else {
        return;
    };
    // The New Game without identities leaves the record's community unset
    // (character-identity.md 4.2): the shipped `set_character_community`
    // sets it.
    let got = run(
        &mut scripts,
        "if actor_stats.add_to_ranking ~= nil then actor_stats.add_to_ranking(stalker:id()) end\n\
         actor_stats.add_to_ranking(actor:id())\n\
         stalker:set_character_community('stalker', 0, 0)\n\
         actor:set_character_community('actor', 0, 0)\n\
         stalker:set_character_rank(500) actor:set_character_rank(100)\n\
         return stalker:character_community(), stalker:team(), actor_stats.get_actor_ranking()",
    );
    let stalker_number = list(&system, "game_relations", "communities")
        .into_iter()
        .find(|(n, _)| n == "stalker")
        .map(|(_, n)| n as f64);
    assert_eq!(
        got,
        [
            Value::from("stalker"),
            Value::Number(stalker_number.unwrap()),
            Value::Number(1.0)
        ]
    );
    let a = alife.borrow();
    let ranking: Vec<u16> = a.relations.ranking.iter().map(|e| e.id).collect();
    assert_eq!(ranking, [stalker, player], "sorted by rank");
}

/// 4.1, 4.2, 7, 9 with the shipped values: the limits, `set_relation`'s
/// goodwills, the thresholds, the monster table's shifted `human` row and
/// the storyline reward.
#[test]
fn the_shipped_relations() {
    let Some((mut scripts, _, system, _)) = started() else {
        return;
    };
    let got = run(
        &mut scripts,
        "local a = actor:id()\n\
         stalker:set_goodwill(5000, actor) local high = stalker:goodwill(actor)\n\
         stalker:set_goodwill(-5000, actor) local low = stalker:goodwill(actor)\n\
         local r = {}\n\
         for i, v in ipairs({0, 1, 2, -1}) do stalker:set_relation(v, actor) r[i] = stalker:goodwill(actor) end\n\
         relation_registry.set_community_goodwill('freedom', a, -5000)\n\
         local freedom = relation_registry.community_goodwill('freedom', a)\n\
         relation_registry.set_community_goodwill('dolg', a, 5000)\n\
         local dolg = relation_registry.community_goodwill('dolg', a)\n\
         return high, low, r[1], r[2], r[3], r[4], freedom, dolg",
    );
    assert_eq!(
        numbers_of(&got),
        [
            1000.0, -1000.0, 1000.0, 0.0, -1000.0, -1000.0, -3000.0, 1000.0
        ]
    );
    // The attitude at the thresholds, its other terms the communities'
    // table (the rank and reputation tables are all 0).
    let communities = list(&system, "game_relations", "communities");
    let position = |n: &str| communities.iter().position(|(c, _)| c == n).unwrap();
    let row = system
        .get("communities_relations", "stalker")
        .flatten()
        .unwrap();
    let table = value::int(row.split(',').nth(position("actor")).unwrap()) as i32;
    let mut at = |goodwill: i32| {
        let code = format!(
            "stalker:set_character_community('stalker', 0, 0) actor:set_character_community('actor', 0, 0)\n\
             stalker:set_goodwill(0, actor)\n\
             relation_registry.set_community_goodwill('stalker', actor:id(), {goodwill})\n\
             return stalker:general_goodwill(actor), stalker:relation(actor)"
        );
        numbers_of(&run(&mut scripts, &code))
    };
    let neutral = -400 - table;
    assert!((-3000..=1000).contains(&neutral), "{neutral}");
    assert_eq!(at(neutral), [-400.0, 1.0], "at the neutral threshold");
    assert_eq!(at(neutral - 1), [-401.0, 2.0], "below it");
    assert_eq!(at(500 - table), [500.0, 0.0], "at the friend threshold");
    // `[monster_relations]`: the stalker (`human`) towards the dog reads
    // the row's item at the dog's position, the value written one column
    // further on (2.3).
    let monsters = list(&system, "monster_communities", "communities");
    let dog = monsters.iter().position(|(n, _)| n == "dog").unwrap();
    let human = system.get("monster_relations", "human").flatten().unwrap();
    let read: Vec<i64> = human.split(',').map(value::int).collect();
    let relation = |v: i64| match v {
        1 => 0.0,
        0 => 1.0,
        -1 => 2.0,
        -2 => 3.0,
        _ => -1.0,
    };
    let got = run(&mut scripts, "return stalker:relation(dog), dog:team()");
    let dog_number = monsters[dog].1 as f64;
    assert_eq!(numbers_of(&got), [relation(read[dog]), dog_number]);
    // 9: the storyline reward on the player.
    let got = run(
        &mut scripts,
        "actor:change_character_reputation(-actor:character_reputation())\n\
         actor:change_character_reputation(10)\n\
         local p = actor:character_reputation()\n\
         actor:change_character_reputation(actor:character_reputation() + 25)\n\
         relation_registry.change_community_goodwill('dolg', actor:id(), 100)\n\
         actor_stats.add_points('quests', 'storyline', 1, 50)\n\
         actor:set_character_rank(600)\n\
         actor:set_character_rank(actor:character_rank() + 50)\n\
         return actor:character_reputation() - 2 * p, relation_registry.community_goodwill('dolg', actor:id()), \
         actor_stats.get_points('quests'), actor:character_rank()",
    );
    assert_eq!(numbers_of(&got), [25.0, 1000.0, 50.0, 650.0]);
}
