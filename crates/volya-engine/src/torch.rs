// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The player's torches in a level (docs/behavior/torch.md, over
//! [`crate::core::torch`]): each torch item of the player character's
//! inventory is a torch spawned from its stored torch byte (3), attached
//! while it is the item in its own slot and its section is in the actor's
//! `attachable_items` (6), switched off when detached or when it leaves
//! the inventory (4), toggled by the `torch` command and its night vision
//! by `night_vision` (8, 15.3), placed each frame at the player's head and
//! aimed by the smoothed camera (7.4); its night-vision sounds (14) are
//! played by the app on the level's sounds ([`TorchSounds`]).
//!
//! [`TorchData`] is read with the level's objects (the torch model's
//! `torch_definition`, its guide bone, the player's bone it hangs on).
//!
//! Characters' torches have the same data and [`crate::core::torch::Torch`];
//! TODO(#223): they are switched by `sr_light.script` through
//! `enable_attachable_item` (10.2), which needs the script API, so none is
//! spawned yet.

use std::io;

use serde::Serialize;

use crate::core::audio::Category;
use crate::core::character::CharacterState;
use crate::core::character::inventory::{ItemState, Place};
use crate::core::config::PathAliases;
use crate::core::config::{self, value};
use crate::core::glows::Glows;
use crate::core::lights::Lights;
use crate::core::sound::Fatal;
use crate::core::sound::system::{PlayFlags, SoundId, SoundSystem};
use crate::core::torch::{
    self, Aim, Mat4, NightVisionSection, NightVisionSound, Owner, OwnerPose, SoundCommand, Torch,
    TorchDefinition, TorchSection, Vec3,
};
use crate::core::vfs::{self as core_vfs, Vfs};
use crate::data::GameData;
use crate::objects;
use volya_engine_formats::lanims::ColourAnimations;
use volya_engine_formats::model::{self, Bone, Header};
use volya_engine_formats::object::Kind;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The outfit slot (player-state.md 3.3; torch.md 15.1).
pub const OUTFIT_SLOT: usize = 6;

/// What a torch section needs, read with the level.
#[derive(Clone, Debug)]
pub struct TorchData {
    pub section: String,
    pub definition: TorchDefinition,
    pub item: TorchSection,
    /// G: the guide bone's model-space matrix at rest (7).
    pub guide: Mat4,
    /// O (6).
    pub offset: Mat4,
    /// B: the player's bone `attach_bone_name`, at rest (TODO(#362): no
    /// skeletal animation computes it).
    pub owner_bone: Mat4,
    /// The glow's shader index, once resolved with the level's textures.
    pub glow_shader: u32,
}

/// The `torch_definition` of the torch model at `path` (1.2): its user data
/// read as LTX text with includes resolved against `config\`. Without user
/// data, fatal.
pub fn read_definition(
    tree: &dyn Vfs,
    data: &GameData,
    path: &str,
    file: &[u8],
) -> io::Result<TorchDefinition> {
    let context = |e: io::Error| io::Error::new(e.kind(), format!("{path}: {e}"));
    let text = model::user_data(file).map_err(context)?;
    let text = text
        .filter(|t| !t.is_empty())
        .ok_or_else(|| invalid(torch::EMPTY_USER_DATA))?;
    let base = data.aliases.dir("$game_config$")?;
    let user = config::parse_in(tree, &text, base).map_err(context)?;
    let s = (user.section(torch::DEFINITION))
        .ok_or_else(|| invalid(format!("{path}: no [{}]", torch::DEFINITION)))?;
    TorchDefinition::read(s).map_err(context)
}

