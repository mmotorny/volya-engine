// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Dialog conditions and effects (docs/behavior/dialog-conditions.md, cited
//! by section): testing a dialog's or a phrase's conditions (3, [`test`]),
//! a dialog's own conditions for a character's dialog list (2,
//! [`dialog_test`]), and saying a phrase of a dialog in progress (4,
//! [`DialogInProgress`]): its effects, then its continuations tested into
//! the sayable list.
//!
//! What the conditions and effects reach outside the dialog (the player
//! character's information portions and the script functions) is a
//! [`DialogWorld`]; [`PortionsAndFunctions`] is one over a game's
//! [`InfoPortions`] and the script functions ([`DialogFunctions`]).
//! Characters are named by object id. Which character's list is built when
//! and how a talk starts are DG-3's; which sayable phrase is said, and the
//! partner's answer when it is told (4.2 step 7), DG-4's.
//!
//! A dialog's own effects are never run (1), and phrase 0's conditions are
//! never tested (1, 4.1): neither has a use here.

use std::io;
use std::sync::Arc;

use volya_engine_formats::dialog_files::{ConditionsAndEffects, Dialog, Phrase};
use volya_engine_formats::failed_check::failed;

use crate::info_portions::{Event, InfoPortions};

/// What a test asks and the effects do outside the dialog (3, 4.2 step 2).
pub trait DialogWorld {
    /// Whether the player character holds information portion `id`
    /// (`IP` 2): an undeclared id is never held, without error.
    fn player_holds(&mut self, id: &str) -> bool;

    /// Gives `id` to the player character by a transfer *give* (`IP` 3.1),
    /// its receiving complete (`IP` 3.2 to 3.4, the script callbacks
    /// included) before this returns. An undeclared id is fatal.
    fn give_to_player(&mut self, id: &str) -> io::Result<()>;

    /// Disables `id` on the player character by a transfer *disable* (`IP`
    /// 4.1). An undeclared id is fatal.
    fn disable_on_player(&mut self, id: &str) -> io::Result<()>;

    /// Calls the precondition function `function` (3 step 3) with the
    /// handles of characters `first` and `second` and the three texts: the
    /// runtime's truth value of its result. An error it raises is fatal.
    fn call_precondition(
        &mut self,
        function: &str,
        first: u16,
        second: u16,
        texts: [&str; 3],
    ) -> io::Result<bool>;

    /// Calls the action function `function` (4.2 step 2.3) with the
    /// handles of `speaker` and `listener`, the dialog id and the phrase
    /// id; its result is ignored. An error it raises is fatal.
    fn call_action(
        &mut self,
        function: &str,
        speaker: u16,
        listener: u16,
        dialog: &str,
        phrase: &str,
    ) -> io::Result<()>;
}

/// The script functions the conditions and effects call, and the script
/// part of receiving and disabling a portion (`IP` [`Event`]s).
pub trait DialogFunctions {
    /// As [`DialogWorld::call_precondition`].
    fn call_precondition(
        &mut self,
        function: &str,
        first: u16,
        second: u16,
        texts: [&str; 3],
    ) -> io::Result<bool>;

    /// As [`DialogWorld::call_action`].
    fn call_action(
        &mut self,
        function: &str,
        speaker: u16,
        listener: u16,
        dialog: &str,
        phrase: &str,
    ) -> io::Result<()>;

    /// The events of one transfer, in order (`IP` 3.3, 3.4): run before
    /// the next effect (4.2 step 2.1).
    fn portion_events(&mut self, events: Vec<Event>) -> io::Result<()>;
}

/// A [`DialogWorld`] over a game's information portions at game time
/// `now` and its script functions.
pub struct PortionsAndFunctions<'a> {
    pub portions: &'a mut InfoPortions,
    /// Game time, ms: when a portion given is received.
    pub now: u64,
    pub functions: &'a mut dyn DialogFunctions,
}

