// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

use super::*;
use crate::xml::{Document, parse};

fn doc(text: &str) -> Document {
    parse(text.as_bytes()).unwrap()
}

/// The dialog of `<dialog attrs>body</dialog>`.
fn dialog(attrs: &str, body: &str) -> io::Result<Dialog> {
    let d = doc(&format!(
        "<game_dialogs><dialog {attrs}>{body}</dialog></game_dialogs>"
    ));
    Dialog::read(d.root().unwrap().child(DIALOG).unwrap(), "dialogs_test")
}

fn check(e: io::Error) -> FailedCheck {
    FailedCheck::of(&e).expect("a failed check").clone()
}

/// Each phrase's id with its links' ids.
fn graph(d: &Dialog) -> Vec<(&str, Vec<&str>)> {
    let p = d.phrases();
    p.iter()
        .map(|x| {
            (
                x.id.as_str(),
                x.links.iter().map(|&l| p[l].id.as_str()).collect(),
            )
        })
        .collect()
}

// Section 1.

#[test]
fn list_items_are_files_under_gameplay() {
    assert_eq!(file_path("dialogs_escape"), "gameplay\\dialogs_escape.xml");
}

// Section 4.

#[test]
fn a_dialogs_own_values_and_their_defaults() {
    let d = dialog(
        "id=\"a\" priority=\"-5\" pda=\"1\"",
        "<caption>cap</caption><has_info>x</has_info><precondition>s.f</precondition>\
         <dont_has_info>y</dont_has_info><has_info>z</has_info>\
         <phrase_list><phrase id=\"0\"><text>t</text></phrase></phrase_list>",
    )
    .unwrap();
    assert_eq!((d.id.as_str(), d.priority), ("a", -5));
    assert_eq!(d.caption.as_deref(), Some("cap"));
    assert_eq!(d.lists.has_info, [Some("x".into()), Some("z".into())]);
    assert_eq!(d.lists.precondition, [Some("s.f".into())]);
    assert_eq!(d.lists.dont_has_info, [Some("y".into())]);
    assert!(d.lists.action.is_empty() && d.lists.give_info.is_empty());
    assert_eq!(d.caption_text(), "cap");

    let d = dialog(
        "id=\"b\" priority=\"10x\"",
        "<init_func>dialogs.f</init_func>",
    )
    .unwrap();
    assert_eq!(d.priority, 10);
    assert_eq!(d.body, Body::InitFunc("dialogs.f".into()));
    assert!(d.phrases().is_empty() && d.lists.is_empty());
    assert_eq!((d.caption.as_deref(), d.caption_text()), (None, ""));

    // Neither: an empty function name. No priority: 0.
    let d = dialog("id=\"c\"", "").unwrap();
    assert_eq!((d.priority, d.body), (0, Body::InitFunc(String::new())));
}

#[test]
fn init_func_is_read_only_without_a_phrase_list() {
    let d = dialog(
        "id=\"a\"",
        "<init_func>x.y</init_func><phrase_list><phrase id=\"0\"/></phrase_list>\
         <phrase_list><phrase id=\"0\"><text>second</text></phrase></phrase_list>",
    )
    .unwrap();
    // The first phrase list is taken.
    assert_eq!(d.phrases().len(), 1);
    assert_eq!(d.phrases()[0].text, "");
}

#[test]
fn an_empty_caption_stands_back_for_phrase_0() {
    let d = dialog(
        "id=\"a\"",
        "<caption/><phrase_list><phrase id=\"1\"><text>one</text></phrase>\
         <phrase id=\"0\"><text>zero</text><next>1</next></phrase></phrase_list>",
    )
    .unwrap();
    assert_eq!(d.caption, None);
    assert_eq!(d.caption_text(), "zero");
}

// Section 5.

#[test]
fn phrases_are_read_from_phrase_0_along_next() {
    let d = dialog(
        "id=\"a\"",
        "<phrase_list>\
           <phrase id=\"3\"><text>three</text><next>0</next></phrase>\
           <phrase id=\"9\"><text>never</text><next>missing</next></phrase>\
           <phrase id=\"0\"><next>1</next><next>2</next></phrase>\
           <phrase id=\"2\"><text>two</text><next>3</next></phrase>\
           <phrase id=\"1\"><text>one</text><goodwill>-5</goodwill><next>3</next></phrase>\
         </phrase_list>",
    )
    .unwrap();
    // Depth first in the order of `next`; a phrase met again gains only
    // the link, also back to phrase 0 (a cycle).
    assert_eq!(
        graph(&d),
        [
            ("0", vec!["1", "2"]),
            ("1", vec!["3"]),
            ("3", vec!["0"]),
            ("2", vec!["3"]),
        ]
    );
    let p = d.phrase("1").unwrap();
    assert_eq!((p.text.as_str(), p.goodwill), ("one", -5));
    assert_eq!(d.phrase("0").unwrap().goodwill, DEFAULT_GOODWILL);
    // Unreachable phrases are never read, broken or not.
    assert!(d.phrase("9").is_none());
}

