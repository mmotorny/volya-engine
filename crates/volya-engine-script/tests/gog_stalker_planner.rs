// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's stalker planner, checked against
//! docs/behavior/stalker-planner.md (cited by section): the campfire
//! novice of its worked example (8), an `esc_lager` stalker spawned bound
//! by `xr_motivator` on a New Game's simulation ([`campfire::started`])
//! with the shipped logic set-up, runs its scheduled updates (5.1: the
//! binder's `update`, then the think step) with no enemy and no danger,
//! and its root hands control to the campfire scheme's actions. Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#[path = "support/campfire.rs"]
mod campfire;
#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;
#[path = "support/shipped_scripts.rs"]
mod shipped_scripts;

use campfire::started;
use volya_engine_script::Value;
use volya_engine_script::stalker_planner::{ACTION_ALIFE_PLANNER, DANGER_PROPERTY, ENEMY};

/// The campfire scheme's "go to position" and "wait" (`action_script` +
/// 141, + 143; 4.3).
const GO_TO_POSITION: u32 = 202;
const WAIT: u32 = 204;

/// The job's logic (smart-terrain-jobs.md 6: `[logic@esc_lager_kamp1]`
/// activates `kamp1@esc_lager`, whose centre is read with the group's name
/// as `esc_lager_camp_center`), given as the stalker's own `[logic]`: no
/// smart terrain gives jobs yet (CH-28, TODO(#1215)).
const CAMPFIRE_LOGIC: &[u8] =
    b"[logic]\nactive = kamp1@esc_lager\n[kamp1@esc_lager]\ncenter_point = esc_lager_camp_center\n";

/// 8 steps 1 – 4: the root, built at the spawning with the scripts' parts
/// on it, plans "go to position", "wait", then the alife planner, and runs
/// "go to position".
#[test]
fn the_campfire_novice_reaches_the_scripts_actions() {
    let Some((mut scripts, stalker, spawned)) = started(Some(CAMPFIRE_LOGIC)) else {
        return;
    };
    assert_eq!(
        spawned,
        Ok(()),
        "spawning; {:?}",
        scripts.scripts().take_failures()
    );
    let code = format!("return db.storage[{stalker}].active_section");
    let got = scripts.scripts().exec("=logic", code.as_bytes()).unwrap();
    assert_eq!(got, [Value::from("kamp1@esc_lager")], "the active section");
    // The clock well past the 3 s of the enemy rule (1.2).
    let mut now = 10_000;
    for update in 0..20 {
        now += 100;
        scripts
            .services()
            .borrow_mut()
            .device
            .frame(now, 0.1, false);
        let s = scripts.scripts();
        // 5.1 step 2, the binder's `update(delta)`; step 3, the think step.
        if let Err(f) = s.scheduled_update(stalker, 100) {
            panic!("the binder's update: {f:?}; {:?}", s.take_failures());
        }
        if let Err(e) = s.think_step(stalker) {
            panic!("the think step: {e:?}; {:?}", s.take_failures());
        }
        let root = s.motivation_manager(stalker).expect("its root");
        let (current, plan, cached) = s
            .with_planner(root, |p| {
                (p.current_action_id(), p.plan.clone(), p.cached.clone())
            })
            .unwrap();
        // No enemy and no danger: the life goal goes past the camp's
        // actions to the alife planner (1.4, 4.3).
        assert_eq!(cached.get(&ENEMY), Some(&false), "update {update}: enemy");
        assert_eq!(
            cached.get(&DANGER_PROPERTY),
            Some(&false),
            "update {update}: danger"
        );
        assert_eq!(
            plan,
            [GO_TO_POSITION, WAIT, ACTION_ALIFE_PLANNER],
            "update {update}'s plan"
        );
        assert_eq!(current, Some(GO_TO_POSITION), "update {update} runs");
    }
}
