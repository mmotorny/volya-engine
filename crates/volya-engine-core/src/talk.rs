// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Talk start and end (docs/behavior/talk.md, cited by section; DG-3): each
//! character's talk state (1), the offer that starts a talk (2.2), the turn
//! of the player's view toward the partner (4), the rules that end a side
//! and close the talk window (5.1, 5.2) and the start dialog (6). The
//! dialog lists and dialogs a talk offers (3) are the dialog flow's
//! ([`crate::dialogs::flow`]). The window's own
//! input and opening are docs/behavior/talk-window-input.md's (DG-6,
//! `volya_engine::talk_window`).
//!
//! A **character** is a stalker, a trader or the player character (monsters
//! never talk); its **side** of a talk is its talk flag and partner.

use std::collections::BTreeMap;
use std::f32::consts::{FRAC_PI_2, PI};

use crate::torch;

/// The talk window closes when the speakers are farther apart, metres
/// (5.2 rule 3).
pub const TALK_DISTANCE: f32 = 3.0;
/// The view turns only while it is farther from its target, radians (4).
pub const TURN_THRESHOLD: f32 = 0.2;
/// The turn's smoothing law: v_min, v_max (rad/s) and R (4).
pub const TURN_LAW: [f32; 3] = [0.15, 0.2, PI / 6.0];
/// Below this a component of the direction counts as 0 (4).
const EPSILON: f32 = 1e-7;

/// A character's talk state (1): its side of a talk, whether it accepts
/// one, and its start dialogs (6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TalkState {
    pub talking: bool,
    pub partner: Option<u16>,
    /// Set when the object is made (1, B4).
    pub talk_enabled: bool,
    pub trade_enabled: bool,
    /// The start dialog and the default start dialog (6), stalkers' and
    /// traders' only; empty for a new one.
    pub start_dialog: String,
    pub default_start_dialog: String,
}

impl Default for TalkState {
    fn default() -> Self {
        Self {
            talking: false,
            partner: None,
            talk_enabled: true,
            trade_enabled: true,
            start_dialog: String::new(),
            default_start_dialog: String::new(),
        }
    }
}

/// The characters' sides of their talks as the talk reads and ends them:
/// the game's are script API R's characters (CH-7); [`Talks`] is one of
/// its own.
pub trait TalkSides {
    /// `is_talking` (5.4): the talk flag.
    fn is_talking(&self, id: u16) -> bool;
    /// The character `id` talks with, while talking.
    fn partner(&self, id: u16) -> Option<u16>;
    /// Stopping a side (`stop_talk`, 1).
    fn stop_talk(&mut self, id: u16);
    /// Its start dialog (6), empty for none.
    fn start_dialog(&self, id: u16) -> String;
}

/// Character `id`'s scheduled update (5.1), with its health: while
/// talking, its side stops if its partner is no longer talking, and if its
/// own health is 0 or less (a dead character still gets the update).
pub fn scheduled_update(sides: &mut dyn TalkSides, id: u16, health: f32) {
    if !sides.is_talking(id) {
        return;
    }
    let partner_talking = sides.partner(id).is_some_and(|p| sides.is_talking(p));
    if !partner_talking || health <= 0.0 {
        sides.stop_talk(id);
    }
}

/// The talk states of the characters, by object id; a character never
/// touched has the default state.
#[derive(Clone, Debug, Default)]
pub struct Talks {
    states: BTreeMap<u16, TalkState>,
}

impl Talks {
    /// Character `id`'s state.
    pub fn state(&self, id: u16) -> TalkState {
        self.states.get(&id).cloned().unwrap_or_default()
    }

    fn state_mut(&mut self, id: u16) -> &mut TalkState {
        self.states.entry(id).or_default()
    }

    /// `is_talking` (5.4): the talk flag.
    pub fn is_talking(&self, id: u16) -> bool {
        self.states.get(&id).is_some_and(|s| s.talking)
    }

    /// The character `id` talks with, while talking.
    pub fn partner(&self, id: u16) -> Option<u16> {
        self.states.get(&id).filter(|s| s.talking)?.partner
    }

    /// `enable_talk`, `disable_talk` (1): only the next offer reads it.
    pub fn set_talk_enabled(&mut self, id: u16, enabled: bool) {
        self.state_mut(id).talk_enabled = enabled;
    }

    pub fn is_talk_enabled(&self, id: u16) -> bool {
        self.states.get(&id).is_none_or(|s| s.talk_enabled)
    }

    /// Starting a side (1): the flag set and the partner kept. Readying the
    /// speaker's trade with the other is the trade's (#284).
    fn start_side(&mut self, id: u16, partner: u16) {
        let s = self.state_mut(id);
        s.talking = true;
        s.partner = Some(partner);
    }

    /// Stopping a side (`stop_talk`, 1): the flag cleared and the partner
    /// forgotten; the other side and the window are untouched.
    pub fn stop_talk(&mut self, id: u16) {
        if let Some(s) = self.states.get_mut(&id) {
            s.talking = false;
            s.partner = None;
        }
    }

