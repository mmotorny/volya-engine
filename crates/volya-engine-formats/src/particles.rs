// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The particle effect library `particles.xr` (docs/formats/particles-xr.md,
//! "P1"): effect records with their action lists and regions, and group
//! records with their members; and the level's placed-effect records of
//! `level.ps_static` (docs/behavior/level-placed-effects.md section 1.2).
//! What the records do over time is the simulation's (`volya-engine-core`).

use std::io;

use crate::chunks::Chunks;
use crate::cp1251;

/// Top-level chunks of the library (P1 section 1).
pub const VERSION_CHUNK: u32 = 1;
pub const EFFECTS_CHUNK: u32 = 3;
pub const GROUPS_CHUNK: u32 = 4;
/// The only library version the game loads.
pub const VERSION: u16 = 1;
/// The only effect record version (P1 section 2).
pub const EFFECT_VERSION: u16 = 1;
/// The only group record version (P1 section 6).
pub const GROUP_VERSION: u16 = 3;

/// The game's log lines for versions it does not load (P1 sections 1, 6).
pub const UNSUPPORTED_LIBRARY: &str = "PS Library: Unsupported version.";
pub const UNSUPPORTED_GROUP: &str = "!Unsupported PG version. Load failed.";

/// Effect flag word bits (P1 section 3).
pub mod effect_flags {
    pub const SPRITE: u32 = 0x1;
    pub const FRAMES: u32 = 0x400;
    pub const ANIMATED: u32 = 0x800;
    pub const RANDOM_FRAME: u32 = 0x1000;
    pub const RANDOM_PLAYBACK: u32 = 0x2000;
    pub const TIME_LIMIT: u32 = 0x4000;
    pub const ALIGN_TO_PATH: u32 = 0x8000;
    pub const COLLIDE: u32 = 0x10000;
    pub const COLLIDE_DESTROY: u32 = 0x20000;
    pub const VELOCITY_SCALE: u32 = 0x40000;
    pub const COLLIDE_QUERY: u32 = 0x80000;
    pub const DEFAULT_WORLD_ALIGN: u32 = 0x100000;
    pub const FACE_ALIGN: u32 = 0x200000;
    pub const CULL: u32 = 0x400000;
    pub const CULL_CCW: u32 = 0x800000;
}

/// Action flag word bits (P1 section 4.2).
pub mod action_flags {
    /// Stored positions and directions follow the effect's full transform.
    pub const TRANSFORM: u32 = 0x2;
    /// `source`: a new particle's size is uniform.
    pub const SINGLE_SIZE: u32 = 0x2000_0000;
    /// `source`: switched off (set and cleared at run time).
    pub const SILENT: u32 = 0x4000_0000;
    /// `source`: no effect on the simulation.
    pub const SOURCE_MARK: u32 = 0x8000_0000;
}

/// Member flag word bits (P1 section 6.2; docs/behavior/particle-groups.md
/// section 7).
pub mod member_flags {
    /// The stop at the stop time is deferred.
    pub const DEFERRED_STOP: u32 = 0x1;
    /// Per-particle children.
    pub const PER_PARTICLE_CHILD: u32 = 0x2;
    /// The member is used: only these play.
    pub const USED: u32 = 0x4;
    /// A per-particle child that is no longer playing is played again.
    pub const REPLAY_CHILD: u32 = 0x10;
    /// Birth children.
    pub const BIRTH_CHILD: u32 = 0x20;
    /// Death children.
    pub const DEATH_CHILD: u32 = 0x40;
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// −π/2 in `f32` (P1 section 2).
pub const MINUS_HALF_PI: f32 = -1.570_796_4;

/// A region: the 68-byte record of P1 section 5, with its stored derived
/// values.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Region {
    /// 0 – 10 (section 5.2); others generate (0, 0, 0).
    pub kind: u32,
    pub p1: [f32; 3],
    pub p2: [f32; 3],
    pub u: [f32; 3],
    pub v: [f32; 3],
    /// *r*₁ – *r*₄.
    pub r: [f32; 4],
}

/// Region kinds (P1 section 5.2).
pub mod region_kind {
    pub const POINT: u32 = 0;
    pub const LINE: u32 = 1;
    pub const TRIANGLE: u32 = 2;
    pub const PLANE: u32 = 3;
    pub const BOX: u32 = 4;
    pub const SPHERE: u32 = 5;
    pub const CYLINDER: u32 = 6;
    pub const CONE: u32 = 7;
    pub const GAUSSIAN: u32 = 8;
    pub const DISC: u32 = 9;
    pub const RECTANGLE: u32 = 10;
}

/// The fields of a `source` (type 21, P1 section 4.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Source {
    pub position: Region,
    pub velocity: Region,
    pub rotation: Region,
    pub size: Region,
    pub colour: Region,
    pub alpha: f32,
    /// Particles per second.
    pub rate: f32,
    pub age: f32,
    pub age_spread: f32,
    pub parent_velocity: [f32; 3],
    pub parent_motion: f32,
}

