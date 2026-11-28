// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The dialog files' functions as entry points
//! (docs/behavior/script-api-dialogs-and-trade.md 3.1 to 3.3, cited as
//! `R`; docs/behavior/dialog-conditions.md 3, 4.2 and 6): a precondition
//! called with two characters' handles and three texts, its result the
//! runtime's truth value, and an action called with two handles and two
//! texts, its result ignored. What calls them, when and with which
//! characters is `volya_engine_core::dialogs::conditions`, to which
//! [`Scripts`] is a [`DialogWorld`] over the story's portions.

use std::io;

use volya_engine_core::dialogs::conditions::DialogWorld;
use volya_engine_core::info_portions::{Change, InfoPortions};
use volya_engine_formats::cp1251;

use crate::entry_points::{Keep, lua_text};
use crate::loader::Scripts;
use crate::{Error, Value};

impl Scripts {
    /// The handle of object `id` (script API M), nil without one.
    fn handle(&self, id: u16) -> Value {
        self.game_object(id).map_or(Value::Nil, Value::Object)
    }

    /// Calls `function` (looked up by name, `files` 6) in the runtime's
    /// protected mode with `args`, its first result kept as `keep`. An
    /// error it raises is fatal (`LUA error: …`, `R` 3.1, D 1.2). A name no script defines is fatal too: the original
    /// makes the call with no function (`files` 8, believed an access
    /// fault; our wording).
    pub(super) fn call_dialog_function(
        &mut self,
        function: &str,
        keep: Keep,
        args: &[Value],
    ) -> Result<Value, Error> {
        match self.call_entry(function, keep, args)? {
            Some(result) => Ok(result),
            None => Err(self.stop(format!("the dialog function {function} is missing"))),
        }
    }

    /// The precondition `function` for characters `first` and `second`
    /// with `texts` (the dialog id, the id of the phrase just said and the
    /// id of the phrase tested; empty for a dialog's own, `DG-2` 6): five
    /// arguments, the result's truth value (false for `false` and nil, also
    /// no result; true for anything else).
    pub fn call_precondition(
        &mut self,
        function: &str,
        first: u16,
        second: u16,
        texts: [&str; 3],
    ) -> Result<bool, Error> {
        let mut args = vec![self.handle(first), self.handle(second)];
        args.extend(texts.iter().map(|t| lua_text(t)));
        let result = self.call_dialog_function(function, Keep::Truth, &args)?;
        Ok(matches!(result, Value::Boolean(true)))
    }

    /// The action `function` for a phrase `phrase` of dialog `dialog` said
    /// by `speaker` to `listener` (`DG-2` 4.2 step 2.3): four arguments,
    /// the result ignored.
    pub fn call_action(
        &mut self,
        function: &str,
        speaker: u16,
        listener: u16,
        dialog: &str,
        phrase: &str,
    ) -> Result<(), Error> {
        let args = [
            self.handle(speaker),
            self.handle(listener),
            lua_text(dialog),
            lua_text(phrase),
        ];
        self.call_dialog_function(function, Keep::Nothing, &args)?;
        Ok(())
    }

    /// A transfer to the player character through the story.
    fn transfer(&mut self, id: &str, change: Change) -> io::Result<()> {
        let id = cp1251::encode(id).unwrap_or_else(|| id.as_bytes().to_vec());
        self.with_story(|ctx, story| {
            let Some(player) = story.info_portions.as_ref().map(InfoPortions::player) else {
                return Err("the information portions aren't set".into());
            };
            match change {
                Change::Give => ctx.give_portion(story, player, &id),
                Change::Disable => ctx.disable_portion(story, player, &id),
            }
        })
        .map_err(io_error)
    }
}

/// The scripts as the dialogs' world (`DG-2` 3, 4.2): the player
/// character's portions are the story's ([`crate::story::Story`]), each
/// transfer given or disabled through it and its receiving's script part
/// run before the next effect (`IP` 3.3, 3.4), and the functions called as
/// above. A fatal error stops the state; it comes back as the error's text.
impl DialogWorld for Scripts {
    fn player_holds(&mut self, id: &str) -> bool {
        let Some(story) = self.registered_story() else {
            return false;
        };
        let story = story.borrow();
        (story.info_portions.as_ref()).is_some_and(|p| p.holds(p.player(), id))
    }

    fn give_to_player(&mut self, id: &str) -> io::Result<()> {
        self.transfer(id, Change::Give)
    }

    fn disable_on_player(&mut self, id: &str) -> io::Result<()> {
        self.transfer(id, Change::Disable)
    }

    fn call_precondition(
        &mut self,
        function: &str,
        first: u16,
        second: u16,
        texts: [&str; 3],
    ) -> io::Result<bool> {
        Scripts::call_precondition(self, function, first, second, texts).map_err(io_error)
    }

    fn call_action(
        &mut self,
        function: &str,
        speaker: u16,
        listener: u16,
        dialog: &str,
        phrase: &str,
    ) -> io::Result<()> {
        Scripts::call_action(self, function, speaker, listener, dialog, phrase).map_err(io_error)
    }
}

fn io_error(e: Error) -> io::Error {
    io::Error::other(e.message)
}
