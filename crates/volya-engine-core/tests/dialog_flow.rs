// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The dialog flow on the Russian edition's dialogs, against
//! docs/behavior/dialog-flow.md (cited by section): `hello_dialog` (9.1)
//! and the trader's start dialog along both answers and at the talk the
//! end of the tutorial starts (9.2), with the functions answered as
//! docs/behavior/dialog-conditions.md 7.3 states. Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use std::io;
use std::sync::Arc;

use volya_engine_core::config::{self, Config, PathAliases};
use volya_engine_core::dialogs::Dialogs;
use volya_engine_core::dialogs::conditions::{DialogWorld, dialog_test};
use volya_engine_core::dialogs::flow::{DialogFlow, TalkWorld};
use volya_engine_core::info_portions::{Declarations, InfoPortions};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{self, StdVfs};
use volya_engine_formats::dialog_files::Dialog;

const FSGAME: &str = r"$game_data$ =     false| true | $fs_root$      | gamedata\
$game_config$ =   true | false| $game_data$    | config\
";

const PLAYER: u16 = 0;
const TRADER: u16 = 7;
const START: &str = "escape_trader_start_dialog";

type GameVfs = DbVfs<Box<dyn vfs::VfsFile + 'static>>;

/// Answers the first path's functions as 7.3 states (and `hello_dialog`'s
/// with the player character not wounded) from the portions held.
fn precondition(portions: &InfoPortions, function: &str) -> bool {
    let held = |id| portions.holds(PLAYER, id);
    let pair = |a, b| held(a) && !held(b);
    let given = pair("agroprom_military_case", "agroprom_military_case_done")
        || pair("escape_blockpost_case", "escape_blockpost_case_done")
        || pair("tutorial_artefact_start", "tutorial_artefact_done")
        || pair("tutorial_wounded_start", "esc_serious_talk");
    match function {
        "escape_dialog.is_tutorial" => !held("tutorial_end"),
        "escape_dialog.is_not_tutorial" => held("tutorial_end"),
        "escape_dialog.trader_alredy_give_job" => given,
        "escape_dialog.trader_dont_give_job" => !given,
        "escape_dialog.trader_give_extra_job" => {
            pair("escape_blockpost_case", "escape_blockpost_case_done")
        }
        "escape_dialog.trader_need_extra_job" => {
            !held("escape_blockpost_case") && held("esc_return")
        }
        "bar_dialogs.have_darklab_documents" => false,
        "bar_dialogs.hasnt_darklab_documents" => true,
        "dialogs.is_wounded" => false,
        "dialogs.is_not_wounded" => true,
        other => panic!("{other} is not on the first path"),
    }
}

/// The portions alone, for a dialog's own conditions.
struct Conditions<'a>(&'a mut InfoPortions);

impl DialogWorld for Conditions<'_> {
    fn player_holds(&mut self, id: &str) -> bool {
        self.0.holds(PLAYER, id)
    }

    fn give_to_player(&mut self, id: &str) -> io::Result<()> {
        self.0.give(PLAYER, id, 0)
    }

    fn disable_on_player(&mut self, id: &str) -> io::Result<()> {
        self.0.disable(PLAYER, id)
    }

    fn call_precondition(&mut self, f: &str, _: u16, _: u16, _: [&str; 3]) -> io::Result<bool> {
        Ok(precondition(self.0, f))
    }

    fn call_action(&mut self, _: &str, _: u16, _: u16, _: &str, _: &str) -> io::Result<()> {
        Ok(())
    }
}

struct World {
    vfs: GameVfs,
    aliases: PathAliases,
    dialogs: Dialogs,
    portions: InfoPortions,
    draws: u32,
    calls: Vec<String>,
    lines: Vec<(u16, String)>,
}

impl DialogWorld for World {
    fn player_holds(&mut self, id: &str) -> bool {
        self.portions.holds(PLAYER, id)
    }

    fn give_to_player(&mut self, id: &str) -> io::Result<()> {
        self.calls.push(format!("give {id}"));
        self.portions.give(PLAYER, id, 0)
    }

    fn disable_on_player(&mut self, id: &str) -> io::Result<()> {
        self.portions.disable(PLAYER, id)
    }

    fn call_precondition(&mut self, f: &str, _: u16, _: u16, _: [&str; 3]) -> io::Result<bool> {
        Ok(precondition(&self.portions, f))
    }

    fn call_action(
        &mut self,
        function: &str,
        speaker: u16,
        listener: u16,
        _: &str,
        phrase: &str,
    ) -> io::Result<()> {
        (self.calls).push(format!("{function} {speaker} {listener} {phrase}"));
        Ok(())
    }
}

impl TalkWorld for World {
    fn dialog(&mut self, id: &str) -> io::Result<Arc<Dialog>> {
        self.dialogs.load(&self.vfs, &self.aliases, id)
    }

    /// `IP` 6 over the portions, the trader without actor dialogs.
    fn player_dialog_list(&mut self, player: u16, partner: u16) -> io::Result<Vec<String>> {
        let Self {
            vfs,
            aliases,
            dialogs,
            portions,
            ..
        } = self;
        portions.dialog_list(&[], &mut |p, id| {
            let d = dialogs.load(vfs, aliases, id)?;
            dialog_test(&d, &mut Conditions(p), player, partner)
        })
    }

    fn draw(&mut self) -> u32 {
        self.draws += 1;
        12_345
    }

    fn attitude(&mut self, _: u16, _: u16) -> i32 {
        panic!("every goodwill is equal (4)")
    }

