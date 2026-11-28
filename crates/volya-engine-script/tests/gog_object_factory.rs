// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's class registrations (BYOA) through the object
//! factory, checked against
//! docs/behavior/script-api-class-ids-and-the-object-factory.md: the
//! scripts' `clsid` reads (5.4), then startup's `class_registrators` call
//! (4) over test engine classes named after the classes the registrations
//! and their script classes name (none is the game's), the 54 entries it
//! adds (3.3), the `clsid` table (5.2, 5.3) and creating the scripts'
//! objects (6.2). Skips when `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;
#[path = "support/shipped_scripts.rs"]
mod shipped_scripts;

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use shipped_scripts::{ENGINE_BASES, FSGAME, GameTrees, Token, tokens};
use volya_engine_core::config::PathAliases;
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{StdVfs, Vfs, read};
use volya_engine_script::Value;
use volya_engine_script::classes::EngineClass;
use volya_engine_script::loader::{Common, Prefix, Scripts};
use volya_engine_script::object_factory::{CreatedObject, ENGINE_ENTRIES, Form, Source, class_id};

/// The shipped registrations (3.3, the "script" rows of 5.3): id text,
/// class-id name, client class, server class (none for the 3-argument
/// form).
const REGISTRATIONS: [(&str, &str, &str, Option<&str>); 54] = [
    (
        "AI_STL_S",
        "script_stalker",
        "CAI_Stalker",
        Some("se_stalker.se_stalker"),
    ),
    (
        "AI_TRD_S",
        "script_trader",
        "CAI_Trader",
        Some("se_stalker.se_trader"),
    ),
    (
        "CL_LAST",
        "game_cl_LastStanding",
        "game_last_cl.game_cl_LastStanding",
        None,
    ),
    (
        "C_HLCP_S",
        "script_heli",
        "CHelicopter",
        Some("se_car.se_heli"),
    ),
    (
        "E_STLK",
        "equ_stalker_s",
        "CStalkerOutfit",
        Some("se_item.se_outfit"),
    ),
    ("MAIN_MNU", "main_menu", "ui_main_menu.main_menu", None),
    (
        "O_PHYS_S",
        "script_phys",
        "CPhysicObject",
        Some("se_item.se_physic"),
    ),
    (
        "O_SWITCH",
        "switcher",
        "ce_switcher.ce_switcher",
        Some("se_switcher.cse_switcher"),
    ),
    (
        "RE_SPAWN",
        "respawn",
        "ce_smart_zone",
        Some("se_respawn.se_respawn"),
    ),
    (
        "SCRPTART",
        "artefact_s",
        "CElectricBall",
        Some("se_artefact.se_artefact"),
    ),
    ("SCRPTCAR", "car_s", "CCar", Some("se_car.se_car")),
    (
        "SMRTTRRN",
        "smart_terrain",
        "ce_smart_zone",
        Some("smart_terrain.se_smart_terrain"),
    ),
    (
        "SM_BLOOD",
        "bloodsucker_s",
        "CAI_Bloodsucker",
        Some("se_monster.se_monster"),
    ),
    (
        "SM_BOARW",
        "boar_s",
        "CAI_Boar",
        Some("se_monster.se_monster"),
    ),
    (
        "SM_BURER",
        "burer_s",
        "CBurer",
        Some("se_monster.se_monster"),
    ),
    ("SM_CAT_S", "cat_s", "CCat", Some("se_monster.se_monster")),
    (
        "SM_CHIMS",
        "chimera_s",
        "CChimera",
        Some("se_monster.se_monster"),
    ),
    (
        "SM_CONTR",
        "controller_s",
        "CController",
        Some("se_monster.se_monster"),
    ),
    (
        "SM_DOG_F",
        "psy_dog_phantom_s",
        "CPsyDogPhantom",
        Some("se_monster.se_monster"),
    ),
    (
        "SM_DOG_P",
        "psy_dog_s",
        "CPsyDog",
        Some("se_monster.se_monster"),
    ),
    (
        "SM_DOG_S",
        "dog_s",
        "CAI_Dog",
        Some("se_monster.se_monster"),
    ),
    (
        "SM_FLESH",
        "flesh_s",
        "CAI_Flesh",
        Some("se_monster.se_monster"),
    ),
    (
        "SM_GIANT",
        "gigant_s",
        "CPseudoGigant",
        Some("se_monster.se_monster"),
    ),
    (
        "SM_IZLOM",
        "fracture_s",
        "CFracture",
        Some("se_monster.se_monster"),
    ),
    (
        "SM_POLTR",
        "poltergeist_s",
        "CPoltergeist",
        Some("se_monster.se_monster"),
    ),
    (
        "SM_P_DOG",
        "pseudodog_s",
        "CAI_PseudoDog",
        Some("se_monster.se_monster"),
    ),
    (
        "SM_SNORK",
        "snork_s",
        "CSnork",
        Some("se_monster.se_monster"),
    ),
    (
        "SM_TUSHK",
        "tushkano_s",
        "CTushkano",
        Some("se_monster.se_monster"),
    ),
    (
        "SM_ZOMBI",
        "zombie_s",
        "CZombie",
        Some("se_monster.se_monster"),
    ),
    (
        "SPC_RS_S",
        "script_restr",
        "CSpaceRestrictor",
        Some("se_zones.se_restrictor"),
    ),
    (
        "SV_LAST",
        "game_sv_LastStanding",
        "game_last_sv.game_sv_LastStanding",
        None,
    ),
    (
        "TORCH_S",
        "device_torch_s",
        "CTorch",
        Some("se_item.se_item_torch"),
    ),
    (
        "WP_AK74",
        "wpn_ak74_s",
        "CWeaponAK74",
        Some("se_item.se_weapon_magazined_w_gl"),
    ),
    (
        "WP_BINOC",
        "wpn_binocular_s",
        "CWeaponBinoculars",
        Some("se_item.se_weapon_magazined"),
    ),
    (
        "WP_BM16",
        "wpn_bm16_s",
        "CWeaponBM16",
        Some("se_item.se_weapon_shotgun"),
    ),
    (
        "WP_GROZA",
        "wpn_groza_s",
        "CWeaponGroza",
        Some("se_item.se_weapon_magazined_w_gl"),
    ),
    (
        "WP_HPSA",
        "wpn_hpsa_s",
        "CWeaponHPSA",
        Some("se_item.se_weapon_magazined"),
    ),
    (
        "WP_KNIFE",
        "wpn_knife_s",
        "CWeaponKnife",
        Some("se_item.se_weapon"),
    ),
    (
        "WP_LR300",
        "wpn_lr300_s",
        "CWeaponLR300",
        Some("se_item.se_weapon_magazined"),
    ),
    (
        "WP_PM",
        "wpn_pm_s",
        "CWeaponPM",
        Some("se_item.se_weapon_magazined"),
    ),
    (
        "WP_RG6",
        "wpn_rg6_s",
        "CWeaponRG6",
        Some("se_item.se_weapon_shotgun"),
    ),
    (
        "WP_RPG7",
        "wpn_rpg7_s",
        "CWeaponRPG7",
        Some("se_item.se_weapon_magazined"),
    ),
    ("WP_SCOPE", "wpn_scope_s", "CScope", Some("se_item.se_item")),
    (
        "WP_SHOTG",
        "wpn_shotgun_s",
        "CWeaponShotgun",
        Some("se_item.se_weapon_shotgun"),
    ),
    (
        "WP_SVD",
        "wpn_svd_s",
        "CWeaponSVD",
        Some("se_item.se_weapon_magazined"),
    ),
    (
        "WP_SVU",
        "wpn_svu_s",
        "CWeaponSVU",
        Some("se_item.se_weapon_magazined"),
    ),
    (
        "WP_USP45",
        "wpn_usp45_s",
        "CWeaponUSP45",
        Some("se_item.se_weapon_magazined"),
    ),
    (
        "WP_VAL",
        "wpn_val_s",
        "CWeaponVal",
        Some("se_item.se_weapon_magazined"),
    ),
    (
        "WP_VINT",
        "wpn_vintorez_s",
        "CWeaponVintorez",
        Some("se_item.se_weapon_magazined"),
    ),
    (
        "WP_WALTH",
        "wpn_walther_s",
        "CWeaponWalther",
        Some("se_item.se_weapon_magazined"),
    ),
    (
        "ZS_BFUZZ",
        "zone_bfuzz_s",
        "CHairsZone",
        Some("se_zones.se_zone_visual"),
    ),
    (
        "ZS_GALAN",
        "zone_galant_s",
        "CMincer",
        Some("se_zones.se_zone_anom"),
    ),
    (
        "ZS_MBALD",
        "zone_mbald_s",
        "CMosquitoBald",
        Some("se_zones.se_zone_anom"),
    ),
    (
        "ZS_MINCE",
        "zone_mincer_s",
        "CMincer",
        Some("se_zones.se_zone_anom"),
    ),
];

