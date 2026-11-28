// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The information portions of the Russian edition, checked against the
//! measured counts of docs/behavior/info-portions.md (cited by section),
//! with the portions of the first level's trader dialogs (9). Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use volya_engine_core::config::{self, Config, PathAliases};
use volya_engine_core::info_portions::{Declarations, Event, InfoPortions};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{self, StdVfs, Vfs};
use volya_engine_formats::xml::{self, Document, Element};

const FSGAME: &str = r"$game_data$ =     false| true | $fs_root$      | gamedata\
$game_config$ =   true | false| $game_data$    | config\
";

const PLAYER: u16 = 0;

struct Game {
    vfs: DbVfs<Box<dyn vfs::VfsFile + 'static>>,
    aliases: PathAliases,
    system: Config,
}

fn game() -> Option<Game> {
    let archives = gog_archives::archives()?;
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let vfs = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME.as_bytes(), "").unwrap();
    let system = config::load_aliased(&vfs, &aliases, "$game_config$", "system.ltx").unwrap();
    Some(Game {
        vfs,
        aliases,
        system,
    })
}

/// `config\gameplay\<name>`'s bytes.
fn gameplay(g: &Game, name: &str) -> Vec<u8> {
    let path = g
        .aliases
        .path("$game_config$", &format!("gameplay\\{name}"))
        .unwrap();
    vfs::read(&g.vfs, &path).unwrap()
}

fn list(g: &Game, section: &str) -> Vec<String> {
    let files = g.system.read(section, "files").unwrap();
    files.split(',').map(|n| n.trim().to_owned()).collect()
}

fn values(e: &Element, name: &str) -> Vec<String> {
    e.children_named(name)
        .map(|c| c.value().unwrap().to_owned())
        .collect()
}

#[test]
fn the_listed_files_declare_the_measured_portions() {
    let Some(g) = game() else { return };
    // 1.1: 32 names, all present, none with an include.
    let names = list(&g, "info_portions");
    assert_eq!(names.len(), 32);
    assert_eq!(names[0], "info_portions");
    assert_eq!(names[31], "info_stories");
    let mut declaration_only = Vec::new();
    let mut empty_root = Vec::new();
    let mut per_file = BTreeMap::new();
    for name in &names {
        let text = gameplay(&g, &format!("{name}.xml"));
        assert!(!text.windows(8).any(|w| w == b"#include"), "{name}");
        let doc = xml::parse(&text).unwrap();
        match doc.root() {
            None => {
                assert_eq!(text.len(), 45, "{name}");
                declaration_only.push(name.as_str());
            }
            Some(root) => {
                assert_eq!(root.name, "game_information_portions", "{name}");
                assert!(root.elements().all(|e| e.name == "info_portion"));
                let n = root.elements().count();
                if n == 0 {
                    empty_root.push(name.as_str());
                }
                per_file.insert(name.as_str(), n);
            }
        }
    }
    declaration_only.sort();
    assert_eq!(
        declaration_only,
        [
            "info_l04agroprom_u",
            "info_l08rostok_bar",
            "info_l09marshes",
            "info_l09rostok_dolg",
            "info_l12yantar_u",
            "info_l13deadcity",
            "info_l14swamp",
            "info_l17monolith_control",
            "info_l21generators",
            "info_l22warlab",
        ]
    );
    assert_eq!(empty_root, ["info_l11yantar"]);
    assert_eq!(per_file.values().filter(|&&n| n > 0).count(), 21);
    assert_eq!(per_file["info_l01escape"], 140);
    assert_eq!(per_file["info_l07rostok"], 241);

    // 1.1: 43 `info_*.xml` files, 11 never read.
    let dir = g.aliases.path("$game_config$", "gameplay").unwrap();
    let all: BTreeSet<String> = (g.vfs.read_dir(&dir).unwrap().into_iter())
        .map(|e| e.name.to_ascii_lowercase())
        .filter(|n| n.starts_with("info_") && n.ends_with(".xml"))
        .collect();
    assert_eq!(all.len(), 43);
    let unread: Vec<&str> = (all.iter())
        .map(|n| n.trim_end_matches(".xml"))
        .filter(|n| !names.iter().any(|l| l == n))
        .collect();
    assert_eq!(
        unread,
        [
            "info_escape",
            "info_l04darkdolina",
            "info_l05rostok",
            "info_l06yantar",
            "info_l07military",
            "info_l12stanciya",
            "info_l13generators",
            "info_l15radar",
            "info_l18pripyat",
            "info_l20sarcofag",
            "info_news",
        ]
    );
}

