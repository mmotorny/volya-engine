// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's action planner, checked against
//! docs/behavior/script-api-action-planner.md (cited by section): on a
//! game state started with the shipped scripts over a New Game's
//! simulation, an `esc_lager` campfire stalker spawns bound by its
//! section's binding, `xr_motivator`'s, whose `reinit` and `net_spawn`
//! build the shipped scripts' planners on its motivation manager (10,
//! 12.1): `state_mgr`'s evaluators and actions and its own body-state
//! planner, the anomaly evaluator replaced by a constant, the script
//! themes' sound collections on the stalker's profile, and the camp
//! scheme's parts (its `add_to_binder` in place of the logic set-up the
//! scripts reach later, on other specs' names). Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#[path = "support/campfire.rs"]
mod campfire;
#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;
#[path = "support/shipped_scripts.rs"]
mod shipped_scripts;

use campfire::started;
use volya_engine_script::Value;
use volya_engine_script::action_planner::{Evaluation, Evaluator, WorldState};
use volya_engine_script::classes::EnginePart;
use volya_engine_script::loader::Scripts;

/// The ids, preconditions and effects of a planner's actions, as (id,
/// value) pairs.
type Conditions = Vec<(u32, Vec<(u32, bool)>, Vec<(u32, bool)>)>;

/// A planner's evaluators' ids and its actions' conditions.
fn conditions(s: &Scripts, planner: EnginePart) -> (Vec<u32>, Conditions) {
    let pairs = |w: &WorldState| -> Vec<(u32, bool)> {
        w.properties()
            .iter()
            .map(|p| (p.condition, p.value))
            .collect()
    };
    s.with_planner(planner, |p| {
        let evaluators = p.evaluators.iter().map(|e| e.0).collect();
        let actions = (p.actions.iter())
            .map(|a| {
                let action = a.action.borrow();
                (a.id, pairs(&action.preconditions), pairs(&action.effects))
            })
            .collect();
        (evaluators, actions)
    })
    .unwrap()
}

/// 10, 12.1: the campfire stalker's binder builds the shipped scripts'
/// planners on its motivation manager, and `state_mgr`'s own planner.
#[test]
fn a_campfire_stalker_builds_its_planners() {
    let Some((mut scripts, stalker, spawned)) = started(None) else {
        return;
    };
    let collections = (scripts.sounds().borrow().creatures.players)
        .get(&stalker)
        .map_or(0, |p| p.collections.len());
    let s = scripts.scripts();
    assert_eq!(spawned, Ok(()), "spawning; {:?}", s.take_failures());
    // `net_spawn`'s sound loading, on its profile (`profile_name()`):
    // the script themes' collections (creature-sounds.md 1.2 step 3).
    eprintln!("its sound collections: {collections}");
    assert!(collections > 0, "its sound collections");
    let manager = s
        .motivation_manager(stalker)
        .expect("its motivation manager");
    let (evaluators, actions) = conditions(s, manager);
    let ids: Vec<u32> = actions.iter().map(|a| a.0).collect();
    // The engine's (12.2), `state_mgr.bind_manager`'s at `reinit`
    // (`xr_evaluators_id.state_mgr` + 1, + 2 = 145, 146;
    // `xr_actions_id.state_mgr` + 1, + 2 = 162, 163) and the camp scheme's
    // (12.1).
    assert_eq!(evaluators, [0, 2, 4, 6, 7, 8, 42, 145, 146, 1065, 1066]);
    assert_eq!(ids, [2, 56, 57, 58, 59, 60, 162, 163, 202, 204]);
    let of = |id: u32| actions.iter().find(|a| a.0 == id).unwrap();
    // `state_mgr`'s preconditions on the engine's actions: the alife
    // planner and gather items need "idle in life", the combat, anomaly
    // and danger planners "idle in combat"; the camp scheme's on the alife
    // planner.
    let needs = [
        (57, 146),
        (2, 146),
        (58, 145),
        (59, 145),
        (60, 145),
        (57, 1065),
    ];
    for (action, condition) in needs {
        assert!(
            of(action).1.contains(&(condition, true)),
            "action {action} needs {condition}"
        );
    }
    let common = [(1075, false), (1164, false), (2054, false), (3144, false)];
    for (action, at, effect) in [(204, true, (1065, true)), (202, false, (1066, true))] {
        let mut want = vec![(0, true), (7, false), (8, false), (42, false), (1066, at)];
        want.extend(common);
        want.sort();
        assert_eq!(of(action).1, want, "action {action}'s preconditions");
        assert_eq!(of(action).2, [effect], "action {action}'s effect");
    }
    // `net_spawn`: the anomaly evaluator a constant false.
    let anomaly = (s.with_planner(manager, |p| p.evaluators[6])).unwrap();
    assert_eq!(anomaly.0, 42);
    let constant = s.with_part(anomaly.1, |e: &mut Evaluator| {
        matches!(e.evaluation, Evaluation::Constant(false))
    });
    assert_eq!(constant, Some(true), "the anomaly evaluator");
    // `state_mgr_goap`'s body-state planner, set up with the stalker.
    let code = format!(
        "local p = db.storage[{stalker}].state_mgr.planner\n\
         return p, p.object:id(), p:initialized()"
    );
    let got = s.exec("=body_state", code.as_bytes()).unwrap();
    let [
        Value::Object(body_state),
        Value::Number(object),
        Value::Boolean(false),
    ] = got[..]
    else {
        panic!("the body-state planner: {got:?}");
    };
    assert_eq!(object, f64::from(stalker));
    let (evaluators, actions) = conditions(s, body_state);
    eprintln!(
        "the body-state planner: {} evaluators, {} actions",
        evaluators.len(),
        actions.len()
    );
    assert!(!evaluators.is_empty() && !actions.is_empty());
    // One update (6.3), as the binder's `update` runs it (CH-17 4.2): it
    // plans and runs the plan's first action.
    let code = format!(
        "local p = db.storage[{stalker}].state_mgr.planner\n\
         p:update()\n\
         return p:initialized(), p:current_action_id()"
    );
    let got = s.exec("=update", code.as_bytes()).unwrap();
    let plan = s.with_planner(body_state, |p| p.plan.clone()).unwrap();
    eprintln!("its plan: {plan:?}");
    assert!(!plan.is_empty(), "a plan");
    assert_eq!(
        got,
        [Value::Boolean(true), Value::Number(f64::from(plan[0]))]
    );
}
