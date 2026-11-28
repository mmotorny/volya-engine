// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The level's talks (docs/behavior/talk.md, cited by section; DG-3, with
//! docs/behavior/use-action.md 6.2): the characters' talk states
//! ([`crate::core::talk`]), the character under the crosshair and the use
//! action's offer (2.1, 2.2), the characters' scheduled updates (5.1), the
//! talk's set-up with the partner's opening dialog (3, 3.1, 3.2), the
//! player character's dialogs as the choices (3.3), choosing one (3.5),
//! the end of a dialog (3.6) and hiding the window (5.2). The talk screen
//! that shows a talk is [`crate::talk_window::screen`].
//!
//! The dialogs are DG-4's flow ([`DialogFlow`], docs/behavior/dialog-flow.md)
//! with DG-2's conditions and effects, run over the level's information
//! portions, the scripts' functions and the shared generator ([`Story`]).
//!
//! **What waits for other systems**: the scripts' talk calls
//! (`run_talk_dialog`, `stop_talk`, the start dialog's setters) keep script
//! API R's own talk state (#1081); the talk-enabled flag is taken from it
//! by the app. A character's start dialog is taken from its description
//! the first time a talk needs it (6: coming online; nothing changes it
//! before here); saving it is the saved games' (roadmap, Saved games).

use std::io;
use std::sync::Arc;

use serde::Serialize;

use crate::characters::Characters;
use crate::core::collision::geometry::Vec3;
use crate::core::dialogs::Dialogs;
use crate::core::dialogs::conditions::{DialogFunctions, DialogWorld, dialog_test};
use crate::core::dialogs::flow::{DialogFlow, TalkWorld};
use crate::core::particles::random::Shared;
use crate::core::talk::{Speakers, TalkSides};
#[cfg(test)]
use crate::core::talk::{Talks, scheduled_update};
use crate::core::vfs::Vfs;
use crate::data::GameData;
use crate::script::story::SharedInfoPortions;
use crate::talk_window::screen::{Said, TalkStart};
use crate::talk_window::{self, Character, Icon};
use volya_engine_formats::character_descriptions::SpecificCharacter;
use volya_engine_formats::dialog_files::Dialog;
use volya_engine_formats::object::TraderLike;

/// A character the crosshair can pick (use-action.md 2.1): its object id,
/// its model's bounding sphere in the world, its position and health.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    pub id: u16,
    pub sphere: [f32; 4],
    pub position: Vec3,
    pub health: f32,
}

/// A talk partner as its object gives it.
pub struct Partner<'a> {
    pub id: u16,
    pub trader: &'a TraderLike,
    /// Its object section (talk-window.md 9: the special mode).
    pub section: &'a str,
}

/// The player character as the talk needs it.
pub struct Player<'a> {
    pub id: u16,
    pub name: &'a str,
    pub specific_character: &'a str,
}

/// What the talk's dialogs reach (DG-2 3, 4.2; DG-4's [`TalkWorld`]): the
/// level's information portions at game time `now`, the scripts'
/// functions and the shared generator.
pub struct Story<'a> {
    /// Shared with the scripts' story, whose functions reach them too:
    /// borrowed only for each question or change.
    pub portions: &'a SharedInfoPortions,
    pub now: u64,
    pub functions: &'a mut dyn DialogFunctions,
    pub random: &'a Shared,
}

/// The talk world of one call: the story, the dialogs loaded from `tree`,
/// the partner's actor dialogs and the lines said, for the answer log.
struct Talking<'a, 'b> {
    story: &'a mut Story<'b>,
    dialogs: &'a mut Dialogs,
    tree: &'a dyn Vfs,
    data: &'a GameData,
    actor_dialogs: &'a [String],
    lines: Vec<(u16, String)>,
}

/// The dialogs and trade's world (DG-2 3, 4.2): a transfer's receiving and
/// its script part (`IP` 3.3, 3.4) are the portions' and the scripts'
/// story's, which takes their events.
impl DialogWorld for Talking<'_, '_> {
    fn player_holds(&mut self, id: &str) -> bool {
        let p = self.story.portions.borrow();
        p.holds(p.player(), id)
    }