#[test]
fn the_declarations_have_the_measured_children() {
    let Some(g) = game() else { return };
    let d = Declarations::read(&g.vfs, &g.aliases, &g.system).unwrap();
    // 1.2.
    assert_eq!(d.portions.len(), 1707);
    assert_eq!(
        d.portions.ids().iter().filter(|i| i.contains('-')).count(),
        40
    );
    assert!(d.portions.ids().iter().all(|i| {
        i.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    }));
    // `dialog-files` 2, `pda-scope` 3.
    assert_eq!(d.dialogs.len(), 190);
    assert_eq!(d.articles.len(), 429);

    // 1.3.
    let mut counts = BTreeMap::new();
    let mut plain = 0;
    for (_, p) in d.each_portion() {
        for (name, children) in [
            ("dialog", &p.dialogs),
            ("disable", &p.disables),
            ("action", &p.actions),
            ("give_info", &p.give_info),
            ("disable_info", &p.disable_info),
            ("article", &p.articles),
            ("article_disable", &p.article_disables),
            ("task", &p.tasks),
        ] {
            assert!(children.iter().all(|c| !c.is_empty()));
            *counts.entry(name).or_insert(0) += children.len();
        }
        plain += usize::from(p.is_plain());
    }
    assert_eq!(
        counts,
        BTreeMap::from([
            ("action", 2),
            ("article", 297),
            ("article_disable", 0),
            ("dialog", 1),
            ("disable", 61),
            ("disable_info", 0),
            ("give_info", 0),
            ("task", 79),
        ])
    );
    assert_eq!(plain, 1439);
    assert_eq!(
        d.portion("mil_cook_map_spot").unwrap().actions,
        ["mil_tasks.set_cook_mapspot"]
    );
    assert_eq!(
        d.portion("mil_lukash_map_spot").unwrap().actions,
        ["mil_tasks.set_lukash_mapspot"]
    );
    assert_eq!(
        d.portion("global_dialogs").unwrap().dialogs,
        ["actor_break_dialog"]
    );
    assert!(d.dialogs.contains("actor_break_dialog"));

    // 4.2, 7.3: 29 `ui_*` portions, six pairs disabling each other, no
    // portion disables itself.
    let ui: Vec<&str> = (d.each_portion())
        .map(|(id, _)| id)
        .filter(|id| id.starts_with("ui_"))
        .collect();
    assert_eq!(ui.len(), 29);
    let mut pairs = Vec::new();
    for (id, p) in d.each_portion() {
        assert!(!p.disables.iter().any(|x| x == id), "{id}");
        if let [other] = p.disables.as_slice()
            && d.portion(other).unwrap().disables == [id]
            && id < other.as_str()
        {
            pairs.push(id);
        }
    }
    // TODO(#1005): the spec counts six, all `ui_*`; the data has four more.
    assert_eq!(
        pairs,
        [
            "ui_pda",
            "ui_pda_jobs",
            "ui_inventory",
            "ui_talk",
            "ui_trade",
            "ui_car_body",
            "esc_blockpost_alarm",
            "esc_bridge_pass_off",
            "bar_barman_need_hole",
            "bar_barman_stay_hole",
        ]
    );
    for ui in &pairs[..6] {
        assert_eq!(d.portion(ui).unwrap().disables, [format!("{ui}_hide")]);
    }

    // 8: the undeclared articles and tasks, and the disable and dialog
    // children all declared.
    let mut bad_articles = Vec::new();
    let mut bad_tasks = Vec::new();
    for (id, p) in d.each_portion() {
        for a in p.articles.iter().filter(|a| !d.articles.contains(a)) {
            bad_articles.push((id, a.as_str()));
        }
        for t in p.tasks.iter().filter(|t| !d.has_task(t)) {
            bad_tasks.push((id, t.as_str()));
        }
        assert!(p.disables.iter().all(|x| d.portions.contains(x)));
        assert!(p.dialogs.iter().all(|x| d.dialogs.contains(x)));
    }
    bad_articles.sort();
    assert_eq!(
        bad_articles,
        [
            ("agr_nii_fab_mil_task", "agr_nii_fab_mil_task"),
            (
                "agr_nii_security_shedule_file_have",
                "agr_nii_security_shedule"
            ),
            ("tr_escape_broken_car", "tr_escape_broken_car"),
        ]
    );
    bad_tasks.sort();
    assert_eq!(
        bad_tasks,
        [
            ("bar_kill_professor_start", "bar_kill_professor"),
            ("gar_help_newbie", "gar_help_tunnel_newbie"),
            ("mil_test_task", "mil_test_task"),
            ("rad_find_stalkers_free_start", "rad_find_stalkers"),
            ("rad_find_stalkers_reward_start", "rad_find_stalkers"),
            ("rad_find_treasurer_start", "rad_find_treasurer"),
        ]
    );
}

/// The ids a dialog's phrases (or itself) name in `lists`.
fn named(dialog: &Element, lists: &[&str], out: &mut BTreeSet<String>) {
    for list in lists {
        out.extend(values(dialog, list));
    }
    for phrase in dialog
        .children_named("phrase_list")
        .flat_map(|l| l.children_named("phrase"))
    {
        for list in lists {
            out.extend(values(phrase, list));
        }
    }
}

