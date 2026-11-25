// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! State and update layouts per kind (docs/formats/sav.md, section 7.4.2),
//! with the scripted additions of section 7.5.

use std::io;

use super::Context;
use super::blocks::*;
use crate::packet::{Reader, Writer, invalid};

/// Scripted additions of `AI_STL_S` and `SM_*` (section 7.5). Each field is
/// read only from script versions that have it (absent: `None`); the writer
/// writes what is present (the game's writer always writes all).
#[derive(Clone, Debug, PartialEq)]
pub struct CharacterScript {
    pub job_online: Option<JobOnline>,
    pub was_in_smart_terrain: Option<bool>,
    /// `AI_STL_S` only.
    pub death_items_dropped: Option<bool>,
}

/// Job-online mode `j` (0, 1, 2; 3 = condition list).
#[derive(Clone, Debug, PartialEq)]
pub struct JobOnline {
    pub mode: u8,
    /// Present iff `mode` is 3.
    pub condition_list: Option<String>,
}

/// Job-online mode followed by a condition list.
const JOB_ONLINE_CONDITION_LIST: u8 = 3;

/// Which of the fields of [`CharacterScript`] a record has: job online,
/// was in a smart terrain, death items dropped (section 7.5). `stalker`: the
/// stalker's script-version thresholds (3, 5, 6), else the monster's (4, 5,
/// no last field).
fn character_fields(cx: &Context, stalker: bool) -> [bool; 3] {
    let v = cx.script_version;
    [v >= if stalker { 3 } else { 4 }, v >= 5, stalker && v >= 6]
}

/// Whether the class id has the scripted stalker or monster additions.
fn has_character_script(cx: &Context, stalker: bool) -> bool {
    if stalker {
        cx.class_id == "AI_STL_S"
    } else {
        cx.class_id.starts_with("SM_")
    }
}

impl CharacterScript {
    fn read(r: &mut Reader, cx: &Context, stalker: bool) -> io::Result<Self> {
        let [job, smart, death] = character_fields(cx, stalker);
        let job_online = if job {
            let mode = r.u8()?;
            let condition_list = if mode == JOB_ONLINE_CONDITION_LIST {
                Some(r.string()?)
            } else {
                None
            };
            Some(JobOnline {
                mode,
                condition_list,
            })
        } else {
            None
        };
        let was_in_smart_terrain = if smart { Some(r.bool8()?) } else { None };
        let death_items_dropped = if death { Some(r.bool8()?) } else { None };
        Ok(Self {
            job_online,
            was_in_smart_terrain,
            death_items_dropped,
        })
    }

    /// The fields present must be those the record's class id and script
    /// version call for.
    fn check(script: &Option<Self>, cx: &Context, stalker: bool) -> io::Result<()> {
        let expected = has_character_script(cx, stalker);
        match script {
            None if !expected => Ok(()),
            Some(s) if expected => {
                let present = [
                    s.job_online.is_some(),
                    s.was_in_smart_terrain.is_some(),
                    s.death_items_dropped.is_some(),
                ];
                if present == character_fields(cx, stalker) {
                    Ok(())
                } else {
                    Err(invalid(format!(
                        "scripted fields {present:?} do not match script version {}",
                        cx.script_version
                    )))
                }
            }
            _ => Err(invalid(format!(
                "scripted additions {} for this class id",
                if expected { "missing" } else { "not expected" }
            ))),
        }
    }

    fn write(&self, w: &mut Writer) -> io::Result<()> {
        // A later field is read only after the earlier ones.
        if (self.was_in_smart_terrain.is_some() && self.job_online.is_none())
            || (self.death_items_dropped.is_some() && self.was_in_smart_terrain.is_none())
        {
            return Err(invalid("a scripted field without the fields before it"));
        }
        if let Some(j) = &self.job_online {
            w.u8(j.mode);
            if (j.mode == JOB_ONLINE_CONDITION_LIST) != j.condition_list.is_some() {
                return Err(invalid("job-online mode disagrees with its condition list"));
            }
            if let Some(list) = &j.condition_list {
                w.string(list)?;
            }
        }
        if let Some(b) = self.was_in_smart_terrain {
            w.bool8(b);
        }
        if let Some(b) = self.death_items_dropped {
            w.bool8(b);
        }
        Ok(())
    }
}

/// Scripted addition of `ZS_*` anomalies, in single-player games only.
#[derive(Clone, Debug, PartialEq)]
pub struct ArtefactSpawnScript {
    /// `u8` `t`; the time follows iff `t` = 1.
    pub has_last_spawn: u8,
    pub last_spawn: Option<Option<ScriptTime>>,
}

impl ArtefactSpawnScript {
    fn read(r: &mut Reader) -> io::Result<Self> {
        let has_last_spawn = r.u8()?;
        let last_spawn = if has_last_spawn == 1 {
            Some(read_script_time(r)?)
        } else {
            None
        };
        Ok(Self {
            has_last_spawn,
            last_spawn,
        })
    }

    fn write(&self, w: &mut Writer) -> io::Result<()> {
        if (self.has_last_spawn == 1) != self.last_spawn.is_some() {
            return Err(invalid("artefact spawn flag disagrees with its time"));
        }
        w.u8(self.has_last_spawn);
        if let Some(t) = &self.last_spawn {
            write_script_time(w, t)?;
        }
        Ok(())
    }
}

/// Whether the record has the `ZS_*` addition.
fn has_artefact_spawn(cx: &Context) -> bool {
    cx.single_player && cx.class_id.starts_with("ZS_")
}

fn check_artefact_spawn(script: &Option<ArtefactSpawnScript>, cx: &Context) -> io::Result<()> {
    if script.is_some() == has_artefact_spawn(cx) {
        Ok(())
    } else {
        Err(invalid(
            "the artefact-spawn addition disagrees with the class id or single-player",
        ))
    }
}

fn artefact_spawn(r: &mut Reader, cx: &Context) -> io::Result<Option<ArtefactSpawnScript>> {
    if has_artefact_spawn(cx) {
        ArtefactSpawnScript::read(r).map(Some)
    } else {
        Ok(None)
    }
}

fn write_opt<T>(
    w: &mut Writer,
    v: &Option<T>,
    write: impl FnOnce(&T, &mut Writer) -> io::Result<()>,
) -> io::Result<()> {
    v.as_ref().map_or(Ok(()), |v| write(v, w))
}

/// Player (`O_ACTOR`).
#[derive(Clone, Debug, PartialEq)]
pub struct Player {
    pub creature: Creature,
    pub trader: TraderLike,
    pub skeleton: Skeleton,
    /// Vehicle or mounted weapon in use, 0xFFFF none.
    pub holder_id: u16,
    pub update: PlayerUpdate,
}