/// The fields of an action record by type (P1 section 4.3), named by the
/// words the effects' own chunk 36 pairs with each type id.
#[derive(Clone, Debug, PartialEq)]
pub enum ActionKind {
    /// 1.
    Bounce {
        surface: Region,
        tangential: f32,
        resilience: f32,
        cutoff: f32,
    },
    /// 4: the bounds compare with the squared speed.
    Damping {
        factor: [f32; 3],
        lower: f32,
        upper: f32,
    },
    /// 6.
    Follow {
        magnitude: f32,
        epsilon: f32,
        max_radius: f32,
    },
    /// 7.
    Gravitate {
        magnitude: f32,
        epsilon: f32,
        max_radius: f32,
    },
    /// 8.
    Gravity { acceleration: [f32; 3] },
    /// 9.
    Jet {
        centre: [f32; 3],
        acceleration: Region,
        magnitude: f32,
        epsilon: f32,
        max_radius: f32,
    },
    /// 10.
    KillOld { age_limit: f32, kill_younger: u32 },
    /// 12.
    Move,
    /// 14.
    OrbitPoint {
        centre: [f32; 3],
        magnitude: f32,
        epsilon: f32,
        max_radius: f32,
    },
    /// 15.
    RandomAccel { region: Region },
    /// 16.
    RandomDisplace { region: Region },
    /// 17.
    RandomVelocity { region: Region },
    /// 19.
    Sink { kill_inside: u32, region: Region },
    /// 21.
    Source(Box<Source>),
    /// 22.
    SpeedLimit { min: f32, max: f32 },
    /// 23.
    TargetColor {
        colour: [f32; 3],
        alpha: f32,
        scale: f32,
    },
    /// 24.
    TargetSize { size: [f32; 3], scale: [f32; 3] },
    /// 25, and 26, which is read and run alike (P1 4.4).
    TargetRotate { rotation: [f32; 3], scale: f32 },
    /// 27, and 28, which is read and run alike (P1 4.4).
    TargetVelocity { velocity: [f32; 3], scale: f32 },
    /// 29.
    Vortex {
        centre: [f32; 3],
        axis: [f32; 3],
        magnitude: f32,
        epsilon: f32,
        max_radius: f32,
    },
    /// 30.
    Turbulence {
        frequency: f32,
        octaves: i32,
        magnitude: f32,
        offset: f32,
        drift: [f32; 3],
    },
    /// 31.
    Scatter {
        centre: [f32; 3],
        magnitude: f32,
        epsilon: f32,
        max_radius: f32,
    },
    /// Ids 0, 3, 5, 11, 13, 18 and 20: accepted with their sizes, fields
    /// and behaviour not specified (P1 4.4); no effect uses them.
    Unspecified { fields: Vec<u8> },
}

/// One action record (P1 section 4.1).
#[derive(Clone, Debug, PartialEq)]
pub struct Action {
    /// The second type id, the one kept.
    pub type_id: u32,
    /// The action flag word (section 4.2).
    pub flags: u32,
    pub kind: ActionKind,
}

/// Texture frames (chunk 6, P1 section 2.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frames {
    pub width: f32,
    pub height: f32,
    pub per_row: i32,
    pub count: i32,
    /// Frames per second.
    pub rate: f32,
}

impl Default for Frames {
    /// The values kept when chunk 6 is not read (P1 section 2).
    fn default() -> Self {
        Self {
            width: 0.125,
            height: 0.5,
            per_row: 8,
            count: 16,
            rate: 24.0,
        }
    }
}

/// Collision values (chunk 33, P1 section 2.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Collision {
    pub tangential: f32,
    pub resilience: f32,
    /// Compared with a squared speed.
    pub cutoff: f32,
}

/// The sprite (chunk 7, P1 section 2.2).
#[derive(Clone, Debug, PartialEq)]
pub struct Sprite {
    pub shader: String,
    pub texture: String,
}

/// An effect record (P1 section 2), as read or made: [`Library::new`]
/// stores it in the library's region.
#[derive(Clone, Debug, PartialEq)]
pub struct Effect {
    /// Its sub-chunk id in chunk 3 (stored order).
    pub id: u32,
    pub name: String,
    /// Maximum number of live particles.
    pub max_particles: u32,
    pub flags: u32,
    pub actions: Vec<Action>,
    pub sprite: Option<Sprite>,
    pub frames: Frames,
    /// Seconds; 0 when chunk 8 is not read.
    pub time_limit: f32,
    pub collision: Collision,
    pub velocity_scale: [f32; 3],
    pub alignment: [f32; 3],
}

impl Effect {
    /// The effect's length (see [`EffectRecord::length`]).
    pub fn length(&self) -> f32 {
        self.record().length()
    }

    /// The record as the library gives it.
    pub fn record(&self) -> EffectRecord<'_> {
        EffectRecord {
            id: self.id,
            name: &self.name,
            max_particles: self.max_particles,
            flags: self.flags,
            actions: &self.actions,
            sprite: self.sprite.as_ref().map(|s| SpriteRecord {
                shader: &s.shader,
                texture: &s.texture,
            }),
            frames: self.frames,
            time_limit: self.time_limit,
            collision: self.collision,
            velocity_scale: self.velocity_scale,
            alignment: self.alignment,
        }
    }
}

/// A group member (P1 section 6.1), as read or made.
#[derive(Clone, Debug, PartialEq)]
pub struct Member {
    pub effect: String,
    /// Empty: none.
    pub per_particle_child: String,
    pub birth_child: String,
    pub death_child: String,
    /// Seconds.
    pub start_time: f32,
    pub stop_time: f32,
    pub flags: u32,
}

/// A group record (P1 section 6), as read or made.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    /// Its sub-chunk id in chunk 4 (stored order).
    pub id: u32,
    pub name: String,
    /// Meaning unestablished; not read by the group's update.
    pub flags: u32,
    pub members: Vec<Member>,
    /// Seconds; 0 means none.
    pub time_limit: f32,
}

/// The sprite of an [`EffectRecord`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteRecord<'a> {
    pub shader: &'a str,
    pub texture: &'a str,
}

