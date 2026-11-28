// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The dialog files of the Russian edition: every dialog loaded as the
//! program loads it, against the counts docs/formats/dialog-files.md
//! measured (cited by section), and its worked example
//! `escape_trader_start_dialog` (section 10). Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use volya_engine_core::config::{self, Config, PathAliases};
use volya_engine_core::dialogs::Dialogs;
use volya_engine_core::info_portions::Declarations;
use volya_engine_core::text;
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{self, StdVfs};
use volya_engine_formats::dialog_files::{Body, Dialog, FailedCheck, FunctionName, shown};
use volya_engine_formats::xml::{self, Document, Element};

const FSGAME: &str = r"$game_data$ =     false| true | $fs_root$      | gamedata\
$game_config$ =   true | false| $game_data$    | config\
";

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

/// `config\gameplay\<name>.xml`, parsed.
fn gameplay(g: &Game, name: &str) -> Document {
    let path = (g.aliases)
        .path("$game_config$", &format!("gameplay\\{name}.xml"))
        .unwrap();
    xml::parse(&vfs::read(&g.vfs, &path).unwrap()).unwrap()
}

/// Every listed dialog loaded, in registration order.
fn load_all(g: &Game) -> (Dialogs, Vec<Arc<Dialog>>) {
    let declarations = Declarations::read(&g.vfs, &g.aliases, &g.system).unwrap();
    let mut dialogs = Dialogs::new(Arc::new(declarations));
    let ids = dialogs.declarations().dialogs.ids().to_vec();
    let all = (ids.iter())
        .map(|id| dialogs.load(&g.vfs, &g.aliases, id).unwrap())
        .collect();
    (dialogs, all)
}

/// The `phrase` elements of each listed dialog's first `phrase_list`, by
/// dialog id.
fn raw_phrases(g: &Game, files: &[String]) -> BTreeMap<String, Vec<Element>> {
    let mut phrases = BTreeMap::new();
    for name in files {
        let doc = gameplay(g, name);
        for d in doc.root().unwrap().children_named("dialog") {
            let list = d.child("phrase_list").into_iter();
            let elements = list.flat_map(|l| l.children_named("phrase")).cloned();
            phrases.insert(d.attr("id").unwrap().to_owned(), elements.collect());
        }
    }
    phrases
}

fn tally<T: Ord>(items: impl IntoIterator<Item = T>) -> BTreeMap<T, usize> {
    let mut counts = BTreeMap::new();
    for i in items {
        *counts.entry(i).or_default() += 1;
    }
    counts
}

/// Whether a cycle of links is reachable from phrase `0`.
fn has_cycle(d: &Dialog) -> bool {
    fn visit(d: &Dialog, at: usize, state: &mut [u8]) -> bool {
        state[at] = 1;
        for &next in &d.phrases()[at].links {
            if state[next] == 1 || (state[next] == 0 && visit(d, next, state)) {
                return true;
            }
        }
        state[at] = 2;
        false
    }
    let mut state = vec![0; d.phrases().len()];
    !d.phrases().is_empty() && visit(d, 0, &mut state)
}

#[test]
fn the_listed_files_register_the_measured_dialogs() {
    let Some(g) = game() else { return };
    // Section 1: 14 names, in this order.
    let declarations = Declarations::read(&g.vfs, &g.aliases, &g.system).unwrap();
    let files = declarations.dialog_files();
    assert_eq!(
        files,
        [
            "dialogs_test",
            "dialogs",
            "dialogs_escape",
            "dialogs_garbage",
            "dialogs_agroprom",
            "dialogs_deadcity",
            "dialogs_darkvalley",
            "dialogs_pripyat",
            "dialogs_bar",
            "dialogs_yantar",
            "dialogs_military",
            "dialogs_labx18",
            "dialogs_radar",
            "dialogs_aes",
        ]
    );
    // Section 2: every root `game_dialogs`, every child a dialog.
    for name in files {
        let doc = gameplay(&g, name);
        let root = doc.root().unwrap();
        assert_eq!(root.name, "game_dialogs", "{name}");
        assert!(root.elements().all(|e| e.name == "dialog"), "{name}");
    }
    let ids = declarations.dialogs.ids();
    let per_file = tally((0..ids.len()).map(|i| declarations.dialog_place(i).file));
    assert_eq!(
        per_file.into_values().collect::<Vec<_>>(),
        [3, 34, 17, 6, 14, 4, 5, 1, 52, 3, 29, 1, 10, 11]
    );
    // 3.1: 190 ids of lower-case letters, digits and `_`.
    assert_eq!(ids.len(), 190);
    assert!(ids.iter().all(|id| {
        (id.bytes()).all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
    }));
    // 3.2: the dialogs named outside the dialog files.
    assert!(declarations.dialogs.contains("hello_dialog"));
    let global = declarations.portion("global_dialogs").unwrap();
    assert_eq!(global.dialogs, ["actor_break_dialog"]);
    assert!(declarations.dialogs.contains("actor_break_dialog"));
}