    fn give_to_player(&mut self, id: &str) -> io::Result<()> {
        let mut p = self.story.portions.borrow_mut();
        let player = p.player();
        p.give(player, id, self.story.now)
    }

    fn disable_on_player(&mut self, id: &str) -> io::Result<()> {
        let mut p = self.story.portions.borrow_mut();
        let player = p.player();
        p.disable(player, id)
    }

    fn call_precondition(
        &mut self,
        function: &str,
        first: u16,
        second: u16,
        texts: [&str; 3],
    ) -> io::Result<bool> {
        (self.story.functions).call_precondition(function, first, second, texts)
    }

    fn call_action(
        &mut self,
        function: &str,
        speaker: u16,
        listener: u16,
        dialog: &str,
        phrase: &str,
    ) -> io::Result<()> {
        (self.story.functions).call_action(function, speaker, listener, dialog, phrase)
    }
}

impl TalkWorld for Talking<'_, '_> {
    fn dialog(&mut self, id: &str) -> io::Result<Arc<Dialog>> {
        self.dialogs.load(self.tree, &self.data.aliases, id)
    }

    /// 3.3, info-portions.md 6: each dialog tried with its own conditions
    /// (DG-2 2).
    /// The list is built as [`InfoPortions::dialog_list`] builds it, the
    /// portions borrowed only to read the held ones: the conditions'
    /// script functions reach them too.
    fn player_dialog_list(&mut self, player: u16, partner: u16) -> io::Result<Vec<String>> {
        let declarations = self.data.info_portions(self.tree)?;
        let held: Vec<String> = {
            let p = self.story.portions.borrow();
            (p.list(p.player()).iter())
                .filter_map(|k| declarations.portion(&k.id))
                .flat_map(|d| d.dialogs.iter().cloned())
                .collect()
        };
        let ids: Vec<String> = held
            .into_iter()
            .chain(self.actor_dialogs.iter().cloned())
            .collect();
        let mut tried = std::collections::HashSet::new();
        let mut list = Vec::new();
        for id in ids {
            if !tried.insert(id.clone()) {
                continue;
            }
            let mut log = Vec::new();
            declarations.dialogs.look_up(&id, &mut log)?;
            let d = self.dialogs.load(self.tree, &self.data.aliases, &id)?;
            if dialog_test(&d, self, player, partner)? {
                list.push((d.priority, id));
            }
        }
        list.sort_by_key(|&(priority, _)| std::cmp::Reverse(priority));
        Ok(list.into_iter().map(|(_, id)| id).collect())
    }

    fn draw(&mut self) -> u32 {
        self.story.random.next_k()
    }

    // TODO(#984): a character's attitude (CH-7); asked only when the
    // choices' goodwills differ, which no phrase of the edition does.
    fn attitude(&mut self, _: u16, _: u16) -> i32 {
        0
    }

    fn add_line(&mut self, speaker: u16, text: &str) {
        self.lines.push((speaker, text.to_owned()));
    }
}

/// The level's talks.
#[derive(Default)]
pub struct LevelTalk {
    /// `[inventory] take_dist` (use-action.md 2.2).
    take_dist: f32,
    /// The character under the crosshair and its distance (use-action.md
    /// 2.2).
    picked: Option<(u16, f32)>,
    /// The talk sides as last synced ([`LevelTalk::sync`]): the
    /// characters talking and the player character's partner.
    talking: Vec<u16>,
    partner: Option<u16>,
    dialogs: Option<Dialogs>,
    /// The talk window's dialog flow (3.2, 3.5, 3.6; DG-4).
    flow: DialogFlow,
    /// The partner's actor dialogs, read at the set-up (3.3).
    actor_dialogs: Vec<String>,
    /// The speakers and their names in the answer log (talk-window.md 6.1).
    names: Vec<(u16, String)>,
}

