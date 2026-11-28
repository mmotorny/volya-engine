// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

use std::collections::{HashMap, HashSet};

use volya_engine_formats::dialog_files::FailedCheck;
use volya_engine_formats::xml;

use super::*;
use crate::info_portions::tests::state;

const PLAYER: u16 = 0;
const TRADER: u16 = 7;

/// A world that records what it is asked, in order.
#[derive(Default)]
struct Recorder {
    held: HashSet<String>,
    /// Precondition results by function; a function not listed is true.
    results: HashMap<String, bool>,
    calls: Vec<String>,
}

impl Recorder {
    fn holding(ids: &[&str]) -> Self {
        Self {
            held: ids.iter().map(|&i| i.to_owned()).collect(),
            ..Self::default()
        }
    }

    fn with(mut self, function: &str, result: bool) -> Self {
        self.results.insert(function.to_owned(), result);
        self
    }

    fn take(&mut self) -> Vec<String> {
        std::mem::take(&mut self.calls)
    }
}

impl DialogWorld for Recorder {
    fn player_holds(&mut self, id: &str) -> bool {
        self.calls.push(format!("holds {id}"));
        self.held.contains(id)
    }

    fn give_to_player(&mut self, id: &str) -> io::Result<()> {
        self.calls.push(format!("give {id}"));
        self.held.insert(id.to_owned());
        Ok(())
    }

    fn disable_on_player(&mut self, id: &str) -> io::Result<()> {
        self.calls.push(format!("disable {id}"));
        self.held.remove(id);
        Ok(())
    }

    fn call_precondition(
        &mut self,
        function: &str,
        first: u16,
        second: u16,
        [a, b, c]: [&str; 3],
    ) -> io::Result<bool> {
        self.calls.push(format!(
            "{function}({first}, {second}, {a:?}, {b:?}, {c:?})"
        ));
        if function == "fails.badly" {
            return Err(io::Error::other("LUA error: badly"));
        }
        Ok(self.results.get(function).copied().unwrap_or(true))
    }

    fn call_action(
        &mut self,
        function: &str,
        speaker: u16,
        listener: u16,
        dialog: &str,
        phrase: &str,
    ) -> io::Result<()> {
        self.calls.push(format!(
            "{function}({speaker}, {listener}, {dialog:?}, {phrase:?})"
        ));
        Ok(())
    }
}

fn dialog(text: &str) -> Arc<Dialog> {
    let doc = xml::parse(text.as_bytes()).unwrap();
    Arc::new(Dialog::read(doc.root().unwrap(), "test").unwrap())
}

fn lists(text: &str) -> ConditionsAndEffects {
    let doc = xml::parse(format!("<phrase>{text}</phrase>").as_bytes()).unwrap();
    ConditionsAndEffects::read(doc.root().unwrap())
}

const TEXTS: [&str; 3] = ["d", "1", "2"];

#[test]
fn every_has_info_is_held_before_any_dont_has_info_is_asked() {
    // 3 steps 1, 2: in list order, stopping at the first false step.
    let l = lists(
        "<precondition>s.f</precondition><has_info>a</has_info>\
         <has_info>b</has_info><dont_has_info>c</dont_has_info>",
    );
    let mut w = Recorder::holding(&["b"]);
    assert!(!test(&l, &mut w, PLAYER, TRADER, TEXTS).unwrap());
    assert_eq!(w.take(), ["holds a"]);
    let mut w = Recorder::holding(&["a", "b", "c"]);
    assert!(!test(&l, &mut w, PLAYER, TRADER, TEXTS).unwrap());
    assert_eq!(w.take(), ["holds a", "holds b", "holds c"]);
    let mut w = Recorder::holding(&["a", "b"]);
    assert!(test(&l, &mut w, PLAYER, TRADER, TEXTS).unwrap());
    assert_eq!(
        w.take(),
        [
            "holds a",
            "holds b",
            "holds c",
            r#"s.f(0, 7, "d", "1", "2")"#
        ]
    );
}

#[test]
fn the_first_false_precondition_stops_the_test() {
    // 3 step 3: no later function is called.
    let l = lists(
        "<precondition>s.yes</precondition><precondition>s.no</precondition>\
         <precondition>s.later</precondition>",
    );
    let mut w = Recorder::default().with("s.no", false);
    assert!(!test(&l, &mut w, TRADER, PLAYER, TEXTS).unwrap());
    assert_eq!(
        w.take(),
        [
            r#"s.yes(7, 0, "d", "1", "2")"#,
            r#"s.no(7, 0, "d", "1", "2")"#
        ]
    );
    // No lists: true.
    assert!(test(&lists(""), &mut w, PLAYER, TRADER, TEXTS).unwrap());
    assert!(w.take().is_empty());
}