#[test]
fn every_dialog_loads_as_measured() {
    let Some(g) = game() else { return };
    let (dialogs, all) = load_all(&g);
    let declarations = dialogs.declarations();

    // Section 4.
    let init: Vec<usize> = (0..all.len())
        .filter(|&i| matches!(all[i].body, Body::InitFunc(_)))
        .collect();
    assert_eq!(init.len(), 31);
    assert!(init.iter().all(|&i| declarations.dialog_place(i).file == 1));
    assert!(all.iter().all(|d| d.caption.is_none()));
    assert_eq!(
        tally(all.iter().map(|d| d.priority)),
        BTreeMap::from([
            (-5, 1),
            (-3, 1),
            (-2, 1),
            (-1, 14),
            (0, 162),
            (1, 2),
            (2, 2),
            (5, 2),
            (6, 1),
            (7, 1),
            (10, 3)
        ])
    );
    // The dialog-level lists' elements.
    let count = |list: fn(&Dialog) -> usize| all.iter().map(|d| list(d)).sum::<usize>();
    assert_eq!(count(|d| d.lists.has_info.len()), 60);
    assert_eq!(count(|d| d.lists.dont_has_info.len()), 57);
    assert_eq!(count(|d| d.lists.precondition.len()), 51);
    assert_eq!(count(|d| d.lists.action.len()), 0);
    assert_eq!(count(|d| d.lists.give_info.len()), 0);
    assert_eq!(count(|d| d.lists.disable_info.len()), 0);

    // Section 5: 2,296 phrases, 213 of them unreachable and never read.
    let raw = raw_phrases(&g, declarations.dialog_files());
    assert_eq!(raw.values().map(Vec::len).sum::<usize>(), 2296);
    let read: usize = all.iter().map(|d| d.phrases().len()).sum();
    assert_eq!(read, 2296 - 213);
    assert_eq!(
        (raw.values())
            .filter(|p| p.first().is_some_and(|p| p.attr("id") == Some("0")))
            .count(),
        30
    );
    for d in all.iter().filter(|d| matches!(d.body, Body::PhraseList(_))) {
        assert_eq!(d.phrases()[0].id, "0", "{}", d.id);
        assert!(d.phrases().iter().all(|p| p.goodwill == -10_000));
    }
    // 5.2: no `next` repeated within a phrase or naming its own phrase.
    let links = |d: &Dialog| -> Vec<(usize, usize)> {
        (d.phrases().iter().enumerate())
            .flat_map(|(i, p)| p.links.iter().map(move |&l| (i, l)))
            .collect()
    };
    for d in &all {
        let l = links(d);
        assert_eq!(l.iter().collect::<HashSet<_>>().len(), l.len(), "{}", d.id);
        assert!(l.iter().all(|(a, b)| a != b), "{}", d.id);
    }
    assert_eq!(all.iter().filter(|d| has_cycle(d)).count(), 10);
    // Named by the `next` elements of more than one phrase of their dialog
    // (reachable or not).
    let shared: usize = (raw.values())
        .map(|phrases| {
            let nexts =
                (phrases.iter()).flat_map(|p| p.children_named("next").map(|n| n.value().unwrap()));
            tally(nexts).values().filter(|&&n| n > 1).count()
        })
        .sum();
    assert_eq!(shared, 70);

    // Section 6: every function name has one dot.
    for d in &all {
        let mut names: Vec<&str> = Vec::new();
        for lists in std::iter::once(&d.lists).chain(d.phrases().iter().map(|p| &p.lists)) {
            for f in lists.precondition.iter().chain(&lists.action) {
                names.push(f.as_deref().unwrap());
            }
        }
        if let Body::InitFunc(f) = &d.body {
            names.push(f);
        }
        for name in names {
            assert!(FunctionName::split(name).is_some(), "{name}");
        }
    }

    // Section 7: texts of all phrases.
    let localization = text::load_localization(&g.vfs, &g.aliases).unwrap();
    let strings = text::load_string_table(&g.vfs, &g.aliases, &localization).unwrap();
    let texts: Vec<Option<&str>> = (raw.values().flatten())
        .map(|p| p.child("text").map(|t| t.value().unwrap_or("")))
        .collect();
    assert_eq!(texts.iter().filter(|t| t.is_none()).count(), 108);
    assert_eq!(texts.iter().filter(|t| *t == &Some("")).count(), 348);
    let literal: Vec<&str> = (texts.iter().flatten().copied())
        .filter(|t| !t.is_empty() && !strings.contains(t))
        .collect();
    assert_eq!(literal.len(), 13);
    assert!(
        literal.iter().all(|t| {
            t.starts_with("gar_dolg_bribe_") || t.starts_with("optional_dialog_test_")
        })
    );
    // Such a text shows itself.
    assert_eq!(shown(literal[0], &strings), Some(literal[0]));
}

