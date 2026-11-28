// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Smart-terrain tasks (11) and the brains and movement of offline
//! characters (12): `CALifeSmartTerrainTask`, `CALifeMonsterBrain`,
//! `CALifeHumanBrain`, `CALifeMonsterMovementManager`,
//! `CALifeMonsterDetailPathManager` and `CALifeMonsterPatrolPathManager`
//! over the simulation's travel state (`OS` 7).

use volya_engine_core::simulation::{NO_ID, Simulation, Target, Travel};
use volya_engine_formats::patrol_path::PatrolPath;

use super::{
    AlifeSimulator, Context, Results, arg, new_vector, one, text, truth, u16_arg, u32_arg,
};
use crate::Value;
use crate::classes::{EngineClass, Objects, Operand};
use crate::values::{Vector, to_f32};

/// A smart-terrain task (11): the point it names, or none when the path
/// has no such point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SmartTerrainTask {
    pub point: Option<Target>,
}

/// The parts of a character's brain and its managers: the object's id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Brain {
    pub id: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MovementManager {
    pub id: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DetailPathManager {
    pub id: u16,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PatrolPart {
    pub id: u16,
}

/// The movement manager's path types (12.2).
pub const PATH_DETAIL: u32 = 0;
pub const PATH_PATROL: u32 = 2;
pub const PATH_NONE: u32 = 3;

/// A brain's state the simulation doesn't keep (12).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BrainState {
    /// The movement manager's path type; 0 before any is set (our
    /// choice: unestablished).
    pub path_type: u32,
    pub patrol: PatrolPathManager,
}

/// The patrol path manager (12.4).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PatrolPathManager {
    /// The patrol path's name; none when not set or no path has it.
    pub path: Option<String>,
    pub start_type: u32,
    pub route_type: u32,
    pub use_randomness: bool,
    pub start_vertex_index: u32,
    pub started: bool,
    pub completed: bool,
    pub current: Option<u32>,
    pub previous: Option<u32>,
    /// The object's own generator (believed): its state, 0 at first (our
    /// choice).
    pub random: u32,
}

impl PatrolPathManager {
    /// The update (12.4) of an object at `at` (game vertex, level vertex)
    /// on `sim`'s paths. `Err` for the believed division by zero.
    pub fn update(&mut self, sim: &Simulation, at: (u16, u32)) -> Result<(), String> {
        let Some(path) = self.path.as_deref().and_then(|n| sim.paths.get(n)) else {
            return Ok(());
        };
        if !self.started {
            self.previous = None;
            self.started = true;
            self.completed = false;
            self.current = match self.start_type {
                1 => u32::try_from(path.points.len())
                    .ok()
                    .map(|n| n.wrapping_sub(1)),
                2 => nearest(sim, path, at.0),
                3 => Some(self.start_vertex_index),
                // 0, and the start types read past the four cases (our
                // choice: point 0).
                _ => Some(0),
            };
        } else if self.completed {
            return Ok(());
        }
        let Some(current) = self.current else {
            return Ok(());
        };
        let Some(point) = path.points.get(current as usize) else {
            return Ok(());
        };
        if (point.game_vertex, point.level_vertex) != at {
            return Ok(());
        }
        let links: Vec<u32> = (path.links.iter())
            .filter(|l| l.from == current)
            .map(|l| l.to)
            .collect();
        let candidates: Vec<u32> = (links.iter().copied())
            .filter(|&to| Some(to) != self.previous)
            .collect();
        if candidates.is_empty() && self.use_randomness {
            return Err("integer division by zero".into());
        }
        if !candidates.is_empty() {
            let r = if self.use_randomness {
                self.random = self.random.wrapping_mul(0x0808_8405).wrapping_add(1);
                self.random % candidates.len() as u32
            } else {
                0
            };
            self.previous = Some(current);
            self.current = Some(candidates[r as usize]);
        } else if self.route_type == 0 || links.is_empty() {
            // The point the choice then takes is unestablished: it stays.
            self.completed = true;
        } else {
            self.current = Some(links[0]);
        }
        Ok(())
    }