/// The ray from `eye` along unit `dir` meeting sphere `[x, y, z, r]`: the
/// distance to its near side (0 from inside), if ahead.
fn ray_sphere(eye: Vec3, dir: Vec3, [x, y, z, r]: [f32; 4]) -> Option<f32> {
    let oc = [eye[0] - x, eye[1] - y, eye[2] - z];
    let b = oc[0] * dir[0] + oc[1] * dir[1] + oc[2] * dir[2];
    let c = oc[0] * oc[0] + oc[1] * oc[1] + oc[2] * oc[2] - r * r;
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let (near, far) = (-b - disc.sqrt(), -b + disc.sqrt());
    (far >= 0.0).then_some(near.max(0.0))
}

/// A panel's character (talk-window.md 5): its name and its specific
/// character's icon.
// TODO(#279): the rank, community and reputation names.
fn panel(id: u16, name: &str, specific: Option<&SpecificCharacter>) -> Character {
    Character {
        id,
        name: name.to_owned(),
        rank: String::new(),
        community: String::new(),
        reputation: String::new(),
        icon: specific.filter(|s| !s.icon.is_empty()).map(|s| Icon {
            texture: s.icon.clone(),
            rect: None,
        }),
    }
}

impl LevelTalk {
    pub fn new(data: &GameData) -> io::Result<Self> {
        Ok(Self {
            take_dist: crate::core::config::value::float(
                data.system.read("inventory", "take_dist")?,
            ),
            ..Self::default()
        })
    }

    /// The crosshair's character (use-action.md 2.1, 2.2): the nearest of
    /// `candidates` whose sphere the ray from `eye` along `dir` meets closer
    /// than `take_dist`. Which shapes the pick tests, and the level mesh in
    /// front, are not specified for characters (our choice, as the doors'):
    /// the model's bounding sphere, nothing hidden.
    pub fn pick(&mut self, eye: Vec3, dir: Vec3, candidates: &[Candidate]) {
        self.picked = (candidates.iter())
            .filter_map(|c| Some((c.id, ray_sphere(eye, dir, c.sphere)?.max(0.5))))
            .filter(|&(_, t)| t < self.take_dist)
            .min_by(|a, b| a.1.total_cmp(&b.1));
    }

    /// The character under the crosshair and its distance.
    pub fn picked(&self) -> Option<(u16, f32)> {
        self.picked
    }

    /// Character `c` under the crosshair, the use action's (use-action.md 3
    /// step 7, 6.2): the scripts' use and the offer are script API R's
    /// (`offer_talk`); a character with health 0 or less is not offered.
    pub fn usable(c: &Candidate) -> bool {
        c.health > 0.0
    }

    /// The talk sides as `sides` holds them now, for the report: the
    /// characters among `ids` talking and `player`'s partner.
    pub fn sync(&mut self, sides: &dyn TalkSides, ids: &[u16], player: u16) {
        self.talking = (ids.iter().copied())
            .filter(|&id| sides.is_talking(id))
            .collect();
        self.partner = sides.partner(player);
    }

    /// Runs `f` with the talk world over `story`.
    fn with_world<R>(
        &mut self,
        tree: &dyn Vfs,
        data: &GameData,
        story: &mut Story,
        f: impl FnOnce(&mut DialogFlow, &mut Talking) -> io::Result<R>,
    ) -> io::Result<(R, Vec<Said>)> {
        let dialogs = match &mut self.dialogs {
            Some(d) => d,
            None => self.dialogs.insert(Dialogs::new(data.info_portions(tree)?)),
        };
        let mut world = Talking {
            story,
            dialogs,
            tree,
            data,
            actor_dialogs: &self.actor_dialogs,
            lines: Vec::new(),
        };
        let r = f(&mut self.flow, &mut world)?;
        let names = &self.names;
        let said = (world.lines.into_iter())
            .map(|(id, text)| Said {
                speaker: (names.iter())
                    .find(|(n, _)| *n == id)
                    .map(|(_, name)| name.clone())
                    .unwrap_or_default(),
                text,
            })
            .collect();
        Ok((r, said))
    }

