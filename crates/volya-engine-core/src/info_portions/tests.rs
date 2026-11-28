// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

use std::collections::HashMap;
use std::io::Cursor;

use volya_engine_formats::ltx;

use super::*;
use crate::vfs::{DirEntry, VfsFile};

const PLAYER: u16 = 0;
const TRADER: u16 = 7;

const PORTIONS: &str = r#"<?xml version="1.0" encoding="windows-1251"?>
<game_information_portions>
    <info_portion id="plain"/>
    <info_portion id="with_task">
        <task>flash_task</task>
        <article>first_opinions</article>
    </info_portion>
    <info_portion id="ui_pda"><disable>ui_pda_hide</disable></info_portion>
    <info_portion id="ui_pda_hide"><disable>ui_pda</disable></info_portion>
    <info_portion id="scripted">
        <action>mil_tasks.set_cook_mapspot</action>
        <action>mil_tasks.second</action>
        <give_info>plain</give_info>
        <disable_info>other</disable_info>
    </info_portion>
    <info_portion id="other"/>
    <info_portion id="forget"><article_disable>first_opinions</article_disable></info_portion>
    <info_portion id="bad_article"><article>nowhere</article></info_portion>
    <info_portion id="bad_task"><task>nowhere</task></info_portion>
    <info_portion id="bad_disable"><disable>undeclared</disable></info_portion>
    <info_portion id="global_dialogs"><dialog>actor_break_dialog</dialog></info_portion>
    <info_portion id="bad_dialog"><dialog>no_dialog</dialog></info_portion>
    <something_else id="ignored"/>
</game_information_portions>
"#;

const SECOND: &str = r#"<other_root><info_portion id="second"/></other_root>"#;

const ARTICLES: &str = r#"<encyclopedia>
    <article id="first_opinions" article_type="task"/>
    <article id="untyped"/>
</encyclopedia>"#;

const DIALOGS: &str = r#"<game_dialogs>
    <dialog id="actor_break_dialog" priority="-5"/>
    <dialog id="trader_jobs" priority="5"/>
    <dialog id="trader_info"/>
</game_dialogs>"#;

const TASKS: &str = r#"<game_tasks><game_task id="flash_task"/></game_tasks>"#;

fn doc(text: &str) -> Document {
    xml::parse(text.as_bytes()).unwrap()
}

fn files(list: &[(&str, &str)]) -> Vec<(String, Document)> {
    list.iter().map(|(n, t)| (n.to_string(), doc(t))).collect()
}

fn declarations() -> Declarations {
    Declarations::from_documents(
        &files(&[("info_portions", PORTIONS), ("info_second", SECOND)]),
        &files(&[("encyclopedia", ARTICLES)]),
        &files(&[("dialogs", DIALOGS)]),
        &doc(TASKS),
    )
    .unwrap()
}

fn state() -> InfoPortions {
    InfoPortions::new(Arc::new(declarations()), PLAYER)
}

fn ids(list: &[Known]) -> Vec<&str> {
    list.iter().map(|k| k.id.as_str()).collect()
}

/// A file tree in memory, for [`Declarations::read`].
#[derive(Default)]
struct Files(HashMap<String, Vec<u8>>);

struct File(Cursor<Vec<u8>>);

impl io::Read for File {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl io::Seek for File {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.0.seek(pos)
    }
}

impl VfsFile for File {
    fn size(&self) -> u64 {
        self.0.get_ref().len() as u64
    }
}

impl Vfs for Files {
    fn open(&self, path: &str) -> io::Result<Box<dyn VfsFile + '_>> {
        let key = vfs::normalize(path)?;
        match self.0.get(&key) {
            Some(data) => Ok(Box::new(File(Cursor::new(data.clone())))),
            None => Err(io::Error::new(io::ErrorKind::NotFound, key)),
        }
    }

    fn read_dir(&self, _: &str) -> io::Result<Vec<DirEntry>> {
        Err(io::Error::other("not listed"))
    }
}

