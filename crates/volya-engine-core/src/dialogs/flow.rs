// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The dialog flow of a talk (docs/behavior/dialog-flow.md, cited by
//! section): the talk window's current dialog, started by the partner at
//! talk start (2.1) or by the player character from the question list
//! (2.2), each phrase said with its line first and heard by the other
//! speaker (3: a stalker or trader answers at once, 4), the refill mark and
//! the question list it refills (5.1), the empty-text choices the program
//! says on its own (5.2) and a question click (5.3).
//!
//! What the flow reaches outside the dialog is a [`TalkWorld`]: the
//! conditions and effects ([`DialogWorld`]), loading a dialog, the player
//! character's dialog list (`IP` 6, built by its owner from
//! [`crate::info_portions::InfoPortions::dialog_list`]), the shared
//! generator, the partner's value *a* and the answer log. The questions are
//! the flow's ([`DialogFlow::questions`]); the window draws them, scrolled
//! to their beginning after each refill. How a talk starts and ends, which
//! start dialog and actor dialogs a partner has, and which input makes a
//! click are DG-3's and DG-6's.
//!
//! Closing the talk screen drops the current dialog unfinished
//! ([`DialogFlow::close`], 7); ending a side does nothing to it. Not kept:
//! each speaker's record of its dialogs (1), which plays no part in the
//! flow, the talk screen's closing or the end of a side.

use std::io;
use std::sync::Arc;

use volya_engine_formats::dialog_files::{Dialog, FIRST_PHRASE};
use volya_engine_formats::failed_check::failed;

use super::conditions::{DialogInProgress, DialogWorld, dialog_test};

/// The dialog every partner tries after its start dialog (2.1 step 1).
pub const HELLO_DIALOG: &str = "hello_dialog";

/// What the flow reaches beyond the conditions and effects.
pub trait TalkWorld: DialogWorld {
    /// The dialog `id`, loaded (`crate::dialogs::Dialogs::load`): an
    /// undeclared id is fatal.
    fn dialog(&mut self, id: &str) -> io::Result<Arc<Dialog>>;

    /// The player character `player`'s dialog list for `partner`, built as
    /// `IP` 6 states (emptied with its record of tried ids at every build,
    /// each dialog's own conditions tested, ordered by priority with equal
    /// priorities in the order tried, 6.2).
    fn player_dialog_list(&mut self, player: u16, partner: u16) -> io::Result<Vec<String>>;

    /// One draw of the shared generator (`original-determinism` 2.2): 0 …
    /// 32767.
    fn draw(&mut self) -> u32;

    /// The value *a* of character `of` for the pair with `towards` (4):
    /// asked only when the choices' goodwills differ, which no phrase of
    /// the edition does. Believed `of`'s attitude towards `towards`
    /// (unestablished, 10).
    fn attitude(&mut self, of: u16, towards: u16) -> i32;

    /// Adds a phrase's line to the answer log (`window` 6.1): its text
    /// `text` (a string id or a literal, never empty) with `speaker` as
    /// its speaker.
    fn add_line(&mut self, speaker: u16, text: &str);
}

/// An entry of the question list (5.1): what it stands for, a dialog id
/// with no dialog in progress or a phrase id, and its text (a dialog's
/// caption or a phrase's text, never empty).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Question {
    pub id: String,
    pub text: String,
}

/// The description of the failed check of a division by the count of
/// qualifying choices, 0 (4; believed a crash in the original).
pub const DIVISION_BY_ZERO: &str = "integer division by zero";

/// The flow of one talk window: its current dialog, the refill mark, the
/// question list and the player character's dialog list it was last built
/// from.
#[derive(Clone, Debug, Default)]
pub struct DialogFlow {
    player: u16,
    partner: u16,
    current: Option<DialogInProgress>,
    /// The player character's dialog list, as last built (2.2 step 1).
    dialog_list: Vec<String>,
    questions: Vec<Question>,
    refill: bool,
    shown: bool,
}

impl DialogFlow {
    pub fn new() -> Self {
        Self::default()
    }