/// An effect record of the library (P1 section 2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EffectRecord<'a> {
    /// Its sub-chunk id in chunk 3 (stored order).
    pub id: u32,
    pub name: &'a str,
    /// Maximum number of live particles.
    pub max_particles: u32,
    pub flags: u32,
    pub actions: &'a [Action],
    pub sprite: Option<SpriteRecord<'a>>,
    pub frames: Frames,
    /// Seconds; 0 when chunk 8 is not read.
    pub time_limit: f32,
    pub collision: Collision,
    pub velocity_scale: [f32; 3],
    pub alignment: [f32; 3],
}

impl EffectRecord<'_> {
    /// The effect's length: −1 without flag bit 0x4000, else the time
    /// limit (P1 section 2; particle-lifecycle.md section 7).
    pub fn length(&self) -> f32 {
        if self.flags & effect_flags::TIME_LIMIT != 0 {
            self.time_limit
        } else {
            -1.0
        }
    }

    /// The record as an [`Effect`] to make a library of.
    pub fn to_effect(&self) -> Effect {
        Effect {
            id: self.id,
            name: self.name.to_owned(),
            max_particles: self.max_particles,
            flags: self.flags,
            actions: self.actions.to_vec(),
            sprite: self.sprite.map(|s| Sprite {
                shader: s.shader.to_owned(),
                texture: s.texture.to_owned(),
            }),
            frames: self.frames,
            time_limit: self.time_limit,
            collision: self.collision,
            velocity_scale: self.velocity_scale,
            alignment: self.alignment,
        }
    }
}

/// A group member of the library (P1 section 6.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MemberRecord<'a> {
    pub effect: &'a str,
    /// Empty: none.
    pub per_particle_child: &'a str,
    pub birth_child: &'a str,
    pub death_child: &'a str,
    /// Seconds.
    pub start_time: f32,
    pub stop_time: f32,
    pub flags: u32,
}

impl MemberRecord<'_> {
    /// The member as a [`Member`] to make a library of.
    pub fn to_member(&self) -> Member {
        Member {
            effect: self.effect.to_owned(),
            per_particle_child: self.per_particle_child.to_owned(),
            birth_child: self.birth_child.to_owned(),
            death_child: self.death_child.to_owned(),
            start_time: self.start_time,
            stop_time: self.stop_time,
            flags: self.flags,
        }
    }
}

/// A group's members in the library, by their index in the library
/// ([`Library::member`]).
#[derive(Clone, Copy, Debug)]
pub struct Members<'a> {
    library: &'a Library,
    first: u32,
    len: u32,
}

impl<'a> Members<'a> {
    pub fn len(&self) -> usize {
        self.len as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The library's index of member `i`.
    pub fn index(&self, i: usize) -> usize {
        assert!(i < self.len(), "member {i} of {}", self.len);
        self.first as usize + i
    }

    /// Member `i`.
    pub fn get(&self, i: usize) -> MemberRecord<'a> {
        self.library.member(self.index(i))
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = MemberRecord<'a>> + use<'a> {
        let (library, first) = (self.library, self.first as usize);
        (first..first + self.len()).map(move |i| library.member(i))
    }
}

/// A group record of the library (P1 section 6).
#[derive(Clone, Copy, Debug)]
pub struct GroupRecord<'a> {
    /// Its sub-chunk id in chunk 4 (stored order).
    pub id: u32,
    pub name: &'a str,
    /// Meaning unestablished; not read by the group's update.
    pub flags: u32,
    pub members: Members<'a>,
    /// Seconds; 0 means none.
    pub time_limit: f32,
}

impl GroupRecord<'_> {
    /// The record as a [`Group`] to make a library of.
    pub fn to_group(&self) -> Group {
        Group {
            id: self.id,
            name: self.name.to_owned(),
            flags: self.flags,
            members: self.members.iter().map(|m| m.to_member()).collect(),
            time_limit: self.time_limit,
        }
    }
}

/// A name's bytes in [`Library`]'s name buffer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Name {
    start: u32,
    len: u32,
}

#[derive(Clone, Debug)]
struct StoredEffect {
    id: u32,
    name: Name,
    max_particles: u32,
    flags: u32,
    /// The run of the library's actions.
    actions: (u32, u32),
    sprite: Option<(Name, Name)>,
    frames: Frames,
    time_limit: f32,
    collision: Collision,
    velocity_scale: [f32; 3],
    alignment: [f32; 3],
}

#[derive(Clone, Copy, Debug)]
struct StoredMember {
    effect: Name,
    per_particle_child: Name,
    birth_child: Name,
    death_child: Name,
    start_time: f32,
    stop_time: f32,
    flags: u32,
}

#[derive(Clone, Copy, Debug)]
struct StoredGroup {
    id: u32,
    name: Name,
    flags: u32,
    /// The run of the library's members.
    members: (u32, u32),
    time_limit: f32,
}

/// The library: effects and groups, each ordered by name compared as
/// unsigned bytes (P1 section 1). A region (docs/architecture/memory.md):
/// every name once in one buffer, every effect's actions in one table and
/// every group's members in another, each record a run of them.
#[derive(Clone, Debug, Default)]
pub struct Library {
    names: String,
    effects: Vec<StoredEffect>,
    actions: Vec<Action>,
    groups: Vec<StoredGroup>,
    members: Vec<StoredMember>,
    /// The effects' and the groups' indices in the order of their names as
    /// strings, for lookup.
    effect_order: Vec<u32>,
    group_order: Vec<u32>,
    /// The game's log lines while loading (P1 sections 1, 6).
    pub log: Vec<String>,
}

