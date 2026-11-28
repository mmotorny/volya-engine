// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

use std::collections::{HashMap, HashSet, VecDeque};

use volya_engine_formats::dialog_files::FailedCheck;
use volya_engine_formats::xml;

use super::*;

const PLAYER: u16 = 0;
const TRADER: u16 = 7;

/// A world of synthetic dialogs that records what it is asked.
#[derive(Default)]
struct World {
    dialogs: HashMap<String, Arc<Dialog>>,
    held: HashSet<String>,
    /// Precondition results by function; a function not listed is true.
    results: HashMap<String, bool>,
    /// The player character's dialog list, as `IP` 6 would build it.
    list: Vec<String>,
    builds: usize,
    draws: VecDeque<u32>,
    attitude: i32,
    calls: Vec<String>,
    lines: Vec<(u16, String)>,
}

impl World {
    fn new(dialogs: &[&str]) -> Self {
        let mut w = Self::default();
        for text in dialogs {
            let doc = xml::parse(text.as_bytes()).unwrap();
            let d = Dialog::read(doc.root().unwrap(), "test").unwrap();
            w.dialogs.insert(d.id.clone(), Arc::new(d));
        }
        w
    }

    fn with(mut self, function: &str, result: bool) -> Self {
        self.results.insert(function.to_owned(), result);
        self
    }

    fn draws(mut self, draws: &[u32]) -> Self {
        self.draws = draws.iter().copied().collect();
        self
    }

    fn take_lines(&mut self) -> Vec<(u16, String)> {
        std::mem::take(&mut self.lines)
    }
}

impl DialogWorld for World {
    fn player_holds(&mut self, id: &str) -> bool {
        self.held.contains(id)
    }

    fn give_to_player(&mut self, id: &str) -> io::Result<()> {
        self.held.insert(id.to_owned());
        Ok(())
    }

    fn disable_on_player(&mut self, id: &str) -> io::Result<()> {
        self.held.remove(id);
        Ok(())
    }

    fn call_precondition(
        &mut self,
        function: &str,
        first: u16,
        second: u16,
        _: [&str; 3],
    ) -> io::Result<bool> {
        self.calls.push(format!("{function}({first}, {second})"));
        Ok(self.results.get(function).copied().unwrap_or(true))
    }

    fn call_action(
        &mut self,
        function: &str,
        speaker: u16,
        listener: u16,
        _: &str,
        _: &str,
    ) -> io::Result<()> {
        self.calls
            .push(format!("{function}({speaker}, {listener})"));
        Ok(())
    }
}

impl TalkWorld for World {
    fn dialog(&mut self, id: &str) -> io::Result<Arc<Dialog>> {
        (self.dialogs.get(id).cloned()).ok_or_else(|| failed("item not found, id", Some(id)))
    }

    fn player_dialog_list(&mut self, player: u16, partner: u16) -> io::Result<Vec<String>> {
        assert_eq!((player, partner), (PLAYER, TRADER));
        self.builds += 1;
        Ok(self.list.clone())
    }

    fn draw(&mut self) -> u32 {
        self.draws.pop_front().expect("a draw")
    }

    fn attitude(&mut self, of: u16, towards: u16) -> i32 {
        self.calls.push(format!("attitude({of}, {towards})"));
        self.attitude
    }

    fn add_line(&mut self, speaker: u16, text: &str) {
        self.lines.push((speaker, text.to_owned()));
    }
}

/// 9.1's `hello_dialog`.
const HELLO: &str = r#"<dialog id="hello_dialog"><phrase_list>
    <phrase id="0"><next>1</next></phrase>
    <phrase id="1"><next>2</next><next>3</next></phrase>
    <phrase id="2"><precondition>dialogs.is_not_wounded</precondition><text>hello_dialog_2</text></phrase>
    <phrase id="3"><precondition>dialogs.is_wounded</precondition><text>hello_dialog_3</text></phrase>
</phrase_list></dialog>"#;

const ASK: &str = r#"<dialog id="ask"><caption>ask_caption</caption><phrase_list>
    <phrase id="0"><text>ask_0</text><next>1</next><next>2</next><next>3</next></phrase>
    <phrase id="1"><text>answer_1</text><next>111</next><next>112</next></phrase>
    <phrase id="2"><text>answer_2</text></phrase>
    <phrase id="3"><text>answer_3</text></phrase>
    <phrase id="111"><text>reply_111</text></phrase>
    <phrase id="112"><goodwill>5</goodwill><action>s.done</action></phrase>
</phrase_list></dialog>"#;

const UNCAPTIONED: &str =
    r#"<dialog id="silent"><phrase_list><phrase id="0"/></phrase_list></dialog>"#;

/// A `hello_dialog` that ends with its phrase 0, without a line.
const SILENT_HELLO: &str =
    r#"<dialog id="hello_dialog"><phrase_list><phrase id="0"/></phrase_list></dialog>"#;

fn ids(flow: &DialogFlow) -> Vec<&str> {
    flow.questions().iter().map(|q| q.id.as_str()).collect()
}