impl PortionsAndFunctions<'_> {
    /// The transfer's events handed to the functions.
    fn events(&mut self) -> io::Result<()> {
        let events = self.portions.take_events();
        if events.is_empty() {
            return Ok(());
        }
        self.functions.portion_events(events)
    }
}

impl DialogWorld for PortionsAndFunctions<'_> {
    fn player_holds(&mut self, id: &str) -> bool {
        self.portions.holds(self.portions.player(), id)
    }

    fn give_to_player(&mut self, id: &str) -> io::Result<()> {
        self.portions.give(self.portions.player(), id, self.now)?;
        self.events()
    }

    fn disable_on_player(&mut self, id: &str) -> io::Result<()> {
        self.portions.disable(self.portions.player(), id)?;
        self.events()
    }

    fn call_precondition(
        &mut self,
        function: &str,
        first: u16,
        second: u16,
        texts: [&str; 3],
    ) -> io::Result<bool> {
        self.functions
            .call_precondition(function, first, second, texts)
    }

    fn call_action(
        &mut self,
        function: &str,
        speaker: u16,
        listener: u16,
        dialog: &str,
        phrase: &str,
    ) -> io::Result<()> {
        self.functions
            .call_action(function, speaker, listener, dialog, phrase)
    }
}

/// A list item's value: an absent one (`files` 6) is taken as the empty
/// text, so an absent id is never held and is fatal to give or disable,
/// and an absent function name is never found.
// TODO(#1071): what an absent value does is unestablished (`files` 11);
// none occurs in the shipped files.
fn item(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("")
}

