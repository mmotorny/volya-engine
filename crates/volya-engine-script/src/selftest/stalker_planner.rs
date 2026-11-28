// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/stalker-planner.md on every target, one check per
//! section, over the stalker of script API W's checks
//! ([`super::action_planner::state`]): no simulation, so its alife
//! planner runs `action_no_alife`.

use super::action_planner::{STALKER, State, state};
use super::ensure;
use crate::ErrorKind;
use crate::action_planner::{NONE, WorldState};
use crate::classes::EnginePart;
use crate::game_object::GameObject;
use crate::game_object::movement::MovementRequest;
use crate::stalker_planner::*;

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("1 the root", root),
    ("1.2 evaluators", evaluators),
    ("1.3 combat and danger", combat_and_danger),
    ("2.1 death", death),
    ("2.2 life without a fight", life),
    ("3.3 gather items", gather_items),
    ("5.2 the think step", think_step),
    ("6 saving", saving),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| f().err().map(|e| format!("stalker planner {what}: {e}")))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// W's state with the clock at `ms`.
fn at(ms: u64) -> Result<State, String> {
    let t = state()?;
    t.services.borrow_mut().device.frame(ms, 0.0, false);
    Ok(t)
}

fn root_of(t: &State) -> Result<EnginePart, String> {
    t.s.motivation_manager(STALKER)
        .ok_or_else(|| "no root".into())
}

type Conditions = Vec<(u32, Vec<(u32, bool)>, Vec<(u32, bool)>)>;

fn pairs(w: &WorldState) -> Vec<(u32, bool)> {
    w.properties()
        .iter()
        .map(|p| (p.condition, p.value))
        .collect()
}

/// A planner's evaluator ids, its actions' (id, preconditions, effects),
/// its stored set and its current action and plan.
struct Seen {
    evaluators: Vec<u32>,
    actions: Conditions,
    parts: Vec<EnginePart>,
    stored: Vec<(u32, bool)>,
    current: Option<u32>,
    plan: Vec<u32>,
}

fn seen(t: &State, planner: EnginePart) -> Result<Seen, String> {
    t.s.with_planner(planner, |p| Seen {
        evaluators: p.evaluators.iter().map(|e| e.0).collect(),
        actions: (p.actions.iter())
            .map(|a| {
                let a2 = a.action.borrow();
                (a.id, pairs(&a2.preconditions), pairs(&a2.effects))
            })
            .collect(),
        parts: p.actions.iter().map(|a| a.object).collect(),
        stored: p.storage.0.borrow().clone(),
        current: p.current_action_id(),
        plan: p.plan.clone(),
    })
    .ok_or_else(|| "not a planner".into())
}

/// The inner planner of the root's action `id`.
fn inner(t: &State, id: u32) -> Result<EnginePart, String> {
    let root = seen(t, root_of(t)?)?;
    let at = (root.actions.iter().position(|a| a.0 == id)).ok_or("no such action")?;
    Ok(root.parts[at])
}

fn with_stalker(t: &State, f: impl FnOnce(&mut GameObject)) -> Result<(), String> {
    let (objects, _) = t.s.planner_shared().map_err(|e| e.message)?;
    let mut o = objects.borrow_mut();
    f(o.get_mut(STALKER).ok_or("no stalker")?);
    Ok(())
}

fn request(t: &State) -> Result<MovementRequest, String> {
    let (objects, _) = t.s.planner_shared().map_err(|e| e.message)?;
    let o = objects.borrow();
    let o = o.get(STALKER).ok_or("no stalker")?;
    (o.movement.stalker.as_ref())
        .map(|p| p.request.clone())
        .ok_or_else(|| "no movement".into())
}

fn evaluate(t: &mut State, planner: &str, id: u32) -> Result<Result<bool, String>, String> {
    let code = format!("return {planner}:evaluator({id}):evaluate()");
    match t.s.exec("=evaluate", code.as_bytes()) {
        Ok(v) => Ok(Ok(matches!(v.first(), Some(crate::Value::Boolean(true))))),
        Err(e) if e.kind == ErrorKind::Runtime => Ok(Err(e.message)),
        Err(e) => Err(e.message),
    }
}

const ROOT: &str = "obj(10):motivation_action_manager()";