fn check(e: &io::Error) -> &FailedCheck {
    FailedCheck::of(e).expect("a failed check")
}

#[test]
fn hello_dialog_is_said_through_to_its_end() {
    // 9.1: the partner says 0, the program says 1 for the player
    // character, the partner says 2 with its line; the refill starts over
    // with the player character's dialog list.
    let mut w = World::new(&[HELLO, ASK, UNCAPTIONED])
        .with("dialogs.is_wounded", false)
        .draws(&[5, 9]);
    w.list = vec!["ask".into(), "silent".into()];
    let mut flow = DialogFlow::new();
    flow.set_shown(true);
    flow.talk_start(&mut w, PLAYER, TRADER, None).unwrap();
    let current = flow.current().unwrap();
    assert_eq!((current.first_speaker(), current.turn()), (TRADER, PLAYER));
    assert!(w.take_lines().is_empty());
    assert!(flow.questions().is_empty());
    assert!(flow.is_refill_marked());
    flow.update(&mut w).unwrap();
    assert_eq!(w.take_lines(), [(TRADER, "hello_dialog_2".to_owned())]);
    // Both continuations tested, the partner first.
    assert_eq!(
        w.calls,
        ["dialogs.is_not_wounded(7, 0)", "dialogs.is_wounded(7, 0)"]
    );
    assert!(flow.current().is_none());
    assert!(w.draws.is_empty());
    // A dialog with an empty caption adds no question.
    assert_eq!(ids(&flow), ["ask"]);
    assert_eq!(flow.questions()[0].text, "ask_caption");
    assert_eq!(flow.dialog_list(), ["ask", "silent"]);
    assert!(!flow.is_refill_marked());
}

#[test]
fn the_partners_list_puts_its_start_dialog_before_hello_dialog() {
    // 2.1 step 1: tried once each, own conditions for (partner, player),
    // ordered by priority, equal priorities in the order tried.
    let start = r#"<dialog id="start"><precondition>s.start</precondition>
        <phrase_list><phrase id="0"><text>start_0</text></phrase></phrase_list></dialog>"#;
    let mut w = World::new(&[HELLO, start]);
    let mut flow = DialogFlow::new();
    flow.talk_start(&mut w, PLAYER, TRADER, Some("start"))
        .unwrap();
    assert_eq!(w.calls, ["s.start(7, 0)"]);
    // Said and finished at once: no current dialog.
    assert_eq!(w.take_lines(), [(TRADER, "start_0".to_owned())]);
    assert!(flow.current().is_none());

    let mut w = World::new(&[HELLO, start]).with("s.start", false);
    flow.talk_start(&mut w, PLAYER, TRADER, Some("start"))
        .unwrap();
    assert_eq!(flow.current().unwrap().dialog().id, "hello_dialog");

    let high = r#"<dialog id="hello_dialog" priority="1"><phrase_list><phrase id="0"/></phrase_list></dialog>"#;
    let mut w = World::new(&[high, start]);
    flow.talk_start(&mut w, PLAYER, TRADER, Some("start"))
        .unwrap();
    assert!(w.take_lines().is_empty());
    // No `hello_dialog`: fatal.
    let mut w = World::new(&[]);
    assert!(flow.talk_start(&mut w, PLAYER, TRADER, None).is_err());
    let unmet = r#"<dialog id="hello_dialog"><precondition>s.no</precondition>
        <phrase_list><phrase id="0"/></phrase_list></dialog>"#;
    let mut w = World::new(&[unmet]).with("s.no", false);
    flow.talk_start(&mut w, PLAYER, TRADER, None).unwrap();
    assert!(flow.current().is_none());
    assert!(flow.is_refill_marked());
}

#[test]
fn a_question_click_starts_a_dialog_and_says_the_players_phrases() {
    // 2.2, 5.3: the player character says phrase 0, the partner answers
    // at once; the choices are listed in goodwill order, those without
    // text left out.
    let mut w = World::new(&[ASK, SILENT_HELLO]).draws(&[4, 0]);
    w.list = vec!["ask".into()];
    let mut flow = DialogFlow::new();
    flow.set_shown(true);
    flow.talk_start(&mut w, PLAYER, TRADER, None).unwrap();
    // Ignored while a refill is pending.
    flow.click(&mut w, "ask").unwrap();
    assert!(w.take_lines().is_empty());
    flow.update(&mut w).unwrap();
    assert_eq!(ids(&flow), ["ask"]);
    flow.click(&mut w, "ask").unwrap();
    assert!(flow.is_refill_marked());
    // 4 mod 3: the second choice, then no links: finished.
    assert_eq!(
        w.take_lines(),
        [
            (PLAYER, "ask_0".to_owned()),
            (TRADER, "answer_2".to_owned())
        ]
    );
    assert!(flow.current().is_none());
    flow.update(&mut w).unwrap();
    // Offered again: the list is rebuilt at every refill (7).
    assert_eq!(ids(&flow), ["ask"]);
    assert_eq!(w.builds, 2);

    flow.click(&mut w, "ask").unwrap();
    assert_eq!(w.take_lines()[1], (TRADER, "answer_1".to_owned()));
    // The player character's turn: 112 (goodwill 5, no text) first, so
    // only 111 is a question.
    assert_eq!(flow.current().unwrap().sayable().len(), 2);
    flow.update(&mut w).unwrap();
    assert_eq!(ids(&flow), ["111"]);
    assert_eq!(flow.questions()[0].text, "reply_111");
    flow.click(&mut w, "111").unwrap();
    assert_eq!(w.take_lines(), [(PLAYER, "reply_111".to_owned())]);
    assert!(flow.current().is_none());
}

