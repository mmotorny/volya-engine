// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's data streams, checked against
//! docs/behavior/script-api-data-streams.md: the listing's members, and
//! the beginners' village campfire's `spawns\all.spawn` records read
//! through a `net_packet` as their scripted `STATE_Read` reads them (8).
//! Skips when `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;
#[path = "support/shipped_scripts.rs"]
mod shipped_scripts;

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::ops::ControlFlow;
use std::rc::Rc;

use shipped_scripts::{FSGAME, GameTrees};
use volya_engine_core::config::{self, Config, PathAliases};
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::simulation::class_of;
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{StdVfs, read};
use volya_engine_formats::object::{Object, ObjectRecord, SmartZoneScript, section};
use volya_engine_formats::spawn;
use volya_engine_script::Value;
use volya_engine_script::data_streams::{NetPacket, Reader};
use volya_engine_script::entry_points::{GameScripts, Startup};
use volya_engine_script::files::FileSystem;
use volya_engine_script::game_object::GameObjects;
use volya_engine_script::loader::Prefix;
use volya_engine_script::services::EngineServices;
use volya_engine_script::story::Story;

/// The game's trees over the shipped files, with `mods`.
fn trees() -> Option<(Rc<GameTrees>, PathAliases)> {
    let archives = gog_archives::archives()?;
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();
    Some((Rc::new(GameTrees { tree, root }), aliases))
}

/// The engine state and startup over the shipped files.
fn started(trees: Rc<GameTrees>, aliases: &PathAliases) -> (GameScripts, Config) {
    let tree = &trees.tree;
    let system = config::load_aliased(tree, aliases, "$game_config$", "system.ltx").unwrap();
    let game = config::load_aliased(tree, aliases, "$game_config$", "game.ltx").unwrap();
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
    let g = GameScripts::start(Startup {
        trees,
        aliases: aliases.clone(),
        prefix: Prefix::Default,
        services,
        objects: Rc::new(RefCell::new(GameObjects::new())),
        story: Rc::new(RefCell::new(Story::new())),
        dialogs: Rc::default(),
        times: Rc::default(),
        sounds: Rc::default(),
        simulation: Rc::default(),
        level: Rc::default(),
        effects_and_weather: Rc::default(),
        files,
        window_files: Rc::default(),
    })
    .unwrap();
    (g, system)
}

/// The member names the listing (`scripts\lua_help.script`) gives class
/// `name`.
fn listed_members(text: &str, name: &str) -> BTreeSet<String> {
    let mut members = BTreeSet::new();
    let mut inside = false;
    for line in text.lines().map(str::trim) {
        if let Some(rest) = line.strip_prefix("C++ class ") {
            let head = rest.trim_end_matches('{').trim();
            inside = head.split(':').next().unwrap().trim() == name;
            continue;
        }
        if !inside {
            continue;
        }
        if line.starts_with("};") {
            break;
        }
        if let Some(rest) = line.strip_prefix("function ") {
            let word: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            members.insert(word);
        }
    }
    members
}

/// 1: every member the listing gives `net_packet`, `reader` and `ClientID`
/// is one of the registered classes' (63, 32 and 2 functions under 53, 23
/// and 2 names).
#[test]
fn registrations_against_the_listing() {
    let Some((trees, aliases)) = trees() else {
        return;
    };
    let text = read(&trees.tree, "gamedata/scripts/lua_help.script").unwrap();
    let text = String::from_utf8_lossy(&text).into_owned();
    let (mut g, _) = started(trees, &aliases);
    let s = g.scripts();
    let reader = s.new_reader(Reader::over(Vec::new())).unwrap();
    s.exec(
        "=listing",
        b"function test_missing(class, reader, ...)\n\
            local o = class == 'reader' and reader or _G[class]()\n\
            local missing = {}\n\
            for _, n in ipairs({...}) do\n\
              if type(o[n]) ~= 'function' then table.insert(missing, n) end\n\
            end\n\
            return table.concat(missing, ' ')\n\
          end",
    )
    .unwrap();
    for (class, count) in [("net_packet", 53), ("reader", 23), ("ClientID", 2)] {
        let names = listed_members(&text, class);
        assert_eq!(names.len(), count, "{class}'s listed names: {names:?}");
        let mut args = vec![Value::from(class), Value::Object(reader)];
        args.extend(names.iter().map(|n| Value::from(n.as_str())));
        let missing = s.call("test_missing", &args).unwrap();
        assert_eq!(missing, [Value::from("")], "{class}: missing members");
    }
}

