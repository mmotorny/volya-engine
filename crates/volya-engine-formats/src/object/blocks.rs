// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The blocks object layouts are built from (docs/formats/sav.md, section
//! 7.4.1) and the script time of section 7.5. Field names follow the spec's
//! (believed) names; raw values are kept where the meaning is unestablished.

use std::io;

use crate::packet::{Reader, Writer, invalid};

/// A `q8(v, lo, hi)` byte, kept as stored so it round-trips.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Q8(pub u8);

impl Q8 {
    /// `floor((v − lo) / (hi − lo) · 255 + 0.5)`, `v` clamped to the range
    /// (the spec requires it in range; clamping is our choice).
    pub fn quantise(v: f32, lo: f32, hi: f32) -> Self {
        let v = v.clamp(lo, hi);
        Self(((v - lo) / (hi - lo) * 255.0 + 0.5).floor() as u8)
    }

    /// `byte / 255 · (hi − lo) + lo` (believed, section 7.4.1).
    pub fn value(self, lo: f32, hi: f32) -> f32 {
        f32::from(self.0) / 255.0 * (hi - lo) + lo
    }
}

fn q8s<const N: usize>(r: &mut Reader) -> io::Result<[Q8; N]> {
    let bytes = r.take(N)?;
    Ok(std::array::from_fn(|i| Q8(bytes[i])))
}

fn write_q8s(w: &mut Writer, v: &[Q8]) {
    v.iter().for_each(|q| w.u8(q.0));
}

/// `u32` count + count × `u16` object ids.
pub(crate) fn ids(r: &mut Reader) -> io::Result<Vec<u16>> {
    r.list(Reader::u16)
}

pub(crate) fn write_ids(w: &mut Writer, ids: &[u16]) -> io::Result<()> {
    w.list(ids, |w, &id| w.u16(id))
}

/// `u32` count + count × `u8`.
pub(crate) fn bytes_list(r: &mut Reader) -> io::Result<Vec<u8>> {
    r.list(Reader::u8)
}

pub(crate) fn write_bytes_list(w: &mut Writer, v: &[u8]) -> io::Result<()> {
    w.list(v, |w, &b| w.u8(b))
}

/// Object block: every simulation object.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectBlock {
    pub game_graph_vertex: u16,
    /// Distance along the current offline path edge.
    pub distance: f32,
    pub direct_control: u32,
    pub level_graph_node: u32,
    /// Bit 0x200: can be saved; other bits unestablished.
    pub state_flags: u32,
    /// Per-object LTX text.
    pub custom_data: String,
    /// −1 if none.
    pub story_id: i32,
    pub spawn_story_id: i32,
}

/// State flags bit: the object can be saved (section 7.1).
pub const STATE_CAN_BE_SAVED: u32 = 0x200;