impl Library {
    /// Reads `particles.xr`. A missing chunk the game requires (its
    /// assertion failures, P1 section 7) is an error.
    pub fn parse(data: &[u8]) -> io::Result<Self> {
        let top = Chunks::parse(data)?;
        let version = top
            .find(VERSION_CHUNK)
            .filter(|c| !c.payload.is_empty())
            .ok_or_else(|| invalid("particles.xr: no version chunk"))?;
        let mut log = Vec::new();
        if u16_at(version.payload, 0)? != VERSION {
            log.push(UNSUPPORTED_LIBRARY.into());
            return Ok(Library {
                log,
                ..Library::default()
            });
        }
        let mut effects = Vec::new();
        if let Some(chunk) = top.find(EFFECTS_CHUNK) {
            for c in Chunks::parse(chunk.payload)?.list() {
                match parse_effect(c.id(), c.payload)? {
                    Some(effect) => effects.push(effect),
                    None => break,
                }
            }
        }
        let mut groups = Vec::new();
        if let Some(chunk) = top.find(GROUPS_CHUNK) {
            for c in Chunks::parse(chunk.payload)?.list() {
                match parse_group(c.id(), c.payload)? {
                    Some(group) => groups.push(group),
                    None => {
                        log.push(UNSUPPORTED_GROUP.into());
                        break;
                    }
                }
            }
        }
        let mut library = Self::new(effects, groups);
        library.log = log;
        Ok(library)
    }

    /// A library of `effects` and `groups`, ordered by name.
    pub fn new(mut effects: Vec<Effect>, mut groups: Vec<Group>) -> Self {
        let key = |name: &str| cp1251::encode(name).unwrap_or_else(|| name.as_bytes().to_vec());
        effects.sort_by_cached_key(|e| key(&e.name));
        groups.sort_by_cached_key(|g| key(&g.name));
        let members = groups.iter().map(|g| g.members.len()).sum();
        let size = effects
            .iter()
            .map(|e| {
                e.name.len()
                    + e.sprite
                        .as_ref()
                        .map_or(0, |s| s.shader.len() + s.texture.len())
            })
            .chain(groups.iter().flat_map(|g| {
                std::iter::once(g.name.len()).chain(g.members.iter().map(|m| {
                    m.effect.len()
                        + m.per_particle_child.len()
                        + m.birth_child.len()
                        + m.death_child.len()
                }))
            }))
            .sum();
        let mut library = Library {
            names: String::with_capacity(size),
            effects: Vec::with_capacity(effects.len()),
            actions: Vec::with_capacity(effects.iter().map(|e| e.actions.len()).sum()),
            groups: Vec::with_capacity(groups.len()),
            members: Vec::with_capacity(members),
            effect_order: Vec::new(),
            group_order: Vec::new(),
            log: Vec::new(),
        };
        for e in effects {
            let first = library.actions.len() as u32;
            library.actions.extend(e.actions);
            let stored = StoredEffect {
                id: e.id,
                name: library.store(&e.name),
                max_particles: e.max_particles,
                flags: e.flags,
                actions: (first, library.actions.len() as u32),
                sprite: e
                    .sprite
                    .map(|s| (library.store(&s.shader), library.store(&s.texture))),
                frames: e.frames,
                time_limit: e.time_limit,
                collision: e.collision,
                velocity_scale: e.velocity_scale,
                alignment: e.alignment,
            };
            library.effects.push(stored);
        }
        for g in groups {
            let first = library.members.len() as u32;
            for m in &g.members {
                let stored = StoredMember {
                    effect: library.store(&m.effect),
                    per_particle_child: library.store(&m.per_particle_child),
                    birth_child: library.store(&m.birth_child),
                    death_child: library.store(&m.death_child),
                    start_time: m.start_time,
                    stop_time: m.stop_time,
                    flags: m.flags,
                };
                library.members.push(stored);
            }
            let stored = StoredGroup {
                id: g.id,
                name: library.store(&g.name),
                flags: g.flags,
                members: (first, library.members.len() as u32),
                time_limit: g.time_limit,
            };
            library.groups.push(stored);
        }
        let mut order: Vec<u32> = (0..library.effects.len() as u32).collect();
        order.sort_by(|&a, &b| {
            let name = |i: u32| library.name(library.effects[i as usize].name);
            name(a).cmp(name(b))
        });
        library.effect_order = order;
        let mut order: Vec<u32> = (0..library.groups.len() as u32).collect();
        order.sort_by(|&a, &b| {
            let name = |i: u32| library.name(library.groups[i as usize].name);
            name(a).cmp(name(b))
        });
        library.group_order = order;
        library
    }

    fn store(&mut self, name: &str) -> Name {
        let start = self.names.len() as u32;
        self.names.push_str(name);
        Name {
            start,
            len: name.len() as u32,
        }
    }

    fn name(&self, n: Name) -> &str {
        &self.names[n.start as usize..(n.start + n.len) as usize]
    }

    /// Of `order`'s records named `name` by `name_of`, the last.
    fn find(&self, order: &[u32], name_of: impl Fn(usize) -> Name, name: &str) -> Option<usize> {
        let at = order.partition_point(|&i| self.name(name_of(i as usize)) <= name);
        let i = *order.get(at.checked_sub(1)?)? as usize;
        (self.name(name_of(i)) == name).then_some(i)
    }

    /// The effect named exactly `name` (byte for byte); of several, the
    /// last.
    pub fn effect(&self, name: &str) -> Option<usize> {
        self.find(&self.effect_order, |i| self.effects[i].name, name)
    }

    /// The group named exactly `name`; of several, the last.
    pub fn group(&self, name: &str) -> Option<usize> {
        self.find(&self.group_order, |i| self.groups[i].name, name)
    }

    pub fn effect_count(&self) -> usize {
        self.effects.len()
    }

    pub fn group_count(&self) -> usize {
        self.groups.len()
    }

