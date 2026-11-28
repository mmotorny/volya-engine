// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

use volya_engine_formats::dialog_files::{Body, FailedCheck};
use volya_engine_formats::ltx;

use super::*;
use crate::info_portions::tests::{Files, aliases};

const FIRST: &str = r#"<?xml version="1.0" encoding="windows-1251"?>
<game_dialogs>
    <dialog id="hello_dialog" priority="-1">
        <phrase_list>
            <phrase id="1"><text>hello_1</text></phrase>
            <phrase id="0"><text>hello_0</text><next>1</next></phrase>
        </phrase_list>
    </dialog>
    <other id="ignored"/>
    <dialog id="broken">
        <phrase_list><phrase id="0"><next>2</next></phrase></phrase_list>
    </dialog>
</game_dialogs>"#;

const SECOND: &str = r#"<game_dialogs>
    <dialog id="generated"><init_func>dialog_manager.init_intro_dialog</init_func></dialog>
</game_dialogs>"#;

/// A tree with `[dialogs] files = <list>` and empty portions and articles.
fn tree(list: &str) -> (Files, ltx::Config) {
    let mut files = Files::default();
    let mut add = |name: &str, text: &str| {
        files.0.insert(
            format!("gamedata/config/gameplay/{name}"),
            text.as_bytes().to_vec(),
        );
    };
    add("empty.xml", "<root/>");
    add("game_tasks.xml", "<game_tasks/>");
    add("dialogs.xml", FIRST);
    add("dialogs_escape.xml", SECOND);
    add("no_dialogs.xml", "<game_dialogs/>");
    let system = ltx::parse(
        format!(
            "[info_portions]\nfiles = empty\n[encyclopedia]\nfiles = empty\n\
             [dialogs]\nfiles = {list}\n"
        )
        .as_bytes(),
    )
    .unwrap();
    (files, system)
}

fn dialogs(list: &str) -> (Files, io::Result<Dialogs>) {
    let (files, system) = tree(list);
    let d = Declarations::read(&files, &aliases(), &system).map(|d| Dialogs::new(Arc::new(d)));
    (files, d)
}

fn check(e: &io::Error) -> &FailedCheck {
    FailedCheck::of(e).expect("a failed check")
}

#[test]
fn dialogs_load_by_id_from_their_file_once() {
    let (files, d) = dialogs("dialogs, dialogs_escape");
    let mut d = d.unwrap();
    assert_eq!(
        d.declarations().dialog_files(),
        ["dialogs", "dialogs_escape"]
    );
    assert_eq!(
        d.declarations().dialogs.ids(),
        ["hello_dialog", "broken", "generated"]
    );
    let hello = d.load(&files, &aliases(), "hello_dialog").unwrap();
    assert_eq!(hello.priority, -1);
    assert_eq!(hello.caption_text(), "hello_0");
    assert_eq!(hello.phrases()[0].links, [1]);
    // Shared by every later load.
    let again = d.load(&files, &aliases(), "hello_dialog").unwrap();
    assert!(Arc::ptr_eq(&hello, &again));
    let generated = d.load(&files, &aliases(), "generated").unwrap();
    assert_eq!(
        generated.body,
        Body::InitFunc("dialog_manager.init_intro_dialog".into())
    );
}

#[test]
fn a_broken_next_is_fatal_when_the_dialog_is_loaded() {
    let (files, d) = dialogs("dialogs");
    let mut d = d.unwrap();
    let e = d.load(&files, &aliases(), "broken").unwrap_err();
    assert_eq!(check(&e).description, "2");
}

#[test]
fn an_undefined_id_logs_every_dialog_then_is_fatal() {
    let (files, d) = dialogs("dialogs, dialogs_escape");
    let mut d = d.unwrap();
    let e = d.load(&files, &aliases(), "Hello_dialog").unwrap_err();
    let c = check(&e);
    assert_eq!(
        (c.description.as_str(), c.argument.as_deref()),
        ("item not found, id", Some("Hello_dialog"))
    );
    assert_eq!(
        c.log,
        ["[0]=[hello_dialog]", "[1]=[broken]", "[2]=[generated]"]
    );
}

#[test]
fn a_file_listed_twice_is_a_duplicate_only_with_dialogs() {
    let (_, d) = dialogs("dialogs, dialogs");
    assert_eq!(
        d.err().unwrap().to_string(),
        "duplicate item id hello_dialog"
    );
    let (_, d) = dialogs("no_dialogs, dialogs, no_dialogs");
    assert_eq!(d.unwrap().declarations().dialogs.len(), 2);
}

#[test]
fn a_dialog_without_id_and_a_missing_file_are_fatal() {
    let (mut files, system) = tree("dialogs_escape, dialogs");
    files.0.insert(
        "gamedata/config/gameplay/dialogs.xml".into(),
        b"<game_dialogs><dialog id=\"a\"/><dialog/></game_dialogs>".to_vec(),
    );
    let e = Declarations::read(&files, &aliases(), &system).unwrap_err();
    assert_eq!(e.to_string(), "id for item don't set, number 1 in dialogs");
    let (_, d) = dialogs("dialogs, dialogs_missing");
    assert_eq!(
        d.err().unwrap().to_string(),
        "error while parsing XML file dialogs_missing.xml"
    );
}