#[test]
fn a_click_on_a_dialog_not_in_the_list_is_fatal() {
    // 2.2 step 1: the description is the id.
    let mut w = World::new(&[ASK, UNCAPTIONED, SILENT_HELLO]);
    w.list = vec!["silent".into()];
    let mut flow = DialogFlow::new();
    flow.talk_start(&mut w, PLAYER, TRADER, None).unwrap();
    flow.update(&mut w).unwrap();
    for id in ["ask", ""] {
        let e = flow.click(&mut w, id).unwrap_err();
        assert_eq!(check(&e).description, id);
    }
    // In the list without a question (its caption is empty): started.
    flow.click(&mut w, "silent").unwrap();
}

#[test]
fn the_partner_chooses_by_goodwill_and_its_value() {
    // 4: with goodwills that differ, one of all choices while a is less
    // than the lowest; otherwise the division by zero.
    let player_first = r#"<dialog id="ask"><caption>c</caption><phrase_list>
        <phrase id="0"><next>1</next><next>2</next></phrase>
        <phrase id="1"><goodwill>10</goodwill><text>one</text></phrase>
        <phrase id="2"><goodwill>-3</goodwill><text>two</text></phrase>
    </phrase_list></dialog>"#;
    let mut flow = DialogFlow::new();
    let mut w = World::new(&[SILENT_HELLO, player_first]).draws(&[3]);
    w.list = vec!["ask".into()];
    w.attitude = -4;
    flow.talk_start(&mut w, PLAYER, TRADER, None).unwrap();
    flow.update(&mut w).unwrap();
    flow.click(&mut w, "ask").unwrap();
    assert_eq!(w.calls, ["attitude(7, 0)"]);
    assert_eq!(w.take_lines(), [(TRADER, "two".to_owned())]);
    w.attitude = -3;
    flow.update(&mut w).unwrap();
    let e = flow.click(&mut w, "ask").unwrap_err();
    assert_eq!(check(&e).description, DIVISION_BY_ZERO);
}

#[test]
fn choices_all_without_text_are_said_for_the_player_until_some_have_text() {
    // 5.1 step 3, 5.2: one draw each, regardless of goodwill.
    let chain = r#"<dialog id="hello_dialog"><phrase_list>
        <phrase id="0"><next>1</next><next>2</next></phrase>
        <phrase id="1"><goodwill>9</goodwill><next>3</next></phrase>
        <phrase id="2"><action>s.two</action><next>3</next></phrase>
        <phrase id="3"><next>4</next><next>5</next></phrase>
        <phrase id="4"><text>four</text></phrase>
        <phrase id="5"><next>6</next></phrase>
        <phrase id="6"/>
    </phrase_list></dialog>"#;
    let mut w = World::new(&[chain]).draws(&[1, 0, 0]);
    let mut flow = DialogFlow::new();
    flow.set_shown(true);
    flow.talk_start(&mut w, PLAYER, TRADER, None).unwrap();
    flow.update(&mut w).unwrap();
    // 1 mod 2: phrase 2 (after 1 in goodwill order), its action with the
    // player character as speaker; the partner says 3; 4 has text.
    assert_eq!(w.calls, ["s.two(0, 7)"]);
    assert_eq!(ids(&flow), ["4"]);
    assert!(w.take_lines().is_empty());
    assert!(w.draws.len() == 1);
}

#[test]
fn closing_the_talk_screen_drops_the_dialog_in_progress() {
    // 7: a later talk offers it again from its phrase 0.
    let mut w = World::new(&[ASK, SILENT_HELLO]).draws(&[0, 0]);
    w.list = vec!["ask".into()];
    let mut flow = DialogFlow::new();
    flow.talk_start(&mut w, PLAYER, TRADER, None).unwrap();
    flow.update(&mut w).unwrap();
    flow.click(&mut w, "ask").unwrap();
    assert_eq!(flow.current().unwrap().turn(), PLAYER);
    flow.close();
    assert!(flow.current().is_none());
    flow.talk_start(&mut w, PLAYER, TRADER, None).unwrap();
    flow.update(&mut w).unwrap();
    assert_eq!(ids(&flow), ["ask"]);
    w.take_lines();
    flow.click(&mut w, "ask").unwrap();
    assert_eq!(w.take_lines()[0], (PLAYER, "ask_0".to_owned()));
}
