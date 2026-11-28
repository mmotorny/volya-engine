// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The phrase classes (section 2 of
//! docs/behavior/script-api-dialogs-and-trade.md): `CPhraseDialog`,
//! `CPhrase` and `CPhraseScript`, registered without base or constructor
//! (1.2). Scripts receive a dialog from its init function (2.1); its
//! objects refer to the dialog the engine shares ([`SharedDialog`]), so
//! what scripts add changes it.

use std::cell::RefCell;
use std::rc::Rc;

use volya_engine_formats::cp1251;
use volya_engine_formats::dialog_files::{Body, ConditionsAndEffects, Dialog, Phrase};

use super::Context;
use crate::Value;
use crate::classes::EngineClass;

/// A dialog as the engine shares it for the rest of the run (2.1): every
/// use of its id, and the phrase objects scripts hold.
pub type SharedDialog = Rc<RefCell<Dialog>>;

/// A `CPhraseDialog`'s part: the dialog itself.
#[derive(Clone, Debug)]
pub struct PhraseDialog(pub SharedDialog);

/// A `CPhrase`'s part: a phrase of a dialog, by index.
#[derive(Clone, Debug)]
pub struct PhrasePart {
    pub dialog: SharedDialog,
    pub index: usize,
}

/// A `CPhraseScript`'s part: the condition-and-effect part of a phrase
/// (2.3), not a copy.
#[derive(Clone, Debug)]
pub struct PhraseScript {
    pub dialog: SharedDialog,
    pub index: usize,
}

type Results = Result<Vec<Value>, String>;

/// A text argument (our wording for a mismatch; depends on A).
fn text(args: &[Value], i: usize, what: &str) -> Result<String, String> {
    match args.get(i) {
        Some(Value::String(s)) => Ok(cp1251::decode(s)),
        _ => Err(format!("{what}: text expected")),
    }
}

/// The phrases of `dialog`, made a phrase list if it was none.
fn phrases(dialog: &mut Dialog) -> &mut Vec<Phrase> {
    if let Body::InitFunc(_) = dialog.body {
        dialog.body = Body::PhraseList(Vec::new());
    }
    match &mut dialog.body {
        Body::PhraseList(p) => p,
        Body::InitFunc(_) => unreachable!("made a phrase list above"),
    }
}

/// `dlg:AddPhrase(text, id, previous_id, goodwill)` (2.2).
fn add_phrase(
    ctx: &Context,
    dialog: &SharedDialog,
    args: &[Value],
) -> Result<Option<usize>, String> {
    let what = "AddPhrase";
    let text_ = text(args, 0, what)?;
    let id = text(args, 1, what)?;
    let previous = text(args, 2, what)?;
    let Some(Value::Number(goodwill)) = args.get(3) else {
        return Err(format!("{what}: number expected"));
    };
    // Believed truncation toward zero (2.2).
    let goodwill = goodwill.trunc() as i32;
    let mut d = dialog.borrow_mut();
    let list = phrases(&mut d);
    let (index, new) = match list.iter().position(|p| p.id == id) {
        Some(i) => (i, false),
        None => {
            list.push(Phrase {
                id,
                text: text_,
                goodwill,
                lists: ConditionsAndEffects::default(),
                links: Vec::new(),
            });
            (list.len() - 1, true)
        }
    };
    if !previous.is_empty() {
        let Some(from) = list.iter().position(|p| p.id == previous) else {
            // Believed an access fault (2.2): a fatal error here (our
            // wording).
            drop(d);
            return Err(ctx.fatal(
                "CPhraseDialog::AddPhrase",
                line!(),
                &format!("the dialog has no phrase {previous}"),
            ));
        };
        list[from].links.push(index);
    }
    Ok(new.then_some(index))
}

/// Which list of the part a `CPhraseScript` member appends to (2.4).
fn list<'a>(lists: &'a mut ConditionsAndEffects, member: &str) -> &'a mut Vec<Option<String>> {
    match member {
        "AddPrecondition" => &mut lists.precondition,
        "AddAction" => &mut lists.action,
        "AddHasInfo" => &mut lists.has_info,
        "AddDontHasInfo" => &mut lists.dont_has_info,
        "AddGiveInfo" => &mut lists.give_info,
        _ => &mut lists.disable_info,
    }
}

/// The `CPhraseScript` members (2.4).
pub const SCRIPT_MEMBERS: [&str; 6] = [
    "AddPrecondition",
    "AddAction",
    "AddHasInfo",
    "AddDontHasInfo",
    "AddGiveInfo",
    "AddDisableInfo",
];

/// The three classes (1.2).
pub(super) fn classes(ctx: &Context) -> Vec<EngineClass> {
    let c = ctx.clone();
    let dialog = EngineClass::new("CPhraseDialog").method_with_objects(
        "AddPhrase",
        move |d: &mut PhraseDialog, o, args| -> Results {
            c.stopped()?;
            Ok(vec![match add_phrase(&c, &d.0, args)? {
                Some(index) => o.new_object(
                    "CPhrase",
                    PhrasePart {
                        dialog: Rc::clone(&d.0),
                        index,
                    },
                ),
                None => Value::Nil,
            }])
        },
    );
    let c = ctx.clone();
    let phrase = EngineClass::new("CPhrase").method_with_objects(
        "GetPhraseScript",
        move |p: &mut PhrasePart, o, _| -> Results {
            c.stopped()?;
            Ok(vec![o.new_object(
                "CPhraseScript",
                PhraseScript {
                    dialog: Rc::clone(&p.dialog),
                    index: p.index,
                },
            )])
        },
    );
    let script =
        SCRIPT_MEMBERS
            .into_iter()
            .fold(EngineClass::new("CPhraseScript"), |class, member| {
                let c = ctx.clone();
                class.method(member, move |s: &mut PhraseScript, args| -> Results {
                    c.stopped()?;
                    // Exactly a Lua string, appended unchecked (2.4).
                    let entry = text(args, 0, member)?;
                    let mut d = s.dialog.borrow_mut();
                    let phrase = phrases(&mut d)
                        .get_mut(s.index)
                        .ok_or_else(|| format!("{member}: the phrase is gone"))?;
                    list(&mut phrase.lists, member).push(Some(entry));
                    Ok(Vec::new())
                })
            });
    vec![dialog, phrase, script]
}