/// The player's update: Creature update, then the player block.
#[derive(Clone, Debug, PartialEq)]
pub struct PlayerUpdate {
    pub creature: CreatureUpdate,
    pub movement_state: u16,
    pub acceleration: CompressedVector,
    pub velocity: CompressedVector,
    pub radiation: f32,
    pub active_weapon: u8,
    pub physics: PlayerPhysics,
}

/// A vector as a compressed direction (encoding unestablished, kept raw) and
/// a length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CompressedVector {
    pub direction: u16,
    pub magnitude: f32,
}

impl CompressedVector {
    fn read(r: &mut Reader) -> io::Result<Self> {
        Ok(Self {
            direction: r.u16()?,
            magnitude: r.float()?,
        })
    }

    fn write(&self, w: &mut Writer) {
        w.u16(self.direction);
        w.float(self.magnitude);
    }
}

/// The player's physics, by element count `m` (meanings unestablished).
#[derive(Clone, Debug, PartialEq)]
pub enum PlayerPhysics {
    /// `m` = 0.
    None,
    /// `m` = 1: `u8` enabled, 5 × `float3`, 4 × `float`.
    State {
        enabled: u8,
        vectors: [[f32; 3]; 5],
        floats: [f32; 4],
    },
    /// `m` > 1: `u8` `e`, then `m · e + 24` bytes.
    Interpolation { count: u16, e: u8, data: Vec<u8> },
}

impl PlayerUpdate {
    fn read(r: &mut Reader) -> io::Result<Self> {
        let creature = CreatureUpdate::read(r)?;
        let movement_state = r.u16()?;
        let acceleration = CompressedVector::read(r)?;
        let velocity = CompressedVector::read(r)?;
        let radiation = r.float()?;
        let active_weapon = r.u8()?;
        let physics = match r.u16()? {
            0 => PlayerPhysics::None,
            1 => PlayerPhysics::State {
                enabled: r.u8()?,
                vectors: [
                    r.float3()?,
                    r.float3()?,
                    r.float3()?,
                    r.float3()?,
                    r.float3()?,
                ],
                floats: [r.float()?, r.float()?, r.float()?, r.float()?],
            },
            count => {
                let e = r.u8()?;
                let data = r.take(usize::from(count) * usize::from(e) + 24)?.to_vec();
                PlayerPhysics::Interpolation { count, e, data }
            }
        };
        Ok(Self {
            creature,
            movement_state,
            acceleration,
            velocity,
            radiation,
            active_weapon,
            physics,
        })
    }

    fn write(&self, w: &mut Writer) -> io::Result<()> {
        self.creature.write(w);
        w.u16(self.movement_state);
        self.acceleration.write(w);
        self.velocity.write(w);
        w.float(self.radiation);
        w.u8(self.active_weapon);
        match &self.physics {
            PlayerPhysics::None => w.u16(0),
            PlayerPhysics::State {
                enabled,
                vectors,
                floats,
            } => {
                w.u16(1);
                w.u8(*enabled);
                vectors.iter().for_each(|v| w.float3(*v));
                floats.iter().for_each(|f| w.float(*f));
            }
            PlayerPhysics::Interpolation { count, e, data } => {
                if *count < 2 || data.len() != usize::from(*count) * usize::from(*e) + 24 {
                    return Err(invalid("player interpolation data of the wrong size"));
                }
                w.u16(*count);
                w.u8(*e);
                w.bytes(data);
            }
        }
        Ok(())
    }
}

impl Player {
    fn read(s: &mut Reader, u: &mut Reader) -> io::Result<Self> {
        Ok(Self {
            creature: Creature::read(s)?,
            trader: TraderLike::read(s)?,
            skeleton: Skeleton::read(s)?,
            holder_id: s.u16()?,
            update: PlayerUpdate::read(u)?,
        })
    }

    fn write(&self, s: &mut Writer, u: &mut Writer) -> io::Result<()> {
        self.creature.write(s)?;
        self.trader.write(s)?;
        self.skeleton.write(s)?;
        s.u16(self.holder_id);
        self.update.write(u)
    }
}

/// Stalker (`AI_STL`, `AI_STL_S`).
#[derive(Clone, Debug, PartialEq)]
pub struct Stalker {
    pub trader: TraderLike,
    pub monster: MonsterLike,
    pub equipment_preferences: Vec<u8>,
    pub main_weapon_preferences: Vec<u8>,
    pub skeleton: Skeleton,
    /// `AI_STL_S` only.
    pub script: Option<CharacterScript>,
    pub update: MonsterLikeUpdate,
    pub start_dialog: String,
}

impl Stalker {
    fn read(s: &mut Reader, u: &mut Reader, cx: &Context) -> io::Result<Self> {
        Ok(Self {
            trader: TraderLike::read(s)?,
            monster: MonsterLike::read(s)?,
            equipment_preferences: bytes_list(s)?,
            main_weapon_preferences: bytes_list(s)?,
            skeleton: Skeleton::read(s)?,
            script: if has_character_script(cx, true) {
                Some(CharacterScript::read(s, cx, true)?)
            } else {
                None
            },
            update: MonsterLikeUpdate::read(u)?,
            start_dialog: u.string()?,
        })
    }

    fn write(&self, s: &mut Writer, u: &mut Writer) -> io::Result<()> {
        self.trader.write(s)?;
        self.monster.write(s)?;
        write_bytes_list(s, &self.equipment_preferences)?;
        write_bytes_list(s, &self.main_weapon_preferences)?;
        self.skeleton.write(s)?;
        write_opt(s, &self.script, CharacterScript::write)?;
        self.update.write(u);
        u.string(&self.start_dialog)
    }
}

/// Trader (`AI_TRADE`, `AI_TRD_S`).
#[derive(Clone, Debug, PartialEq)]
pub struct Trader {
    pub object: ObjectBlock,
    pub visual: Visual,
    pub trader: TraderLike,
}

/// The monster state: Monster-like, Skeleton, special object id.
#[derive(Clone, Debug, PartialEq)]
pub struct MonsterState {
    pub monster: MonsterLike,
    pub skeleton: Skeleton,
    pub special_object_id: u16,
}

impl MonsterState {
    fn read(r: &mut Reader) -> io::Result<Self> {
        Ok(Self {
            monster: MonsterLike::read(r)?,
            skeleton: Skeleton::read(r)?,
            special_object_id: r.u16()?,
        })
    }