impl TorchData {
    /// Torch section `section` with the model `visual`, hanging on the
    /// player's model `player` (its bind pose).
    pub fn read(
        tree: &dyn Vfs,
        data: &GameData,
        dir: &str,
        section: &str,
        visual: &str,
        player: &[Bone],
    ) -> io::Result<Self> {
        let s = (data.system.section(section))
            .ok_or_else(|| invalid(format!("no item section [{section}]")))?;
        let item = TorchSection::read(s)?;
        let path = objects::locate_model(tree, data, dir, visual)?;
        let file = core_vfs::read(tree, &path)?;
        let definition = read_definition(tree, data, &path, &file)?;
        let context = |e: io::Error| io::Error::new(e.kind(), format!("{path}: {e}"));
        let (_, bones) = model::skeleton(&file).map_err(context)?;
        let bone = |bones: &[Bone], name: &str, what: &str| -> io::Result<Mat4> {
            let name = name.to_ascii_lowercase();
            let i = (bones.iter().position(|b| b.name == name))
                .ok_or_else(|| invalid(format!("{what} has no bone {name}")))?;
            Ok(model::bind_pose(bones)?[i])
        };
        let guide = bone(&bones, &definition.guide_bone, &path)?;
        let owner_bone = bone(player, &item.attach_bone_name, "the player's model")?;
        Ok(Self {
            section: section.to_owned(),
            offset: item.offset(),
            definition,
            item,
            guide,
            owner_bone,
            glow_shader: 0,
        })
    }
}

/// The torch data for every torch of the player character's inventory, by
/// section, with the player's model `player_visual`.
pub fn read_player_torches(
    tree: &dyn Vfs,
    data: &GameData,
    dir: &str,
    character: &CharacterState,
    player_visual: &str,
) -> io::Result<PlayerTorchData> {
    let mut torches: Vec<TorchData> = Vec::new();
    let mut player: Option<(Header, Vec<Bone>)> = None;
    for i in &character.inventory.items {
        if !matches!(i.state, ItemState::Torch { .. })
            || torches.iter().any(|t| t.section == i.config.section)
        {
            continue;
        }
        if player.is_none() {
            let path = objects::locate_model(tree, data, dir, player_visual)?;
            let file = core_vfs::read(tree, &path)?;
            player = Some(
                model::skeleton(&file)
                    .map_err(|e| io::Error::new(e.kind(), format!("{path}: {e}")))?,
            );
        }
        let p = &player.as_ref().expect("read above").1;
        torches.push(TorchData::read(
            tree,
            data,
            dir,
            &i.config.section,
            &i.config.visual,
            p,
        )?);
    }
    // 6: the actor's `attachable_items`.
    let attachable = (data.system.get(
        volya_engine_formats::spawn::ACTOR_SECTION,
        "attachable_items",
    ))
    .flatten()
    .map(|v| value::list(v).map(str::to_owned).collect())
    .unwrap_or_default();
    let sphere = player.map_or(([0.0; 3], 0.0), |(h, _)| (h.sphere_centre, h.sphere_radius));
    Ok(PlayerTorchData {
        torches,
        attachable,
        sphere,
    })
}

/// The torch data a level reads.
#[derive(Clone, Debug, Default)]
pub struct PlayerTorchData {
    pub torches: Vec<TorchData>,
    /// `[actor] attachable_items`.
    pub attachable: Vec<String>,
    /// The player's model's bounding sphere in model space.
    pub sphere: (Vec3, f32),
}

/// One of the player's torches.
#[derive(Clone, Debug)]
struct PlayerTorch {
    /// Its item.
    id: u16,
    /// Its [`TorchData`].
    data: usize,
    torch: Torch,
    attached: bool,
}

/// A night-vision sound command for torch `torch`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TorchSound {
    pub torch: u16,
    pub command: SoundCommand,
}

/// The torch as the engine interface reports it.
#[derive(Clone, Debug, Serialize)]
pub struct TorchReport {
    pub id: u16,
    pub section: String,
    pub on: bool,
    pub attached: bool,
    pub cone_visible: bool,
    pub main_active: bool,
    pub second_active: bool,
    pub glow_on: bool,
    pub night_vision_requested: bool,
    /// The active night-vision effect's section.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub night_vision_effect: Option<String>,
    pub byte: u8,
}

/// The player's torches.
#[derive(Clone, Debug, Default)]
pub struct PlayerTorches {
    data: PlayerTorchData,
    /// The level's name (`disabled_maps`, 15.2).
    level: String,
    torches: Vec<PlayerTorch>,
    sounds: Vec<TorchSound>,
}