impl ObjectBlock {
    pub fn read(r: &mut Reader) -> io::Result<Self> {
        Ok(Self {
            game_graph_vertex: r.u16()?,
            distance: r.float()?,
            direct_control: r.u32()?,
            level_graph_node: r.u32()?,
            state_flags: r.u32()?,
            custom_data: r.string()?,
            story_id: r.s32()?,
            spawn_story_id: r.s32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) -> io::Result<()> {
        w.u16(self.game_graph_vertex);
        w.float(self.distance);
        w.u32(self.direct_control);
        w.u32(self.level_graph_node);
        w.u32(self.state_flags);
        w.string(&self.custom_data)?;
        w.s32(self.story_id);
        w.s32(self.spawn_story_id);
        Ok(())
    }
}

/// Visual block.
#[derive(Clone, Debug, PartialEq)]
pub struct Visual {
    pub model: String,
    pub flags: u8,
}

impl Visual {
    pub fn read(r: &mut Reader) -> io::Result<Self> {
        Ok(Self {
            model: r.string()?,
            flags: r.u8()?,
        })
    }

    pub fn write(&self, w: &mut Writer) -> io::Result<()> {
        w.string(&self.model)?;
        w.u8(self.flags);
        Ok(())
    }
}

/// One shape of a Shape block.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// Type 0.
    Sphere { centre: [f32; 3], radius: f32 },
    /// Type 1: the box's 4×3 transform, three axes then the centre.
    Box([[f32; 3]; 4]),
}

/// Shape block: `u8` count, then the shapes. Other types than 0 and 1 "do not
/// occur" and are rejected (our choice).
pub fn read_shapes(r: &mut Reader) -> io::Result<Vec<Shape>> {
    let count = r.u8()?;
    (0..count)
        .map(|_| match r.u8()? {
            0 => Ok(Shape::Sphere {
                centre: r.float3()?,
                radius: r.float()?,
            }),
            1 => Ok(Shape::Box([
                r.float3()?,
                r.float3()?,
                r.float3()?,
                r.float3()?,
            ])),
            t => Err(invalid(format!("shape type {t}"))),
        })
        .collect()
}

pub fn write_shapes(w: &mut Writer, shapes: &[Shape]) -> io::Result<()> {
    w.u8(u8::try_from(shapes.len()).map_err(|_| invalid("more than 255 shapes"))?);
    for shape in shapes {
        match shape {
            Shape::Sphere { centre, radius } => {
                w.u8(0);
                w.float3(*centre);
                w.float(*radius);
            }
            Shape::Box(rows) => {
                w.u8(1);
                rows.iter().for_each(|row| w.float3(*row));
            }
        }
    }
    Ok(())
}

/// Skeleton flags bit: saved bone state follows.
pub const SKELETON_BONE_STATE: u8 = 0x04;
/// Skeleton flags bit deciding whether physics objects, skeleton objects,
/// vehicles and helicopters are saved (section 7.1: saved if clear).
pub const SKELETON_NOT_SAVED: u8 = 0x08;

/// Skeleton block.
#[derive(Clone, Debug, PartialEq)]
pub struct Skeleton {
    pub startup_animation: String,
    pub flags: u8,
    pub source_id: u16,
    /// Present iff `flags` has [`SKELETON_BONE_STATE`]; boxed, as no
    /// record of a New Game has one.
    pub bone_state: Option<Box<BoneState>>,
}

/// Saved bone state of a Skeleton block.
#[derive(Clone, Debug, PartialEq)]
pub struct BoneState {
    pub bones_mask: u64,
    pub root_bone: u16,
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub bones: Vec<Bone>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Bone {
    /// Axis by axis in the box `min … max`.
    pub position: [Q8; 3],
    /// Quaternion components in −1 … 1 (order unestablished).
    pub rotation: [Q8; 4],
    pub enabled: u8,
}

impl Skeleton {
    pub fn read(r: &mut Reader) -> io::Result<Self> {
        let startup_animation = r.string()?;
        let flags = r.u8()?;
        let source_id = r.u16()?;
        let bone_state = if flags & SKELETON_BONE_STATE != 0 {
            let bones_mask = r.u64()?;
            let root_bone = r.u16()?;
            let min = r.float3()?;
            let max = r.float3()?;
            let count = r.u16()?;
            let bones = (0..count)
                .map(|_| {
                    Ok(Bone {
                        position: q8s(r)?,
                        rotation: q8s(r)?,
                        enabled: r.u8()?,
                    })
                })
                .collect::<io::Result<_>>()?;
            Some(Box::new(BoneState {
                bones_mask,
                root_bone,
                min,
                max,
                bones,
            }))
        } else {
            None
        };
        Ok(Self {
            startup_animation,
            flags,
            source_id,
            bone_state,
        })
    }

    /// Writes the block; flags bit 0x04 must agree with `bone_state`.
    pub fn write(&self, w: &mut Writer) -> io::Result<()> {
        if (self.flags & SKELETON_BONE_STATE != 0) != self.bone_state.is_some() {
            return Err(invalid(format!(
                "skeleton flags {:#x} disagree with its bone state",
                self.flags
            )));
        }
        w.string(&self.startup_animation)?;
        w.u8(self.flags);
        w.u16(self.source_id);
        if let Some(b) = &self.bone_state {
            w.u64(b.bones_mask);
            w.u16(b.root_bone);
            w.float3(b.min);
            w.float3(b.max);
            w.u16(u16::try_from(b.bones.len()).map_err(|_| invalid("too many bones"))?);
            for bone in &b.bones {
                write_q8s(w, &bone.position);
                write_q8s(w, &bone.rotation);
                w.u8(bone.enabled);
            }
        }
        Ok(())
    }
}

/// Creature block = Object, Visual, then these.
#[derive(Clone, Debug, PartialEq)]
pub struct Creature {
    pub object: ObjectBlock,
    pub visual: Visual,
    pub team: u8,
    pub squad: u8,
    pub group: u8,
    /// 0 = dead.
    pub health: f32,
    pub dynamic_out_restrictions: Vec<u16>,
    pub dynamic_in_restrictions: Vec<u16>,
    /// 0xFFFF none.
    pub killer_id: u16,
    pub death_time: u64,
}

impl Creature {
    pub fn read(r: &mut Reader) -> io::Result<Self> {
        Ok(Self {
            object: ObjectBlock::read(r)?,
            visual: Visual::read(r)?,
            team: r.u8()?,
            squad: r.u8()?,
            group: r.u8()?,
            health: r.float()?,
            dynamic_out_restrictions: ids(r)?,
            dynamic_in_restrictions: ids(r)?,
            killer_id: r.u16()?,
            death_time: r.u64()?,
        })
    }

