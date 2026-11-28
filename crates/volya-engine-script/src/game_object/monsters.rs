// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Script API X's monster members (docs/behavior/script-api-movement-sight-
//! and-memory.md section 11, with 8.1's `skip_transfer_enemy`): home, jump,
//! berserk, the bloodsucker's control, the zombie's fake death, the panic
//! thresholds and `get_monster_hit_info`, and the classes `MonsterHitInfo`
//! and `MonsterSpace`. What the monsters do with these is CH-12, CH-21,
//! CH-22 and CH-23's (TODO(#1133)): the members set and read the state.

use super::members::{Call, Handle, boolean, member};
use super::{BLOODSUCKER, Context, Event, ObjectClass};
use crate::Value;
use crate::classes::{EngineClass, EnginePart, Objects, Operand};
use crate::values::{Vector, lua_number, to_bits32, to_f32};

/// The zombie class `fake_death_*` test for (2).
pub const ZOMBIE: &str = "CZombie";

/// `MonsterSpace`'s constants (11.6).
pub const MONSTER_SPACE: [(&str, i32); 5] = [
    ("head_anim_normal", 0),
    ("head_anim_angry", 1),
    ("head_anim_glad", 2),
    ("head_anim_kind", 3),
    ("sound_script", 128),
];

/// A monster's home (11.1).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Home {
    /// The level's patrol path; `None` without one.
    pub path: Option<Vec<u8>>,
    pub min_radius: f32,
    pub max_radius: f32,
    pub aggressive: bool,
}

/// A monster's state of section 11.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Monster {
    /// 11.3: loaded from the section's `panic_threshold`.
    pub panic_threshold: f32,
    pub default_panic_threshold: f32,
    pub home: Home,
    /// When `berserk()` was last called, ms of `time_global()`.
    pub berserk_start: Option<u32>,
    /// 8.1's flag.
    pub skip_transfer_enemy: bool,
    /// The last jump asked for: target and factor.
    pub jump: Option<([f32; 3], f32)>,
    /// A bloodsucker's control of the player's view.
    pub alien_control: Option<bool>,
    /// A zombie's fake death.
    pub fake_death: Option<bool>,
}

impl Monster {
    /// A monster of `class`, its thresholds 0 until it loads.
    pub fn of(class: ObjectClass) -> Self {
        Monster {
            alien_control: (class == ObjectClass::Named(BLOODSUCKER)).then_some(false),
            fake_death: (class == ObjectClass::Named(ZOMBIE)).then_some(false),
            ..Monster::default()
        }
    }
}

/// A `MonsterHitInfo` (11.4).
#[derive(Clone, Debug, PartialEq)]
pub struct MonsterHitInfo {
    /// The hitter's handle.
    pub who: Option<EnginePart>,
    pub direction: [f32; 3],
    pub time: i32,
}

impl Default for MonsterHitInfo {
    /// No hit recorded (11.4).
    fn default() -> Self {
        MonsterHitInfo {
            who: None,
            direction: [0.0, 0.0, 1.0],
            time: 0,
        }
    }
}

fn monster<'a>(c: &'a mut Call) -> Result<Option<&'a mut Monster>, String> {
    Ok(c.object()?.movement.monster.as_mut())
}

fn none() -> Result<Vec<Value>, String> {
    Ok(Vec::new())
}