#[test]
fn the_first_path_uses_the_trader_portions() {
    let Some(g) = game() else { return };
    let d = Arc::new(Declarations::read(&g.vfs, &g.aliases, &g.system).unwrap());
    let doc: Document = xml::parse(&gameplay(&g, "dialogs_escape.xml")).unwrap();
    let dialog = |id: &str| {
        (doc.root().unwrap().children_named("dialog"))
            .find(|e| e.attr("id") == Some(id))
            .unwrap_or_else(|| panic!("{id}"))
            .clone()
    };
    // 9 (`dialogue-scope` 3): the portions the trader's start dialog gives
    // and tests, and those its actor dialogs test.
    let mut gives = BTreeSet::new();
    let mut tests = BTreeSet::new();
    let start = dialog("escape_trader_start_dialog");
    named(&start, &["give_info", "disable_info"], &mut gives);
    named(&start, &["has_info", "dont_has_info"], &mut tests);
    for id in [
        "escape_trader_talk_info",
        "escape_trader_jobs",
        "escape_trader_done_blockpost_box",
    ] {
        named(&dialog(id), &["has_info", "dont_has_info"], &mut tests);
    }
    // Every portion they name is declared (`dialog-files` 10).
    for id in gives.iter().chain(&tests) {
        assert!(d.portions.contains(id), "{id}");
    }
    // TODO(#999): `esc_return` and `escape_blockpost_case_have`, which
    // section 9 omits, taken as the review gap states them.
    let path = BTreeSet::from([
        "esc_return",
        "esc_serious_talk",
        "esc_trader_can_leave",
        "esc_trader_experienced",
        "esc_trader_newbie",
        "esc_trader_tutorial_chosen",
        "escape_blockpost_case_have",
        "tutorial_end",
        "tutorial_wounded_start",
    ]);
    assert!(
        path.iter()
            .all(|id| gives.contains(*id) || tests.contains(*id))
    );
    assert!(gives.contains("esc_trader_can_leave"));
    let text = gameplay(&g, "info_l01escape.xml");
    let file = xml::parse(&text).unwrap();
    let in_escape: BTreeSet<&str> = (file.root().unwrap().children_named("info_portion"))
        .filter_map(|e| e.attr("id"))
        .collect();
    for id in &path {
        assert!(in_escape.contains(id), "{id}");
        let p = d.portion(id).unwrap();
        if *id == "tutorial_wounded_start" {
            assert_eq!(p.tasks, ["esc_flash_task"]);
            assert_eq!(p.articles, ["sl_first_opinions"]);
            assert!(p.disables.is_empty() && p.actions.is_empty() && p.dialogs.is_empty());
        } else {
            assert!(p.is_plain(), "{id}");
        }
    }

    // Given to the player character: the task and the article (3.4), and
    // each portion's callback.
    let mut s = InfoPortions::new(d, PLAYER);
    let time = 63_471_533_400_000;
    s.give(PLAYER, "global_dialogs", time).unwrap();
    for id in [
        "esc_trader_experienced",
        "esc_trader_tutorial_chosen",
        "esc_trader_can_leave",
        "tutorial_wounded_start",
    ] {
        s.give(PLAYER, id, time).unwrap();
    }
    assert!(s.holds(PLAYER, "esc_trader_can_leave"));
    assert_eq!(s.tasks()[0].id, "esc_flash_task");
    assert_eq!(s.articles()[0].id, "sl_first_opinions");
    let callbacks = (s.take_events().into_iter())
        .filter(|e| matches!(e, Event::InventoryInfo { .. }))
        .count();
    assert_eq!(callbacks, 5);
    // 6: `global_dialogs` gives the first talk `actor_break_dialog`.
    let offered = s.dialog_list(&[], &mut |_, _| Ok(true)).unwrap();
    assert_eq!(offered, ["actor_break_dialog"]);
    // 8: a bad task child stops the game on the player character only.
    s.give(7, "gar_help_newbie", time).unwrap();
    let e = s.give(PLAYER, "gar_help_newbie", time).unwrap_err();
    assert_eq!(
        e.to_string(),
        "NavigateToNode failed in XML file game_tasks.xml"
    );
}

#[test]
fn every_pda_window_portion_is_declared() {
    // 7.3: the `ui_*` portions the engine gives are declared.
    let Some(g) = game() else { return };
    let d = Declarations::read(&g.vfs, &g.aliases, &g.system).unwrap();
    for id in [
        "ui_pda",
        "ui_pda_hide",
        "ui_inventory",
        "ui_inventory_hide",
        "ui_talk",
        "ui_talk_hide",
        "ui_trade",
        "ui_trade_hide",
        "ui_car_body",
        "ui_car_body_hide",
        "ui_pda_events",
        "ui_pda_contacts",
        "ui_pda_ranking",
        "ui_pda_actor_info",
        "ui_pda_encyclopedia",
        "ui_pda_map_local",
        "ui_pda_news_hide",
    ] {
        assert!(d.portions.contains(id), "{id}");
    }
}