    /// The offer of 2.2: `partner` accepts when its talk is enabled and both
    /// it and `player` have health above 0 (whether either is talking
    /// already is not tested); then the partner starts its side, then the
    /// player character. Returns whether it accepted.
    pub fn offer(
        &mut self,
        partner: u16,
        partner_health: f32,
        player: u16,
        player_health: f32,
    ) -> bool {
        if !self.is_talk_enabled(partner) || partner_health <= 0.0 || player_health <= 0.0 {
            return false;
        }
        self.start_side(partner, player);
        self.start_side(player, partner);
        true
    }

    /// Character `id`'s scheduled update (5.1), with its health: while
    /// talking, its side stops if its partner is no longer talking, and if
    /// its own health is 0 or less.
    pub fn scheduled_update(&mut self, id: u16, health: f32) {
        scheduled_update(self, id, health);
    }

    /// The characters talking, in id order.
    pub fn talking(&self) -> Vec<u16> {
        (self.states.iter())
            .filter(|(_, s)| s.talking)
            .map(|(&id, _)| id)
            .collect()
    }

    /// Coming online (6): a character whose start dialog is empty takes
    /// `description`'s (its character information's) as its start dialog
    /// and default start dialog.
    pub fn come_online(&mut self, id: u16, description: &str) {
        let s = self.state_mut(id);
        if s.start_dialog.is_empty() {
            s.start_dialog = description.to_owned();
            s.default_start_dialog = description.to_owned();
        }
    }

    /// `set_start_dialog` (6): the next set-up's opening list uses it.
    pub fn set_start_dialog(&mut self, id: u16, dialog: &str) {
        self.state_mut(id).start_dialog = dialog.to_owned();
    }

    /// `restore_default_start_dialog` (6).
    pub fn restore_default_start_dialog(&mut self, id: u16) {
        let s = self.state_mut(id);
        s.start_dialog = s.default_start_dialog.clone();
    }
}

/// The camera's target point (4): the centre of the partner's bounding
/// sphere `[x, y, z, r]` raised by half its radius (believed).
pub fn target_point([x, y, z, r]: [f32; 4]) -> [f32; 3] {
    [x, y + r / 2.0, z]
}

/// The target yaw and pitch toward `d` = target point − the camera's
/// position (4): yaw the direction of (dx, dz), pitch −atan(dy / h), with
/// their values for components below 10⁻⁷.
pub fn target_angles([dx, dy, dz]: [f32; 3]) -> (f32, f32) {
    let yaw = if dx.abs() < EPSILON && dz.abs() < EPSILON {
        0.0
    } else if dz.abs() < EPSILON {
        if dx > 0.0 { FRAC_PI_2 } else { -FRAC_PI_2 }
    } else if dz > 0.0 {
        (dx / dz).atan()
    } else {
        (dx / dz).atan() - PI
    };
    let h = (dx * dx + dz * dz).sqrt();
    let pitch = if h >= EPSILON {
        -(dy / h).atan()
    } else if dy.abs() < EPSILON {
        0.0
    } else if dy > 0.0 {
        -FRAC_PI_2
    } else {
        FRAC_PI_2
    };
    (yaw, pitch)
}

/// One angle of the turn (4): moved by the smoothing law of
/// docs/behavior/torch.md 7.4 step 2 with [`TURN_LAW`] over `dt` seconds
/// only while its difference from `target` (both, and the difference,
/// brought into [−π, π]) exceeds [`TURN_THRESHOLD`].
pub fn turn(angle: f32, target: f32, dt: f32) -> f32 {
    let d = torch::difference(torch::wrap(target) - torch::wrap(angle));
    if d.abs() > TURN_THRESHOLD {
        torch::smooth_with(angle, target, dt, TURN_LAW)
    } else {
        angle
    }
}

/// The camera at `eye` with `yaw` and `pitch` turned toward the partner's
/// bounding sphere over `dt` seconds (4): the yaw, then the pitch.
pub fn turn_view(eye: [f32; 3], sphere: [f32; 4], yaw: f32, pitch: f32, dt: f32) -> (f32, f32) {
    let t = target_point(sphere);
    let (ty, tp) = target_angles([t[0] - eye[0], t[1] - eye[1], t[2] - eye[2]]);
    (turn(yaw, ty, dt), turn(pitch, tp, dt))
}

/// What the talk window's update tests to close itself (5.2): the player
/// character talking, both speakers' objects there, the distance between
/// their positions and the partner's special mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Speakers {
    pub player_talking: bool,
    pub both_exist: bool,
    pub distance: f32,
    pub special_mode: bool,
}

impl Speakers {
    /// Rules 1–3 of 5.2.
    pub fn window_closes(&self) -> bool {
        !self.player_talking
            || !self.both_exist
            || (self.distance > TALK_DISTANCE && !self.special_mode)
    }
}

impl TalkSides for Talks {
    fn is_talking(&self, id: u16) -> bool {
        Talks::is_talking(self, id)
    }

    fn partner(&self, id: u16) -> Option<u16> {
        Talks::partner(self, id)
    }

    fn stop_talk(&mut self, id: u16) {
        Talks::stop_talk(self, id)
    }

    fn start_dialog(&self, id: u16) -> String {
        self.state(id).start_dialog
    }
}

#[cfg(test)]
mod tests;