#[test]
fn an_error_a_precondition_raises_is_fatal() {
    // 3, Errors.
    let l = lists("<precondition>fails.badly</precondition>");
    let e = test(&l, &mut Recorder::default(), PLAYER, TRADER, TEXTS).unwrap_err();
    assert_eq!(e.to_string(), "LUA error: badly");
}

#[test]
fn an_absent_value_is_the_empty_text() {
    // TODO(#1071): our reading.
    let l = lists("<has_info/>");
    let mut w = Recorder::default();
    assert!(!test(&l, &mut w, PLAYER, TRADER, TEXTS).unwrap());
    assert_eq!(w.take(), ["holds "]);
}

#[test]
fn a_dialogs_own_conditions_get_the_dialog_id_and_two_empty_texts() {
    // 2, 6: (character, partner, dialog id, "", "").
    let d = dialog(
        r#"<dialog id="talk"><precondition>s.f</precondition>
           <action>s.never</action><give_info>never</give_info>
           <phrase_list><phrase id="0"/></phrase_list></dialog>"#,
    );
    let mut w = Recorder::default();
    assert!(dialog_test(&d, &mut w, TRADER, PLAYER).unwrap());
    assert_eq!(w.take(), [r#"s.f(7, 0, "talk", "", "")"#]);
    // Its own effects are never run (1), also when it starts and is said.
    let mut talk = DialogInProgress::start(d, PLAYER, TRADER);
    assert!(talk.say(0, &mut w).unwrap().finished);
    assert!(w.take().is_empty());
}

const TALK: &str = r#"<dialog id="talk"><phrase_list>
    <phrase id="0">
        <precondition>s.never_tested</precondition>
        <give_info>a</give_info><give_info>b</give_info>
        <disable_info>c</disable_info>
        <action>s.first</action><action>s.second</action>
        <next>1</next><next>2</next><next>3</next><next>4</next>
    </phrase>
    <phrase id="1"><has_info>a</has_info><next>5</next></phrase>
    <phrase id="2"><dont_has_info>c</dont_has_info><goodwill>5</goodwill></phrase>
    <phrase id="3"><precondition>s.no</precondition></phrase>
    <phrase id="4"><precondition>s.yes</precondition></phrase>
    <phrase id="5"><precondition>s.no</precondition></phrase>
</phrase_list></dialog>"#;

#[test]
fn starting_makes_phrase_0_sayable_untested_and_the_first_speakers_turn() {
    // 4.1.
    let talk = DialogInProgress::start(dialog(TALK), TRADER, PLAYER);
    assert_eq!(talk.sayable(), [0]);
    assert_eq!(talk.turn(), TRADER);
    assert!(talk.said().is_none());
    assert!(!talk.is_finished());
}

#[test]
fn saying_a_phrase_runs_its_effects_then_tests_every_continuation() {
    // 4.2: the trader starts and says phrase 0.
    let mut talk = DialogInProgress::start(dialog(TALK), TRADER, PLAYER);
    let mut w = Recorder::holding(&["c"]).with("s.no", false);
    let told = talk.say(0, &mut w).unwrap();
    assert_eq!(
        told,
        Told {
            listener: PLAYER,
            finished: false
        }
    );
    assert_eq!(talk.said().unwrap().id, "0");
    assert_eq!(talk.turn(), PLAYER);
    assert_eq!(
        w.take(),
        [
            // Step 2: give, disable, then actions with (S, L, dialog, phrase).
            "give a",
            "give b",
            "disable c",
            r#"s.first(7, 0, "talk", "0")"#,
            r#"s.second(7, 0, "talk", "0")"#,
            // Step 4: every continuation, in link order, with (L, S), seeing
            // the effects.
            "holds a",
            "holds c",
            r#"s.no(0, 7, "talk", "0", "3")"#,
            r#"s.yes(0, 7, "talk", "0", "4")"#,
        ]
    );
    // Step 6: greater goodwill first, equal goodwill in link order.
    let ids: Vec<_> = (talk.sayable().iter())
        .map(|&i| talk.dialog().phrases()[i].id.as_str())
        .collect();
    assert_eq!(ids, ["2", "1", "4"]);
}

#[test]
fn a_phrase_without_continuations_finishes_the_dialog() {
    // 4.2 step 3.
    let mut talk = DialogInProgress::start(dialog(TALK), TRADER, PLAYER);
    let mut w = Recorder::default();
    talk.say(0, &mut w).unwrap();
    let two = talk.dialog().phrases().iter().position(|p| p.id == "2");
    let told = talk.say(two.unwrap(), &mut w).unwrap();
    assert_eq!(
        told,
        Told {
            listener: TRADER,
            finished: true
        }
    );
    assert!(talk.is_finished());
    assert!(talk.sayable().is_empty());
    assert_eq!(talk.turn(), TRADER);
}

#[test]
fn no_sayable_continuation_is_fatal() {
    // 4.2 step 5.
    let mut talk = DialogInProgress::start(dialog(TALK), TRADER, PLAYER);
    let mut w = Recorder::default().with("s.no", false);
    talk.say(0, &mut w).unwrap();
    let one = talk.dialog().phrases().iter().position(|p| p.id == "1");
    let e = talk.say(one.unwrap(), &mut w).unwrap_err();
    let check = FailedCheck::of(&e).unwrap();
    assert_eq!(
        check.description,
        "No available phrase to say, dialog[talk]"
    );
    assert_eq!(check.argument, None);
    // A phrase that is not sayable is refused.
    let mut talk = DialogInProgress::start(dialog(TALK), TRADER, PLAYER);
    let e = talk.say(1, &mut w).unwrap_err();
    assert_eq!(e.kind(), io::ErrorKind::InvalidInput);
}

/// Functions that record their calls and the portion events they get.
#[derive(Default)]
struct Functions {
    calls: Vec<String>,
}

impl DialogFunctions for Functions {
    fn call_precondition(&mut self, f: &str, _: u16, _: u16, _: [&str; 3]) -> io::Result<bool> {
        self.calls.push(f.to_owned());
        Ok(true)
    }

    fn call_action(&mut self, f: &str, _: u16, _: u16, _: &str, _: &str) -> io::Result<()> {
        self.calls.push(f.to_owned());
        Ok(())
    }

    fn portion_events(&mut self, events: Vec<Event>) -> io::Result<()> {
        for e in events {
            match e {
                Event::InventoryInfo { portion } => self.calls.push(format!("inventory {portion}")),
                Event::Action { function, .. } => self.calls.push(format!("portion {function}")),
                _ => {}
            }
        }
        Ok(())
    }
}

#[test]
fn the_effects_go_to_the_player_character_each_received_before_the_next() {
    // 4.2 step 2.1: each portion received with its callbacks before the
    // next id, whichever character says the phrase.
    let d = dialog(
        r#"<dialog id="talk"><phrase_list><phrase id="0">
            <give_info>scripted</give_info><give_info>with_task</give_info>
            <disable_info>plain</disable_info><action>s.act</action>
            <next>1</next></phrase>
            <phrase id="1"><has_info>with_task</has_info>
            <dont_has_info>plain</dont_has_info></phrase>
        </phrase_list></dialog>"#,
    );
    let mut portions = state();
    let mut functions = Functions::default();
    let mut w = PortionsAndFunctions {
        portions: &mut portions,
        now: 5,
        functions: &mut functions,
    };
    let mut talk = DialogInProgress::start(d, TRADER, PLAYER);
    talk.say(0, &mut w).unwrap();
    assert_eq!(talk.sayable(), [1]);
    assert!(portions.holds(PLAYER, "scripted"));
    assert!(!portions.holds(TRADER, "scripted"));
    assert!(!portions.holds(PLAYER, "plain"));
    assert_eq!(
        functions.calls,
        [
            "portion mil_tasks.set_cook_mapspot",
            "portion mil_tasks.second",
            "inventory plain",
            "inventory scripted",
            "inventory with_task",
            "s.act",
        ]
    );
    // An undeclared id is fatal when the effect runs.
    let d = dialog(
        r#"<dialog id="t"><phrase_list><phrase id="0">
            <give_info>undeclared</give_info></phrase></phrase_list></dialog>"#,
    );
    let mut w = PortionsAndFunctions {
        portions: &mut portions,
        now: 5,
        functions: &mut functions,
    };
    let e = DialogInProgress::start(d, PLAYER, TRADER)
        .say(0, &mut w)
        .unwrap_err();
    assert_eq!(e.to_string(), "item not found, id undeclared");
}

#[test]
fn a_dialog_list_keeps_the_dialogs_whose_own_conditions_hold() {
    // 2 with `IP` 6: the test asks the portions held at the build.
    let mut portions = state();
    let mut functions = Functions::default();
    let mut build = |portions: &mut InfoPortions| {
        portions.dialog_list(&["trader_info", "trader_jobs"], &mut |p, id| {
            let d = dialog(&format!(
                r#"<dialog id="{id}"><has_info>plain</has_info>
                   <phrase_list><phrase id="0"/></phrase_list></dialog>"#
            ));
            let mut w = PortionsAndFunctions {
                portions: p,
                now: 0,
                functions: &mut functions,
            };
            dialog_test(&d, &mut w, PLAYER, TRADER)
        })
    };
    assert!(build(&mut portions).unwrap().is_empty());
    portions.give(PLAYER, "plain", 1).unwrap();
    assert_eq!(
        build(&mut portions).unwrap(),
        ["trader_jobs", "trader_info"]
    );
}