    /// The current point's target, if any.
    fn target(&self, sim: &Simulation) -> Option<Target> {
        let path = sim.paths.get(self.path.as_deref()?)?;
        let p = path.points.get(self.current? as usize)?;
        Some(Target {
            game_vertex: p.game_vertex,
            level_vertex: p.level_vertex,
            position: p.position,
        })
    }
}

/// The nearest point (12.4): the first on game vertex `gv`, else the one
/// whose game vertex's global point is closest to `gv`'s (of equals the
/// first).
fn nearest(sim: &Simulation, path: &PatrolPath, gv: u16) -> Option<u32> {
    if let Some(i) = path.points.iter().position(|p| p.game_vertex == gv) {
        return u32::try_from(i).ok();
    }
    let graph = &sim.graph.graph;
    let from = graph.vertex(gv)?.global_point;
    let mut best: Option<(f32, usize)> = None;
    for (i, p) in path.points.iter().enumerate() {
        let Some(v) = graph.vertex(p.game_vertex) else {
            continue;
        };
        let d = (0..3)
            .map(|k| (v.global_point[k] - from[k]).powi(2))
            .sum::<f32>()
            .sqrt();
        if best.is_none_or(|(b, _)| d < b) {
            best = Some((d, i));
        }
    }
    best.and_then(|(_, i)| u32::try_from(i).ok())
}

/// The point `index` of patrol path `name` (11).
pub fn task_point(sim: &Simulation, name: &str, index: u32) -> Option<Option<Target>> {
    let path = sim.paths.get(name)?;
    Some(path.points.get(index as usize).map(|p| Target {
        game_vertex: p.game_vertex,
        level_vertex: p.level_vertex,
        position: p.position,
    }))
}

/// `f` on object `id`'s travel and brain state; a missing object or one
/// without travel reads through no object (our wording).
fn with_travel<R>(
    ctx: &Context,
    what: &str,
    id: u16,
    f: impl FnOnce(&mut AlifeSimulator) -> Result<R, String>,
) -> Result<R, String> {
    ctx.with_simulation(what, |s| {
        let has = (s.simulation.as_ref())
            .and_then(|sim| sim.object(id))
            .is_some_and(|o| o.travel.is_some());
        if !has {
            return Err(ctx.no_object(what, &format!("object {id} has no brain")));
        }
        s.brains.entry(id).or_default();
        f(s)
    })
}

fn travel(s: &mut AlifeSimulator, id: u16) -> &mut Travel {
    (s.simulation.as_mut())
        .and_then(|sim| sim.object_mut(id))
        .and_then(|o| o.travel.as_deref_mut())
        .expect("checked")
}

fn location(s: &AlifeSimulator, id: u16) -> (u16, u32) {
    let o = s
        .simulation
        .as_ref()
        .and_then(|sim| sim.object(id))
        .expect("checked");
    let b = o.record.object.object_block();
    (b.game_graph_vertex, b.level_graph_node)
}

/// The detail path manager's update (12.3): the movement update of `OS`
/// 7.5, which runs only when the game time is past the travel clock.
fn detail_update(ctx: &Context, s: &mut AlifeSimulator, id: u16) {
    let (now, factor) = (ctx.game_time(), s.normal_time_factor);
    if let Some(sim) = s.simulation.as_mut() {
        sim.movement_update(id, now, factor);
    }
}

/// The movement manager's update (12.2).
fn movement_update(ctx: &Context, s: &mut AlifeSimulator, id: u16) -> Result<(), String> {
    match s.brains.get(&id).map_or(PATH_DETAIL, |b| b.path_type) {
        PATH_DETAIL => detail_update(ctx, s, id),
        PATH_PATROL => {
            let at = location(s, id);
            let sim = s.simulation.as_ref().expect("checked");
            let mut patrol = s
                .brains
                .get(&id)
                .map(|b| b.patrol.clone())
                .unwrap_or_default();
            let result = patrol.update(sim, at);
            let target = patrol.target(sim);
            s.brains.entry(id).or_default().patrol = patrol;
            result.map_err(|e| ctx.no_object("update", &e))?;
            let Some(target) = target else {
                return Err(ctx.no_object("update", "the patrol path manager has no point"));
            };
            travel(s, id).target = target;
            detail_update(ctx, s, id);
        }
        _ => {}
    }
    Ok(())
}