    /// Effect `i`, in name order.
    pub fn effect_at(&self, i: usize) -> EffectRecord<'_> {
        let e = &self.effects[i];
        EffectRecord {
            id: e.id,
            name: self.name(e.name),
            max_particles: e.max_particles,
            flags: e.flags,
            actions: &self.actions[e.actions.0 as usize..e.actions.1 as usize],
            sprite: e.sprite.map(|(shader, texture)| SpriteRecord {
                shader: self.name(shader),
                texture: self.name(texture),
            }),
            frames: e.frames,
            time_limit: e.time_limit,
            collision: e.collision,
            velocity_scale: e.velocity_scale,
            alignment: e.alignment,
        }
    }

    /// Group `i`, in name order.
    pub fn group_at(&self, i: usize) -> GroupRecord<'_> {
        let g = &self.groups[i];
        GroupRecord {
            id: g.id,
            name: self.name(g.name),
            flags: g.flags,
            members: Members {
                library: self,
                first: g.members.0,
                len: g.members.1 - g.members.0,
            },
            time_limit: g.time_limit,
        }
    }

    /// The member at `i` of every group's members ([`Members::index`]).
    pub fn member(&self, i: usize) -> MemberRecord<'_> {
        let m = &self.members[i];
        MemberRecord {
            effect: self.name(m.effect),
            per_particle_child: self.name(m.per_particle_child),
            birth_child: self.name(m.birth_child),
            death_child: self.name(m.death_child),
            start_time: m.start_time,
            stop_time: m.stop_time,
            flags: m.flags,
        }
    }

    /// The effects in name order.
    pub fn effects(&self) -> impl ExactSizeIterator<Item = EffectRecord<'_>> {
        (0..self.effects.len()).map(|i| self.effect_at(i))
    }

    /// The groups in name order.
    pub fn groups(&self) -> impl ExactSizeIterator<Item = GroupRecord<'_>> {
        (0..self.groups.len()).map(|i| self.group_at(i))
    }
}

fn u16_at(b: &[u8], at: usize) -> io::Result<u16> {
    b.get(at..at + 2)
        .map(|s| u16::from_le_bytes(s.try_into().unwrap()))
        .ok_or_else(|| invalid("record too short"))
}

/// A little-endian reader over a record.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take(&mut self, n: usize) -> io::Result<&'a [u8]> {
        let s = self
            .bytes
            .get(self.at..self.at + n)
            .ok_or_else(|| invalid(format!("record ends at {} of {n} bytes", self.at)))?;
        self.at += n;
        Ok(s)
    }

    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn i32(&mut self) -> io::Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn f32(&mut self) -> io::Result<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn float3(&mut self) -> io::Result<[f32; 3]> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }

    fn string(&mut self) -> io::Result<String> {
        let rest = &self.bytes[self.at.min(self.bytes.len())..];
        let end = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| invalid("string without its terminator"))?;
        self.at += end + 1;
        Ok(cp1251::decode(&rest[..end]))
    }

    fn region(&mut self) -> io::Result<Region> {
        Ok(Region {
            kind: self.u32()?,
            p1: self.float3()?,
            p2: self.float3()?,
            u: self.float3()?,
            v: self.float3()?,
            r: [self.f32()?, self.f32()?, self.f32()?, self.f32()?],
        })
    }
}

/// The record size after the header the game reads for type `t`
/// (P1 sections 4.3, 4.4); `None` for the ids it does not accept.
pub fn action_size(t: u32) -> Option<usize> {
    Some(match t {
        0 => 80,
        1 => 80,
        3 => 4,
        4 => 20,
        5 => 32,
        6..=8 => 12,
        9 => 92,
        10 => 8,
        11 => 12,
        12 => 0,
        13 => 36,
        14 => 24,
        15..=17 => 68,
        18 => 4,
        19 | 20 => 72,
        21 => 372,
        22 => 8,
        23 => 20,
        24 => 24,
        25..=28 => 16,
        29 => 36,
        30 => 28,
        31 => 24,
        _ => return None,
    })
}