/// 1.1 – 1.3, 2: the root's parts and the inner planners'.
fn root() -> Result<(), String> {
    let t = at(10_000)?;
    let root = seen(&t, root_of(&t)?)?;
    ensure(root.evaluators == [0, 2, 4, 6, 7, 8, 42], || {
        format!("root evaluators {:?}", root.evaluators)
    })?;
    let ids: Vec<u32> = root.actions.iter().map(|a| a.0).collect();
    ensure(ids == [2, 56, 57, 58, 59, 60], || {
        format!("root actions {ids:?}")
    })?;
    let want: Conditions = vec![
        (
            2,
            vec![(0, true), (6, true), (7, false), (8, false), (42, false)],
            vec![(6, false)],
        ),
        (56, vec![(0, false), (2, false)], vec![(2, true)]),
        (
            57,
            vec![
                (0, true),
                (4, false),
                (6, false),
                (7, false),
                (8, false),
                (42, false),
            ],
            vec![(4, true)],
        ),
        (
            58,
            vec![(0, true), (7, true), (42, false)],
            vec![(7, false)],
        ),
        (59, vec![(0, true), (42, true)], vec![(42, false)]),
        (
            60,
            vec![(0, true), (7, false), (8, true), (42, false)],
            vec![(8, false)],
        ),
    ];
    ensure(root.actions == want, || {
        format!("root conditions {:?}", root.actions)
    })?;
    ensure(root.stored == [(42, false)], || {
        format!("root set {:?}", root.stored)
    })?;
    let death = seen(&t, inner(&t, ACTION_DEATH_PLANNER)?)?;
    ensure(death.evaluators == [1, 2], || {
        format!("death evaluators {:?}", death.evaluators)
    })?;
    let want: Conditions = vec![
        (0, vec![(1, true)], vec![(2, true)]),
        (1, vec![(1, false)], vec![(1, true)]),
    ];
    ensure(death.actions == want, || {
        format!("death actions {:?}", death.actions)
    })?;
    ensure(death.stored == [(1, false)], || {
        format!("death set {:?}", death.stored)
    })?;
    let alife = seen(&t, inner(&t, ACTION_ALIFE_PLANNER)?)?;
    ensure(alife.evaluators == [3, 4, 5], || {
        format!("alife evaluators {:?}", alife.evaluators)
    })?;
    let want: Conditions = vec![
        (3, vec![(3, false), (4, false)], vec![(4, true)]),
        (4, vec![(3, true), (5, true)], vec![(5, false)]),
        (5, vec![(3, true), (4, false), (5, false)], vec![(4, true)]),
    ];
    ensure(alife.actions == want, || {
        format!("alife actions {:?}", alife.actions)
    })?;
    let anomaly = seen(&t, inner(&t, ACTION_ANOMALY_PLANNER)?)?;
    ensure(anomaly.evaluators == [42, 43], || {
        format!("anomaly evaluators {:?}", anomaly.evaluators)
    })?;
    let want: Conditions = vec![
        (10, vec![(43, true)], vec![(43, false)]),
        (11, vec![(42, true), (43, false)], vec![(42, false)]),
    ];
    ensure(anomaly.actions == want, || {
        format!("anomaly actions {:?}", anomaly.actions)
    })?;
    ensure(anomaly.stored == [(42, false)], || {
        format!("anomaly set {:?}", anomaly.stored)
    })?;
    for (id, goal) in [(56, (2, true)), (57, (4, true)), (59, (42, false))] {
        let planner = inner(&t, id)?;
        let got = t.s.with_planner(planner, |p| pairs(&p.goal));
        ensure(got == Some(vec![goal]), || {
            format!("planner {id}'s goal {got:?}")
        })?;
    }
    Ok(())
}