    fn write(&self, w: &mut Writer) -> io::Result<()> {
        self.monster.write(w)?;
        self.skeleton.write(w)?;
        w.u16(self.special_object_id);
        Ok(())
    }
}

/// Monster (`AI_FLESH`, …, `SM_*`).
#[derive(Clone, Debug, PartialEq)]
pub struct Monster {
    pub state: MonsterState,
    /// `SM_*` only.
    pub script: Option<CharacterScript>,
    pub update: MonsterLikeUpdate,
}

/// Crow or phantom (`AI_CROW`, `AI_PHANT`).
#[derive(Clone, Debug, PartialEq)]
pub struct CrowOrPhantom {
    pub creature: Creature,
    pub update: CreatureUpdate,
}

/// Vehicle (`C_NIVA`, `SCRPTCAR`).
#[derive(Clone, Debug, PartialEq)]
pub struct Vehicle {
    pub object: ObjectBlock,
    pub visual: Visual,
    pub skeleton: Skeleton,
    pub health: f32,
}

/// Helicopter (`C_HLCPTR`, `C_HLCP_S`).
#[derive(Clone, Debug, PartialEq)]
pub struct Helicopter {
    pub object: ObjectBlock,
    pub visual: Visual,
    pub motion: String,
    pub skeleton: Skeleton,
    pub startup_animation: String,
    pub engine_sound: String,
}

/// Item (scopes, medkits, artefacts, …).
#[derive(Clone, Debug, PartialEq)]
pub struct PlainItem {
    pub item: Item,
    pub update: ItemUpdate,
}

/// Torch (`D_TORCH`, `TORCH_S`).
#[derive(Clone, Debug, PartialEq)]
pub struct Torch {
    pub item: Item,
    pub update: ItemUpdate,
    /// Bit 0: on, bit 1: night vision on, bit 2: unestablished.
    pub torch_flags: u8,
}

/// PDA (`D_PDA`).
#[derive(Clone, Debug, PartialEq)]
pub struct Pda {
    pub item: Item,
    pub original_owner_id: u16,
    pub specific_character: String,
    pub info_portion: String,
    pub update: ItemUpdate,
}

/// Document (`II_DOC`).
#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub item: Item,
    pub info_portion: String,
    pub update: ItemUpdate,
}

/// Outfit (`EQU_*`, `E_STLK`).
#[derive(Clone, Debug, PartialEq)]
pub struct Outfit {
    pub item: Item,
    pub update: ItemUpdate,
    /// `q8(condition, 0, 1)`.
    pub condition: Q8,
}

/// Ammunition (`AMMO`, `A_*`).
#[derive(Clone, Debug, PartialEq)]
pub struct Ammunition {
    pub item: Item,
    pub rounds_left: u16,
    pub update: ItemUpdate,
    pub update_rounds_left: u16,
}

/// Knife (`W_KNIFE`, `WP_KNIFE`).
#[derive(Clone, Debug, PartialEq)]
pub struct Knife {
    pub weapon: WeaponState,
    pub update: WeaponUpdate,
}

/// Magazine weapon.
#[derive(Clone, Debug, PartialEq)]
pub struct MagazineWeapon {
    pub weapon: WeaponState,
    pub update: WeaponUpdate,
    pub fire_mode: u8,
}

/// Weapon with grenade launcher.
#[derive(Clone, Debug, PartialEq)]
pub struct WeaponWithGrenadeLauncher {
    pub weapon: WeaponState,
    pub grenade_mode: bool,
    pub update: WeaponUpdate,
    pub fire_mode: u8,
}

/// Shotgun.
#[derive(Clone, Debug, PartialEq)]
pub struct Shotgun {
    pub weapon: WeaponState,
    pub update: WeaponUpdate,
    pub fire_mode: u8,
    /// Ammo types of the loaded rounds (`u8` count).
    pub loaded_ammo_types: Vec<u8>,
}

/// Anomaly (`Z_MBALD`, …, `ZS_*`).
#[derive(Clone, Debug, PartialEq)]
pub struct AnomalyObject {
    pub anomaly: Anomaly,
    /// `ZS_MBALD`, `ZS_GALAN`, `ZS_MINCE` in single-player games.
    pub script: Option<ArtefactSpawnScript>,
}

/// Visual anomaly (`Z_BFUZZ`, `Z_RUSTYH`, `Z_AMEBA`, `ZS_BFUZZ`).
#[derive(Clone, Debug, PartialEq)]
pub struct VisualAnomaly {
    pub anomaly: Anomaly,
    pub visual: Visual,
    pub idle_animation: String,
    pub attack_animation: String,
    /// `ZS_BFUZZ` in single-player games.
    pub script: Option<ArtefactSpawnScript>,
}

/// Torrid zone (`Z_TORRID`).
#[derive(Clone, Debug, PartialEq)]
pub struct TorridZone {
    pub zone: Zone,
    pub motion: String,
}

/// Level changer (`LVL_CHNG`).
#[derive(Clone, Debug, PartialEq)]
pub struct LevelChanger {
    pub object: ObjectBlock,
    pub shapes: Vec<Shape>,
    pub restrictor_type: u8,
    pub destination_game_vertex: u16,
    pub destination_level_node: u32,
    pub destination_position: [f32; 3],
    pub destination_direction: [f32; 3],
    pub destination_level: String,
    pub destination_graph_point: String,
    pub silent: bool,
}

/// Restrictor (`SCRIPTZN`, `SPACE_RS`, `SPC_RS_S`) and climbable's
/// relatives: Object, Shape, restrictor type.
#[derive(Clone, Debug, PartialEq)]
pub struct Restrictor {
    pub object: ObjectBlock,
    pub shapes: Vec<Shape>,
    pub restrictor_type: u8,
}

impl Restrictor {
    fn read(r: &mut Reader) -> io::Result<Self> {
        Ok(Self {
            object: ObjectBlock::read(r)?,
            shapes: read_shapes(r)?,
            restrictor_type: r.u8()?,
        })
    }

    fn write(&self, w: &mut Writer) -> io::Result<()> {
        self.object.write(w)?;
        write_shapes(w, &self.shapes)?;
        w.u8(self.restrictor_type);
        Ok(())
    }
}

/// Smart zone (`SMRTZONE`, `SMRTTRRN`, `RE_SPAWN`).
#[derive(Clone, Debug, PartialEq)]
pub struct SmartZone {
    pub restrictor: Restrictor,
    pub script: SmartZoneScript,
}

/// The scripted additions of a smart zone by class id.
#[derive(Clone, Debug, PartialEq)]
pub enum SmartZoneScript {
    /// `SMRTZONE`.
    None,
    /// `SMRTTRRN`.
    SmartTerrain(SmartTerrain),
    /// `RE_SPAWN`: ids of objects it spawned (`u8` count).
    Respawn(Vec<u16>),
}

