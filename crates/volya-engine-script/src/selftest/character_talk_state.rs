// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/character-talk-state.md (CH-7) on every target, one check
//! per section, over the characters of [`super::dialogs_and_trade`]'s
//! state: which objects can talk (1), the talk-enabled flag (2), the
//! character's side (3) and the start dialog (6), each character made
//! again as it comes online ([`come_online`]).

use super::dialogs_and_trade::{DOG, PLAYER, STALKER, State, TRADER, check_lua, data, state};
use super::ensure;
use crate::game_object::{EngineState, ObjectClass};

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("1 which objects can talk", which_objects_can_talk),
    ("2 the talk-enabled flag", talk_enabled_flag),
    ("3 the character's side", side),
    ("6 the start dialog", start_dialog),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| {
            f().err()
                .map(|e| format!("character talk state {what}: {e}"))
        })
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// The trader's start dialog in the shipped data (8).
const TRADER_DIALOG: &str = "escape_trader_start_dialog";

/// Character `id` goes offline (its online object destroyed) and comes
/// online again (a new one made, offline-simulation.md 6.3), with the
/// character information's start dialog `dialog`.
fn come_online(t: &mut State, id: u16, dialog: &str) -> Result<(), String> {
    let (class, name, clsid) = match id {
        PLAYER => (ObjectClass::Player, "actor", "O_ACTOR"),
        STALKER => (ObjectClass::Named("CAI_Stalker"), "stalker", "AI_STL_S"),
        DOG => (ObjectClass::Named("CAI_Dog"), "dog", "AI_DOG_R"),
        TRADER => (ObjectClass::Named("CAI_Trader"), "trader", "AI_TRD_S"),
        _ => return Err(format!("no character {id}")),
    };
    t.s.destroy_object(id).map_err(|e| e.error.message)?;
    let state = EngineState {
        start_dialog: dialog.as_bytes().to_vec(),
        ..EngineState::default()
    };
    t.objects.borrow_mut().prepare(id, state);
    t.s.spawn_engine_object(class, data(id, name, clsid))
        .map_err(|e| e.error.message)?;
    Ok(())
}

/// Section 1.1: the query of the player, stalkers and traders is their
/// flag; a monster's is always false whatever its flag holds, so an
/// offer to it is refused.
pub fn which_objects_can_talk() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "1.1",
        r#"
for _, o in ipairs({actor, stalker, trader}) do
  eq(o:is_talk_enabled(), true, "enabled")
  o:disable_talk()
  eq(o:is_talk_enabled(), false, "disabled")
  o:enable_talk()
end
dog:enable_talk()
eq(dog:is_talk_enabled(), false, "a monster")
actor:run_talk_dialog(dog)
eq(dog:is_talking(), false, "a monster refuses")
eq(actor:is_talking(), false, "the player's side not started")
"#,
    )?;
    let objects = t.objects.borrow();
    let dog = (objects.get(DOG).and_then(|o| o.owner.as_ref())).ok_or("no dog")?;
    ensure(dog.talk.talk_enabled, || {
        "enable_talk sets a monster's flag".into()
    })
}

/// Section 2: enabled each time a character comes online (2.1); only the
/// two members change it, and a change does not end a talk (2.2).
pub fn talk_enabled_flag() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "2.2",
        r#"
actor:run_talk_dialog(trader)
eq(trader:is_talking(), true, "talking")
trader:disable_talk()
eq(trader:is_talking(), true, "disabling does not end the talk")
trader:stop_talk() actor:stop_talk()
eq(trader:is_talk_enabled(), false, "ending the talk does not enable it")
trader:enable_talk()
actor:run_talk_dialog(trader)
trader:stop_talk() actor:stop_talk()
eq(trader:is_talk_enabled(), true, "nor does a talk disable it")
trader:disable_talk() stalker:disable_talk() actor:disable_talk()
"#,
    )?;
    for id in [TRADER, STALKER, PLAYER] {
        come_online(&mut t, id, "")?;
    }
    check_lua(
        &mut t,
        "2.1",
        r#"
eq(trader:is_talk_enabled(), true, "the trader, enabled at coming online")
eq(stalker:is_talk_enabled(), true, "a stalker, enabled at coming online")
eq(actor:is_talk_enabled(), true, "the player, enabled when made")
"#,
    )
}

/// Section 3: not talking and no partner before the first talk (believed),
/// written by starting and stopping a side, and gone with the online
/// object.
pub fn side() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "3",
        r#"
eq(trader:is_talking(), false, "not talking before the first talk")
actor:run_talk_dialog(trader)
eq(trader:is_talking(), true, "the side started")
"#,
    )?;
    come_online(&mut t, TRADER, "")?;
    check_lua(
        &mut t,
        "3",
        r#"eq(trader:is_talking(), false, "not talking after coming online again")"#,
    )?;
    let objects = t.objects.borrow();
    let trader = (objects.get(TRADER).and_then(|o| o.owner.as_ref())).ok_or("no trader")?;
    ensure(trader.talk.partner.is_none(), || {
        format!("the partner {:?}", trader.talk.partner)
    })
}

/// Section 6: at coming online a stalker's and a trader's start dialog and
/// default start dialog are the character information's (6.1), an id no
/// dialog file defines included (6.3); only `set_start_dialog` and
/// `restore_default_start_dialog` change them, and a new online object
/// takes the character information's again. Other characters have none.
pub fn start_dialog() -> Result<(), String> {
    let mut t = state()?;
    come_online(&mut t, TRADER, TRADER_DIALOG)?;
    come_online(&mut t, STALKER, "gar_wounded_bandit")?;
    come_online(&mut t, DOG, "dog_dialog")?;
    come_online(&mut t, PLAYER, "actor_dialog")?;
    check_lua(
        &mut t,
        "6.1",
        &format!(
            r#"
eq(trader:get_start_dialog(), "{TRADER_DIALOG}", "the trader's at coming online")
eq(stalker:get_start_dialog(), "gar_wounded_bandit", "kept without being looked up")
trader:set_start_dialog("other_dialog")
eq(trader:get_start_dialog(), "other_dialog", "set")
trader:restore_default_start_dialog()
eq(trader:get_start_dialog(), "{TRADER_DIALOG}", "the default")
trader:set_start_dialog("other_dialog")
"#
        ),
    )?;
    come_online(&mut t, TRADER, TRADER_DIALOG)?;
    check_lua(
        &mut t,
        "6.1",
        &format!(
            r##"
eq(trader:get_start_dialog(), "{TRADER_DIALOG}", "the character information's again")
eq(select("#", dog:get_start_dialog()), 0, "a monster has none")
eq(select("#", actor:get_start_dialog()), 0, "the player has none")
"##
        ),
    )?;
    let objects = t.objects.borrow();
    let talk = |id| (objects.get(id).and_then(|o| o.owner.as_ref())).map(|o| o.talk.clone());
    let (trader, dog, player) = (
        talk(TRADER).ok_or("no trader")?,
        talk(DOG).ok_or("no dog")?,
        talk(PLAYER).ok_or("no player")?,
    );
    ensure(
        trader.default_start_dialog == TRADER_DIALOG.as_bytes(),
        || format!("the default {:?}", trader.default_start_dialog),
    )?;
    let none =
        |t: &crate::dialogs::Talk| t.start_dialog.is_empty() && t.default_start_dialog.is_empty();
    ensure(none(&dog) && none(&player), || {
        "a start dialog kept on a monster or the player".into()
    })
}