/// The effect section of the outfit in slot 6 (15.1).
fn outfit_effect(character: &CharacterState) -> Option<&str> {
    let inv = &character.inventory;
    let id = inv.slots[OUTFIT_SLOT]?;
    let item = inv.items.iter().find(|i| i.id == id)?;
    if item.config.class_id.kind() != Kind::Outfit {
        return None;
    }
    item.config.nightvision_sect.as_deref()
}

/// The player never withholds torch lights (4); the setting is read only
/// for characters.
const PLAYER_SETTING: bool = true;
const PLAYER: Option<Owner> = Some(Owner::Player);

impl PlayerTorches {
    pub fn new(data: PlayerTorchData, level: &str) -> Self {
        Self {
            data,
            level: level.to_owned(),
            ..Self::default()
        }
    }

    /// The torch data of every section, to resolve their glows' shaders.
    pub fn data_mut(&mut self) -> &mut [TorchData] {
        &mut self.data.torches
    }

    fn disabled(&self, data: usize) -> bool {
        (self.data.torches[data].item.night_vision.as_ref())
            .is_some_and(|nv| nv.disabled(&self.level))
    }

    /// The night-vision section of torch `id`.
    pub fn night_vision(&self, id: u16) -> Option<&NightVisionSection> {
        let t = self.torches.iter().find(|t| t.id == id)?;
        self.data.torches[t.data].item.night_vision.as_ref()
    }

    /// This frame's sound commands, taken.
    pub fn take_sounds(&mut self) -> Vec<TorchSound> {
        std::mem::take(&mut self.sounds)
    }

    /// Follows the inventory (3, 4, 6): a new torch item spawns from its
    /// torch byte; one that left the inventory leaves its owner and goes; a
    /// torch is attached while it is the item in its own slot and its
    /// section is attachable, and switched off when detached. The torch
    /// bytes are written back (9).
    // TODO(#362): a torch that leaves the inventory is not placed in the
    // world (dropped items are not, #111), so it goes with its lights.
    pub fn sync(&mut self, character: &mut CharacterState, lights: &mut Lights, glows: &mut Glows) {
        let outfit = outfit_effect(character).map(str::to_owned);
        let inv = &character.inventory;
        let mut k = 0;
        while k < self.torches.len() {
            if inv.items.iter().any(|i| i.id == self.torches[k].id) {
                k += 1;
                continue;
            }
            let mut t = self.torches.remove(k);
            let disabled = self.disabled(t.data);
            let mut out = Vec::new();
            t.torch.leave(
                PLAYER,
                PLAYER_SETTING,
                outfit.as_deref(),
                disabled,
                lights,
                glows,
                &mut out,
            );
            self.sounds
                .extend(out.into_iter().map(|command| TorchSound {
                    torch: t.id,
                    command,
                }));
            t.torch.destroy(lights, glows);
        }
        for i in &inv.items {
            let ItemState::Torch { torch_flags } = i.state else {
                continue;
            };
            if self.torches.iter().any(|t| t.id == i.id) {
                continue;
            }
            // TODO(#362): a torch section whose data was not read with the
            // level (one given later) is not a torch here.
            let Some(data) = self
                .data
                .torches
                .iter()
                .position(|d| d.section == i.config.section)
            else {
                continue;
            };
            let d = &self.data.torches[data];
            let torch = Torch::spawn(
                &d.definition,
                d.glow_shader,
                torch_flags,
                PLAYER,
                PLAYER_SETTING,
                d.item.night_vision.is_some(),
                lights,
                glows,
            );
            self.torches.push(PlayerTorch {
                id: i.id,
                data,
                torch,
                attached: false,
            });
        }
        let mut attached_sections: Vec<usize> = Vec::new();
        for t in &mut self.torches {
            let Some(item) = inv.items.iter().find(|i| i.id == t.id) else {
                continue;
            };
            let own_slot = item.config.slot_number();
            let in_slot = matches!(item.place, Place::Slot(s) if Some(s) == own_slot)
                && own_slot.is_some_and(|s| inv.slots[usize::from(s)] == Some(t.id));
            let can = in_slot
                && self.data.attachable.contains(&item.config.section)
                && !attached_sections.contains(&t.data);
            if t.attached && !can {
                t.torch.switch(false, PLAYER, PLAYER_SETTING, lights, glows);
            }
            t.attached = can;
            if can {
                attached_sections.push(t.data);
            }
        }
        for t in &self.torches {
            let byte = t.torch.byte(t.attached);
            if let Some(item) = character.inventory.items.iter_mut().find(|i| i.id == t.id) {
                item.state = ItemState::Torch { torch_flags: byte };
            }
        }
    }