    pub fn write(&self, w: &mut Writer) -> io::Result<()> {
        self.object.write(w)?;
        self.visual.write(w)?;
        w.u8(self.team);
        w.u8(self.squad);
        w.u8(self.group);
        w.float(self.health);
        write_ids(w, &self.dynamic_out_restrictions)?;
        write_ids(w, &self.dynamic_in_restrictions)?;
        w.u16(self.killer_id);
        w.u64(self.death_time);
        Ok(())
    }
}

/// Monster-like block = Creature, then these.
#[derive(Clone, Debug, PartialEq)]
pub struct MonsterLike {
    pub creature: Creature,
    /// Comma-separated restrictor names.
    pub out_restrictions: String,
    pub in_restrictions: String,
    /// 0xFFFF none.
    pub smart_terrain_id: u16,
    pub smart_terrain_task_reached: u8,
}

impl MonsterLike {
    pub fn read(r: &mut Reader) -> io::Result<Self> {
        Ok(Self {
            creature: Creature::read(r)?,
            out_restrictions: r.string()?,
            in_restrictions: r.string()?,
            smart_terrain_id: r.u16()?,
            smart_terrain_task_reached: r.u8()?,
        })
    }

    pub fn write(&self, w: &mut Writer) -> io::Result<()> {
        self.creature.write(w)?;
        w.string(&self.out_restrictions)?;
        w.string(&self.in_restrictions)?;
        w.u16(self.smart_terrain_id);
        w.u8(self.smart_terrain_task_reached);
        Ok(())
    }
}

/// Trader-like block: characters that own money and a profile.
#[derive(Clone, Debug, PartialEq)]
pub struct TraderLike {
    pub money: u32,
    pub specific_character: String,
    pub trader_flags: u32,
    pub character_profile: String,
    /// Index; −1 none.
    pub community: i32,
    pub rank: i32,
    pub reputation: i32,
    pub character_name: String,
}

impl TraderLike {
    pub fn read(r: &mut Reader) -> io::Result<Self> {
        Ok(Self {
            money: r.u32()?,
            specific_character: r.string()?,
            trader_flags: r.u32()?,
            character_profile: r.string()?,
            community: r.s32()?,
            rank: r.s32()?,
            reputation: r.s32()?,
            character_name: r.string()?,
        })
    }

    pub fn write(&self, w: &mut Writer) -> io::Result<()> {
        w.u32(self.money);
        w.string(&self.specific_character)?;
        w.u32(self.trader_flags);
        w.string(&self.character_profile)?;
        w.s32(self.community);
        w.s32(self.rank);
        w.s32(self.reputation);
        w.string(&self.character_name)?;
        Ok(())
    }
}

/// Item block = Object, Visual, condition.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub object: ObjectBlock,
    pub visual: Visual,
    /// 0 … 1.
    pub condition: f32,
}

impl Item {
    pub fn read(r: &mut Reader) -> io::Result<Self> {
        Ok(Self {
            object: ObjectBlock::read(r)?,
            visual: Visual::read(r)?,
            condition: r.float()?,
        })
    }