fn parse_action(r: &mut Reader) -> io::Result<Action> {
    let first = r.u32()?;
    // Ids 2 and above 31 select no layout (P1 4.4; what the game does then
    // is unestablished, believed a crash): an error here.
    let size = action_size(first).ok_or_else(|| invalid(format!("action type {first}")))?;
    let flags = r.u32()?;
    let type_id = r.u32()?;
    let start = r.at;
    let kind = match first {
        1 => ActionKind::Bounce {
            surface: r.region()?,
            tangential: r.f32()?,
            resilience: r.f32()?,
            cutoff: r.f32()?,
        },
        4 => ActionKind::Damping {
            factor: r.float3()?,
            lower: r.f32()?,
            upper: r.f32()?,
        },
        6 => ActionKind::Follow {
            magnitude: r.f32()?,
            epsilon: r.f32()?,
            max_radius: r.f32()?,
        },
        7 => ActionKind::Gravitate {
            magnitude: r.f32()?,
            epsilon: r.f32()?,
            max_radius: r.f32()?,
        },
        8 => ActionKind::Gravity {
            acceleration: r.float3()?,
        },
        9 => ActionKind::Jet {
            centre: r.float3()?,
            acceleration: r.region()?,
            magnitude: r.f32()?,
            epsilon: r.f32()?,
            max_radius: r.f32()?,
        },
        10 => ActionKind::KillOld {
            age_limit: r.f32()?,
            kill_younger: r.u32()?,
        },
        12 => ActionKind::Move,
        14 => ActionKind::OrbitPoint {
            centre: r.float3()?,
            magnitude: r.f32()?,
            epsilon: r.f32()?,
            max_radius: r.f32()?,
        },
        15 => ActionKind::RandomAccel {
            region: r.region()?,
        },
        16 => ActionKind::RandomDisplace {
            region: r.region()?,
        },
        17 => ActionKind::RandomVelocity {
            region: r.region()?,
        },
        19 => ActionKind::Sink {
            kill_inside: r.u32()?,
            region: r.region()?,
        },
        21 => ActionKind::Source(Box::new(Source {
            position: r.region()?,
            velocity: r.region()?,
            rotation: r.region()?,
            size: r.region()?,
            colour: r.region()?,
            alpha: r.f32()?,
            rate: r.f32()?,
            age: r.f32()?,
            age_spread: r.f32()?,
            parent_velocity: r.float3()?,
            parent_motion: r.f32()?,
        })),
        22 => ActionKind::SpeedLimit {
            min: r.f32()?,
            max: r.f32()?,
        },
        23 => ActionKind::TargetColor {
            colour: r.float3()?,
            alpha: r.f32()?,
            scale: r.f32()?,
        },
        24 => ActionKind::TargetSize {
            size: r.float3()?,
            scale: r.float3()?,
        },
        25 | 26 => ActionKind::TargetRotate {
            rotation: r.float3()?,
            scale: r.f32()?,
        },
        27 | 28 => ActionKind::TargetVelocity {
            velocity: r.float3()?,
            scale: r.f32()?,
        },
        29 => ActionKind::Vortex {
            centre: r.float3()?,
            axis: r.float3()?,
            magnitude: r.f32()?,
            epsilon: r.f32()?,
            max_radius: r.f32()?,
        },
        30 => ActionKind::Turbulence {
            frequency: r.f32()?,
            octaves: r.i32()?,
            magnitude: r.f32()?,
            offset: r.f32()?,
            drift: r.float3()?,
        },
        31 => ActionKind::Scatter {
            centre: r.float3()?,
            magnitude: r.f32()?,
            epsilon: r.f32()?,
            max_radius: r.f32()?,
        },
        _ => ActionKind::Unspecified {
            fields: r.take(size)?.to_vec(),
        },
    };
    debug_assert_eq!(r.at - start, size);
    Ok(Action {
        type_id,
        flags,
        kind,
    })
}

/// Decodes an action list (chunk 4, P1 section 4.1).
pub fn parse_actions(bytes: &[u8]) -> io::Result<Vec<Action>> {
    if bytes.is_empty() {
        return Ok(Vec::new());
    }
    let mut r = Reader::new(bytes);
    let n = r.u32()?;
    let mut actions = Vec::with_capacity(n.min(64) as usize);
    for _ in 0..n {
        actions.push(parse_action(&mut r)?);
    }
    Ok(actions)
}

/// A chunk the reader requires (an assertion failure in the game when
/// missing): present and not empty.
fn required<'a>(chunks: &Chunks<'a>, id: u32, what: &str) -> io::Result<&'a [u8]> {
    chunks
        .find(id)
        .map(|c| c.payload)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| invalid(format!("{what}: chunk {id} missing")))
}

/// One effect record, or `None` when it fails to load (its version).
fn parse_effect(id: u32, payload: &[u8]) -> io::Result<Option<Effect>> {
    use effect_flags as f;
    let what = format!("effect {id}");
    let chunks = Chunks::parse(payload)?;
    let version = u16_at(required(&chunks, 1, &what)?, 0)?;
    let name = Reader::new(required(&chunks, 2, &what)?).string()?;
    let max = Reader::new(required(&chunks, 3, &what)?).u32()?;
    if version != EFFECT_VERSION {
        return Ok(None);
    }
    let actions = parse_actions(required(&chunks, 4, &what)?)?;
    // Believed 0 when the chunk is absent (P1 section 2).
    let flags = match chunks.find(5) {
        Some(c) if c.payload.len() >= 4 => Reader::new(c.payload).u32()?,
        _ => 0,
    };
    let sprite = if flags & f::SPRITE != 0 {
        let mut r = Reader::new(required(&chunks, 7, &what)?);
        Some(Sprite {
            shader: r.string()?,
            texture: r.string()?,
        })
    } else {
        None
    };
    let frames = if flags & f::FRAMES != 0 {
        let mut r = Reader::new(required(&chunks, 6, &what)?);
        let width = r.f32()?;
        let height = r.f32()?;
        r.take(8)?;
        Frames {
            width,
            height,
            per_row: r.i32()?,
            count: r.i32()?,
            rate: r.f32()?,
        }
    } else {
        Frames::default()
    };
    let time_limit = if flags & f::TIME_LIMIT != 0 {
        Reader::new(required(&chunks, 8, &what)?).f32()?
    } else {
        0.0
    };
    let collision = if flags & f::COLLIDE != 0 {
        let mut r = Reader::new(required(&chunks, 33, &what)?);
        Collision {
            tangential: r.f32()?,
            resilience: r.f32()?,
            cutoff: r.f32()?,
        }
    } else {
        Collision {
            tangential: 1.0,
            resilience: 0.0,
            cutoff: 0.0,
        }
    };
    let velocity_scale = if flags & f::VELOCITY_SCALE != 0 {
        Reader::new(required(&chunks, 34, &what)?).float3()?
    } else {
        [0.0; 3]
    };
    let alignment = match chunks.find(37) {
        Some(c) if flags & f::ALIGN_TO_PATH != 0 && !c.payload.is_empty() => {
            Reader::new(c.payload).float3()?
        }
        _ => [MINUS_HALF_PI, 0.0, 0.0],
    };
    Ok(Some(Effect {
        id,
        name,
        max_particles: max,
        flags,
        actions,
        sprite,
        frames,
        time_limit,
        collision,
        velocity_scale,
        alignment,
    }))
}