/// The smart-terrain fields (section 7.5).
#[derive(Clone, Debug, PartialEq)]
pub struct SmartTerrain {
    pub duration_end: Option<ScriptTime>,
    pub idle_end: Option<ScriptTime>,
    /// `None` when not working (`w` = 0). When working, the rest of the
    /// object state as stored: its gulag states are defined by the shipped
    /// scripts, not the spec, so they are kept raw (our choice).
    pub working: Option<Vec<u8>>,
}

impl SmartTerrain {
    fn read(r: &mut Reader) -> io::Result<Self> {
        let duration_end = read_script_time(r)?;
        let idle_end = read_script_time(r)?;
        let working = if r.bool8()? {
            Some(r.take(r.rest().len())?.to_vec())
        } else {
            None
        };
        Ok(Self {
            duration_end,
            idle_end,
            working,
        })
    }

    fn write(&self, w: &mut Writer) -> io::Result<()> {
        write_script_time(w, &self.duration_end)?;
        write_script_time(w, &self.idle_end)?;
        w.bool8(self.working.is_some());
        if let Some(rest) = &self.working {
            w.bytes(rest);
        }
        Ok(())
    }
}

/// Team base (`Z_TEAMBS`).
#[derive(Clone, Debug, PartialEq)]
pub struct TeamBase {
    pub restrictor: Restrictor,
    pub team: u8,
}

/// Plain visual object (`O_SEARCH`, `W_MOUNTD`, `SCRPTOBJ`, `O_INVBOX`).
#[derive(Clone, Debug, PartialEq)]
pub struct PlainVisual {
    pub object: ObjectBlock,
    pub visual: Visual,
}

/// Stationary gun (`W_STMGUN`).
#[derive(Clone, Debug, PartialEq)]
pub struct StationaryGun {
    pub object: ObjectBlock,
    pub visual: Visual,
    pub working: bool,
    pub aim_direction: [f32; 3],
}

/// Hanging lamp (`O_HLAMP`).
#[derive(Clone, Debug, PartialEq)]
pub struct HangingLamp {
    pub object: ObjectBlock,
    pub visual: Visual,
    pub skeleton: Skeleton,
    pub colour: u32,
    pub brightness: f32,
    pub colour_animator: String,
    pub range: f32,
    pub light_flags: u16,
    pub startup_animation: String,
    pub fixed_bones: String,
    pub health: f32,
    pub virtual_size: f32,
    pub ambient_radius: f32,
    pub ambient_power: f32,
    pub ambient_texture: String,
    pub light_texture: String,
    pub light_bone: String,
    pub spot_cone_angle: f32,
    pub glow_texture: String,
    pub glow_radius: f32,
    pub ambient_light_bone: String,
}

/// Physics object (`O_PHYSIC`, `P_DSTRBL`, `O_PHYS_S`).
#[derive(Clone, Debug, PartialEq)]
pub struct PhysicsObject {
    pub object: ObjectBlock,
    pub visual: Visual,
    pub skeleton: Skeleton,
    pub physics_type: u32,
    pub mass: f32,
    pub fixed_bones: String,
}

/// Breakable (`O_BRKBL`).
#[derive(Clone, Debug, PartialEq)]
pub struct Breakable {
    pub object: ObjectBlock,
    pub visual: Visual,
    pub health: f32,
}

/// Climbable (`O_CLMBL`).
#[derive(Clone, Debug, PartialEq)]
pub struct Climbable {
    pub object: ObjectBlock,
    pub shapes: Vec<Shape>,
}

/// Skeleton object (`P_SKELET`).
#[derive(Clone, Debug, PartialEq)]
pub struct SkeletonObject {
    pub object: ObjectBlock,
    pub visual: Visual,
    pub skeleton: Skeleton,
}

/// Online/offline group (`ON_OFF_G`).
#[derive(Clone, Debug, PartialEq)]
pub struct OnOffGroup {
    pub object: ObjectBlock,
    pub member_ids: Vec<u16>,
}

/// Monster group (`AI_FLE_G`).
#[derive(Clone, Debug, PartialEq)]
pub struct MonsterGroup {
    pub state: MonsterState,
    /// 0/1; repeated in the update.
    pub flag: u32,
    /// Meaning unestablished.
    pub unestablished: u16,
    pub member_ids: Vec<u16>,
    pub update: MonsterLikeUpdate,
    pub update_flag: u32,
}

const SMART_TERRAIN: &str = "SMRTTRRN";
const RESPAWN: &str = "RE_SPAWN";

/// The state and update of an object, by kind.
#[derive(Clone, Debug, PartialEq)]
pub enum Object {
    Player(Player),
    Stalker(Stalker),
    Trader(Trader),
    Monster(Monster),
    CrowOrPhantom(CrowOrPhantom),
    Vehicle(Vehicle),
    Helicopter(Helicopter),
    Item(PlainItem),
    Torch(Torch),
    Pda(Pda),
    Document(Document),
    Outfit(Outfit),
    Ammunition(Ammunition),
    Knife(Knife),
    MagazineWeapon(MagazineWeapon),
    WeaponWithGrenadeLauncher(WeaponWithGrenadeLauncher),
    Shotgun(Shotgun),
    Zone(Zone),
    Anomaly(AnomalyObject),
    VisualAnomaly(VisualAnomaly),
    TorridZone(TorridZone),
    LevelChanger(LevelChanger),
    Restrictor(Restrictor),
    SmartZone(SmartZone),
    TeamBase(TeamBase),
    PlainVisual(PlainVisual),
    StationaryGun(StationaryGun),
    HangingLamp(HangingLamp),
    PhysicsObject(PhysicsObject),
    Breakable(Breakable),
    Climbable(Climbable),
    SkeletonObject(SkeletonObject),
    OnOffGroup(OnOffGroup),
    MonsterGroup(MonsterGroup),
}

use super::Kind;