#[test]
fn of_two_phrases_with_one_id_only_the_first_is_read() {
    let d = dialog(
        "id=\"a\"",
        "<phrase_list><phrase id=\"0\"><text>first</text></phrase>\
         <phrase id=\"0\"><text>second</text></phrase></phrase_list>",
    )
    .unwrap();
    assert_eq!(d.phrases().len(), 1);
    assert_eq!(d.phrases()[0].text, "first");
}

#[test]
fn texts_and_lists_of_a_phrase() {
    let d = dialog(
        "id=\"a\"",
        "<phrase_list><phrase id=\"0\"><text></text><text>later</text>\
           <action>a.one</action><give_info>g</give_info><action>a.two</action>\
           <disable_info/><next>1</next><next>2</next></phrase>\
         <phrase id=\"1\"><text/></phrase><phrase id=\"2\"/></phrase_list>",
    )
    .unwrap();
    let p = d.phrases();
    // The first `text` only; missing and empty are the empty text.
    assert_eq!(
        (p[0].text.as_str(), p[1].text.as_str(), p[2].text.as_str()),
        ("", "", "")
    );
    assert_eq!(
        p[0].lists.action,
        [Some("a.one".into()), Some("a.two".into())]
    );
    assert_eq!(p[0].lists.give_info, [Some("g".into())]);
    // An element without text: an absent value.
    assert_eq!(p[0].lists.disable_info, [None]);
}

#[test]
fn a_next_naming_no_phrase_is_fatal_when_reached() {
    let c = check(
        dialog(
            "id=\"a\"",
            "<phrase_list><phrase id=\"0\"><next>1</next></phrase>\
             <phrase id=\"1\"><next>412</next></phrase></phrase_list>",
        )
        .unwrap_err(),
    );
    assert_eq!((c.description.as_str(), c.argument), ("412", None));
}

#[test]
fn no_phrase_0_is_fatal() {
    let c = check(dialog("id=\"a\"", "<phrase_list><phrase id=\"1\"/></phrase_list>").unwrap_err());
    assert_eq!(c.argument.as_deref(), Some("gameplay\\dialogs_test.xml"));
}

#[test]
fn an_empty_next_finds_a_phrase_without_id() {
    let d = dialog(
        "id=\"a\"",
        "<phrase_list><phrase id=\"0\"><next/></phrase>\
         <phrase><text>no id</text><next>0</next></phrase></phrase_list>",
    )
    .unwrap();
    // The phrase without id is read; it links nowhere (5.2 step 3, as
    // disputed in review).
    assert_eq!(graph(&d), [("0", vec![""]), ("", vec![])]);
    let c = check(
        dialog(
            "id=\"a\"",
            "<phrase_list><phrase id=\"0\"><next/></phrase></phrase_list>",
        )
        .unwrap_err(),
    );
    assert_eq!(c.description, "");
}

// Section 6.

#[test]
fn function_names_split_at_their_dot() {
    let f = FunctionName::split("escape_dialog.run_tutorial_pda").unwrap();
    assert_eq!(
        (f.namespace, f.function),
        ("escape_dialog", "run_tutorial_pda")
    );
    assert!(f.loads_namespace());
    assert!(!FunctionName::split("_G.f").unwrap().loads_namespace());
    assert!(FunctionName::split("nodot").is_none());
    assert!(FunctionName::split("a.b.c").is_none());
}

// Section 7.

#[test]
fn texts_show_through_the_string_table() {
    let strings = StringTable::new([("esc_hello".to_owned(), "Hello.".to_owned())]);
    assert_eq!(shown("esc_hello", &strings), Some("Hello."));
    assert_eq!(
        shown("gar_dolg_bribe_1", &strings),
        Some("gar_dolg_bribe_1")
    );
    assert_eq!(shown("", &strings), None);
}