/// One group record, or `None` when it fails to load (its version).
fn parse_group(id: u32, payload: &[u8]) -> io::Result<Option<Group>> {
    let what = format!("group {id}");
    let chunks = Chunks::parse(payload)?;
    if u16_at(required(&chunks, 1, &what)?, 0)? != GROUP_VERSION {
        return Ok(None);
    }
    let name = Reader::new(required(&chunks, 2, &what)?).string()?;
    let flags = match chunks.find(3) {
        Some(c) if c.payload.len() >= 4 => Reader::new(c.payload).u32()?,
        _ => 0,
    };
    // Chunk 7's members have only the first two strings (P1 section 6; no
    // shipped group has it).
    let (bytes, full) = match chunks.find(4).filter(|c| !c.payload.is_empty()) {
        Some(c) => (c.payload, true),
        None => (required(&chunks, 7, &what)?, false),
    };
    let mut r = Reader::new(bytes);
    let n = r.u32()?;
    let mut members = Vec::with_capacity(n.min(64) as usize);
    for _ in 0..n {
        let effect = r.string()?;
        let per_particle_child = r.string()?;
        let (birth_child, death_child) = if full {
            (r.string()?, r.string()?)
        } else {
            (String::new(), String::new())
        };
        members.push(Member {
            effect,
            per_particle_child,
            birth_child,
            death_child,
            start_time: r.f32()?,
            stop_time: r.f32()?,
            flags: r.u32()?,
        });
    }
    let time_limit = match chunks.find(5) {
        Some(c) if c.payload.len() >= 4 => Reader::new(c.payload).f32()?,
        _ => 0.0,
    };
    Ok(Some(Group {
        id,
        name,
        flags,
        members,
        time_limit,
    }))
}

/// A record of `level.ps_static` (docs/behavior/level-placed-effects.md
/// section 1.2).
#[derive(Clone, Debug, PartialEq)]
pub struct PlacedRecord {
    /// An effect or group name of `particles.xr`.
    pub name: String,
    /// *M*, row by row (row-vector convention, row 3 the translation).
    pub transform: [[f32; 4]; 4],
}

/// The name buffer's size: a longer name fails an assertion (section 1.2).
const NAME_BUFFER: usize = 256;