    /// The talk's set-up (3 steps 1–2) when the player character is
    /// talking with `partner`: the speakers for the panels and titles, then
    /// the dialog flow's talk start with the partner's start dialog (3.1,
    /// 3.2; DG-4 2.1), its lines said. `None` when the player character is
    /// not talking (3: the set-up does nothing).
    pub fn set_up(
        &mut self,
        tree: &dyn Vfs,
        data: &GameData,
        player: &Player,
        partner: &Partner,
        sides: &dyn TalkSides,
        story: &mut Story,
    ) -> io::Result<Option<TalkStart>> {
        if !sides.is_talking(player.id) {
            return Ok(None);
        }
        let mut characters = Characters::new(tree, data);
        let own = match player.specific_character {
            "" => None,
            id => Some(characters.specific_character(id)?.clone()),
        };
        let specific = characters.for_trader(partner.trader)?.cloned();
        // CH-2 5: the online character's name is its object's character
        // name.
        let partner_name = data.strings.get(&partner.trader.character_name).to_owned();
        self.names = vec![
            (player.id, player.name.to_owned()),
            (partner.id, partner_name.clone()),
        ];
        // 3.3: reading the actor dialogs needs a specific character.
        self.actor_dialogs = match &specific {
            Some(s) => s.actor_dialogs.clone(),
            None => {
                return Err(io::Error::other(format!(
                    "{}: no specific character",
                    partner.trader.character_profile
                )));
            }
        };
        let start_dialog = sides.start_dialog(partner.id);
        let start_dialog = (!start_dialog.is_empty()).then_some(start_dialog.as_str());
        let ((), said) = self.with_world(tree, data, story, |flow, world| {
            flow.set_shown(true);
            flow.talk_start(world, player.id, partner.id, start_dialog)
        })?;
        Ok(Some(TalkStart {
            player: panel(player.id, player.name, own.as_ref()),
            partner: panel(partner.id, &partner_name, specific.as_ref()),
            special_mode: talk_window::has_special_mode(data, partner.section),
            said,
        }))
    }

    /// Marks the question list for a refill (3.3; DG-4 5.1).
    pub fn mark_refill(&mut self) {
        self.flow.mark_refill();
    }

    /// The window's update of the dialog flow (DG-4 5.1): when marked, the
    /// question list refilled, as text keys, and the lines said on the way
    /// (the empty-text choices said on their own, 5.2).
    pub fn refill(
        &mut self,
        tree: &dyn Vfs,
        data: &GameData,
        story: &mut Story,
    ) -> io::Result<(Option<Vec<String>>, Vec<Said>)> {
        if !self.flow.is_refill_marked() {
            return Ok((None, Vec::new()));
        }
        let ((), said) = self.with_world(tree, data, story, |flow, world| flow.update(world))?;
        let texts = (self.flow.questions().iter())
            .map(|q| q.text.clone())
            .collect();
        Ok((Some(texts), said))
    }

    /// A click on question `index` (3.5; DG-6 3.4, DG-4 5.3): the dialog
    /// started or the phrase said, the lines said.
    pub fn click(
        &mut self,
        tree: &dyn Vfs,
        data: &GameData,
        index: usize,
        story: &mut Story,
    ) -> io::Result<Vec<Said>> {
        let Some(id) = self.flow.questions().get(index).map(|q| q.id.clone()) else {
            return Ok(Vec::new());
        };
        let ((), said) =
            self.with_world(tree, data, story, |flow, world| flow.click(world, &id))?;
        Ok(said)
    }

    /// The talk window hidden by any route (5.2): the dialog in progress
    /// dropped (DG-4 7), and the player character's side stopped while it
    /// talks.
    pub fn hidden(&mut self, sides: &mut dyn TalkSides, player: u16) {
        self.flow.close();
        self.flow.set_shown(false);
        if sides.is_talking(player) {
            sides.stop_talk(player);
        }
    }