/// `brain:update()` (12.1).
fn brain_update(ctx: &Context, s: &mut AlifeSimulator, id: u16) -> Result<(), String> {
    let now = ctx.game_time();
    let sim = s.simulation.as_mut().expect("checked");
    sim.choose_smart_terrain(id, now);
    let terrain = sim.smart_terrain(id).filter(|&t| t != NO_ID);
    let Some(terrain) = terrain else {
        s.brains.entry(id).or_default().path_type = PATH_NONE;
        return movement_update(ctx, s, id);
    };
    // The task's point; a task naming no path or point leaves the target
    // (our choice: the scripts' `task` gives a path name, `OS` 7.3).
    let target =
        (sim.smart_terrain_task(terrain, id)).and_then(|name| task_point(sim, &name, 0).flatten());
    s.brains.entry(id).or_default().path_type = PATH_DETAIL;
    if let Some(target) = target {
        travel(s, id).target = target;
    }
    movement_update(ctx, s, id)
}

fn number(v: f64) -> Value {
    Value::Number(v)
}

fn task_arg(o: &Objects, v: &Value) -> Option<SmartTerrainTask> {
    match o.get::<SmartTerrainTask>(v) {
        Some(Operand::Other(t)) => Some(t),
        _ => None,
    }
}

fn task_class(ctx: &Context) -> EngineClass {
    let make = |ctx: &Context| {
        let c = ctx.clone();
        move |_, args: &[Value]| -> Result<SmartTerrainTask, String> {
            c.stopped()?;
            let name = text(args, 0, "CALifeSmartTerrainTask")?;
            let index = match arg(args, 1) {
                Value::Nil => 0,
                _ => u32_arg(args, 1, "CALifeSmartTerrainTask")?,
            };
            c.with_simulation("CALifeSmartTerrainTask", |s| {
                let sim = s.simulation.as_ref().expect("running");
                match task_point(sim, &name, index) {
                    Some(point) => Ok(SmartTerrainTask { point }),
                    None => {
                        Err(c
                            .no_object("CALifeSmartTerrainTask", &format!("no patrol path {name}")))
                    }
                }
            })
        }
    };
    let mut class = EngineClass::new("CALifeSmartTerrainTask")
        .constructor(1, |_| Some(0), make(ctx))
        .constructor(2, |_| Some(0), make(ctx))
        .constructible();
    for name in ["game_vertex_id", "level_vertex_id", "position"] {
        let c = ctx.clone();
        class = class.method_with_objects(name, move |t: &mut SmartTerrainTask, o, _| {
            c.stopped()?;
            let Some(p) = t.point else {
                return Err(c.no_object(name, "the task has no point"));
            };
            one(match name {
                "game_vertex_id" => number(f64::from(p.game_vertex)),
                "level_vertex_id" => number(f64::from(p.level_vertex)),
                _ => new_vector(o, p.position),
            })
        });
    }
    class
}

fn brain_class(ctx: &Context) -> [EngineClass; 2] {
    let c = ctx.clone();
    let brain = EngineClass::new("CALifeMonsterBrain").method_with_objects(
        "movement",
        move |b: &mut Brain, o, _| {
            c.stopped()?;
            with_travel(&c, "movement", b.id, |_| Ok(()))?;
            one(o.new_object("CALifeMonsterMovementManager", MovementManager { id: b.id }))
        },
    );
    let c = ctx.clone();
    let brain = brain.method("can_choose_alife_tasks", move |b: &mut Brain, args| {
        c.stopped()?;
        let on = truth(args, 0);
        with_travel(&c, "can_choose_alife_tasks", b.id, |s| {
            travel(s, b.id).can_choose = on;
            Ok(Vec::new())
        })
    });
    let c = ctx.clone();
    let brain = brain.method("update", move |b: &mut Brain, _| {
        c.stopped()?;
        with_travel(&c, "update", b.id, |s| brain_update(&c, s, b.id))?;
        Ok(Vec::new())
    });
    [
        brain,
        EngineClass::new("CALifeHumanBrain").base("CALifeMonsterBrain"),
    ]
}