    pub fn write(&self, w: &mut Writer) -> io::Result<()> {
        self.object.write(w)?;
        self.visual.write(w)?;
        w.float(self.condition);
        Ok(())
    }
}

/// `20π`, the top of the first velocity's range.
pub const FIRST_VELOCITY_MAX: f32 = 62.831856;
/// Item update flags: the first velocity is zero and omitted.
pub const ITEM_NO_FIRST_VELOCITY: u8 = 0x40;
/// Item update flags: the second velocity is zero and omitted.
pub const ITEM_NO_SECOND_VELOCITY: u8 = 0x80;

/// Item update: the physics state of an item.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemUpdate {
    /// `f`: bits 0–4 the number of physics elements; 0x20 a further state
    /// flag (believed: physics active); 0x40 and 0x80 omit the velocities.
    pub flags: u8,
    /// Present iff `flags` ≠ 0.
    pub physics: Option<ItemPhysics>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ItemPhysics {
    pub position: [f32; 3],
    /// Quaternion components in 0 … 1.
    pub rotation: [Q8; 4],
    /// Believed angular, in 0 … 20π; absent iff flags bit 0x40.
    pub first_velocity: Option<[Q8; 3]>,
    /// Believed linear, in −32 … 32; absent iff flags bit 0x80.
    pub second_velocity: Option<[Q8; 3]>,
}

impl ItemUpdate {
    pub fn read(r: &mut Reader) -> io::Result<Self> {
        let flags = r.u8()?;
        let physics = if flags != 0 {
            Some(ItemPhysics {
                position: r.float3()?,
                rotation: q8s(r)?,
                first_velocity: if flags & ITEM_NO_FIRST_VELOCITY == 0 {
                    Some(q8s(r)?)
                } else {
                    None
                },
                second_velocity: if flags & ITEM_NO_SECOND_VELOCITY == 0 {
                    Some(q8s(r)?)
                } else {
                    None
                },
            })
        } else {
            None
        };
        Ok(Self { flags, physics })
    }