impl Object {
    /// Reads the object state `s` and the update data `u` of a `kind`.
    pub(super) fn read(
        kind: Kind,
        s: &mut Reader,
        u: &mut Reader,
        cx: &Context,
    ) -> io::Result<Self> {
        Ok(match kind {
            Kind::Player => Self::Player(Player::read(s, u)?),
            Kind::Stalker => Self::Stalker(Stalker::read(s, u, cx)?),
            Kind::Trader => Self::Trader(Trader {
                object: ObjectBlock::read(s)?,
                visual: Visual::read(s)?,
                trader: TraderLike::read(s)?,
            }),
            Kind::Monster => Self::Monster(Monster {
                state: MonsterState::read(s)?,
                script: if has_character_script(cx, false) {
                    Some(CharacterScript::read(s, cx, false)?)
                } else {
                    None
                },
                update: MonsterLikeUpdate::read(u)?,
            }),
            Kind::CrowOrPhantom => Self::CrowOrPhantom(CrowOrPhantom {
                creature: Creature::read(s)?,
                update: CreatureUpdate::read(u)?,
            }),
            Kind::Vehicle => Self::Vehicle(Vehicle {
                object: ObjectBlock::read(s)?,
                visual: Visual::read(s)?,
                skeleton: Skeleton::read(s)?,
                health: s.float()?,
            }),
            Kind::Helicopter => Self::Helicopter(Helicopter {
                object: ObjectBlock::read(s)?,
                visual: Visual::read(s)?,
                motion: s.string()?,
                skeleton: Skeleton::read(s)?,
                startup_animation: s.string()?,
                engine_sound: s.string()?,
            }),
            Kind::Item => Self::Item(PlainItem {
                item: Item::read(s)?,
                update: ItemUpdate::read(u)?,
            }),
            Kind::Torch => Self::Torch(Torch {
                item: Item::read(s)?,
                update: ItemUpdate::read(u)?,
                torch_flags: u.u8()?,
            }),
            Kind::Pda => Self::Pda(Pda {
                item: Item::read(s)?,
                original_owner_id: s.u16()?,
                specific_character: s.string()?,
                info_portion: s.string()?,
                update: ItemUpdate::read(u)?,
            }),
            Kind::Document => Self::Document(Document {
                item: Item::read(s)?,
                info_portion: s.string()?,
                update: ItemUpdate::read(u)?,
            }),
            Kind::Outfit => Self::Outfit(Outfit {
                item: Item::read(s)?,
                update: ItemUpdate::read(u)?,
                condition: Q8(u.u8()?),
            }),
            Kind::Ammunition => Self::Ammunition(Ammunition {
                item: Item::read(s)?,
                rounds_left: s.u16()?,
                update: ItemUpdate::read(u)?,
                update_rounds_left: u.u16()?,
            }),
            Kind::Knife => Self::Knife(Knife {
                weapon: WeaponState::read(s)?,
                update: WeaponUpdate::read(u)?,
            }),
            Kind::MagazineWeapon => Self::MagazineWeapon(MagazineWeapon {
                weapon: WeaponState::read(s)?,
                update: WeaponUpdate::read(u)?,
                fire_mode: u.u8()?,
            }),
            Kind::WeaponWithGrenadeLauncher => {
                Self::WeaponWithGrenadeLauncher(WeaponWithGrenadeLauncher {
                    weapon: WeaponState::read(s)?,
                    grenade_mode: u.bool8()?,
                    update: WeaponUpdate::read(u)?,
                    fire_mode: u.u8()?,
                })
            }
            Kind::Shotgun => Self::Shotgun(Shotgun {
                weapon: WeaponState::read(s)?,
                update: WeaponUpdate::read(u)?,
                fire_mode: u.u8()?,
                loaded_ammo_types: {
                    let count = u.u8()?;
                    u.take(usize::from(count))?.to_vec()
                },
            }),
            Kind::Zone => Self::Zone(Zone::read(s)?),
            Kind::Anomaly => Self::Anomaly(AnomalyObject {
                anomaly: Anomaly::read(s)?,
                script: artefact_spawn(s, cx)?,
            }),
            Kind::VisualAnomaly => Self::VisualAnomaly(VisualAnomaly {
                anomaly: Anomaly::read(s)?,
                visual: Visual::read(s)?,
                idle_animation: s.string()?,
                attack_animation: s.string()?,
                script: artefact_spawn(s, cx)?,
            }),
            Kind::TorridZone => Self::TorridZone(TorridZone {
                zone: Zone::read(s)?,
                motion: s.string()?,
            }),
            Kind::LevelChanger => Self::LevelChanger(LevelChanger {
                object: ObjectBlock::read(s)?,
                shapes: read_shapes(s)?,
                restrictor_type: s.u8()?,
                destination_game_vertex: s.u16()?,
                destination_level_node: s.u32()?,
                destination_position: s.float3()?,
                destination_direction: s.float3()?,
                destination_level: s.string()?,
                destination_graph_point: s.string()?,
                silent: s.bool8()?,
            }),
            Kind::Restrictor => Self::Restrictor(Restrictor::read(s)?),
            Kind::SmartZone => Self::SmartZone(SmartZone {
                restrictor: Restrictor::read(s)?,
                script: match cx.class_id {
                    SMART_TERRAIN => SmartZoneScript::SmartTerrain(SmartTerrain::read(s)?),
                    RESPAWN => {
                        let count = s.u8()?;
                        SmartZoneScript::Respawn(
                            (0..count).map(|_| s.u16()).collect::<io::Result<_>>()?,
                        )
                    }
                    _ => SmartZoneScript::None,
                },
            }),
            Kind::TeamBase => Self::TeamBase(TeamBase {
                restrictor: Restrictor::read(s)?,
                team: s.u8()?,
            }),
            Kind::PlainVisual => Self::PlainVisual(PlainVisual {
                object: ObjectBlock::read(s)?,
                visual: Visual::read(s)?,
            }),
            Kind::StationaryGun => Self::StationaryGun(StationaryGun {
                object: ObjectBlock::read(s)?,
                visual: Visual::read(s)?,
                working: u.bool8()?,
                aim_direction: u.float3()?,
            }),
            Kind::HangingLamp => Self::HangingLamp(HangingLamp {
                object: ObjectBlock::read(s)?,
                visual: Visual::read(s)?,
                skeleton: Skeleton::read(s)?,
                colour: s.u32()?,
                brightness: s.float()?,
                colour_animator: s.string()?,
                range: s.float()?,
                light_flags: s.u16()?,
                startup_animation: s.string()?,
                fixed_bones: s.string()?,
                health: s.float()?,
                virtual_size: s.float()?,
                ambient_radius: s.float()?,
                ambient_power: s.float()?,
                ambient_texture: s.string()?,
                light_texture: s.string()?,
                light_bone: s.string()?,
                spot_cone_angle: s.float()?,
                glow_texture: s.string()?,
                glow_radius: s.float()?,
                ambient_light_bone: s.string()?,
            }),
            Kind::PhysicsObject => Self::PhysicsObject(PhysicsObject {
                object: ObjectBlock::read(s)?,
                visual: Visual::read(s)?,
                skeleton: Skeleton::read(s)?,
                physics_type: s.u32()?,
                mass: s.float()?,
                fixed_bones: s.string()?,
            }),
            Kind::Breakable => Self::Breakable(Breakable {
                object: ObjectBlock::read(s)?,
                visual: Visual::read(s)?,
                health: s.float()?,
            }),
            Kind::Climbable => Self::Climbable(Climbable {
                object: ObjectBlock::read(s)?,
                shapes: read_shapes(s)?,
            }),
            Kind::SkeletonObject => Self::SkeletonObject(SkeletonObject {
                object: ObjectBlock::read(s)?,
                visual: Visual::read(s)?,
                skeleton: Skeleton::read(s)?,
            }),
            Kind::OnOffGroup => Self::OnOffGroup(OnOffGroup {
                object: ObjectBlock::read(s)?,
                member_ids: ids(s)?,
            }),
            Kind::MonsterGroup => Self::MonsterGroup(MonsterGroup {
                state: MonsterState::read(s)?,
                flag: s.u32()?,
                unestablished: s.u16()?,
                member_ids: ids(s)?,
                update: MonsterLikeUpdate::read(u)?,
                update_flag: u.u32()?,
            }),
        })
    }