/// Values of 5.3's table, spot-checked: the first, the last, the 3
/// conditional registrations and the 3 the listing lacks (5.2).
const VALUES: &[(&str, usize)] = &[
    ("art_bast_artefact", 0),
    ("script_stalker", 33),
    ("game_cl_LastStanding", 45),
    ("script_heli", 49),
    ("main_menu", 76),
    ("script_phys", 85),
    ("script_restr", 115),
    ("game_sv_LastStanding", 119),
    ("zone_mincer_s", 174),
    ("zone", 187),
];

/// The `clsid` fields a script reads: `clsid.name` not after `.` or `:`.
fn clsid_reads(tokens: &[Token]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for i in 0..tokens.len().saturating_sub(2) {
        let after_dot = i > 0 && matches!(tokens[i - 1], Token::Punct(b'.' | b':'));
        if !after_dot
            && tokens[i] == Token::Name("clsid".into())
            && tokens[i + 1] == Token::Punct(b'.')
            && let Token::Name(name) = &tokens[i + 2]
        {
            names.insert(name.clone());
        }
    }
    names
}

#[test]
fn shipped_registrations() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let tree = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME, "").unwrap();

    // The table's names: the engine's and the registrations'.
    let table: BTreeSet<&str> = ENGINE_ENTRIES
        .iter()
        .map(|e| e.1)
        .chain(REGISTRATIONS.iter().map(|r| r.1))
        .collect();
    assert_eq!(table.len(), 188, "distinct names");

    // 5.4: the scripts' reads of `clsid` (the listing `lua_help` aside).
    let dir = "gamedata/scripts";
    let mut reads = BTreeMap::new();
    for entry in tree.read_dir(dir).unwrap() {
        let Some(name) = entry.name.strip_suffix(".script") else {
            continue;
        };
        if name == "lua_help" {
            continue;
        }
        let text = read(&tree, &format!("{dir}/{}", entry.name)).unwrap();
        let names = clsid_reads(&tokens(&text));
        if !names.is_empty() {
            let name = if name == "_g" { "_G" } else { name };
            reads.insert(name.to_owned(), names);
        }
    }
    // (Which of them are loadable, docs/behavior/script-loading.md 3, isn't
    // computed here.)
    let all: BTreeSet<&String> = reads.values().flatten().collect();
    assert_eq!(
        (reads.len(), all.len()),
        (26, 106),
        "scripts reading clsid, names read"
    );
    // The names the table lacks, with the scripts reading them.
    let lacking: BTreeMap<&str, Vec<&str>> = all
        .iter()
        .filter(|n| !table.contains(n.as_str()))
        .map(|n| {
            let readers = reads
                .iter()
                .filter(|(_, names)| names.contains(*n))
                .map(|(script, _)| script.as_str())
                .collect();
            (n.as_str(), readers)
        })
        .collect();
    let want: BTreeMap<&str, Vec<&str>> = [
        ("rat", vec!["", "xr_assistance"]),
        ("rat_group", vec!["", "xr_assistance"]),
        ("soldier", vec!["", "xr_assistance", "xr_talker"]),
        ("wpn_fn2000_s", vec!["xr_state"]),
        ("wpn_fort_s", vec!["xr_state"]),
        ("wpn_shotgun_S", vec!["xr_state"]),
    ]
    .into_iter()
    .collect();
    assert_eq!(lacking, want, "names the table lacks, and their readers");
    let unread = table
        .iter()
        .filter(|n| !all.iter().any(|r| r.as_str() == **n))
        .count();
    assert_eq!(unread, 88, "names no script reads");

    // Startup: the engine classes, `_G`, the class registrators (4).
    let common = Common::read(&tree, &aliases).unwrap();
    assert_eq!(
        common.class_registrators,
        ["class_registrator.register", "game_registrator.register"]
    );
    let mut s = Scripts::new(Rc::new(GameTrees { tree, root }), &aliases, Prefix::Default).unwrap();
    let mut engine: BTreeMap<&str, Vec<usize>> = ENGINE_BASES
        .iter()
        .map(|(name, counts)| (*name, counts.to_vec()))
        .collect();
    for (_, _, client, _) in REGISTRATIONS {
        if !client.contains('.') {
            engine.entry(client).or_insert_with(|| vec![0]);
        }
    }
    assert_eq!(
        engine.len(),
        ENGINE_BASES.len() + 48,
        "48 engine client classes"
    );
    for (name, counts) in &engine {
        let mut class = EngineClass::new(name);
        for &count in counts {
            class = class.constructor(
                count,
                |_| Some(0),
                |_, args| {
                    Ok(args
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect::<Vec<String>>())
                },
            );
        }
        s.register_class(class).unwrap();
    }
    // Spec B's `editor()`, always false (its 4.2), stood in for here; so
    // is the engine namespace `game` a script's top level reads
    // (`smart_terrain`), by a value any use of which gives itself.
    s.exec(
        "=stand_ins",
        b"function editor() return false end
          local any = {}
          setmetatable(any, {__index = function() return any end, __call = function() return any end})
          game = any",
    )
    .unwrap();
    s.load_globals().unwrap();
    s.call_class_registrators(&common).unwrap();

    // 3.3: 54 entries, as registered.
    let entries = s.object_factory().entries().to_vec();
    let missing: Vec<&str> = REGISTRATIONS
        .iter()
        .map(|r| r.0)
        .filter(|id| !entries.iter().any(|e| e.id == class_id(id.as_bytes())))
        .collect();
    assert!(
        missing.is_empty(),
        "registrations without an entry: {missing:?}; failures: {:?}",
        s.take_failures()
    );
    assert_eq!(entries.len(), 188, "entries");
    let ids: BTreeSet<u64> = entries.iter().map(|e| e.id).collect();
    let names: BTreeSet<&[u8]> = entries.iter().map(|e| e.name.as_slice()).collect();
    assert_eq!((ids.len(), names.len()), (188, 188), "distinct ids, names");
    let script: Vec<_> = entries
        .iter()
        .filter_map(|e| match e.source {
            Source::Script { form, .. } => Some((e.id_text(), e.name.clone(), form)),
            Source::Engine(_) => None,
        })
        .collect();
    let want: Vec<_> = REGISTRATIONS
        .iter()
        .map(|(id, name, _, server)| {
            let form = if server.is_some() {
                Form::FourArguments
            } else {
                Form::ThreeArguments
            };
            ((*id).to_owned(), name.as_bytes().to_vec(), form)
        })
        .collect();
    assert_eq!(script, want, "the script entries");
    let four = script.iter().filter(|e| e.2 == Form::FourArguments).count();
    assert_eq!((four, script.len() - four), (51, 3), "4- and 3-argument");

    // 5.2, 5.3: `clsid`, every name at its position.
    for (position, entry) in entries.iter().enumerate() {
        let name = String::from_utf8(entry.name.clone()).unwrap();
        let value = s
            .exec("=clsid", format!("return clsid.{name}").as_bytes())
            .unwrap();
        assert_eq!(value, [Value::Number(position as f64)], "clsid.{name}");
    }
    for &(name, value) in VALUES {
        let got = s
            .exec("=clsid", format!("return clsid.{name}").as_bytes())
            .unwrap();
        assert_eq!(got, [Value::Number(value as f64)], "5.3: clsid.{name}");
    }

    // 6.2: the client objects of the 48 engine client classes, made
    // without arguments (TODO(#935): gap 1, the simplest reading), and
    // every server object, made with its section. (The 4 script client
    // classes' `__init` call engine methods of other specs.)
    for (id, _, client, server) in REGISTRATIONS {
        if !client.contains('.') {
            let created = s.create_client_object(class_id(id.as_bytes())).unwrap();
            let CreatedObject::Script(part) = created.object else {
                panic!("{id} ({client}): {created:?}");
            };
            let args = s.with_part(part, |a: &mut Vec<String>| a.clone());
            assert_eq!(args, Some(Vec::new()), "{id} ({client}): no arguments");
            assert!(s.release_object(part).unwrap());
        }
        if server.is_none() {
            continue;
        }
        let section = format!("[test_{id}]\r\nclass = {id}\r\n");
        let system = volya_engine_formats::ltx::parse(section.as_bytes()).unwrap();
        let created = s
            .create_server_object(&system, &format!("test_{id}"))
            .unwrap();
        let CreatedObject::Script(part) = created.object else {
            panic!("{id} ({server:?}): {created:?}");
        };
        let args = s.with_part(part, |a: &mut Vec<String>| a.clone());
        assert_eq!(
            args,
            Some(vec![format!("test_{id}")]),
            "{id} ({server:?}): the section"
        );
    }
}