fn movement_class(ctx: &Context) -> EngineClass {
    let c = ctx.clone();
    let class = EngineClass::new("CALifeMonsterMovementManager").method_with_objects(
        "detail",
        move |m: &mut MovementManager, o, _| {
            c.stopped()?;
            one(o.new_object(
                "CALifeMonsterDetailPathManager",
                DetailPathManager { id: m.id },
            ))
        },
    );
    let c = ctx.clone();
    let class = class.method_with_objects("patrol", move |m: &mut MovementManager, o, _| {
        c.stopped()?;
        one(o.new_object("CALifeMonsterPatrolPathManager", PatrolPart { id: m.id }))
    });
    let c = ctx.clone();
    let class = class.method("path_type", move |m: &mut MovementManager, args| {
        c.stopped()?;
        let set = match args.first() {
            Some(_) => Some(u32_arg(args, 0, "path_type")?),
            None => None,
        };
        with_travel(&c, "path_type", m.id, |s| {
            let brain = s.brains.entry(m.id).or_default();
            match set {
                Some(t) => {
                    brain.path_type = t;
                    Ok(Vec::new())
                }
                None => one(number(f64::from(brain.path_type))),
            }
        })
    });
    let class = class.method("actual", |_: &mut MovementManager, _| {
        one(Value::Boolean(true))
    });
    class.method("completed", |_: &mut MovementManager, _| {
        one(Value::Boolean(true))
    })
}

fn detail_class(ctx: &Context) -> EngineClass {
    let c = ctx.clone();
    let class = EngineClass::new("CALifeMonsterDetailPathManager").method_with_objects(
        "target",
        move |d: &mut DetailPathManager, o, args| {
            c.stopped()?;
            let task = task_arg(o, arg(args, 0));
            let full = match args.len() {
                3 => Some((
                    u16_arg(args, 0, "target")?,
                    u32_arg(args, 1, "target")?,
                    match o.get::<Vector>(arg(args, 2)) {
                        Some(Operand::Other(v)) => [v.x, v.y, v.z],
                        _ => return Err("target: vector expected".into()),
                    },
                )),
                _ => None,
            };
            let vertex = match (&task, &full) {
                (None, None) => Some(u16_arg(args, 0, "target")?),
                _ => None,
            };
            with_travel(&c, "target", d.id, |s| {
                let target = match (task, full, vertex) {
                    (Some(t), _, _) => match t.point {
                        Some(p) => p,
                        None => return Err(c.no_object("target", "the task has no point")),
                    },
                    (_, Some((gv, lv, p)), _) => Target {
                        game_vertex: gv,
                        level_vertex: lv,
                        position: p,
                    },
                    (_, _, Some(gv)) => {
                        let sim = s.simulation.as_ref().expect("running");
                        let Some(v) = sim.graph.graph.vertex(gv) else {
                            return Err(c.no_object("target", &format!("no game vertex {gv}")));
                        };
                        Target {
                            game_vertex: gv,
                            level_vertex: v.level_vertex,
                            position: v.level_point,
                        }
                    }
                    _ => unreachable!("one form applies"),
                };
                travel(s, d.id).target = target;
                Ok(Vec::new())
            })
        },
    );
    let mut class = class;
    for name in ["completed", "actual", "failed"] {
        let c = ctx.clone();
        class = class.method(name, move |d: &mut DetailPathManager, _| {
            c.stopped()?;
            with_travel(&c, name, d.id, |s| {
                let at = location(s, d.id);
                let t = travel(s, d.id);
                one(Value::Boolean(match name {
                    "completed" => (t.target.game_vertex, t.target.level_vertex) == at,
                    "actual" => t.path.last() == Some(&t.target.game_vertex),
                    _ => t.path.is_empty(),
                }))
            })
        });
    }
    // 12.3: registered as `speed` followed by a tab.
    let c = ctx.clone();
    class.method("speed\t", move |d: &mut DetailPathManager, args| {
        c.stopped()?;
        let set = match args.first() {
            Some(_) => Some(to_f32(super::number(args, 0, "speed")?)),
            None => None,
        };
        with_travel(&c, "speed", d.id, |s| {
            let t = travel(s, d.id);
            match set {
                Some(v) => {
                    t.speed = v;
                    Ok(Vec::new())
                }
                None => one(number(f64::from(t.speed))),
            }
        })
    })
}