fn aliases() -> PathAliases {
    PathAliases::parse(
        b"$game_data$ = false| true | $fs_root$ | gamedata\\\r\n\
          $game_config$ = true | false| $game_data$ | config\\\r\n",
        "",
    )
    .unwrap()
}

fn tree(portion_files: &str) -> (Files, Config) {
    let mut files = Files::default();
    let mut add = |name: &str, text: &str| {
        files
            .0
            .insert(format!("gamedata/config/{name}"), text.as_bytes().to_vec());
    };
    add("gameplay/info_portions.xml", PORTIONS);
    add(
        "gameplay/info_second.xml",
        "<?xml version=\"1.0\"?>\n<root>\n#include \"gameplay\\part.xml\"\n</root>",
    );
    add("gameplay/part.xml", r#"<info_portion id="second"/>"#);
    add("gameplay/info_empty.xml", "<?xml version=\"1.0\"?>");
    add("gameplay/encyclopedia.xml", ARTICLES);
    add("gameplay/dialogs.xml", DIALOGS);
    add("gameplay/game_tasks.xml", TASKS);
    let system = ltx::parse(
        format!(
            "[info_portions]\nfiles = {portion_files}\n[encyclopedia]\nfiles = encyclopedia\n\
             [dialogs]\nfiles = dialogs\n"
        )
        .as_bytes(),
    )
    .unwrap();
    (files, system)
}

#[test]
fn the_listed_files_are_read_in_order_with_their_includes() {
    // 1.1: each name trimmed, `.xml` appended, `#include` lines expanded; a
    // file with no element declares nothing.
    let (files, system) = tree("info_portions , info_second,info_empty");
    let d = Declarations::read(&files, &aliases(), &system).unwrap();
    assert_eq!(d.portions.len(), 13);
    assert_eq!(d.portions.ids()[0], "plain");
    assert_eq!(d.portions.ids()[12], "second");
    assert_eq!(d.articles.len(), 2);
    assert_eq!(d.dialogs.len(), 3);
    assert!(d.has_task("flash_task") && !d.has_task("nowhere"));
}

#[test]
fn a_file_that_cannot_be_opened_stops_the_game() {
    let (files, system) = tree("info_portions, info_missing");
    let e = Declarations::read(&files, &aliases(), &system).unwrap_err();
    assert_eq!(
        e.to_string(),
        "error while parsing XML file info_missing.xml"
    );
}

#[test]
fn the_root_name_is_not_checked_and_other_children_are_ignored() {
    // 1.1.
    let d = declarations();
    assert!(d.portions.contains("second"));
    assert!(!d.portions.contains("ignored"));
}

#[test]
fn a_portion_without_an_id_or_declared_twice_stops_the_game() {
    // 1.2.
    let none = Declarations::from_documents(
        &files(&[(
            "info_x",
            r#"<r><info_portion id="a"/><x/><info_portion/></r>"#,
        )]),
        &[],
        &[],
        &Document::default(),
    );
    assert_eq!(
        none.unwrap_err().to_string(),
        "id for item don't set, number 1 in info_x"
    );
    let twice = Declarations::from_documents(
        &files(&[
            ("info_a", r#"<r><info_portion id="a"/></r>"#),
            ("info_b", r#"<r><info_portion id="a"/></r>"#),
        ]),
        &[],
        &[],
        &Document::default(),
    );
    assert_eq!(twice.unwrap_err().to_string(), "duplicate item id a");
}

#[test]
fn looking_up_an_undeclared_id_logs_the_index_then_stops_the_game() {
    // 1.2.
    let d = declarations();
    let mut log = Vec::new();
    assert_eq!(d.portions.look_up("with_task", &mut log).unwrap(), 1);
    assert!(log.is_empty());
    let e = d.portions.look_up("missing", &mut log).unwrap_err();
    assert_eq!(e.to_string(), "item not found, id missing");
    assert_eq!(log.len(), 13);
    assert_eq!(log[0], "[0]=[plain]");
    assert_eq!(log[12], "[12]=[second]");
}

#[test]
fn the_children_are_read_in_file_order() {
    // 1.3.
    let d = declarations();
    let scripted = d.portion("scripted").unwrap();
    assert_eq!(
        scripted.actions,
        ["mil_tasks.set_cook_mapspot", "mil_tasks.second"]
    );
    assert_eq!(scripted.give_info, ["plain"]);
    assert_eq!(scripted.disable_info, ["other"]);
    assert!(d.portion("plain").unwrap().is_plain());
    assert_eq!(
        d.portion("global_dialogs").unwrap().dialogs,
        ["actor_break_dialog"]
    );
    assert_eq!(
        d.article("first_opinions").unwrap().article_type.as_deref(),
        Some("task")
    );
}

#[test]
fn the_list_keeps_ids_and_times_in_the_order_given() {
    // 2: holds, an undeclared id does not hold, an object without a list
    // holds nothing.
    let mut s = state();
    s.give(TRADER, "plain", 100).unwrap();
    s.give(TRADER, "other", 200).unwrap();
    assert_eq!(ids(s.list(TRADER)), ["plain", "other"]);
    assert_eq!(s.received_time(TRADER, "other"), Some(200));
    assert_eq!(s.received_time(TRADER, "second"), None);
    assert!(s.holds(TRADER, "plain"));
    assert!(!s.holds(TRADER, "undeclared"));
    assert!(!s.holds(99, "plain"));
    // Disabling keeps the others' order (believed).
    s.give(TRADER, "second", 300).unwrap();
    s.disable(TRADER, "other").unwrap();
    assert_eq!(ids(s.list(TRADER)), ["plain", "second"]);
    let registry: Vec<_> = s.registry().keys().copied().collect();
    assert_eq!(registry, [TRADER]);
}

#[test]
fn restoring_a_registry_causes_no_effect() {
    // 2: loading restores only the entries.
    let mut s = state();
    let entry = Known {
        id: "with_task".into(),
        received: 5,
    };
    s.restore(BTreeMap::from([(PLAYER, vec![entry.clone()])]));
    assert_eq!(s.list(PLAYER), [entry]);
    assert!(s.tasks().is_empty() && s.articles().is_empty());
    assert!(s.take_events().is_empty());
}

#[test]
fn a_transfer_sends_a_message_that_gives_again_when_it_returns() {
    // 3.1: the message, the lookup, receiving at once.
    let mut s = state();
    s.give(TRADER, "plain", 10).unwrap();
    assert!(s.holds(TRADER, "plain"));
    assert_eq!(
        s.sent(),
        [Message {
            holder: TRADER,
            id: "plain".into(),
            change: Change::Give
        }]
    );
    // Normally the returning message does nothing.
    s.deliver(20).unwrap();
    assert_eq!(s.received_time(TRADER, "plain"), Some(10));
    assert!(s.sent().is_empty());
    // Disabled before it returned, the give gives again, and the returning
    // disable removes it again (believed).
    s.give(TRADER, "other", 30).unwrap();
    s.disable(TRADER, "other").unwrap();
    assert!(!s.holds(TRADER, "other"));
    s.deliver(40).unwrap();
    assert!(!s.holds(TRADER, "other"));
}

#[test]
fn giving_an_undeclared_portion_stops_the_game_after_its_message() {
    // 3.1 step 2, 8.
    let mut s = state();
    let e = s.give(PLAYER, "undeclared", 0).unwrap_err();
    assert_eq!(e.to_string(), "item not found, id undeclared");
    assert_eq!(s.sent().len(), 1);
    assert!(!s.holds(PLAYER, "undeclared"));
    assert_eq!(s.take_log().len(), 13);
}

#[test]
fn receiving_a_held_portion_does_nothing() {
    // 3.2 step 1.
    let mut s = state();
    s.give(PLAYER, "with_task", 1).unwrap();
    s.take_events();
    s.give(PLAYER, "with_task", 2).unwrap();
    assert_eq!(s.received_time(PLAYER, "with_task"), Some(1));
    assert!(s.take_events().is_empty());
    assert_eq!(s.tasks().len(), 1);
}

#[test]
fn a_disable_child_disables_on_the_same_holder() {
    // 3.2 step 5, 4.2: the pairs are a two-state switch.
    let mut s = state();
    s.give(TRADER, "ui_pda", 1).unwrap();
    assert_eq!(ids(s.list(TRADER)), ["ui_pda"]);
    s.give(TRADER, "ui_pda_hide", 2).unwrap();
    assert_eq!(ids(s.list(TRADER)), ["ui_pda_hide"]);
    s.give(TRADER, "ui_pda", 3).unwrap();
    assert_eq!(ids(s.list(TRADER)), ["ui_pda"]);
    // Each disable is a transfer with a message of its own.
    let changes: Vec<_> = s.sent().iter().map(|m| (m.id.as_str(), m.change)).collect();
    assert_eq!(
        changes,
        [
            ("ui_pda", Change::Give),
            ("ui_pda_hide", Change::Disable),
            ("ui_pda_hide", Change::Give),
            ("ui_pda", Change::Disable),
            ("ui_pda", Change::Give),
            ("ui_pda_hide", Change::Disable),
        ]
    );
}

#[test]
fn a_disable_child_naming_an_undeclared_portion_stops_the_game() {
    // 3.2 step 5, 8.
    let mut s = state();
    let e = s.give(TRADER, "bad_disable", 0).unwrap_err();
    assert_eq!(e.to_string(), "item not found, id undeclared");
}

#[test]
fn the_script_part_calls_the_actions_and_gives_to_the_player_character() {
    // 3.3: whichever holder received it, give_info and disable_info act on
    // the player character; other holders get nothing of 3.4.
    let mut s = state();
    s.give(PLAYER, "other", 1).unwrap();
    s.take_events();
    s.give(TRADER, "scripted", 2).unwrap();
    assert_eq!(ids(s.list(TRADER)), ["scripted"]);
    assert_eq!(ids(s.list(PLAYER)), ["plain"]);
    assert_eq!(
        s.take_events(),
        [
            Event::Action {
                holder: TRADER,
                function: "mil_tasks.set_cook_mapspot".into()
            },
            Event::Action {
                holder: TRADER,
                function: "mil_tasks.second".into()
            },
            // The player character received `plain` (3.4), then `other` was
            // disabled on it (4.2).
            Event::InventoryInfo {
                portion: "plain".into()
            },
            Event::DialogsChanged,
            Event::DialogsChanged,
        ]
    );
}

#[test]
fn the_player_character_receives_articles_tasks_and_the_callback() {
    // 3.4 and the first path (9): tutorial_wounded_start's task and article.
    let mut s = state();
    s.give(PLAYER, "with_task", 1000).unwrap();
    assert_eq!(
        s.articles(),
        [KnownArticle {
            received: 1000,
            id: "first_opinions".into(),
            read: false,
            article_type: Some("task".into()),
        }]
    );
    assert_eq!(
        s.tasks(),
        [GivenTask {
            id: "flash_task".into(),
            received: 1000
        }]
    );
    assert_eq!(
        s.take_events(),
        [
            Event::ArticleInfo {
                article: "first_opinions".into()
            },
            Event::TaskState {
                task: "flash_task".into()
            },
            Event::InventoryInfo {
                portion: "with_task".into()
            },
            Event::DialogsChanged,
        ]
    );
    // An article_disable removes it; a known article is not added again.
    s.give(PLAYER, "forget", 2000).unwrap();
    assert!(s.articles().is_empty());
}

#[test]
fn other_holders_get_no_articles_or_tasks() {
    // 3.4, 8: bad children do not stop the game on another holder.
    let mut s = state();
    s.give(TRADER, "with_task", 1).unwrap();
    s.give(TRADER, "bad_article", 1).unwrap();
    s.give(TRADER, "bad_task", 1).unwrap();
    assert!(s.articles().is_empty() && s.tasks().is_empty());
    assert!(s.take_events().is_empty());
}

#[test]
fn an_undeclared_article_or_task_stops_the_game_on_the_player_character() {
    // 3.4, 8.
    let mut s = state();
    let e = s.give(PLAYER, "bad_article", 1).unwrap_err();
    assert_eq!(e.to_string(), "item not found, id nowhere");
    assert_eq!(s.take_log(), ["[0]=[first_opinions]", "[1]=[untyped]"]);
    let e = s.give(PLAYER, "bad_task", 1).unwrap_err();
    assert_eq!(
        e.to_string(),
        "NavigateToNode failed in XML file game_tasks.xml"
    );
}

#[test]
fn disabling_removes_the_entry_and_nothing_else() {
    // 4.2: the articles and tasks stay; on the player character the
    // dialogue window is marked.
    let mut s = state();
    s.give(PLAYER, "with_task", 1).unwrap();
    s.take_events();
    s.disable(PLAYER, "with_task").unwrap();
    assert!(!s.holds(PLAYER, "with_task"));
    assert_eq!((s.articles().len(), s.tasks().len()), (1, 1));
    assert_eq!(s.take_events(), [Event::DialogsChanged]);
    // Not held: nothing removed, no error.
    s.disable(TRADER, "plain").unwrap();
    assert!(s.take_events().is_empty());
}

#[test]
fn disabling_an_undeclared_portion_stops_the_game() {
    // 4.1.
    let mut s = state();
    let e = s.disable(TRADER, "undeclared").unwrap_err();
    assert_eq!(e.to_string(), "item not found, id undeclared");
}

#[test]
fn the_dialog_child_offers_its_dialog_in_every_talk() {
    // 6: held portions' dialogs, then the partner's, each tried once,
    // filtered by their conditions and ordered by priority.
    let mut s = state();
    let mut all = |_: &str| true;
    assert_eq!(
        s.dialog_list(&["trader_info", "trader_jobs"], &mut all)
            .unwrap(),
        ["trader_jobs", "trader_info"]
    );
    s.give(PLAYER, "global_dialogs", 1).unwrap();
    s.give(TRADER, "bad_dialog", 1).unwrap();
    assert_eq!(
        s.dialog_list(&["trader_info", "actor_break_dialog"], &mut all)
            .unwrap(),
        ["trader_info", "actor_break_dialog"]
    );
    let mut tried = Vec::new();
    let mut none = |id: &str| {
        tried.push(id.to_owned());
        id != "trader_info"
    };
    assert_eq!(
        s.dialog_list(&["trader_info", "actor_break_dialog"], &mut none)
            .unwrap(),
        ["actor_break_dialog"]
    );
    assert_eq!(tried, ["actor_break_dialog", "trader_info"]);
    // Disabling withdraws it at the next build.
    s.disable(PLAYER, "global_dialogs").unwrap();
    assert!(s.dialog_list(&[], &mut all).unwrap().is_empty());
}

#[test]
fn a_dialog_child_naming_no_dialog_stops_the_game_when_the_list_is_built() {
    // 6 step 3, 8.
    let mut s = state();
    s.give(PLAYER, "bad_dialog", 1).unwrap();
    let e = s.dialog_list(&[], &mut |_| true).unwrap_err();
    assert_eq!(e.to_string(), "item not found, id no_dialog");
}

#[test]
fn a_document_gives_its_portion_only_when_the_message_returns() {
    // 7.4.
    let mut s = state();
    s.document_taken(PLAYER, "with_task");
    s.document_taken(PLAYER, "");
    assert!(!s.holds(PLAYER, "with_task"));
    assert_eq!(s.sent().len(), 1);
    s.deliver(50).unwrap();
    assert_eq!(s.received_time(PLAYER, "with_task"), Some(50));
    assert_eq!(s.tasks().len(), 1);
    // An undeclared one stops the game when it is applied.
    s.document_taken(TRADER, "undeclared");
    let e = s.deliver(60).unwrap_err();
    assert_eq!(e.to_string(), "item not found, id undeclared");
}