    /// The `torch` command (8): the first attached torch toggled, while the
    /// player's health is above 0.
    pub fn torch_command(&mut self, alive: bool, lights: &mut Lights, glows: &mut Glows) {
        if !alive {
            return;
        }
        if let Some(t) = self.torches.iter_mut().find(|t| t.attached) {
            let on = !t.torch.on;
            t.torch.switch(on, PLAYER, PLAYER_SETTING, lights, glows);
        }
    }

    /// The `night_vision` command (15.3): the first attached torch's request
    /// toggled, under the `torch` command's conditions.
    pub fn night_vision_command(&mut self, alive: bool, character: &CharacterState) {
        if !alive {
            return;
        }
        let outfit = outfit_effect(character);
        let Some(k) = self.torches.iter().position(|t| t.attached) else {
            return;
        };
        let disabled = self.disabled(self.torches[k].data);
        let t = &mut self.torches[k];
        let Some(q) = t.torch.night_vision.as_ref().map(|n| !n.requested) else {
            return;
        };
        let mut out = Vec::new();
        t.torch
            .switch_night_vision(q, PLAYER, outfit, disabled, &mut out);
        let id = t.id;
        self.sounds.extend(
            out.into_iter()
                .map(|command| TorchSound { torch: id, command }),
        );
    }

    /// A frame of `dt` seconds (7): the torches that are on placed at the
    /// player's head with its feet at `feet`, aimed by `aim`, seen from
    /// `eye`; coloured at clock `t` seconds by their animators in
    /// `animations` (7.5; the game clock of lanims-xr.md 6.2, taken as the
    /// global clock the lamps run on).
    // TODO(#362): the player's transform turns with the camera's yaw (its
    // body's orientation is not specified) and its bones are at rest.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        feet: Vec3,
        aim: Aim,
        eye: Vec3,
        dt: f32,
        t: f32,
        animations: &ColourAnimations,
        lights: &mut Lights,
        glows: &mut Glows,
    ) {
        let transform = model::transform(feet, [0.0, -aim.yaw, 0.0]);
        let (centre, radius) = self.data.sphere;
        let centre = model::transform_point(&transform, centre);
        for p in &mut self.torches {
            let d = &self.data.torches[p.data];
            let pose = OwnerPose {
                owner: Owner::Player,
                transform,
                bone: d.owner_bone,
                centre,
                radius,
            };
            let animation = (animations.find(d.definition.colour_animator.as_bytes()))
                .and_then(|i| animations.entries.get(i))
                .map(|e| (e, t));
            p.torch.frame(
                &d.guide,
                &d.offset,
                &pose,
                eye,
                aim,
                dt,
                PLAYER_SETTING,
                animation,
                lights,
                glows,
            );
        }
    }

    /// What the page's touch Torch and Night vision buttons show (our
    /// design, #363), each `None` while its command would do nothing, else
    /// whether it is on: the torch while one is attached (the command
    /// reaches the first, 8); its night vision while that torch has it and
    /// its effect is active or the worn outfit has an effect section (15.2:
    /// without one a press only toggles the request).
    pub fn buttons(&self, character: &CharacterState) -> (Option<bool>, Option<bool>) {
        let Some(t) = self.torches.iter().find(|t| t.attached) else {
            return (None, None);
        };
        let night = (t.torch.night_vision.as_ref())
            .filter(|n| n.effect.is_some() || outfit_effect(character).is_some())
            .map(|n| n.effect.is_some());
        (Some(t.torch.on), night)
    }

    /// The first attached torch, else the first, as the engine interface
    /// reports it.
    pub fn report(&self, lights: &Lights, glows: &Glows) -> Option<TorchReport> {
        let t = (self.torches.iter().find(|t| t.attached)).or(self.torches.first())?;
        let nv = t.torch.night_vision.as_ref();
        Some(TorchReport {
            id: t.id,
            section: self.data.torches[t.data].section.clone(),
            on: t.torch.on,
            attached: t.attached,
            cone_visible: t.torch.cone_visible,
            main_active: lights.get(t.torch.main).is_some_and(|r| r.active),
            second_active: lights.get(t.torch.second).is_some_and(|r| r.active),
            glow_on: glows.get(t.torch.glow).is_some_and(|g| g.on),
            night_vision_requested: nv.is_some_and(|n| n.requested),
            night_vision_effect: nv.and_then(|n| n.effect.clone()),
            byte: t.torch.byte(t.attached),
        })
    }

    /// The torches' records, for tests: (main light, second light, glow)
    /// of the first attached torch.
    pub fn records(
        &self,
    ) -> Option<(
        crate::core::lights::LightId,
        crate::core::lights::LightId,
        crate::core::glows::GlowId,
    )> {
        let t = self.torches.iter().find(|t| t.attached)?;
        Some((t.torch.main, t.torch.second, t.torch.glow))
    }
}