    /// Writes what is present; `flags` must agree with it.
    pub fn write(&self, w: &mut Writer) -> io::Result<()> {
        let consistent = match &self.physics {
            None => self.flags == 0,
            Some(p) => {
                self.flags != 0
                    && p.first_velocity.is_none() == (self.flags & ITEM_NO_FIRST_VELOCITY != 0)
                    && p.second_velocity.is_none() == (self.flags & ITEM_NO_SECOND_VELOCITY != 0)
            }
        };
        if !consistent {
            return Err(invalid(format!(
                "item update flags {:#x} disagree with its fields",
                self.flags
            )));
        }
        w.u8(self.flags);
        if let Some(p) = &self.physics {
            w.float3(p.position);
            write_q8s(w, &p.rotation);
            if let Some(v) = &p.first_velocity {
                write_q8s(w, v);
            }
            if let Some(v) = &p.second_velocity {
                write_q8s(w, v);
            }
        }
        Ok(())
    }
}

/// Creature update.
#[derive(Clone, Debug, PartialEq)]
pub struct CreatureUpdate {
    pub health: f32,
    pub timestamp: u32,
    pub flags: u8,
    /// The same value as the spawn packet's position.
    pub position: [f32; 3],
    pub model_yaw: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
    pub team: u8,
    pub squad: u8,
    pub group: u8,
}

impl CreatureUpdate {
    pub fn read(r: &mut Reader) -> io::Result<Self> {
        Ok(Self {
            health: r.float()?,
            timestamp: r.u32()?,
            flags: r.u8()?,
            position: r.float3()?,
            model_yaw: r.float()?,
            yaw: r.float()?,
            pitch: r.float()?,
            roll: r.float()?,
            team: r.u8()?,
            squad: r.u8()?,
            group: r.u8()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        w.float(self.health);
        w.u32(self.timestamp);
        w.u8(self.flags);
        w.float3(self.position);
        w.float(self.model_yaw);
        w.float(self.yaw);
        w.float(self.pitch);
        w.float(self.roll);
        w.u8(self.team);
        w.u8(self.squad);
        w.u8(self.group);
    }
}

/// Monster-like update = Creature update, then offline movement.
#[derive(Clone, Debug, PartialEq)]
pub struct MonsterLikeUpdate {
    pub creature: CreatureUpdate,
    pub next_game_vertex: u16,
    pub previous_game_vertex: u16,
    pub distance_from_previous: f32,
    pub distance_to_next: f32,
}

impl MonsterLikeUpdate {
    pub fn read(r: &mut Reader) -> io::Result<Self> {
        Ok(Self {
            creature: CreatureUpdate::read(r)?,
            next_game_vertex: r.u16()?,
            previous_game_vertex: r.u16()?,
            distance_from_previous: r.float()?,
            distance_to_next: r.float()?,
        })
    }

    pub fn write(&self, w: &mut Writer) {
        self.creature.write(w);
        w.u16(self.next_game_vertex);
        w.u16(self.previous_game_vertex);
        w.float(self.distance_from_previous);
        w.float(self.distance_to_next);
    }
}

/// Weapon state = Item, then these.
#[derive(Clone, Debug, PartialEq)]
pub struct WeaponState {
    pub item: Item,
    pub ammo_in_reserve: u16,
    pub ammo_in_magazine: u16,
    pub weapon_state: u8,
    pub installed_addons: u8,
    pub ammo_type: u8,
}

impl WeaponState {
    pub fn read(r: &mut Reader) -> io::Result<Self> {
        Ok(Self {
            item: Item::read(r)?,
            ammo_in_reserve: r.u16()?,
            ammo_in_magazine: r.u16()?,
            weapon_state: r.u8()?,
            installed_addons: r.u8()?,
            ammo_type: r.u8()?,
        })
    }

    pub fn write(&self, w: &mut Writer) -> io::Result<()> {
        self.item.write(w)?;
        w.u16(self.ammo_in_reserve);
        w.u16(self.ammo_in_magazine);
        w.u8(self.weapon_state);
        w.u8(self.installed_addons);
        w.u8(self.ammo_type);
        Ok(())
    }
}

/// Weapon update = Item update, then these.
#[derive(Clone, Debug, PartialEq)]
pub struct WeaponUpdate {
    pub item: ItemUpdate,
    /// `q8(condition, 0, 1)`.
    pub condition: Q8,
    pub weapon_flags: u8,
    pub ammo_in_magazine: u16,
    pub addons: u8,
    pub ammo_type: u8,
    pub weapon_state: u8,
    pub zoom: u8,
}

impl WeaponUpdate {
    pub fn read(r: &mut Reader) -> io::Result<Self> {
        Ok(Self {
            item: ItemUpdate::read(r)?,
            condition: Q8(r.u8()?),
            weapon_flags: r.u8()?,
            ammo_in_magazine: r.u16()?,
            addons: r.u8()?,
            ammo_type: r.u8()?,
            weapon_state: r.u8()?,
            zoom: r.u8()?,
        })
    }

    pub fn write(&self, w: &mut Writer) -> io::Result<()> {
        self.item.write(w)?;
        w.u8(self.condition.0);
        w.u8(self.weapon_flags);
        w.u16(self.ammo_in_magazine);
        w.u8(self.addons);
        w.u8(self.ammo_type);
        w.u8(self.weapon_state);
        w.u8(self.zoom);
        Ok(())
    }
}

/// Zone = Object, Shape, then these.
#[derive(Clone, Debug, PartialEq)]
pub struct Zone {
    pub object: ObjectBlock,
    pub shapes: Vec<Shape>,
    pub restrictor_type: u8,
    pub maximum_power: f32,
    pub owner_id: u32,
    pub enabled_time: u32,
    pub disabled_time: u32,
    pub start_time_shift: u32,
}

impl Zone {
    pub fn read(r: &mut Reader) -> io::Result<Self> {
        Ok(Self {
            object: ObjectBlock::read(r)?,
            shapes: read_shapes(r)?,
            restrictor_type: r.u8()?,
            maximum_power: r.float()?,
            owner_id: r.u32()?,
            enabled_time: r.u32()?,
            disabled_time: r.u32()?,
            start_time_shift: r.u32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) -> io::Result<()> {
        self.object.write(w)?;
        write_shapes(w, &self.shapes)?;
        w.u8(self.restrictor_type);
        w.float(self.maximum_power);
        w.u32(self.owner_id);
        w.u32(self.enabled_time);
        w.u32(self.disabled_time);
        w.u32(self.start_time_shift);
        Ok(())
    }
}

/// Anomaly = Zone, then these.
#[derive(Clone, Debug, PartialEq)]
pub struct Anomaly {
    pub zone: Zone,
    pub offline_interactive_radius: f32,
    pub artefact_spawn_count: u16,
    pub artefact_position_offset: u32,
}

impl Anomaly {
    pub fn read(r: &mut Reader) -> io::Result<Self> {
        Ok(Self {
            zone: Zone::read(r)?,
            offline_interactive_radius: r.float()?,
            artefact_spawn_count: r.u16()?,
            artefact_position_offset: r.u32()?,
        })
    }

    pub fn write(&self, w: &mut Writer) -> io::Result<()> {
        self.zone.write(w)?;
        w.float(self.offline_interactive_radius);
        w.u16(self.artefact_spawn_count);
        w.u32(self.artefact_position_offset);
        Ok(())
    }
}

/// A script time (section 7.5). `None` where the year byte is 0 ("unset").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScriptTime {
    /// Year − 2000, never 0.
    pub year: u8,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
    pub millisecond: u16,
}

pub fn read_script_time(r: &mut Reader) -> io::Result<Option<ScriptTime>> {
    let year = r.u8()?;
    if year == 0 {
        return Ok(None);
    }
    Ok(Some(ScriptTime {
        year,
        month: r.u8()?,
        day: r.u8()?,
        hour: r.u8()?,
        minute: r.u8()?,
        second: r.u8()?,
        millisecond: r.u16()?,
    }))
}

pub fn write_script_time(w: &mut Writer, t: &Option<ScriptTime>) -> io::Result<()> {
    let Some(t) = t else {
        w.u8(0);
        return Ok(());
    };
    if t.year == 0 {
        return Err(invalid("script time with year byte 0 is unset"));
    }
    w.bytes(&[t.year, t.month, t.day, t.hour, t.minute, t.second]);
    w.u16(t.millisecond);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn q8_follows_the_formula() {
        assert_eq!(Q8::quantise(0.0, 0.0, 1.0), Q8(0));
        assert_eq!(Q8::quantise(1.0, 0.0, 1.0), Q8(255));
        assert_eq!(Q8::quantise(0.0, -1.0, 1.0), Q8(128));
        assert_eq!(Q8::quantise(5.0, -1.0, 1.0), Q8(255));
        assert_eq!(Q8(255).value(-32.0, 32.0), 32.0);
        assert_eq!(Q8(0).value(0.0, FIRST_VELOCITY_MAX), 0.0);
    }

    #[test]
    fn item_update_flags_decide_fields() {
        let mut w = Writer::new();
        w.u8(0x41);
        w.float3([1.0, 2.0, 3.0]);
        w.bytes(&[1, 2, 3, 4, 5, 6, 7]);
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        let u = ItemUpdate::read(&mut r).unwrap();
        r.finish("item update").unwrap();
        let p = u.physics.as_ref().unwrap();
        assert_eq!(p.first_velocity, None);
        assert_eq!(p.second_velocity, Some([Q8(5), Q8(6), Q8(7)]));
        let mut w = Writer::new();
        u.write(&mut w).unwrap();
        assert_eq!(w.into_inner(), bytes);
        let broken = ItemUpdate { flags: 0x01, ..u };
        assert!(broken.write(&mut Writer::new()).is_err());
        let mut r = Reader::new(&[0]);
        assert_eq!(ItemUpdate::read(&mut r).unwrap().physics, None);
    }

    #[test]
    fn skeleton_bone_state_round_trips() {
        let s = Skeleton {
            startup_animation: "idle".into(),
            flags: SKELETON_BONE_STATE,
            source_id: 0xFFFF,
            bone_state: Some(Box::new(BoneState {
                bones_mask: u64::MAX,
                root_bone: 2,
                min: [-1.0; 3],
                max: [1.0; 3],
                bones: vec![Bone {
                    position: [Q8(1), Q8(2), Q8(3)],
                    rotation: [Q8(4), Q8(5), Q8(6), Q8(7)],
                    enabled: 1,
                }],
            })),
        };
        let mut w = Writer::new();
        s.write(&mut w).unwrap();
        let bytes = w.into_inner();
        assert_eq!(bytes.len(), 5 + 1 + 2 + 8 + 2 + 24 + 2 + 8);
        let mut r = Reader::new(&bytes);
        assert_eq!(Skeleton::read(&mut r).unwrap(), s);
        r.finish("skeleton").unwrap();
    }

    #[test]
    fn script_times_and_shapes() {
        let mut w = Writer::new();
        write_script_time(&mut w, &None).unwrap();
        let t = ScriptTime {
            year: 12,
            month: 5,
            day: 1,
            hour: 5,
            minute: 30,
            second: 0,
            millisecond: 999,
        };
        write_script_time(&mut w, &Some(t)).unwrap();
        let shapes = [
            Shape::Sphere {
                centre: [1.0, 2.0, 3.0],
                radius: 4.0,
            },
            Shape::Box([[1.0; 3], [2.0; 3], [3.0; 3], [4.0; 3]]),
        ];
        write_shapes(&mut w, &shapes).unwrap();
        let bytes = w.into_inner();
        assert_eq!(bytes.len(), 1 + 8 + 1 + 17 + 49);
        let mut r = Reader::new(&bytes);
        assert_eq!(read_script_time(&mut r).unwrap(), None);
        assert_eq!(read_script_time(&mut r).unwrap(), Some(t));
        assert_eq!(read_shapes(&mut r).unwrap(), shapes);
        r.finish("test").unwrap();
        assert!(read_shapes(&mut Reader::new(&[1, 2])).is_err());
    }
}