    /// The dialog in progress (1).
    pub fn current(&self) -> Option<&DialogInProgress> {
        self.current.as_ref()
    }

    /// The question list, in order (5.1).
    pub fn questions(&self) -> &[Question] {
        &self.questions
    }

    /// The player character's dialog list as last built (5.1 step 2).
    pub fn dialog_list(&self) -> &[String] {
        &self.dialog_list
    }

    pub fn is_refill_marked(&self) -> bool {
        self.refill
    }

    /// Sets the refill mark (5.1): also when the player character is given
    /// or loses a portion while the window is open (`IP` 3.4 step 4, 4.2).
    pub fn mark_refill(&mut self) {
        self.refill = true;
    }

    /// Whether the talk window is shown: the player character hearing a
    /// phrase marks a refill only then (3 step 5).
    pub fn set_shown(&mut self, shown: bool) {
        self.shown = shown;
    }

    /// Talk start between `player` and `partner` (2.1), after the answer
    /// log is emptied: the question list is emptied, the partner's own
    /// list built from `start_dialog` (when it has one, DG-3) and
    /// [`HELLO_DIALOG`], its first dialog started by the partner, which
    /// says phrase 0; then the window is marked for a refill.
    pub fn talk_start(
        &mut self,
        world: &mut dyn TalkWorld,
        player: u16,
        partner: u16,
        start_dialog: Option<&str>,
    ) -> io::Result<()> {
        self.player = player;
        self.partner = partner;
        self.questions.clear();
        // Step 1.
        let mut list: Vec<Arc<Dialog>> = Vec::new();
        let mut tried: Vec<&str> = Vec::new();
        for id in start_dialog.into_iter().chain([HELLO_DIALOG]) {
            if tried.contains(&id) {
                continue;
            }
            tried.push(id);
            let dialog = world.dialog(id)?;
            if dialog_test(&dialog, world, partner, player)? {
                list.push(dialog);
            }
        }
        list.sort_by_key(|d| std::cmp::Reverse(d.priority));
        // Step 2.
        if let Some(dialog) = list.into_iter().next() {
            self.start(world, dialog, partner, player)?;
        }
        // Step 3.
        self.refill = true;
        Ok(())
    }

    /// The talk screen closes, by any of its routes (7): the current
    /// dialog is dropped, not finished, so a later talk never continues
    /// it. Ending a side (`stop_talk`) and switching to trade and back
    /// leave it as it was.
    pub fn close(&mut self) {
        self.current = None;
    }