    /// What the window's update tests (5.2) for `player` and `partner`,
    /// their positions where they exist.
    pub fn speakers(
        player_talking: bool,
        positions: [Option<Vec3>; 2],
        special_mode: bool,
    ) -> Speakers {
        let distance = match positions {
            [Some(a), Some(b)] => {
                let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
                (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt()
            }
            _ => f32::INFINITY,
        };
        Speakers {
            player_talking,
            both_exist: positions.iter().all(Option::is_some),
            distance,
            special_mode,
        }
    }

    /// The talk as tests see it.
    pub fn report(&self) -> TalkReport {
        TalkReport {
            picked: self.picked.map(|(id, _)| id),
            talking: self.talking.clone(),
            partner: self.partner,
            dialog: self.flow.current().map(|d| d.dialog().id.clone()),
            first_speaker: self.flow.current().map(|d| d.first_speaker()),
            choices: (self.flow.questions().iter())
                .map(|q| q.id.clone())
                .collect(),
        }
    }
}

/// The talks as the engine interface reports them.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct TalkReport {
    /// The character under the crosshair within 2 m.
    pub picked: Option<u16>,
    /// The characters talking.
    pub talking: Vec<u16>,
    /// The player character's partner.
    pub partner: Option<u16>,
    /// The dialog in progress and its first speaker.
    pub dialog: Option<String>,
    pub first_speaker: Option<u16>,
    /// What the questions listed stand for: dialog or phrase ids.
    pub choices: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pick_meets_a_sphere_ahead() {
        let eye = [0.0, 1.5, 0.0];
        assert_eq!(
            ray_sphere(eye, [0.0, 0.0, 1.0], [0.0, 1.5, 2.0, 0.5]),
            Some(1.5)
        );
        assert_eq!(
            ray_sphere(eye, [0.0, 0.0, -1.0], [0.0, 1.5, 2.0, 0.5]),
            None
        );
        assert_eq!(ray_sphere(eye, [1.0, 0.0, 0.0], [0.0, 1.5, 2.0, 0.5]), None);
        assert_eq!(
            ray_sphere(eye, [0.0, 0.0, 1.0], [0.0, 1.5, 0.2, 0.5]),
            Some(0.0)
        );
        let mut t = LevelTalk {
            take_dist: 2.0,
            ..LevelTalk::default()
        };
        let c = |id, z| Candidate {
            id,
            sphere: [0.0, 1.5, z, 0.5],
            position: [0.0, 0.0, z],
            health: 1.0,
        };
        t.pick(eye, [0.0, 0.0, 1.0], &[c(3, 2.2), c(4, 1.0), c(5, 3.0)]);
        assert_eq!(t.picked(), Some((4, 0.5)), "the nearest, at least 0.5 m");
        t.pick(eye, [0.0, 0.0, 1.0], &[c(5, 3.0)]);
        assert_eq!(t.picked(), None, "2 m or farther");
    }

    /// talk.md 5.1, 5.2: hiding stops the player character's side, the
    /// partner's at its scheduled update; beyond 3 m the window closes.
    #[test]
    fn hiding_ends_the_players_side() {
        let mut t = LevelTalk::default();
        let mut sides = Talks::default();
        assert!(sides.offer(84, 1.0, 0, 1.0));
        let s = LevelTalk::speakers(true, [Some([0.0; 3]), Some([0.0, 0.0, 2.9])], false);
        assert!(!s.window_closes());
        let far = LevelTalk::speakers(true, [Some([0.0; 3]), Some([0.0, 0.0, 3.1])], false);
        assert!(far.window_closes());
        assert!(LevelTalk::speakers(true, [Some([0.0; 3]), None], false).window_closes());
        t.hidden(&mut sides, 0);
        t.sync(&sides, &[0, 84], 0);
        assert_eq!(t.report().talking, [84], "the partner's side goes on");
        scheduled_update(&mut sides, 84, 1.0);
        t.sync(&sides, &[0, 84], 0);
        assert!(t.report().talking.is_empty(), "until its scheduled update");
    }
}