/// A record of `spawns\all.spawn`: its index, its object's name, its
/// spawn packet and its parsed record.
struct Record {
    index: u32,
    name: String,
    spawn: Vec<u8>,
    record: ObjectRecord,
}

/// Whether the custom data's `[smart_terrains]` section names `esc_lager`.
fn names_esc_lager(custom: &str) -> bool {
    let mut inside = false;
    for line in custom.lines().map(str::trim) {
        if line.starts_with('[') {
            inside = line.eq_ignore_ascii_case("[smart_terrains]");
        } else if inside && line.split(['=', ' ', '\t']).next() == Some("esc_lager") {
            return true;
        }
    }
    false
}

/// The u16 at byte `at`.
fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

/// 8: the campfire's stalkers (the 11 `se_stalker` records whose
/// `[smart_terrains]` name `esc_lager`, `esc_lager1` among them) and the
/// smart terrain `esc_lager` read through the engine's packet holding
/// their spawn packet, at the read position the engine's `STATE_Read`
/// leaves (the script part is their last 3 bytes, 8), as the scripted
/// `STATE_Read` reads; and written back as the scripted `STATE_Write`
/// writes them.
#[test]
fn the_campfire_records_through_a_packet() {
    let Some((trees, aliases)) = trees() else {
        return;
    };
    let file = read(&trees.tree, "gamedata/spawns/all.spawn").unwrap();
    let (mut g, system) = started(trees, &aliases);
    let mut stalkers = Vec::new();
    let mut terrain = None;
    spawn::for_each_record(&file, |r| {
        let section = section(r.spawn)?;
        let class = class_of(&system, &section)?.to_owned();
        if class != "AI_STL_S" && class != "SMRTTRRN" {
            return Ok(ControlFlow::Continue(()));
        }
        let record = ObjectRecord::parse(r.spawn, r.update, &class, true)?;
        let name = record.header.name.as_str().to_owned();
        let custom = record.object.object_block().custom_data.as_str().to_owned();
        let entry = Record {
            index: r.index,
            name: name.clone(),
            spawn: r.spawn.to_vec(),
            record,
        };
        if class == "AI_STL_S" && names_esc_lager(&custom) {
            stalkers.push(entry);
        } else if class == "SMRTTRRN" && name == "esc_lager" {
            terrain = Some(entry);
        }
        Ok(ControlFlow::Continue(()))
    })
    .unwrap();
    assert_eq!(stalkers.len(), 11, "the stalkers of esc_lager");
    let first = stalkers.iter().find(|r| r.name == "esc_lager1").unwrap();
    assert_eq!(
        (first.index, first.spawn.len(), u16_at(&first.spawn, 65)),
        (93, 284, 219),
        "esc_lager1's record, spawn packet and data size (8)"
    );
    let terrain = terrain.unwrap();
    assert_eq!(
        (
            terrain.index,
            terrain.spawn.len(),
            u16_at(&terrain.spawn, 70)
        ),
        (101, 234, 164),
        "esc_lager's record, spawn packet and data size (8)"
    );

    let s = g.scripts();
    s.exec(
        "=campfire",
        b"function test_stalker_read(packet)\n\
            local before = packet:r_elapsed()\n\
            local mode = packet:r_u8()\n\
            local was_in_smart_terrain = packet:r_bool()\n\
            local death_droped = packet:r_bool()\n\
            return before, mode, was_in_smart_terrain, death_droped, packet:r_tell(), packet:r_eof()\n\
          end\n\
          function test_stalker_write(packet, mode, was, dropped)\n\
            packet:w_u8(mode) packet:w_bool(was) packet:w_bool(dropped)\n\
          end\n\
          function test_terrain_read(packet)\n\
            local before = packet:r_elapsed()\n\
            local duration_end = utils.r_CTime(packet)\n\
            local after_one = packet:r_tell()\n\
            local idle_end = utils.r_CTime(packet)\n\
            local gulag_working = packet:r_bool()\n\
            test_terrain_times = {duration_end, idle_end}\n\
            return before, after_one, gulag_working, packet:r_tell(), packet:r_eof()\n\
          end\n\
          function test_terrain_write(packet, working)\n\
            utils.w_CTime(packet, test_terrain_times[1])\n\
            utils.w_CTime(packet, test_terrain_times[2])\n\
            packet:w_bool(working)\n\
          end",
    )
    .unwrap();

    for r in &stalkers {
        let len = r.spawn.len() as u32;
        assert_eq!(
            &r.spawn[r.spawn.len() - 3..],
            [2, 0, 0],
            "{}'s script part",
            r.name
        );
        let mut packet = NetPacket::holding(&r.spawn).unwrap();
        packet.read_position = len - 3;
        let part = s.new_packet(packet).unwrap();
        let got = s.call("test_stalker_read", &[Value::Object(part)]).unwrap();
        assert_eq!(
            got,
            [
                3.0.into(),
                2.0.into(),
                false.into(),
                false.into(),
                f64::from(len).into(),
                true.into(),
            ],
            "{}: 2 (nil), false, false (8)",
            r.name
        );
        let positions = s.with_part(part, |p: &mut NetPacket| p.read_position);
        assert_eq!(positions, Some(len), "{}: the engine's packet read", r.name);
        // As the formats crate reads the scripted additions (sav.md 7.5).
        let Object::Stalker(stalker) = &r.record.object else {
            panic!("{} isn't a stalker", r.name);
        };
        let script = stalker.script.as_ref().unwrap();
        assert_eq!(script.job_online.as_ref().map(|j| j.mode), Some(2));
        assert_eq!(script.was_in_smart_terrain, Some(false));
        assert_eq!(script.death_items_dropped, Some(false));
        assert!(s.release_stream(part).unwrap());
        // The second read: the scripted writer writes the same bytes.
        let written = s.new_packet(NetPacket::new()).unwrap();
        s.call(
            "test_stalker_write",
            &[
                Value::Object(written),
                got[1].clone(),
                got[2].clone(),
                got[3].clone(),
            ],
        )
        .unwrap();
        let bytes = s.with_part(written, |p: &mut NetPacket| p.written().to_vec());
        assert_eq!(bytes, Some(vec![2, 0, 0]), "{}: written back", r.name);
        s.release_stream(written).unwrap();
    }

    let len = terrain.spawn.len() as u32;
    assert_eq!(&terrain.spawn[terrain.spawn.len() - 3..], [0, 0, 0]);
    let mut packet = NetPacket::holding(&terrain.spawn).unwrap();
    packet.read_position = len - 3;
    let part = s.new_packet(packet).unwrap();
    let got = s.call("test_terrain_read", &[Value::Object(part)]).unwrap();
    assert_eq!(
        got,
        [
            3.0.into(),
            232.0.into(),
            false.into(),
            234.0.into(),
            true.into(),
        ],
        "esc_lager: unset, unset, not working (8)"
    );
    let SmartZoneScript::SmartTerrain(t) = (match &terrain.record.object {
        Object::SmartZone(z) => &z.script,
        _ => panic!("esc_lager isn't a smart zone"),
    }) else {
        panic!("esc_lager has no smart terrain fields");
    };
    assert_eq!(
        (t.duration_end, t.idle_end, &t.working),
        (None, None, &None)
    );
    s.release_stream(part).unwrap();
    let written = s.new_packet(NetPacket::new()).unwrap();
    s.call(
        "test_terrain_write",
        &[Value::Object(written), got[2].clone()],
    )
    .unwrap();
    let bytes = s.with_part(written, |p: &mut NetPacket| p.written().to_vec());
    assert_eq!(bytes, Some(vec![0, 0, 0]), "esc_lager: written back");
    s.release_stream(written).unwrap();
}