    /// A click on the question standing for `id` (5.3; message 46, DG-6):
    /// ignored while the refill mark is set; otherwise with no dialog in
    /// progress the dialog `id` is started (2.2), else the player character
    /// says the phrase `id`; then the window is marked for a refill.
    pub fn click(&mut self, world: &mut dyn TalkWorld, id: &str) -> io::Result<()> {
        if self.refill {
            return Ok(());
        }
        match &self.current {
            None => {
                // 2.2 step 1: the original formats a text it neither
                // shows nor logs, then the fetch fails.
                if id.is_empty() || !self.dialog_list.iter().any(|d| d == id) {
                    return Err(failed(id, None));
                }
                let dialog = world.dialog(id)?;
                self.start(world, dialog, self.player, self.partner)?;
            }
            Some(current) => {
                let Some(phrase) = (current.dialog().phrases().iter()).position(|p| p.id == id)
                else {
                    // Not reached from the window's own questions, which
                    // stand for the choices; our reading, as the spec is
                    // silent.
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("no phrase {id} in the current dialog"),
                    ));
                };
                self.say(world, phrase)?;
            }
        }
        self.refill = true;
        Ok(())
    }

    /// One of the window's updates (5.1): refills the question list when
    /// the mark is set, then clears it.
    pub fn update(&mut self, world: &mut dyn TalkWorld) -> io::Result<()> {
        if !self.refill {
            return Ok(());
        }
        loop {
            // Step 1.
            self.questions.clear();
            let Some(current) = &self.current else {
                // Step 2.
                self.dialog_list = world.player_dialog_list(self.player, self.partner)?;
                for id in self.dialog_list.clone() {
                    let dialog = world.dialog(&id)?;
                    let text = dialog.caption_text();
                    if !text.is_empty() {
                        self.questions.push(Question {
                            id,
                            text: text.to_owned(),
                        });
                    }
                }
                break;
            };
            if current.turn() != self.player {
                // Step 4.
                break;
            }
            // Step 3.
            let phrases = current.dialog().phrases();
            let choices = current.sayable();
            if !choices.is_empty() && choices.iter().all(|&i| phrases[i].text.is_empty()) {
                let r = world.draw() as usize;
                let phrase = choices[r % choices.len()];
                self.say(world, phrase)?;
                // Starts over: with no dialog in progress, or with the new
                // choices (listed, or again all without text).
                continue;
            }
            self.questions = (choices.iter())
                .map(|&i| &phrases[i])
                .filter(|p| !p.text.is_empty())
                .map(|p| Question {
                    id: p.id.clone(),
                    text: p.text.clone(),
                })
                .collect();
            break;
        }
        // Step 5.
        self.refill = false;
        Ok(())
    }

    /// `first` starts `dialog` with `second` and says phrase 0 (2.1 step
    /// 2, 2.2 step 2); finished, there is no current dialog.
    fn start(
        &mut self,
        world: &mut dyn TalkWorld,
        dialog: Arc<Dialog>,
        first: u16,
        second: u16,
    ) -> io::Result<()> {
        // TODO(#907): a dialog whose phrases its `init_func` builds
        // (DG-9) has none here.
        // Phrase 0 is the first read (`files` 5.2).
        if dialog
            .phrases()
            .first()
            .is_none_or(|p| p.id != FIRST_PHRASE)
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("dialog {} has no phrase 0", dialog.id),
            ));
        }
        self.current = Some(DialogInProgress::start(dialog, first, second));
        self.say(world, 0)
    }

    /// The speaker whose turn it is says `phrase` (3), its line first:
    /// then the other speaker hears it, the partner answering at once
    /// (4), the player character marking a refill while the window is
    /// shown. A dialog finished on the way stops being the current one.
    fn say(&mut self, world: &mut dyn TalkWorld, mut phrase: usize) -> io::Result<()> {
        loop {
            let Some(current) = self.current.as_mut() else {
                return Ok(());
            };
            let speaker = current.turn();
            let text = &current.dialog().phrases()[phrase].text;
            if !text.is_empty() {
                world.add_line(speaker, text);
            }
            let told = current.say(phrase, world)?;
            if told.finished {
                self.current = None;
            }
            // Step 5.
            if told.listener == self.player {
                if self.shown {
                    self.refill = true;
                }
                return Ok(());
            }
            let Some(current) = &self.current else {
                return Ok(());
            };
            phrase = answer(current, world)?;
        }
    }
}

/// The partner's choice (4) among the choices of `current`, in goodwill
/// order: one of all of them, by one draw, when their goodwills are equal
/// or its value *a* is less than the lowest; otherwise none qualifies and
/// the division by their count, 0, fails.
fn answer(current: &DialogInProgress, world: &mut dyn TalkWorld) -> io::Result<usize> {
    let phrases = current.dialog().phrases();
    let choices = current.sayable();
    let Some(&last) = choices.last() else {
        return Err(failed(DIVISION_BY_ZERO, None));
    };
    let lowest = phrases[last].goodwill;
    let equal = choices.iter().all(|&i| phrases[i].goodwill == lowest);
    let answering = current.turn();
    let other = if answering == current.first_speaker() {
        current.second_speaker()
    } else {
        current.first_speaker()
    };
    if equal || world.attitude(answering, other) < lowest {
        let r = world.draw() as usize;
        Ok(choices[r % choices.len()])
    } else {
        Err(failed(DIVISION_BY_ZERO, None))
    }
}

#[cfg(test)]
mod tests;
