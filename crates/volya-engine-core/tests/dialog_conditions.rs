// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The dialog conditions and effects of the Russian edition, against
//! docs/behavior/dialog-conditions.md (cited by section): the lists the
//! listed dialog files carry (1, 2, 7.1) and the worked example's tests
//! (8) with the functions answered as 7.3 states. Skips when
//! `VOLYA_GOG_INSTALLER` is unset. The shipped functions themselves run in
//! `volya-engine-script`'s `gog_dialogs`.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::collections::BTreeSet;
use std::io;
use std::sync::Arc;

use volya_engine_core::config::{self, Config, PathAliases};
use volya_engine_core::dialogs::Dialogs;
use volya_engine_core::dialogs::conditions::{DialogInProgress, DialogWorld};
use volya_engine_core::info_portions::{Declarations, InfoPortions};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{self, StdVfs};
use volya_engine_formats::dialog_files::{ConditionsAndEffects, FIRST_PHRASE};
use volya_engine_formats::xml;

const FSGAME: &str = r"$game_data$ =     false| true | $fs_root$      | gamedata\
$game_config$ =   true | false| $game_data$    | config\
";

const PLAYER: u16 = 0;
const TRADER: u16 = 7;

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

fn names(list: &[Option<String>]) -> impl Iterator<Item = &str> {
    list.iter().map(|v| v.as_deref().unwrap())
}

/// 1, 2 and 7.1: the lists of every dialog and every phrase element of
/// the listed files.
#[test]
fn the_listed_files_conditions_and_effects() {
    let Some(g) = game() else {
        return;
    };
    let declarations = Declarations::read(&g.vfs, &g.aliases, &g.system).unwrap();
    let mut dialog_level = Vec::new();
    let mut phrase_level = Vec::new();
    let mut actions = Vec::new();
    let mut first_phrases = [0; 6];
    let mut first_with_conditions = Vec::new();
    for name in declarations.dialog_files() {
        let path = (g.aliases)
            .path("$game_config$", &format!("gameplay\\{name}.xml"))
            .unwrap();
        let doc = xml::parse(&vfs::read(&g.vfs, &path).unwrap()).unwrap();
        for d in doc.root().unwrap().children_named("dialog") {
            let id = d.attr("id").unwrap();
            let own = ConditionsAndEffects::read(d);
            // 1: no dialog has its own effects.
            assert!(own.action.is_empty(), "{id}");
            assert!(
                own.give_info.is_empty() && own.disable_info.is_empty(),
                "{id}"
            );
            dialog_level.extend(names(&own.precondition).map(str::to_owned));
            let phrases = d.child("phrase_list").into_iter();
            for p in phrases.flat_map(|l| l.children_named("phrase")) {
                let lists = ConditionsAndEffects::read(p);
                // No element without text occurs (`files` 11).
                for list in [
                    &lists.precondition,
                    &lists.action,
                    &lists.has_info,
                    &lists.dont_has_info,
                    &lists.give_info,
                    &lists.disable_info,
                ] {
                    assert!(list.iter().all(Option::is_some), "{id}");
                }
                phrase_level.extend(names(&lists.precondition).map(str::to_owned));
                actions.extend(names(&lists.action).map(str::to_owned));
                if p.attr("id") == Some(FIRST_PHRASE) {
                    let counts = [
                        &lists.has_info,
                        &lists.dont_has_info,
                        &lists.precondition,
                        &lists.give_info,
                        &lists.action,
                        &lists.disable_info,
                    ];
                    for (n, list) in first_phrases.iter_mut().zip(counts) {
                        *n += usize::from(!list.is_empty());
                    }
                    if !lists.precondition.is_empty() {
                        first_with_conditions.push((
                            id.to_owned(),
                            lists.precondition.clone(),
                            name.clone(),
                        ));
                    }
                }
            }
        }
    }
    // 2, 7.1: 51 dialog-level and 83 phrase-level preconditions, 304
    // actions.
    assert_eq!(dialog_level.len(), 51);
    assert_eq!(phrase_level.len(), 83);
    assert_eq!(actions.len(), 304);
    let distinct = |v: &[String]| v.iter().cloned().collect::<BTreeSet<_>>();
    let (by_dialogs, by_phrases) = (distinct(&dialog_level), distinct(&phrase_level));
    assert_eq!(by_dialogs.len(), 21);
    assert_eq!(by_phrases.len(), 52);
    assert_eq!(by_dialogs.intersection(&by_phrases).count(), 8);
    let preconditions: BTreeSet<_> = by_dialogs.union(&by_phrases).cloned().collect();
    let actions = distinct(&actions);
    assert_eq!(preconditions.len(), 65);
    assert_eq!(actions.len(), 78);
    assert_eq!(preconditions.intersection(&actions).count(), 0);
    // 1: phrase 0's lists: none has `has_info`, `dont_has_info` or
    // `disable_info`; 14 have `give_info`, 9 `action`; one a precondition.
    assert_eq!(first_phrases, [0, 0, 1, 14, 9, 0]);
    assert_eq!(
        first_with_conditions,
        [(
            "optional_dialog_test".to_owned(),
            vec![Some("dialogs.dialog_allowed".to_owned())],
            "dialogs_test".to_owned()
        )]
    );
}

