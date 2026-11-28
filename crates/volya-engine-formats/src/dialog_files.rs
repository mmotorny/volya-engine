// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The dialog files (docs/formats/dialog-files.md, cited by section): a
//! dialog as the program reads it from its element (section 4, [`Dialog`]),
//! its phrases read from phrase `0` along `next` (section 5, [`Phrase`]),
//! the six condition and effect lists (section 6,
//! [`ConditionsAndEffects`]), script function names (section 6,
//! [`FunctionName`]), how a text is shown (section 7, [`shown`]) and the
//! fatal errors of broken references (section 8, as [`FailedCheck`]s).
//!
//! Which files are read and the dialog id space (sections 1, 3.1) are the
//! start step's (`volya_engine_core::info_portions::Declarations`); loading
//! a dialog by id (3.2) is `volya_engine_core::dialogs`, and what the lists
//! decide and do (DG-2) `volya_engine_core::dialogs::conditions`. How a
//! talk starts is DG-3's, how a dialog proceeds DG-4's.

use std::io;

pub use crate::failed_check::FailedCheck;
use crate::failed_check::failed;
use crate::ltx::value;
use crate::text::string_table::StringTable;
use crate::xml::Element;

/// The root's children that are dialogs (section 2).
pub const DIALOG: &str = "dialog";
/// A phrase's `goodwill` when it has none (5.2).
pub const DEFAULT_GOODWILL: i32 = -10_000;
/// The id of a dialog's first phrase (5.1).
pub const FIRST_PHRASE: &str = "0";
/// The namespace whose functions are looked up without loading a script
/// (section 6).
pub const GLOBAL_NAMESPACE: &str = "_G";

/// List item `item`'s file under `$game_config$` (section 1).
pub fn file_path(item: &str) -> String {
    format!("gameplay\\{item}.xml")
}

/// An element's value (section 2): its text without surrounding white
/// space when its first child is text, otherwise absent.
fn value(e: &Element) -> Option<&str> {
    e.value()
}

/// The value of `parent`'s first child named `name`, or the empty text.
fn first_or_empty<'a>(parent: &'a Element, name: &str) -> &'a str {
    parent.child(name).and_then(value).unwrap_or("")
}

/// The six lists a dialog and a phrase carry (section 6): each the values
/// of all children of that name in document order; a child without text
/// gives an absent value (`None`).
// TODO(#1071): what an absent value does when the list is used is
// unestablished (section 11; DG-2 does not settle it); none occurs in the
// shipped files. `volya_engine_core::dialogs::conditions` takes it as the
// empty text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConditionsAndEffects {
    /// Script function names.
    pub precondition: Vec<Option<String>>,
    /// Script function names.
    pub action: Vec<Option<String>>,
    /// Information-portion ids.
    pub has_info: Vec<Option<String>>,
    pub dont_has_info: Vec<Option<String>>,
    pub give_info: Vec<Option<String>>,
    pub disable_info: Vec<Option<String>>,
}

impl ConditionsAndEffects {
    /// The lists of `e`'s children, in the order the program reads them
    /// (section 6).
    pub fn read(e: &Element) -> Self {
        let list = |name| {
            e.children_named(name)
                .map(|c| value(c).map(str::to_owned))
                .collect()
        };
        Self {
            precondition: list("precondition"),
            action: list("action"),
            has_info: list("has_info"),
            dont_has_info: list("dont_has_info"),
            give_info: list("give_info"),
            disable_info: list("disable_info"),
        }
    }

    /// Whether all six lists are empty.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A phrase of a dialog as read (5.2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Phrase {
    /// Its attribute `id`; a phrase without one has the empty id (5.1).
    pub id: String,
    /// Its first `text` element's value, or the empty text (section 7): a
    /// string id or a literal.
    pub text: String,
    /// Its first `goodwill` element as an integer, or [`DEFAULT_GOODWILL`].
    pub goodwill: i32,
    pub lists: ConditionsAndEffects,
    /// Its continuations, as indices into [`Dialog`]'s phrases, in the
    /// document order of its `next` elements (5.2).
    pub links: Vec<usize>,
}

/// What follows a dialog's own values (section 4): its phrases, or the
/// script function that builds them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Body {
    /// The phrases read from the first `phrase_list` (5.2), in the order
    /// read: phrase `0` first.
    PhraseList(Vec<Phrase>),
    /// No `phrase_list`: the value of `init_func`, empty when there is
    /// none (section 4). The program calls it, once, when the dialog is
    /// loaded, to build the phrases (DG-9).
    InitFunc(String),
}

/// A dialog as read from its element (section 4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dialog {
    pub id: String,
    /// The attribute `priority` as an integer, 0 when absent.
    pub priority: i32,
    /// The value of the first `caption` element.
    pub caption: Option<String>,
    /// The dialog's own conditions and effects: only its conditions are
    /// used (DG-2 1); its effects are never run.
    pub lists: ConditionsAndEffects,
    pub body: Body,
}