#[test]
fn broken_references_fail_only_when_reached() {
    let Some(g) = game() else { return };
    let (mut dialogs, _) = load_all(&g);
    // Section 8: 2111 → 412 cannot be reached.
    let d = dialogs
        .load(&g.vfs, &g.aliases, "val_escort_nap1_dialog")
        .unwrap();
    assert!(d.phrase("2111").is_none());
    // 3.2: dialogs named by characters that no listed file defines.
    for id in [
        "bar_bar_psih_give_medkit",
        "bar_bar_psih_start_pain",
        "bar_bar_psih_talk_gorbun",
        "bar_bar_psih_talk_lisiy",
        "bar_bar_psih_talk_pain",
        "bar_svoboda_volkodav_info_dp",
        "bar_svoboda_volkodav_start",
        "gar_wounded_bandit",
    ] {
        let e = dialogs.load(&g.vfs, &g.aliases, id).unwrap_err();
        let c = FailedCheck::of(&e).unwrap();
        assert_eq!(c.description, "item not found, id");
        assert_eq!(c.argument.as_deref(), Some(id));
        assert_eq!(c.log.len(), 190);
        let last = &dialogs.declarations().dialogs.ids()[189];
        assert_eq!(c.log[189], format!("[189]=[{last}]"));
    }
}

#[test]
fn the_traders_start_dialog() {
    let Some(g) = game() else { return };
    let (mut dialogs, _) = load_all(&g);
    let d = dialogs
        .load(&g.vfs, &g.aliases, "escape_trader_start_dialog")
        .unwrap();
    // Section 10.
    assert_eq!(d.priority, 0);
    assert!(d.lists.is_empty());
    let raw = &raw_phrases(&g, &["dialogs_escape".to_owned()])["escape_trader_start_dialog"];
    assert_eq!(raw.len(), 51);
    let read: HashSet<&str> = d.phrases().iter().map(|p| p.id.as_str()).collect();
    let unread: Vec<&str> = (raw.iter())
        .map(|p| p.attr("id").unwrap())
        .filter(|id| !read.contains(id))
        .collect();
    let mut sorted = unread.clone();
    sorted.sort_by_key(|id| id.parse::<u64>().unwrap());
    assert_eq!(
        sorted,
        [
            "6", "111", "199", "511", "571", "1111", "1999", "5111", "5711", "51111", "57111",
            "521211", "5212111"
        ]
    );
    let ids =
        |p: &[usize]| -> Vec<&str> { p.iter().map(|&i| d.phrases()[i].id.as_str()).collect() };
    assert_eq!(
        ids(&d.phrases()[0].links),
        ["12", "13", "14", "15", "18", "19", "17", "20", "30", "66"]
    );
    assert_eq!(
        d.phrase("31").unwrap().lists.action,
        [
            Some("dialogs.break_dialog".into()),
            Some("escape_dialog.run_tutorial_pda".into())
        ]
    );
    assert_eq!(
        d.phrase("32111").unwrap().lists.give_info,
        [
            Some("esc_trader_can_leave".into()),
            Some("tutorial_wounded_start".into())
        ]
    );
    // The undefined function is in an unread phrase.
    assert!(d.phrases().iter().all(|p| {
        !(p.lists.action.iter().flatten()).any(|f| f == "escape_dialog.give_wounded_flash_reward")
    }));
}