/// Tests `lists` (3) with characters `first` and `second` and the texts
/// handed to the precondition functions: every `has_info` id held by the
/// player character, then no `dont_has_info` id held, then each
/// precondition function true, in list order, stopping at the first false
/// step.
pub fn test(
    lists: &ConditionsAndEffects,
    world: &mut dyn DialogWorld,
    first: u16,
    second: u16,
    texts: [&str; 3],
) -> io::Result<bool> {
    if !lists.has_info.iter().all(|id| world.player_holds(item(id))) {
        return Ok(false);
    }
    if lists
        .dont_has_info
        .iter()
        .any(|id| world.player_holds(item(id)))
    {
        return Ok(false);
    }
    for function in &lists.precondition {
        if !world.call_precondition(item(function), first, second, texts)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Whether `dialog` is added to `character`'s dialog list for a talk with
/// `partner` (2): its own conditions tested with `character` first,
/// `partner` second, and the texts the dialog id and twice the empty text.
/// Skipping a dialog tried before during the same build, and loading it,
/// are the list's (`IP` 6, [`InfoPortions::dialog_list`]).
pub fn dialog_test(
    dialog: &Dialog,
    world: &mut dyn DialogWorld,
    character: u16,
    partner: u16,
) -> io::Result<bool> {
    test(
        &dialog.lists,
        world,
        character,
        partner,
        [&dialog.id, "", ""],
    )
}

/// The description of the failed check when no continuation can be said
/// (4.2 step 5); its argument is none.
pub fn no_available_phrase(dialog: &str) -> String {
    format!("No available phrase to say, dialog[{dialog}]")
}

/// What saying a phrase tells its listener (4.2 step 7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Told {
    /// The listener, whose turn it now is.
    pub listener: u16,
    /// Whether the phrase said has no continuation (4.2 step 3).
    pub finished: bool,
}

/// A dialog in progress between its first and second speaker (4): the
/// phrase just said, whose turn it is and the sayable list.
#[derive(Clone, Debug)]
pub struct DialogInProgress {
    dialog: Arc<Dialog>,
    first_speaker: u16,
    second_speaker: u16,
    /// Whether it is the first speaker's turn.
    first_speakers_turn: bool,
    /// The phrase just said, by index into the dialog's phrases.
    said: Option<usize>,
    /// The sayable phrases, by index, in order (4.2 step 6).
    sayable: Vec<usize>,
    finished: bool,
}

impl DialogInProgress {
    /// `first_speaker` starts `dialog` with `second_speaker` (4.1): no
    /// phrase said yet, phrase 0 alone sayable without a test, and the turn
    /// the first speaker's.
    pub fn start(dialog: Arc<Dialog>, first_speaker: u16, second_speaker: u16) -> Self {
        let sayable = if dialog.phrases().is_empty() {
            Vec::new()
        } else {
            vec![0]
        };
        Self {
            dialog,
            first_speaker,
            second_speaker,
            first_speakers_turn: true,
            said: None,
            sayable,
            finished: false,
        }
    }

    pub fn dialog(&self) -> &Arc<Dialog> {
        &self.dialog
    }

    pub fn first_speaker(&self) -> u16 {
        self.first_speaker
    }

    pub fn second_speaker(&self) -> u16 {
        self.second_speaker
    }

    /// The character whose turn it is: who says the next phrase.
    pub fn turn(&self) -> u16 {
        if self.first_speakers_turn {
            self.first_speaker
        } else {
            self.second_speaker
        }
    }

    /// The phrase just said.
    pub fn said(&self) -> Option<&Phrase> {
        self.said.map(|i| &self.dialog.phrases()[i])
    }

    /// The sayable phrases, by index into the dialog's phrases, in order:
    /// kept from the last phrase said, tested nothing since (4.3).
    pub fn sayable(&self) -> &[usize] {
        &self.sayable
    }

    /// Whether the phrase last said had no continuation (4.2 step 3).
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Says phrase `phrase` (an index into the dialog's phrases, one of
    /// the sayable ones), steps 1 to 6 of 4.2: the turn passes, its effects
    /// run on the player character, then every continuation is tested with
    /// the listener first and the speaker second, those true kept, ordered
    /// by goodwill, greater first. None true is fatal. Telling the listener
    /// (step 7) is the caller's, with what this returns.
    pub fn say(&mut self, phrase: usize, world: &mut dyn DialogWorld) -> io::Result<Told> {
        if !self.sayable.contains(&phrase) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("phrase {phrase} is not sayable"),
            ));
        }
        // Step 1.
        let speaker = self.turn();
        self.said = Some(phrase);
        self.first_speakers_turn = !self.first_speakers_turn;
        let listener = self.turn();
        let dialog = Arc::clone(&self.dialog);
        let said = &dialog.phrases()[phrase];
        // Step 2.
        let lists = &said.lists;
        for id in &lists.give_info {
            world.give_to_player(item(id))?;
        }
        for id in &lists.disable_info {
            world.disable_on_player(item(id))?;
        }
        for function in &lists.action {
            world.call_action(item(function), speaker, listener, &dialog.id, &said.id)?;
        }
        // Step 3.
        self.sayable.clear();
        self.finished = said.links.is_empty();
        if self.finished {
            return Ok(Told {
                listener,
                finished: true,
            });
        }
        // Step 4.
        for &next in &said.links {
            let continuation = &dialog.phrases()[next];
            let texts = [
                dialog.id.as_str(),
                said.id.as_str(),
                continuation.id.as_str(),
            ];
            if test(&continuation.lists, world, listener, speaker, texts)? {
                self.sayable.push(next);
            }
        }
        // Step 5.
        if self.sayable.is_empty() {
            return Err(failed(no_available_phrase(&dialog.id), None));
        }
        // Step 6: a stable order, so equal goodwill keeps link order (more
        // than 32 sayable phrases are unestablished, 9; none occurs).
        let phrases = dialog.phrases();
        self.sayable
            .sort_by_key(|&i| std::cmp::Reverse(phrases[i].goodwill));
        Ok(Told {
            listener,
            finished: false,
        })
    }
}

#[cfg(test)]
mod tests;