/// 1.2: the root's evaluators.
fn evaluators() -> Result<(), String> {
    let mut t = at(1_000)?;
    let got = evaluate(&mut t, ROOT, ENEMY)?;
    ensure(matches!(&got, Err(e) if e.contains("#1205")), || {
        format!("enemy below 3 s with no record: {got:?}")
    })?;
    let mut t = at(10_000)?;
    for (id, want) in [
        (0, true),
        (2, false),
        (4, false),
        (6, false),
        (7, false),
        (8, false),
        (42, false),
    ] {
        let got = evaluate(&mut t, ROOT, id)?;
        ensure(got == Ok(want), || format!("evaluator {id}: {got:?}"))?;
    }
    with_stalker(&t, |o| {
        let c = &mut o.stalker.as_mut().unwrap().choices;
        c.enemy_chosen_at = Some(7_500);
        c.danger = true;
        c.item = Some(20);
    })?;
    for (id, want) in [(7, true), (8, true), (6, true)] {
        let got = evaluate(&mut t, ROOT, id)?;
        ensure(got == Ok(want), || {
            format!("chosen, evaluator {id}: {got:?}")
        })?;
    }
    with_stalker(&t, |o| {
        o.stalker.as_mut().unwrap().choices.enemy_chosen_at = Some(7_000);
        o.entity.as_mut().unwrap().health = 0.0;
    })?;
    for (id, want) in [(7, false), (0, false)] {
        let got = evaluate(&mut t, ROOT, id)?;
        ensure(got == Ok(want), || {
            format!("3 s on, evaluator {id}: {got:?}")
        })?;
    }
    // The anomaly rule: a stored anomaly without an enemy, then with one.
    let storage = (t.s.with_planner(root_of(&t)?, |p| p.storage.clone())).ok_or("root")?;
    storage.set(ANOMALY, true);
    let got = evaluate(&mut t, ROOT, ANOMALY)?;
    ensure(got == Ok(true), || format!("stored anomaly: {got:?}"))?;
    with_stalker(&t, |o| o.stalker.as_mut().unwrap().choices.enemy = Some(20))?;
    let got = evaluate(&mut t, ROOT, ANOMALY)?;
    ensure(matches!(&got, Err(e) if e.contains("#1227")), || {
        format!("anomaly with an enemy: {got:?}")
    })
}

/// 1.3: the combat and danger planners are CH-18's and CH-19's.
fn combat_and_danger() -> Result<(), String> {
    for (danger, issue, first) in [(false, "#1227", 58), (true, "#1228", 60)] {
        let mut t = at(10_000)?;
        with_stalker(&t, |o| {
            let c = &mut o.stalker.as_mut().unwrap().choices;
            if danger {
                c.danger = true;
            } else {
                c.enemy = Some(20);
            }
        })?;
        let got = t.s.update_motivation_manager(STALKER);
        ensure(
            matches!(&got, Err(e) if e.kind == ErrorKind::Fatal && e.message.contains(issue)),
            || format!("{issue}: {got:?}"),
        )?;
        let plan = seen(&t, root_of(&t)?)?.plan;
        ensure(plan == [first, 57], || format!("{issue}'s plan {plan:?}"))?;
    }
    Ok(())
}

/// 2.1, 3.2: a dead stalker runs `action_dying`, then `action_dead`.
fn death() -> Result<(), String> {
    let mut t = at(10_000)?;
    with_stalker(&t, |o| o.entity.as_mut().unwrap().health = 0.0)?;
    t.s.update_motivation_manager(STALKER)
        .map_err(|e| e.message)?;
    let root = seen(&t, root_of(&t)?)?;
    ensure(root.current == Some(56), || {
        format!("root runs {:?}", root.current)
    })?;
    let death = inner(&t, ACTION_DEATH_PLANNER)?;
    let d = seen(&t, death)?;
    ensure(
        d.current == Some(ACTION_DYING) && d.stored == [(1, true)],
        || format!("dying: {:?} {:?}", d.current, d.stored),
    )?;
    t.s.update_motivation_manager(STALKER)
        .map_err(|e| e.message)?;
    t.s.update_motivation_manager(STALKER)
        .map_err(|e| e.message)?;
    let d = seen(&t, death)?;
    ensure(d.current == Some(ACTION_DEAD), || {
        format!("then {:?}", d.current)
    })
}

/// 2.2, 3.4: no simulation, so `action_no_alife` runs.
fn life() -> Result<(), String> {
    let mut t = at(10_000)?;
    with_stalker(&t, |o| {
        let r = &mut o.movement.stalker.as_mut().unwrap().request;
        r.desired_position_set = true;
        r.desired_direction_set = true;
    })?;
    t.s.update_motivation_manager(STALKER)
        .map_err(|e| e.message)?;
    let root = seen(&t, root_of(&t)?)?;
    ensure(root.plan == [57] && root.current == Some(57), || {
        format!("root {:?} {:?}", root.plan, root.current)
    })?;
    let alife = seen(&t, inner(&t, ACTION_ALIFE_PLANNER)?)?;
    ensure(alife.current == Some(ACTION_NO_ALIFE), || {
        format!("alife runs {:?}", alife.current)
    })?;
    let r = request(&t)?;
    let want = (false, false, 0, 0, 1, 0, 1);
    let got = (
        r.desired_position_set,
        r.desired_direction_set,
        r.target.path_type,
        r.detail_path_type,
        r.target.body_state,
        r.target.movement_type,
        r.target.mental_state,
    );
    ensure(got == want, || format!("no_alife's targets {got:?}"))?;
    // It keeps running; the death planner is not set up yet.
    t.s.update_motivation_manager(STALKER)
        .map_err(|e| e.message)?;
    let death = seen(&t, inner(&t, ACTION_DEATH_PLANNER)?)?;
    ensure(death.current == Some(NONE), || {
        format!("death {:?}", death.current)
    })
}