/// A world answering the first path's functions as 7.3 states, from the
/// portions the player character holds, and recording the calls.
struct FirstPath<'a> {
    portions: &'a mut InfoPortions,
    calls: Vec<String>,
}

impl FirstPath<'_> {
    fn held(&self, id: &str) -> bool {
        self.portions.holds(PLAYER, id)
    }
}

impl DialogWorld for FirstPath<'_> {
    fn player_holds(&mut self, id: &str) -> bool {
        self.held(id)
    }

    fn give_to_player(&mut self, id: &str) -> io::Result<()> {
        self.calls.push(format!("give {id}"));
        self.portions.give(PLAYER, id, 0)
    }

    fn disable_on_player(&mut self, id: &str) -> io::Result<()> {
        self.calls.push(format!("disable {id}"));
        self.portions.disable(PLAYER, id)
    }

    fn call_precondition(
        &mut self,
        function: &str,
        first: u16,
        second: u16,
        texts: [&str; 3],
    ) -> io::Result<bool> {
        self.calls
            .push(format!("{function} {first} {second} {texts:?}"));
        let pair = |a, b| self.held(a) && !self.held(b);
        let given = pair("agroprom_military_case", "agroprom_military_case_done")
            || pair("escape_blockpost_case", "escape_blockpost_case_done")
            || pair("tutorial_artefact_start", "tutorial_artefact_done")
            || pair("tutorial_wounded_start", "esc_serious_talk");
        Ok(match function {
            "escape_dialog.is_tutorial" => !self.held("tutorial_end"),
            "escape_dialog.is_not_tutorial" => self.held("tutorial_end"),
            "escape_dialog.trader_alredy_give_job" => given,
            "escape_dialog.trader_dont_give_job" => !given,
            "escape_dialog.trader_give_extra_job" => {
                pair("escape_blockpost_case", "escape_blockpost_case_done")
            }
            "escape_dialog.trader_need_extra_job" => {
                !self.held("escape_blockpost_case") && self.held("esc_return")
            }
            // No documents in the first speaker's inventory.
            "bar_dialogs.have_darklab_documents" => false,
            "bar_dialogs.hasnt_darklab_documents" => true,
            other => panic!("{other} is not on the first path"),
        })
    }

    fn call_action(
        &mut self,
        function: &str,
        speaker: u16,
        listener: u16,
        dialog: &str,
        phrase: &str,
    ) -> io::Result<()> {
        self.calls
            .push(format!("{function} {speaker} {listener} {dialog} {phrase}"));
        Ok(())
    }
}

/// The sayable phrases' ids.
fn sayable(talk: &DialogInProgress) -> Vec<&str> {
    let phrases = talk.dialog().phrases();
    (talk.sayable().iter())
        .map(|&i| phrases[i].id.as_str())
        .collect()
}