impl Dialog {
    /// The dialog of element `e`, the dialog of list item `item`'s file
    /// (sections 4, 5.2): its phrases are read from phrase `0` along
    /// `next`. A `next` naming no phrase of the dialog, and a phrase list
    /// without phrase `0`, are fatal errors (section 8).
    pub fn read(e: &Element, item: &str) -> io::Result<Self> {
        let body = match e.child("phrase_list") {
            Some(list) => Body::PhraseList(read_phrases(list, item)?),
            None => Body::InitFunc(first_or_empty(e, "init_func").to_owned()),
        };
        Ok(Self {
            id: e.attr("id").unwrap_or_default().to_owned(),
            priority: e.attr("priority").map_or(0, |v| value::int(v) as i32),
            caption: e.child("caption").and_then(value).map(str::to_owned),
            lists: ConditionsAndEffects::read(e),
            body,
        })
    }

    /// The phrases read, phrase `0` first; none for an [`Body::InitFunc`].
    pub fn phrases(&self) -> &[Phrase] {
        match &self.body {
            Body::PhraseList(phrases) => phrases,
            Body::InitFunc(_) => &[],
        }
    }

    /// The phrase read with id `id`.
    pub fn phrase(&self, id: &str) -> Option<&Phrase> {
        self.phrases().iter().find(|p| p.id == id)
    }

    /// The text that stands for the dialog where the talk window lists
    /// dialogs to choose from (section 4): its caption, or when that is
    /// absent or empty the text of phrase `0`.
    pub fn caption_text(&self) -> &str {
        match self.caption.as_deref() {
            Some(c) if !c.is_empty() => c,
            _ => self.phrases().first().map_or("", |p| p.text.as_str()),
        }
    }
}

/// The first `phrase` child of `list` whose `id` (empty when it has none)
/// equals `id` exactly (5.1).
fn look_up<'a>(list: &'a Element, id: &str) -> Option<&'a Element> {
    list.children_named("phrase")
        .find(|p| p.attr("id").unwrap_or("") == id)
}

/// The phrases of `list`, read from phrase `0` along `next` (5.2).
fn read_phrases(list: &Element, item: &str) -> io::Result<Vec<Phrase>> {
    let mut phrases: Vec<Phrase> = Vec::new();
    // Steps 1 and 2 for phrase `0`, read with no previous phrase.
    let Some(first) = look_up(list, FIRST_PHRASE) else {
        // The XML reader's lookup from no element (section 8): the spec
        // names its argument, not its description, which is ours.
        return Err(failed("lookup from no element", Some(&file_path(item))));
    };
    // The phrases whose `next` elements are being followed (step 4), each
    // with the elements still to follow, in document order.
    let mut stack = Vec::new();
    if let Some(read) = read_phrase(&mut phrases, first) {
        stack.push((read, first.children_named("next")));
    }
    while let Some((previous, nexts)) = stack.last_mut() {
        let previous = *previous;
        let Some(next) = nexts.next() else {
            stack.pop();
            continue;
        };
        let id = value(next).unwrap_or("");
        let Some(e) = look_up(list, id) else {
            // Section 8: the description is the missing id.
            return Err(failed(id, None));
        };
        let new = read_phrase(&mut phrases, e);
        let at = new.unwrap_or_else(|| {
            (phrases.iter())
                .position(|p| p.id == e.attr("id").unwrap_or(""))
                .expect("a phrase read before")
        });
        // Step 3.
        // TODO(#995): disputed in review (5.2 step 3): the test is on the
        // previous phrase's id, not its existence; the reviewer's reading,
        // no link from a phrase with the empty id, is taken.
        if !phrases[previous].id.is_empty() {
            phrases[previous].links.push(at);
        }
        if let Some(new) = new {
            stack.push((new, e.children_named("next")));
        }
    }
    Ok(phrases)
}

/// Step 2 of 5.2: phrase element `e` becomes a phrase of the dialog unless
/// a phrase with its id was read before; its index when it is new.
fn read_phrase(phrases: &mut Vec<Phrase>, e: &Element) -> Option<usize> {
    let id = e.attr("id").unwrap_or("");
    if phrases.iter().any(|p| p.id == id) {
        return None;
    }
    phrases.push(Phrase {
        id: id.to_owned(),
        text: first_or_empty(e, "text").to_owned(),
        goodwill: e
            .child("goodwill")
            .and_then(value)
            .map_or(DEFAULT_GOODWILL, |v| value::int(v) as i32),
        lists: ConditionsAndEffects::read(e),
        links: Vec::new(),
    });
    Some(phrases.len() - 1)
}

/// A script function name split into its namespace and function
/// (section 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FunctionName<'a> {
    pub namespace: &'a str,
    pub function: &'a str,
}

impl<'a> FunctionName<'a> {
    /// `script.function`, for a name with exactly one dot.
    // TODO(#995): how a name with no dot or more than one is split is a
    // review gap; such a name gives none here.
    pub fn split(name: &'a str) -> Option<Self> {
        let (namespace, function) = name.split_once('.')?;
        (!function.contains('.')).then_some(Self {
            namespace,
            function,
        })
    }

    /// Whether the namespace is loaded before the function is looked up
    /// (section 6: every namespace but [`GLOBAL_NAMESPACE`]).
    pub fn loads_namespace(&self) -> bool {
        self.namespace != GLOBAL_NAMESPACE
    }
}

/// What the talk window shows for text `text` (section 7): nothing for the
/// empty text, otherwise the string table's string for it, or the text
/// itself when it is not a string id.
pub fn shown<'a>(text: &'a str, strings: &'a StringTable) -> Option<&'a str> {
    (!text.is_empty()).then(|| strings.get(text))
}

#[cfg(test)]
mod tests;