/// 3.3: a chosen item leads the plan with gather items.
fn gather_items() -> Result<(), String> {
    let mut t = at(10_000)?;
    with_stalker(&t, |o| o.stalker.as_mut().unwrap().choices.item = Some(20))?;
    let item = {
        let (objects, _) = t.s.planner_shared().map_err(|e| e.message)?;
        let o = objects.borrow();
        let i = o.get(20).ok_or("no rifle")?;
        (
            i.level_vertex,
            [i.placement[3][0], i.placement[3][1], i.placement[3][2]],
        )
    };
    t.s.update_motivation_manager(STALKER)
        .map_err(|e| e.message)?;
    let root = seen(&t, root_of(&t)?)?;
    ensure(root.plan == [2, 57], || format!("plan {:?}", root.plan))?;
    let r = request(&t)?;
    let got = (
        r.target.path_type,
        r.target.mental_state,
        r.dest_level_vertex,
        r.desired_position,
        r.desired_position_set,
    );
    let want = (1, 0, Some(item.0), item.1, true);
    ensure(got == want, || format!("gather's targets {got:?}"))?;
    // The item gone: the alife planner, the desired position dropped.
    with_stalker(&t, |o| o.stalker.as_mut().unwrap().choices.item = None)?;
    t.s.update_motivation_manager(STALKER)
        .map_err(|e| e.message)?;
    let root = seen(&t, root_of(&t)?)?;
    ensure(root.current == Some(57), || {
        format!("then {:?}", root.current)
    })?;
    ensure(!request(&t)?.desired_position_set, || {
        "position dropped".into()
    })
}

/// 5.1 step 3, 5.2: the think step updates the root, unless scripts have
/// taken the stalker over.
fn think_step() -> Result<(), String> {
    let mut t = at(10_000)?;
    with_stalker(&t, |o| o.scripted.as_mut().unwrap().controlled = true)?;
    t.s.think_step(STALKER).map_err(|e| e.message)?;
    let root = seen(&t, root_of(&t)?)?;
    ensure(root.current == Some(NONE), || {
        format!("scripted: {:?}", root.current)
    })?;
    with_stalker(&t, |o| o.scripted.as_mut().unwrap().controlled = false)?;
    t.s.think_step(STALKER).map_err(|e| e.message)?;
    let root = seen(&t, root_of(&t)?)?;
    ensure(root.current == Some(57), || {
        format!("thinking: {:?}", root.current)
    })
}

/// 6: the root's saved state, its inner planners' in action order, then
/// its stored set.
fn saving() -> Result<(), String> {
    let t = at(10_000)?;
    let saved =
        t.s.save_motivation_manager(STALKER)
            .map_err(|e| e.message)?;
    let pair = |id: u8, v: u8| [1, 0, 0, 0, id, 0, 0, 0, v];
    let empty = [0u8, 0, 0, 0];
    let mut want = Vec::new();
    want.extend(pair(1, 0)); // 56, death
    want.extend(empty); // 57, alife
    want.extend(empty); // 58, combat
    want.extend(pair(42, 0)); // 59, anomaly
    want.extend(empty); // 60, danger
    want.extend(pair(42, 0)); // the root's
    ensure(saved == want, || format!("saved {saved:?}"))?;
    let mut changed = saved.clone();
    changed[8] = 1;
    let read =
        t.s.load_motivation_manager(STALKER, &changed)
            .map_err(|e| e.message)?;
    ensure(read == changed.len(), || format!("read {read}"))?;
    let death = seen(&t, inner(&t, ACTION_DEATH_PLANNER)?)?;
    ensure(death.stored == [(1, true)], || {
        format!("loaded {:?}", death.stored)
    })?;
    let short = t.s.load_motivation_manager(STALKER, &saved[..10]);
    ensure(short.is_err(), || "a short save".into())
}