/// Says the sayable phrase `id`.
fn say(talk: &mut DialogInProgress, id: &str, world: &mut FirstPath) -> bool {
    let at = (talk.dialog().phrases().iter())
        .position(|p| p.id == id)
        .unwrap();
    talk.say(at, world).unwrap().finished
}

/// 8: the trader's start dialog along the first path and the other answer.
#[test]
fn the_traders_start_dialog() {
    let Some(g) = game() else {
        return;
    };
    let declarations = Arc::new(Declarations::read(&g.vfs, &g.aliases, &g.system).unwrap());
    let mut dialogs = Dialogs::new(Arc::clone(&declarations));
    let dialog = (dialogs.load(&g.vfs, &g.aliases, "escape_trader_start_dialog")).unwrap();
    assert!(dialog.lists.is_empty());
    let mut portions = InfoPortions::new(Arc::clone(&declarations), PLAYER);
    let mut w = FirstPath {
        portions: &mut portions,
        calls: Vec::new(),
    };
    let mut talk = DialogInProgress::start(Arc::clone(&dialog), TRADER, PLAYER);
    assert!(!say(&mut talk, "0", &mut w));
    // Exactly three functions, and only 12 sayable.
    let texts = |next: &str| format!("[\"escape_trader_start_dialog\", \"0\", \"{next}\"]");
    assert_eq!(
        w.calls,
        [
            format!("escape_dialog.is_tutorial 0 7 {}", texts("12")),
            format!("escape_dialog.is_not_tutorial 0 7 {}", texts("17")),
            format!("escape_dialog.trader_give_extra_job 0 7 {}", texts("20")),
        ]
    );
    w.calls.clear();
    assert_eq!(sayable(&talk), ["12"]);
    // The path, the speakers alternating.
    let path = [
        ("12", PLAYER),
        ("3", TRADER),
        ("32", PLAYER),
        ("321", TRADER),
        ("3211", PLAYER),
        ("32111", TRADER),
    ];
    for (id, speaker) in path {
        assert_eq!(talk.turn(), speaker, "{id}");
        assert!(!say(&mut talk, id, &mut w), "{id}");
    }
    assert_eq!(talk.turn(), PLAYER);
    assert_eq!(sayable(&talk), ["321111"]);
    assert!(say(&mut talk, "321111", &mut w));
    assert_eq!(
        w.calls,
        [
            "give esc_trader_experienced",
            "give esc_trader_tutorial_chosen",
            "give esc_trader_can_leave",
            "give tutorial_wounded_start",
            "dialogs.break_dialog 0 7 escape_trader_start_dialog 321111",
        ]
    );
    for id in ["esc_trader_can_leave", "tutorial_wounded_start"] {
        assert!(w.held(id), "{id}");
    }

    // The other answer, 31: two portions, two actions, then the trader's 311.
    let mut portions = InfoPortions::new(Arc::clone(&declarations), PLAYER);
    let mut w = FirstPath {
        portions: &mut portions,
        calls: Vec::new(),
    };
    let mut talk = DialogInProgress::start(Arc::clone(&dialog), TRADER, PLAYER);
    for id in ["0", "12", "3"] {
        say(&mut talk, id, &mut w);
    }
    w.calls.clear();
    assert!(!say(&mut talk, "31", &mut w));
    assert_eq!(sayable(&talk), ["311"]);
    assert!(say(&mut talk, "311", &mut w));
    assert_eq!(
        w.calls,
        [
            "give esc_trader_newbie",
            "give esc_trader_tutorial_chosen",
            "dialogs.break_dialog 0 7 escape_trader_start_dialog 31",
            "escape_dialog.run_tutorial_pda 0 7 escape_trader_start_dialog 31",
            "dialogs.break_dialog 7 0 escape_trader_start_dialog 311",
        ]
    );

    // A later talk: 12 fails at its `dont_has_info`, 15 passes.
    let mut talk = DialogInProgress::start(dialog, TRADER, PLAYER);
    say(&mut talk, "0", &mut w);
    assert_eq!(sayable(&talk), ["15"]);
}