    /// Writes the object state to `s` and the update data to `u`.
    pub(super) fn write(&self, s: &mut Writer, u: &mut Writer) -> io::Result<()> {
        match self {
            Self::Player(o) => o.write(s, u)?,
            Self::Stalker(o) => o.write(s, u)?,
            Self::Trader(o) => {
                o.object.write(s)?;
                o.visual.write(s)?;
                o.trader.write(s)?;
            }
            Self::Monster(o) => {
                o.state.write(s)?;
                write_opt(s, &o.script, CharacterScript::write)?;
                o.update.write(u);
            }
            Self::CrowOrPhantom(o) => {
                o.creature.write(s)?;
                o.update.write(u);
            }
            Self::Vehicle(o) => {
                o.object.write(s)?;
                o.visual.write(s)?;
                o.skeleton.write(s)?;
                s.float(o.health);
            }
            Self::Helicopter(o) => {
                o.object.write(s)?;
                o.visual.write(s)?;
                s.string(&o.motion)?;
                o.skeleton.write(s)?;
                s.string(&o.startup_animation)?;
                s.string(&o.engine_sound)?;
            }
            Self::Item(o) => {
                o.item.write(s)?;
                o.update.write(u)?;
            }
            Self::Torch(o) => {
                o.item.write(s)?;
                o.update.write(u)?;
                u.u8(o.torch_flags);
            }
            Self::Pda(o) => {
                o.item.write(s)?;
                s.u16(o.original_owner_id);
                s.string(&o.specific_character)?;
                s.string(&o.info_portion)?;
                o.update.write(u)?;
            }
            Self::Document(o) => {
                o.item.write(s)?;
                s.string(&o.info_portion)?;
                o.update.write(u)?;
            }
            Self::Outfit(o) => {
                o.item.write(s)?;
                o.update.write(u)?;
                u.u8(o.condition.0);
            }
            Self::Ammunition(o) => {
                o.item.write(s)?;
                s.u16(o.rounds_left);
                o.update.write(u)?;
                u.u16(o.update_rounds_left);
            }
            Self::Knife(o) => {
                o.weapon.write(s)?;
                o.update.write(u)?;
            }
            Self::MagazineWeapon(o) => {
                o.weapon.write(s)?;
                o.update.write(u)?;
                u.u8(o.fire_mode);
            }
            Self::WeaponWithGrenadeLauncher(o) => {
                o.weapon.write(s)?;
                u.bool8(o.grenade_mode);
                o.update.write(u)?;
                u.u8(o.fire_mode);
            }
            Self::Shotgun(o) => {
                o.weapon.write(s)?;
                o.update.write(u)?;
                u.u8(o.fire_mode);
                u.u8(u8::try_from(o.loaded_ammo_types.len())
                    .map_err(|_| invalid("more than 255 loaded rounds"))?);
                u.bytes(&o.loaded_ammo_types);
            }
            Self::Zone(o) => o.write(s)?,
            Self::Anomaly(o) => {
                o.anomaly.write(s)?;
                write_opt(s, &o.script, ArtefactSpawnScript::write)?;
            }
            Self::VisualAnomaly(o) => {
                o.anomaly.write(s)?;
                o.visual.write(s)?;
                s.string(&o.idle_animation)?;
                s.string(&o.attack_animation)?;
                write_opt(s, &o.script, ArtefactSpawnScript::write)?;
            }
            Self::TorridZone(o) => {
                o.zone.write(s)?;
                s.string(&o.motion)?;
            }
            Self::LevelChanger(o) => {
                o.object.write(s)?;
                write_shapes(s, &o.shapes)?;
                s.u8(o.restrictor_type);
                s.u16(o.destination_game_vertex);
                s.u32(o.destination_level_node);
                s.float3(o.destination_position);
                s.float3(o.destination_direction);
                s.string(&o.destination_level)?;
                s.string(&o.destination_graph_point)?;
                s.bool8(o.silent);
            }
            Self::Restrictor(o) => o.write(s)?,
            Self::SmartZone(o) => {
                o.restrictor.write(s)?;
                match &o.script {
                    SmartZoneScript::None => {}
                    SmartZoneScript::SmartTerrain(t) => t.write(s)?,
                    SmartZoneScript::Respawn(ids) => {
                        s.u8(u8::try_from(ids.len())
                            .map_err(|_| invalid("more than 255 spawned objects"))?);
                        ids.iter().for_each(|&id| s.u16(id));
                    }
                }
            }
            Self::TeamBase(o) => {
                o.restrictor.write(s)?;
                s.u8(o.team);
            }
            Self::PlainVisual(o) => {
                o.object.write(s)?;
                o.visual.write(s)?;
            }
            Self::StationaryGun(o) => {
                o.object.write(s)?;
                o.visual.write(s)?;
                u.bool8(o.working);
                u.float3(o.aim_direction);
            }
            Self::HangingLamp(o) => {
                o.object.write(s)?;
                o.visual.write(s)?;
                o.skeleton.write(s)?;
                s.u32(o.colour);
                s.float(o.brightness);
                s.string(&o.colour_animator)?;
                s.float(o.range);
                s.u16(o.light_flags);
                s.string(&o.startup_animation)?;
                s.string(&o.fixed_bones)?;
                s.float(o.health);
                s.float(o.virtual_size);
                s.float(o.ambient_radius);
                s.float(o.ambient_power);
                s.string(&o.ambient_texture)?;
                s.string(&o.light_texture)?;
                s.string(&o.light_bone)?;
                s.float(o.spot_cone_angle);
                s.string(&o.glow_texture)?;
                s.float(o.glow_radius);
                s.string(&o.ambient_light_bone)?;
            }
            Self::PhysicsObject(o) => {
                o.object.write(s)?;
                o.visual.write(s)?;
                o.skeleton.write(s)?;
                s.u32(o.physics_type);
                s.float(o.mass);
                s.string(&o.fixed_bones)?;
            }
            Self::Breakable(o) => {
                o.object.write(s)?;
                o.visual.write(s)?;
                s.float(o.health);
            }
            Self::Climbable(o) => {
                o.object.write(s)?;
                write_shapes(s, &o.shapes)?;
            }
            Self::SkeletonObject(o) => {
                o.object.write(s)?;
                o.visual.write(s)?;
                o.skeleton.write(s)?;
            }
            Self::OnOffGroup(o) => {
                o.object.write(s)?;
                write_ids(s, &o.member_ids)?;
            }
            Self::MonsterGroup(o) => {
                if o.update_flag != o.flag {
                    return Err(invalid(format!(
                        "monster group update flag {} differs from its state flag {}",
                        o.update_flag, o.flag
                    )));
                }
                o.state.write(s)?;
                s.u32(o.flag);
                s.u16(o.unestablished);
                write_ids(s, &o.member_ids)?;
                o.update.write(u);
                u.u32(o.update_flag);
            }
        }
        Ok(())
    }