/// A torch's night-vision sounds in the sound system (14): each sound's
/// variants, in the effects category, and its current emitter's sound.
#[derive(Debug)]
pub struct TorchSounds {
    sounds: Vec<Vec<(SoundId, torch::Variant)>>,
    current: [Option<SoundId>; 4],
}

impl TorchSounds {
    /// Creates the sounds of `section`: fatal when a file cannot be opened
    /// (S1 section 5); the caller discards what was made on failure.
    pub fn create(
        system: &mut SoundSystem,
        tree: &dyn Vfs,
        aliases: &PathAliases,
        section: &NightVisionSection,
    ) -> Result<Self, Fatal> {
        let sounds = NightVisionSound::ALL
            .iter()
            .map(|&s| {
                (section.variants(s).iter())
                    .map(|v| {
                        Ok((
                            system.create(tree, aliases, &v.name, Category::Effects)?,
                            v.clone(),
                        ))
                    })
                    .collect::<Result<Vec<_>, Fatal>>()
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            sounds,
            current: [None; 4],
        })
    }

    /// Carries out `command` (14): playing stops the sound's current emitter
    /// at once, chooses a variant at random and plays it at the owner's
    /// position, 2D at (0, 0, 0) for the player, with the variant's delay
    /// and volume; stopping stops it at once.
    // TODO(#147): the sounds' value 0x40400000 for the reports to the AI is
    // S24's, whose reports do not exist.
    pub fn apply(&mut self, system: &mut SoundSystem, command: SoundCommand) {
        let (sound, play) = match command {
            SoundCommand::Play { sound, looped } => (sound, Some(looped)),
            SoundCommand::Stop(sound) => (sound, None),
        };
        let k = sound as usize;
        if let Some(id) = self.current[k].take() {
            system.stop(id);
        }
        let Some(looped) = play else { return };
        let variants = &self.sounds[k];
        if variants.is_empty() {
            return;
        }
        let r = system.random().draw() as usize;
        let (id, v) = &variants[r % variants.len()];
        system.play_at(
            *id,
            [0.0; 3],
            PlayFlags {
                looped,
                two_d: true,
                delay: v.delay,
            },
        );
        system.set_play_volume(*id, v.volume);
        self.current[k] = Some(*id);
    }

    pub fn discard(&self, system: &mut SoundSystem) {
        for (id, _) in self.sounds.iter().flatten() {
            system.discard(*id);
        }
    }
}
