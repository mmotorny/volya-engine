// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The game graph (10): `game_graph()`'s `CGameGraph` and its vertices,
//! `GameGraph__CVertex`, over the simulation's graph.

use volya_engine_formats::game_graph::GraphVertex;

use super::{Context, Results, new_vector, one, truth, u16_arg};
use crate::Value;
use crate::classes::{EngineClass, Operand};

/// The part of the one `CGameGraph` object.
#[derive(Clone, Copy, Debug)]
pub struct GamePart;

/// A vertex as scripts hold it: its number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Vertex {
    pub number: u16,
}

/// `f` on vertex `v`'s record. A number past the table reads past it
/// (10.2, unestablished): fatal here (our wording).
fn with_vertex(
    ctx: &Context,
    what: &str,
    v: u16,
    f: impl FnOnce(&GraphVertex) -> Results,
) -> Results {
    ctx.with_simulation(what, |s| {
        let sim = s.simulation.as_ref().expect("running");
        match sim.graph.graph.vertex(v) {
            Some(vertex) => f(vertex),
            None => Err(ctx.no_object(what, &format!("no game vertex {v}"))),
        }
    })
}

pub(crate) fn classes(ctx: &Context) -> Vec<EngineClass> {
    let c = ctx.clone();
    let vertex = EngineClass::new("GameGraph__CVertex").method_with_objects(
        "level_point",
        move |v: &mut Vertex, o, _| {
            c.stopped()?;
            with_vertex(&c, "level_point", v.number, |x| {
                one(new_vector(o, x.level_point))
            })
        },
    );
    let c = ctx.clone();
    let vertex = vertex.method_with_objects("game_point", move |v: &mut Vertex, o, _| {
        c.stopped()?;
        with_vertex(&c, "game_point", v.number, |x| {
            one(new_vector(o, x.global_point))
        })
    });
    let c = ctx.clone();
    let vertex = vertex.method("level_id", move |v: &mut Vertex, _| {
        c.stopped()?;
        with_vertex(&c, "level_id", v.number, |x| {
            one(Value::Number(f64::from(x.level_id)))
        })
    });
    let c = ctx.clone();
    let vertex = vertex.method("level_vertex_id", move |v: &mut Vertex, _| {
        c.stopped()?;
        with_vertex(&c, "level_vertex_id", v.number, |x| {
            one(Value::Number(f64::from(x.level_vertex)))
        })
    });
    let c = ctx.clone();
    let graph = EngineClass::new("CGameGraph").method_with_objects(
        "vertex",
        move |_: &mut GamePart, o, args| {
            c.stopped()?;
            let number = u16_arg(args, 0, "vertex")?;
            one(o.new_object("GameGraph__CVertex", Vertex { number }))
        },
    );
    let c = ctx.clone();
    let graph = graph.method("valid_vertex_id", move |_: &mut GamePart, args| {
        c.stopped()?;
        let v = u16_arg(args, 0, "valid_vertex_id")?;
        c.with_simulation("valid_vertex_id", |s| {
            let sim = s.simulation.as_ref().expect("running");
            one(Value::Boolean(
                usize::from(v) < sim.graph.graph.vertices.len(),
            ))
        })
    });
    let c = ctx.clone();
    let graph = graph.method_with_objects("vertex_id", move |_: &mut GamePart, o, args| {
        c.stopped()?;
        match o.get::<Vertex>(args.first().unwrap_or(&Value::Nil)) {
            Some(Operand::Other(v)) => one(Value::Number(f64::from(v.number))),
            _ => Err("vertex_id: game vertex expected".into()),
        }
    });
    let c = ctx.clone();
    let graph = graph.method("accessible", move |_: &mut GamePart, args| {
        c.stopped()?;
        let v = u16_arg(args, 0, "accessible")?;
        c.with_simulation("accessible", |s| {
            let sim = s.simulation.as_mut().expect("running");
            // Past the vertices: a flag past theirs (10.2); none here.
            if args.len() < 2 {
                return one(Value::Boolean(sim.graph.accessible(v)));
            }
            sim.graph.set_accessible(v, truth(args, 1));
            Ok(Vec::new())
        })
    });
    vec![vertex, graph]
}