    /// The kind this variant is.
    pub fn kind(&self) -> Kind {
        match self {
            Self::Player(_) => Kind::Player,
            Self::Stalker(_) => Kind::Stalker,
            Self::Trader(_) => Kind::Trader,
            Self::Monster(_) => Kind::Monster,
            Self::CrowOrPhantom(_) => Kind::CrowOrPhantom,
            Self::Vehicle(_) => Kind::Vehicle,
            Self::Helicopter(_) => Kind::Helicopter,
            Self::Item(_) => Kind::Item,
            Self::Torch(_) => Kind::Torch,
            Self::Pda(_) => Kind::Pda,
            Self::Document(_) => Kind::Document,
            Self::Outfit(_) => Kind::Outfit,
            Self::Ammunition(_) => Kind::Ammunition,
            Self::Knife(_) => Kind::Knife,
            Self::MagazineWeapon(_) => Kind::MagazineWeapon,
            Self::WeaponWithGrenadeLauncher(_) => Kind::WeaponWithGrenadeLauncher,
            Self::Shotgun(_) => Kind::Shotgun,
            Self::Zone(_) => Kind::Zone,
            Self::Anomaly(_) => Kind::Anomaly,
            Self::VisualAnomaly(_) => Kind::VisualAnomaly,
            Self::TorridZone(_) => Kind::TorridZone,
            Self::LevelChanger(_) => Kind::LevelChanger,
            Self::Restrictor(_) => Kind::Restrictor,
            Self::SmartZone(_) => Kind::SmartZone,
            Self::TeamBase(_) => Kind::TeamBase,
            Self::PlainVisual(_) => Kind::PlainVisual,
            Self::StationaryGun(_) => Kind::StationaryGun,
            Self::HangingLamp(_) => Kind::HangingLamp,
            Self::PhysicsObject(_) => Kind::PhysicsObject,
            Self::Breakable(_) => Kind::Breakable,
            Self::Climbable(_) => Kind::Climbable,
            Self::SkeletonObject(_) => Kind::SkeletonObject,
            Self::OnOffGroup(_) => Kind::OnOffGroup,
            Self::MonsterGroup(_) => Kind::MonsterGroup,
        }
    }

    /// Checks the scripted additions (section 7.5) against what the class
    /// id, script version and single-player flag of `cx` call for, so that a
    /// written record reads back.
    pub(super) fn check(&self, cx: &Context) -> io::Result<()> {
        match self {
            Self::Stalker(o) => CharacterScript::check(&o.script, cx, true),
            Self::Monster(o) => CharacterScript::check(&o.script, cx, false),
            Self::Anomaly(o) => check_artefact_spawn(&o.script, cx),
            Self::VisualAnomaly(o) => check_artefact_spawn(&o.script, cx),
            Self::SmartZone(o) => {
                let ok = matches!(
                    (cx.class_id, &o.script),
                    (SMART_TERRAIN, SmartZoneScript::SmartTerrain(_))
                        | (RESPAWN, SmartZoneScript::Respawn(_))
                ) || (!matches!(cx.class_id, SMART_TERRAIN | RESPAWN)
                    && o.script == SmartZoneScript::None);
                if ok {
                    Ok(())
                } else {
                    Err(invalid("smart-zone additions disagree with the class id"))
                }
            }
            _ => Ok(()),
        }
    }

    /// The Object block every kind starts with.
    pub fn object_block(&self) -> &ObjectBlock {
        match self {
            Self::Player(o) => &o.creature.object,
            Self::Stalker(o) => &o.monster.creature.object,
            Self::Trader(o) => &o.object,
            Self::Monster(o) => &o.state.monster.creature.object,
            Self::CrowOrPhantom(o) => &o.creature.object,
            Self::Vehicle(o) => &o.object,
            Self::Helicopter(o) => &o.object,
            Self::Item(o) => &o.item.object,
            Self::Torch(o) => &o.item.object,
            Self::Pda(o) => &o.item.object,
            Self::Document(o) => &o.item.object,
            Self::Outfit(o) => &o.item.object,
            Self::Ammunition(o) => &o.item.object,
            Self::Knife(o) => &o.weapon.item.object,
            Self::MagazineWeapon(o) => &o.weapon.item.object,
            Self::WeaponWithGrenadeLauncher(o) => &o.weapon.item.object,
            Self::Shotgun(o) => &o.weapon.item.object,
            Self::Zone(o) => &o.object,
            Self::Anomaly(o) => &o.anomaly.zone.object,
            Self::VisualAnomaly(o) => &o.anomaly.zone.object,
            Self::TorridZone(o) => &o.zone.object,
            Self::LevelChanger(o) => &o.object,
            Self::Restrictor(o) => &o.object,
            Self::SmartZone(o) => &o.restrictor.object,
            Self::TeamBase(o) => &o.restrictor.object,
            Self::PlainVisual(o) => &o.object,
            Self::StationaryGun(o) => &o.object,
            Self::HangingLamp(o) => &o.object,
            Self::PhysicsObject(o) => &o.object,
            Self::Breakable(o) => &o.object,
            Self::Climbable(o) => &o.object,
            Self::SkeletonObject(o) => &o.object,
            Self::OnOffGroup(o) => &o.object,
            Self::MonsterGroup(o) => &o.state.monster.creature.object,
        }
    }