/// Reads `level.ps_static`: every top-level chunk, in stored order and
/// whatever its id, is one record; compressed chunks are decompressed.
pub fn parse_placed(data: &[u8]) -> io::Result<Vec<PlacedRecord>> {
    let mut records = Vec::new();
    for chunk in Chunks::parse(data)?.iter() {
        let payload = chunk.content()?;
        let end = payload
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(payload.len());
        if end >= NAME_BUFFER {
            return Err(invalid("level.ps_static: Dest string less than needed."));
        }
        let name = cp1251::decode(&payload[..end]);
        // What a payload too short for the transform gives is unestablished
        // (section 1.2): an error here.
        let rest = payload
            .get(end + 1..end + 1 + 64)
            .ok_or_else(|| invalid("level.ps_static: record without its transform"))?;
        let mut r = Reader::new(rest);
        let mut transform = [[0.0; 4]; 4];
        for row in &mut transform {
            for v in row.iter_mut() {
                *v = r.f32()?;
            }
        }
        records.push(PlacedRecord { name, transform });
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(id: u32, payload: &[u8]) -> Vec<u8> {
        let mut v = id.to_le_bytes().to_vec();
        v.extend((payload.len() as u32).to_le_bytes());
        v.extend(payload);
        v
    }

    fn f32s(values: &[f32]) -> Vec<u8> {
        values.iter().flat_map(|v| v.to_le_bytes()).collect()
    }

    #[test]
    fn action_sizes_are_those_of_p1() {
        let used = [
            (1, 80),
            (4, 20),
            (6, 12),
            (7, 12),
            (8, 12),
            (9, 92),
            (10, 8),
            (12, 0),
            (14, 24),
            (15, 68),
            (16, 68),
            (17, 68),
            (19, 72),
            (21, 372),
            (22, 8),
            (23, 20),
            (24, 24),
            (25, 16),
            (27, 16),
            (29, 36),
            (30, 28),
            (31, 24),
        ];
        let unused = [
            (0, 80),
            (3, 4),
            (5, 32),
            (11, 12),
            (13, 36),
            (18, 4),
            (20, 72),
            (26, 16),
            (28, 16),
        ];
        for (t, size) in used.into_iter().chain(unused) {
            assert_eq!(action_size(t), Some(size), "type {t}");
        }
        assert_eq!(action_size(2), None);
        assert_eq!(action_size(32), None);
    }

    #[test]
    fn an_action_list_is_read_record_by_record() {
        let mut bytes = 3u32.to_le_bytes().to_vec();
        for (t, flags, fields) in [
            (8u32, 0x2u32, f32s(&[0.0, -9.8, 0.0])),
            (12, 0, Vec::new()),
            (26, 0, f32s(&[1.0, 2.0, 3.0, 0.5])),
        ] {
            bytes.extend(t.to_le_bytes());
            bytes.extend(flags.to_le_bytes());
            bytes.extend(t.to_le_bytes());
            bytes.extend(fields);
        }
        let actions = parse_actions(&bytes).unwrap();
        assert_eq!(actions.len(), 3);
        assert_eq!(
            actions[0].kind,
            ActionKind::Gravity {
                acceleration: [0.0, -9.8, 0.0]
            }
        );
        assert_eq!(actions[0].flags, 2);
        assert_eq!(actions[1].kind, ActionKind::Move);
        assert_eq!(actions[2].type_id, 26);
        assert_eq!(
            actions[2].kind,
            ActionKind::TargetRotate {
                rotation: [1.0, 2.0, 3.0],
                scale: 0.5
            }
        );
    }

    fn effect_record(name: &str, flags: u32, extra: &[(u32, Vec<u8>)]) -> Vec<u8> {
        let mut actions = 1u32.to_le_bytes().to_vec();
        actions.extend([12u32, 0, 12].iter().flat_map(|v| v.to_le_bytes()));
        let mut name_z = name.as_bytes().to_vec();
        name_z.push(0);
        let mut r = chunk(1, &1u16.to_le_bytes());
        r.extend(chunk(2, &name_z));
        r.extend(chunk(3, &7u32.to_le_bytes()));
        r.extend(chunk(4, &actions));
        r.extend(chunk(5, &flags.to_le_bytes()));
        for (id, payload) in extra {
            r.extend(chunk(*id, payload));
        }
        r
    }

    fn library(effects: &[Vec<u8>], groups: &[Vec<u8>]) -> Vec<u8> {
        let list = |records: &[Vec<u8>]| {
            records
                .iter()
                .enumerate()
                .flat_map(|(i, r)| chunk(i as u32, r))
                .collect::<Vec<u8>>()
        };
        let mut file = chunk(1, &1u16.to_le_bytes());
        file.extend(chunk(3, &list(effects)));
        file.extend(chunk(4, &list(groups)));
        file
    }

    #[test]
    fn effects_take_the_defaults_of_unread_chunks_and_sort_by_name() {
        let file = library(
            &[
                effect_record(
                    "b\\second",
                    0x1,
                    &[(7, b"particles\\add\0pfx\\x\0".to_vec())],
                ),
                effect_record("a\\first", 0x4000 | 0x8000, &[(8, f32s(&[1.5]))]),
            ],
            &[],
        );
        let lib = Library::parse(&file).unwrap();
        assert_eq!(lib.effect_at(0).name, "a\\first");
        assert_eq!(lib.effect_at(0).id, 1);
        assert_eq!(lib.effect_at(0).time_limit, 1.5);
        assert_eq!(lib.effect_at(0).length(), 1.5);
        // Chunk 37 absent: the default kept.
        assert_eq!(lib.effect_at(0).alignment, [MINUS_HALF_PI, 0.0, 0.0]);
        assert_eq!(lib.effect_at(0).frames, Frames::default());
        assert_eq!(lib.effect_at(1).length(), -1.0);
        assert_eq!(lib.effect_at(1).sprite.unwrap().texture, "pfx\\x");
        assert_eq!(
            lib.effect_at(1).collision,
            Collision {
                tangential: 1.0,
                resilience: 0.0,
                cutoff: 0.0
            }
        );
        assert_eq!(lib.effect("b\\second"), Some(1));
        assert_eq!(lib.effect("B\\second"), None);
    }

    #[test]
    fn a_flag_gated_chunk_must_be_present() {
        let file = library(&[effect_record("a", 0x4000, &[])], &[]);
        assert!(Library::parse(&file).is_err());
    }

    #[test]
    fn a_failed_record_ends_its_list() {
        let mut old = effect_record("b", 0, &[]);
        old[8..10].copy_from_slice(&2u16.to_le_bytes());
        let file = library(
            &[effect_record("a", 0, &[]), old, effect_record("c", 0, &[])],
            &[],
        );
        let lib = Library::parse(&file).unwrap();
        assert_eq!(lib.effect_count(), 1);
    }

    #[test]
    fn an_unsupported_library_loads_nothing() {
        let mut file = library(&[effect_record("a", 0, &[])], &[]);
        file[8..10].copy_from_slice(&2u16.to_le_bytes());
        let lib = Library::parse(&file).unwrap();
        assert_eq!(lib.effect_count(), 0);
        assert_eq!(lib.log, [UNSUPPORTED_LIBRARY]);
    }

    #[test]
    fn group_members_are_read_with_their_children_and_times() {
        let mut members = 1u32.to_le_bytes().to_vec();
        members.extend(b"a\0child\0\0death\0");
        members.extend(f32s(&[0.5, 2.0]));
        members.extend(0x46u32.to_le_bytes());
        let mut g = chunk(1, &3u16.to_le_bytes());
        g.extend(chunk(2, b"g\0"));
        g.extend(chunk(3, &0u32.to_le_bytes()));
        g.extend(chunk(4, &members));
        g.extend(chunk(5, &f32s(&[4.0])));
        let lib = Library::parse(&library(&[effect_record("a", 0, &[])], &[g])).unwrap();
        let group = lib.group_at(lib.group("g").unwrap());
        assert_eq!(group.time_limit, 4.0);
        assert_eq!(
            group
                .members
                .iter()
                .map(|m| m.to_member())
                .collect::<Vec<_>>(),
            [Member {
                effect: "a".into(),
                per_particle_child: "child".into(),
                birth_child: String::new(),
                death_child: "death".into(),
                start_time: 0.5,
                stop_time: 2.0,
                flags: 0x46,
            }]
        );
    }

    #[test]
    fn placed_records_are_read_from_every_chunk() {
        let mut payload = b"tests\\muhi\0".to_vec();
        let m = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -255.71, -18.8, -127.2, 1.0,
        ];
        payload.extend(f32s(&m));
        let mut file = chunk(0, &payload);
        file.extend(chunk(7, &payload));
        let records = parse_placed(&file).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].name, "tests\\muhi");
        assert_eq!(records[1].transform[3], [-255.71, -18.8, -127.2, 1.0]);
        let long = [vec![b'a'; 256], vec![0], f32s(&m)].concat();
        assert!(parse_placed(&chunk(0, &long)).is_err());
        assert!(parse_placed(&chunk(0, b"a\0")).is_err());
    }
}