/// The members of section 11 and `skip_transfer_enemy`.
pub(super) fn game_object_members(class: EngineClass, ctx: &Context) -> EngineClass {
    let class = member(class, ctx, "set_home", |c| {
        let name = c.text(0)?;
        let (min_radius, max_radius) = (to_f32(c.number(1)?), to_f32(c.number(2)?));
        let aggressive = c.boolean(3)?;
        let exists = {
            let physics = c.ctx.physics.borrow();
            (physics.level.as_ref()).is_some_and(|l| l.patrol_point(&name, 0).is_some())
        };
        if let Some(m) = monster(c)? {
            m.home = Home {
                path: exists.then_some(name),
                min_radius,
                max_radius,
                aggressive,
            };
        }
        none()
    });
    let class = member(class, ctx, "remove_home", |c| {
        if let Some(m) = monster(c)? {
            m.home.path = None;
            m.home.aggressive = false;
        }
        none()
    });
    let class = member(class, ctx, "jump", |c| {
        let p = c.vector(0)?;
        let factor = to_f32(c.number(1)?);
        if let Some(m) = monster(c)? {
            m.jump = Some(([p.x, p.y, p.z], factor));
        }
        none()
    });
    let class = member(class, ctx, "berserk", |c| {
        let now = c.ctx.services.borrow().device.time_global();
        if let Some(m) = monster(c)? {
            m.berserk_start = Some(now);
        }
        none()
    });
    let class = member(class, ctx, "skip_transfer_enemy", |c| {
        let on = c.boolean(0)?;
        if let Some(m) = monster(c)? {
            m.skip_transfer_enemy = on;
        }
        none()
    });
    let class = member(class, ctx, "set_alien_control", |c| {
        let on = c.boolean(0)?;
        let id = c.id;
        let Some(control) = monster(c)?.and_then(|m| m.alien_control.as_mut()) else {
            return none();
        };
        *control = on;
        // The bloodsucker hidden and the player's weapon hidden by 10.4's
        // event while it lasts (TODO(#1133): the camera and screen effects
        // and the HUD flag).
        c.objects.push(Event::Visibility {
            object: id,
            hidden: on,
        });
        super::movement::hide_weapon_event(c, on);
        none()
    });
    let class = member(class, ctx, "fake_death_fall_down", |c| {
        let Some(fake) = monster(c)?.and_then(|m| m.fake_death.as_mut()) else {
            return Ok(boolean(false));
        };
        let started = !*fake;
        *fake = true;
        Ok(boolean(started))
    });
    let class = member(class, ctx, "fake_death_stand_up", |c| {
        // TODO(#1133): its stand-up test is CH-23's: the first try ends it.
        if let Some(fake) = monster(c)?.and_then(|m| m.fake_death.as_mut()) {
            *fake = false;
        }
        none()
    });
    let class = member(class, ctx, "set_custom_panic_threshold", |c| {
        let v = to_f32(c.number(0)?);
        if let Some(m) = monster(c)? {
            m.panic_threshold = v;
        }
        none()
    });
    let class = member(class, ctx, "set_default_panic_threshold", |c| {
        if let Some(m) = monster(c)? {
            m.panic_threshold = m.default_panic_threshold;
        }
        none()
    });
    // TODO(#1133): no hit is recorded (CH-21), so every monster's is "no
    // hit", as the fallback.
    member(class, ctx, "get_monster_hit_info", |c| {
        Ok(vec![
            c.o.new_object("MonsterHitInfo", MonsterHitInfo::default()),
        ])
    })
}

/// `MonsterHitInfo` (11.4): no constructor; its properties assign the copy.
fn hit_info_class() -> EngineClass {
    EngineClass::new("MonsterHitInfo")
        .property_with_objects(
            "who",
            |h: &MonsterHitInfo, _: &Objects| Ok(h.who.map_or(Value::Nil, Value::Object)),
            Some(Box::new(
                |h: &mut MonsterHitInfo, o: &Objects, v: &Value| {
                    h.who = match v {
                        Value::Nil => None,
                        v => match o.get::<Handle>(v) {
                            Some(Operand::Other(handle)) => Some(handle.part),
                            _ => return Err("MonsterHitInfo.who: game object expected".into()),
                        },
                    };
                    Ok(())
                },
            )),
        )
        .property_with_objects(
            "direction",
            |h: &MonsterHitInfo, o: &Objects| {
                let [x, y, z] = h.direction;
                Ok(o.new_object("vector", Vector::new(x, y, z)))
            },
            Some(Box::new(
                |h: &mut MonsterHitInfo, o: &Objects, v: &Value| match o.get::<Vector>(v) {
                    Some(Operand::Other(d)) => {
                        h.direction = [d.x, d.y, d.z];
                        Ok(())
                    }
                    _ => Err("MonsterHitInfo.direction: vector expected".into()),
                },
            )),
        )
        .writable_property(
            "time",
            |h: &MonsterHitInfo| Value::Number(f64::from(h.time)),
            |h, v| {
                h.time = to_bits32(lua_number(v));
                Ok(())
            },
        )
}

/// The classes of 11.4 and 11.6.
pub(super) fn classes(_: &Context) -> Vec<EngineClass> {
    let space = MONSTER_SPACE
        .into_iter()
        .fold(EngineClass::new("MonsterSpace"), |class, (name, value)| {
            class.constant(name, value)
        });
    vec![hit_info_class(), space]
}