    /// The Object block, to change.
    pub fn object_block_mut(&mut self) -> &mut ObjectBlock {
        match self {
            Self::Player(o) => &mut o.creature.object,
            Self::Stalker(o) => &mut o.monster.creature.object,
            Self::Trader(o) => &mut o.object,
            Self::Monster(o) => &mut o.state.monster.creature.object,
            Self::CrowOrPhantom(o) => &mut o.creature.object,
            Self::Vehicle(o) => &mut o.object,
            Self::Helicopter(o) => &mut o.object,
            Self::Item(o) => &mut o.item.object,
            Self::Torch(o) => &mut o.item.object,
            Self::Pda(o) => &mut o.item.object,
            Self::Document(o) => &mut o.item.object,
            Self::Outfit(o) => &mut o.item.object,
            Self::Ammunition(o) => &mut o.item.object,
            Self::Knife(o) => &mut o.weapon.item.object,
            Self::MagazineWeapon(o) => &mut o.weapon.item.object,
            Self::WeaponWithGrenadeLauncher(o) => &mut o.weapon.item.object,
            Self::Shotgun(o) => &mut o.weapon.item.object,
            Self::Zone(o) => &mut o.object,
            Self::Anomaly(o) => &mut o.anomaly.zone.object,
            Self::VisualAnomaly(o) => &mut o.anomaly.zone.object,
            Self::TorridZone(o) => &mut o.zone.object,
            Self::LevelChanger(o) => &mut o.object,
            Self::Restrictor(o) => &mut o.object,
            Self::SmartZone(o) => &mut o.restrictor.object,
            Self::TeamBase(o) => &mut o.restrictor.object,
            Self::PlainVisual(o) => &mut o.object,
            Self::StationaryGun(o) => &mut o.object,
            Self::HangingLamp(o) => &mut o.object,
            Self::PhysicsObject(o) => &mut o.object,
            Self::Breakable(o) => &mut o.object,
            Self::Climbable(o) => &mut o.object,
            Self::SkeletonObject(o) => &mut o.object,
            Self::OnOffGroup(o) => &mut o.object,
            Self::MonsterGroup(o) => &mut o.state.monster.creature.object,
        }
    }

    /// The Visual block, for kinds that have one (section 7.4.2).
    pub fn visual(&self) -> Option<&Visual> {
        Some(match self {
            Self::Player(o) => &o.creature.visual,
            Self::Stalker(o) => &o.monster.creature.visual,
            Self::Trader(o) => &o.visual,
            Self::Monster(o) => &o.state.monster.creature.visual,
            Self::CrowOrPhantom(o) => &o.creature.visual,
            Self::Vehicle(o) => &o.visual,
            Self::Helicopter(o) => &o.visual,
            Self::Item(o) => &o.item.visual,
            Self::Torch(o) => &o.item.visual,
            Self::Pda(o) => &o.item.visual,
            Self::Document(o) => &o.item.visual,
            Self::Outfit(o) => &o.item.visual,
            Self::Ammunition(o) => &o.item.visual,
            Self::Knife(o) => &o.weapon.item.visual,
            Self::MagazineWeapon(o) => &o.weapon.item.visual,
            Self::WeaponWithGrenadeLauncher(o) => &o.weapon.item.visual,
            Self::Shotgun(o) => &o.weapon.item.visual,
            Self::VisualAnomaly(o) => &o.visual,
            Self::PlainVisual(o) => &o.visual,
            Self::StationaryGun(o) => &o.visual,
            Self::HangingLamp(o) => &o.visual,
            Self::PhysicsObject(o) => &o.visual,
            Self::Breakable(o) => &o.visual,
            Self::SkeletonObject(o) => &o.visual,
            Self::MonsterGroup(o) => &o.state.monster.creature.visual,
            Self::Zone(_)
            | Self::Anomaly(_)
            | Self::TorridZone(_)
            | Self::LevelChanger(_)
            | Self::Restrictor(_)
            | Self::SmartZone(_)
            | Self::TeamBase(_)
            | Self::Climbable(_)
            | Self::OnOffGroup(_) => return None,
        })
    }

    /// The Creature block, for kinds that have one.
    pub fn creature(&self) -> Option<&Creature> {
        Some(match self {
            Self::Player(o) => &o.creature,
            Self::Stalker(o) => &o.monster.creature,
            Self::Monster(o) => &o.state.monster.creature,
            Self::CrowOrPhantom(o) => &o.creature,
            Self::MonsterGroup(o) => &o.state.monster.creature,
            _ => return None,
        })
    }

    /// The Creature block, to change.
    pub fn creature_mut(&mut self) -> Option<&mut Creature> {
        Some(match self {
            Self::Player(o) => &mut o.creature,
            Self::Stalker(o) => &mut o.monster.creature,
            Self::Monster(o) => &mut o.state.monster.creature,
            Self::CrowOrPhantom(o) => &mut o.creature,
            Self::MonsterGroup(o) => &mut o.state.monster.creature,
            _ => return None,
        })
    }

    /// The Monster-like block (stalkers, monsters, monster groups).
    pub fn monster_like(&self) -> Option<&MonsterLike> {
        Some(match self {
            Self::Stalker(o) => &o.monster,
            Self::Monster(o) => &o.state.monster,
            Self::MonsterGroup(o) => &o.state.monster,
            _ => return None,
        })
    }

    /// The Monster-like block, to change.
    pub fn monster_like_mut(&mut self) -> Option<&mut MonsterLike> {
        Some(match self {
            Self::Stalker(o) => &mut o.monster,
            Self::Monster(o) => &mut o.state.monster,
            Self::MonsterGroup(o) => &mut o.state.monster,
            _ => return None,
        })
    }

    /// The Skeleton block, for kinds that have one.
    pub fn skeleton(&self) -> Option<&Skeleton> {
        match self {
            Self::Player(o) => Some(&o.skeleton),
            Self::Stalker(o) => Some(&o.skeleton),
            Self::Monster(o) => Some(&o.state.skeleton),
            Self::MonsterGroup(o) => Some(&o.state.skeleton),
            Self::Vehicle(o) => Some(&o.skeleton),
            Self::Helicopter(o) => Some(&o.skeleton),
            Self::HangingLamp(o) => Some(&o.skeleton),
            Self::PhysicsObject(o) => Some(&o.skeleton),
            Self::SkeletonObject(o) => Some(&o.skeleton),
            _ => None,
        }
    }
}