fn patrol_class(ctx: &Context) -> EngineClass {
    /// One member over the manager's state (and the simulation for the
    /// path and the current point).
    fn on(
        ctx: &Context,
        name: &'static str,
        f: impl Fn(&Context, &mut AlifeSimulator, u16, &Objects, &[Value]) -> Results + 'static,
    ) -> impl Fn(&mut PatrolPart, &Objects, &[Value]) -> Results + 'static {
        let c = ctx.clone();
        move |p: &mut PatrolPart, o, args| {
            c.stopped()?;
            with_travel(&c, name, p.id, |s| f(&c, s, p.id, o, args))
        }
    }
    fn patrol(s: &mut AlifeSimulator, id: u16) -> &mut PatrolPathManager {
        &mut s.brains.entry(id).or_default().patrol
    }
    let point = |c: &Context, s: &mut AlifeSimulator, id: u16, name: &str| {
        let sim = s.simulation.as_ref().expect("running");
        (patrol_ref(s, id).target(sim)).ok_or_else(|| c.no_object(name, "no current point"))
    };
    fn patrol_ref(s: &AlifeSimulator, id: u16) -> PatrolPathManager {
        s.brains
            .get(&id)
            .map(|b| b.patrol.clone())
            .unwrap_or_default()
    }
    let class = EngineClass::new("CALifeMonsterPatrolPathManager");
    let class = class.method_with_objects(
        "path",
        on(ctx, "path", |_, s, id, _, args| {
            let name = text(args, 0, "path")?;
            let exists = s
                .simulation
                .as_ref()
                .is_some_and(|sim| sim.paths.get(&name).is_some());
            let p = patrol(s, id);
            let new = exists.then_some(name);
            if !(p.started && p.path == new) {
                p.started = false;
            }
            p.path = new;
            Ok(Vec::new())
        }),
    );
    let mut class = class;
    for name in ["start_type", "route_type", "use_randomness"] {
        class = class.method_with_objects(
            name,
            on(ctx, name, move |_, s, id, _, args| {
                let p = patrol(s, id);
                if args.is_empty() {
                    return one(match name {
                        "start_type" => number(f64::from(p.start_type)),
                        "route_type" => number(f64::from(p.route_type)),
                        _ => Value::Boolean(p.use_randomness),
                    });
                }
                match name {
                    "start_type" => p.start_type = u32_arg(args, 0, name)?,
                    "route_type" => p.route_type = u32_arg(args, 0, name)?,
                    _ => p.use_randomness = truth(args, 0),
                }
                Ok(Vec::new())
            }),
        );
    }
    let class = class.method_with_objects(
        "start_vertex_index",
        on(ctx, "start_vertex_index", |_, s, id, _, args| {
            patrol(s, id).start_vertex_index = u32_arg(args, 0, "start_vertex_index")?;
            Ok(Vec::new())
        }),
    );
    let class = class.method_with_objects(
        "actual",
        on(ctx, "actual", |_, s, id, _, _| {
            one(Value::Boolean(patrol(s, id).started))
        }),
    );
    let class = class.method_with_objects(
        "completed",
        on(ctx, "completed", |_, s, id, _, _| {
            let p = patrol(s, id);
            one(Value::Boolean(p.started && p.completed))
        }),
    );
    let class = class.method_with_objects(
        "target_game_vertex_id",
        on(ctx, "target_game_vertex_id", move |c, s, id, _, _| {
            point(c, s, id, "target_game_vertex_id").map(|t| vec![number(f64::from(t.game_vertex))])
        }),
    );
    let class = class.method_with_objects(
        "target_level_vertex_id",
        on(ctx, "target_level_vertex_id", move |c, s, id, _, _| {
            point(c, s, id, "target_level_vertex_id")
                .map(|t| vec![number(f64::from(t.level_vertex))])
        }),
    );
    class.method_with_objects(
        "target_position",
        on(ctx, "target_position", move |c, s, id, o, _| {
            point(c, s, id, "target_position").map(|t| vec![new_vector(o, t.position)])
        }),
    )
}

pub(crate) fn classes(ctx: &Context) -> Vec<EngineClass> {
    let [brain, human] = brain_class(ctx);
    vec![
        task_class(ctx),
        movement_class(ctx),
        detail_class(ctx),
        patrol_class(ctx),
        brain,
        human,
    ]
}