    fn add_line(&mut self, speaker: u16, text: &str) {
        self.lines.push((speaker, text.to_owned()));
    }
}

fn world() -> Option<World> {
    let archives = gog_archives::archives()?;
    let root: &'static StdVfs =
        Box::leak(Box::new(StdVfs::new(archives[0].file.parent().unwrap())));
    let vfs = DbVfs::open_game(root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME.as_bytes(), "").unwrap();
    let system: Config =
        config::load_aliased(&vfs, &aliases, "$game_config$", "system.ltx").unwrap();
    let declarations = Arc::new(Declarations::read(&vfs, &aliases, &system).unwrap());
    Some(World {
        vfs,
        aliases,
        dialogs: Dialogs::new(Arc::clone(&declarations)),
        portions: InfoPortions::new(declarations, PLAYER),
        draws: 0,
        calls: Vec::new(),
        lines: Vec::new(),
    })
}

impl World {
    /// The text of phrase `id` of the dialog in progress.
    fn text(&self, flow: &DialogFlow, id: &str) -> String {
        let d = flow.current().unwrap().dialog();
        let p = d.phrases().iter().find(|p| p.id == id).unwrap();
        p.text.clone()
    }

    fn take_lines(&mut self) -> Vec<(u16, String)> {
        std::mem::take(&mut self.lines)
    }
}

fn questions(flow: &DialogFlow) -> Vec<&str> {
    flow.questions().iter().map(|q| q.id.as_str()).collect()
}

/// 9.1: `hello_dialog` from a partner without a start dialog.
#[test]
fn hello_dialog() {
    let Some(mut w) = world() else {
        return;
    };
    let mut flow = DialogFlow::new();
    flow.set_shown(true);
    flow.talk_start(&mut w, PLAYER, TRADER, None).unwrap();
    assert_eq!(flow.current().unwrap().dialog().id, "hello_dialog");
    let line = w.text(&flow, "2");
    assert_eq!(line, "hello_dialog_2");
    assert!(w.take_lines().is_empty());
    flow.update(&mut w).unwrap();
    // 1 said for the player character, 2 by the trader with its line.
    assert_eq!(w.take_lines(), [(TRADER, line)]);
    assert!(flow.current().is_none());
    assert_eq!(w.draws, 2);
    // A New Game's player character holds nothing offering a dialog; the
    // trader here has no actor dialogs.
    assert!(flow.questions().is_empty());
}

/// 9.2: the trader's start dialog, through 32, and through 31 followed by
/// the talk the end of the tutorial starts.
#[test]
fn the_traders_start_dialog() {
    let Some(mut w) = world() else {
        return;
    };
    let mut flow = DialogFlow::new();
    flow.set_shown(true);
    flow.talk_start(&mut w, PLAYER, TRADER, Some(START))
        .unwrap();
    assert_eq!(flow.current().unwrap().dialog().id, START);
    assert!(w.take_lines().is_empty());
    flow.update(&mut w).unwrap();
    // 12 said for the player character (no line), 3 by the trader.
    let three = w.text(&flow, "3");
    assert_eq!(w.take_lines(), [(TRADER, three)]);
    assert_eq!(questions(&flow), ["31", "32"]);
    let path = [("32", "321", "3211"), ("3211", "32111", "321111")];
    for (said, answer, next) in path {
        let lines = [
            (PLAYER, w.text(&flow, said)),
            (TRADER, w.text(&flow, answer)),
        ];
        flow.click(&mut w, said).unwrap();
        assert_eq!(w.take_lines(), lines, "{said}");
        flow.update(&mut w).unwrap();
        assert_eq!(questions(&flow), [next]);
    }
    let last = w.text(&flow, "321111");
    w.calls.clear();
    flow.click(&mut w, "321111").unwrap();
    assert_eq!(w.take_lines(), [(PLAYER, last)]);
    assert_eq!(w.calls, ["dialogs.break_dialog 0 7 321111"]);
    assert!(flow.current().is_none());
    flow.update(&mut w).unwrap();
    assert!(flow.questions().is_empty());
    // 12, 3, 321 and 32111: one draw each.
    assert_eq!(w.draws, 4);

    // A New Game's other answer, 31: the trader still says 311 (no line)
    // after the talk was ended.
    let Some(mut w) = world() else {
        return;
    };
    let mut flow = DialogFlow::new();
    flow.set_shown(true);
    flow.talk_start(&mut w, PLAYER, TRADER, Some(START))
        .unwrap();
    flow.update(&mut w).unwrap();
    let line = w.text(&flow, "31");
    assert!(w.text(&flow, "311").is_empty());
    w.take_lines();
    w.calls.clear();
    flow.click(&mut w, "31").unwrap();
    assert_eq!(w.take_lines(), [(PLAYER, line)]);
    assert_eq!(
        w.calls,
        [
            "give esc_trader_newbie",
            "give esc_trader_tutorial_chosen",
            "dialogs.break_dialog 0 7 31",
            "escape_dialog.run_tutorial_pda 0 7 31",
            "dialogs.break_dialog 7 0 311",
        ]
    );
    assert!(flow.current().is_none());

    // The talk the end of the tutorial starts: 15 said for the player
    // character, then the trader's 321 and the path of 32.
    flow.talk_start(&mut w, PLAYER, TRADER, Some(START))
        .unwrap();
    flow.update(&mut w).unwrap();
    let answer = w.text(&flow, "321");
    assert_eq!(w.take_lines(), [(TRADER, answer)]);
    assert_eq!(questions(&flow), ["3211"]);
}
