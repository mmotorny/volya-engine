// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The world against physics-world.md's figures and order.

use std::cell::RefCell;
use std::rc::Rc;

use super::*;

type Log = Rc<RefCell<Vec<String>>>;

struct Fake {
    name: &'static str,
    kind: Kind,
    geometry: bool,
    height: f32,
    gravity: bool,
    resting: bool,
    forces: Vec<Vec3>,
    log: Log,
}

impl Fake {
    fn new(name: &'static str, kind: Kind, log: &Log) -> Self {
        Self {
            name,
            kind,
            geometry: true,
            height: 0.0,
            gravity: true,
            resting: false,
            forces: Vec::new(),
            log: log.clone(),
        }
    }

    fn say(&self, what: &str) {
        self.log.borrow_mut().push(format!("{} {what}", self.name));
    }
}

impl Object for Fake {
    fn kind(&self) -> Kind {
        self.kind
    }
    fn has_geometry(&self) -> bool {
        self.geometry
    }
    fn height(&self) -> f32 {
        self.height
    }
    fn set_gravity(&mut self, on: bool) {
        self.gravity = on;
    }
    fn add_force(&mut self, force: Vec3) {
        self.forces.push(force);
    }
    fn pre_step(&mut self, _: &Params) {
        self.say("pre");
    }
    fn post_step(&mut self, _: &Params) -> bool {
        self.say("post");
        self.resting
    }
    fn set_aside(&mut self) {
        self.say("aside");
    }
    fn restore(&mut self) {
        self.say("restore");
    }
    fn clear_level_contacts(&mut self) {
        self.say("clear");
    }
    fn below_floor(&mut self) {
        self.say("below");
    }
}

struct FakeSolver(Log);

impl Solver<Fake> for FakeSolver {
    fn collide(&mut self, _: &Params, objects: &mut Active<'_, Fake>) {
        let n = objects.iter_mut().count();
        self.0.borrow_mut().push(format!("collide {n}"));
    }
    fn solve(&mut self, p: &Params, _: &mut Active<'_, Fake>) {
        self.0.borrow_mut().push(format!("solve {}", p.iterations));
    }
    fn drop_contacts(&mut self) {
        self.0.borrow_mut().push("drop".into());
    }
}

struct FakeUpdater(Log);

impl Updater for FakeUpdater {
    fn pre_step(&mut self, _: &Params) {
        self.0.borrow_mut().push("updater pre".into());
    }
    fn post_step(&mut self, _: &Params) {
        self.0.borrow_mut().push("updater post".into());
    }
}

fn world(log: &Log) -> World<Fake, FakeSolver> {
    World::new(
        FakeSolver(log.clone()),
        INITIAL_STEP,
        LIBRARY_ITERATIONS,
        -12.0,
    )
}

#[test]
fn creation_sets_the_parameters_of_1_2_and_1_3() {
    let p = Params::new(0.01, LIBRARY_ITERATIONS);
    assert_eq!(p.gravity, [0.0, -19.62, 0.0]);
    assert!((p.erp - 0.375).abs() < 1e-6, "{}", p.erp);
    assert!((p.cfm - 1.5625e-6).abs() < 1e-12, "{}", p.cfm);
    assert_eq!(
        (p.iterations, p.relaxation, p.surface_layer),
        (20, 1.1, 0.001)
    );
    assert_eq!(p.max_correcting_speed, f32::INFINITY);
    let log = Log::default();
    let w = world(&log);
    assert_eq!(w.floor(), -42.0);
    assert_eq!((w.counter(), w.accumulator(), w.frozen()), (0, 0.0, false));
}

#[test]
fn correcting_speed_has_no_limit() {
    let p = Params::new(0.01, 20);
    assert_eq!(p.correcting_speed(0.0005, 0.375), 0.0);
    assert!((p.correcting_speed(0.011, 0.5) - 0.5).abs() < 1e-5);
    assert!((p.correcting_speed(1000.001, 1.0) - 100_000.0).abs() < 1.0);
}

#[test]
fn gravity_follows_the_switch() {
    let p = Params::new(0.01, 20);
    assert_eq!(p.gravity_on(true), [0.0, -19.62, 0.0]);
    assert_eq!(p.gravity_on(false), [0.0; 3]);
    let log = Log::default();
    let mut w = world(&log);
    let id = w.add(Fake::new("a", Kind::Shell, &log));
    assert!(w.get(id).unwrap().gravity);
    w.set_gravity(id, false);
    assert!(!w.get(id).unwrap().gravity);
}

#[test]
fn a_frame_runs_whole_steps_and_keeps_the_remainder() {
    let log = Log::default();
    let mut w = world(&log);
    assert_eq!(w.frame(0.005, 0), 0);
    assert_eq!(w.accumulator(), 0.005);
    assert_eq!(w.frame(0.0251, 0), 3);
    assert!(
        (w.accumulator() - 0.0001).abs() < 1e-6,
        "{}",
        w.accumulator()
    );
    assert_eq!(w.counter(), 3);
    // Paused: dt_f = 0 runs nothing.
    assert_eq!(w.frame(0.0, 0), 0);
    // 2.2: at most 10 steps at dt_f = 0.1 and the default step.
    let mut w = world(&log);
    assert_eq!(w.frame(0.1, 0), 10);
}

#[test]
fn a_non_whole_frequency_can_run_more_than_its_quotient() {
    // 2.2, disputed in review: at 75 a frame can run 8 steps.
    let log = Log::default();
    let mut w = world(&log);
    let h = 1.0 / 75.0;
    w.set_step(h, 0.0);
    w.frame(0.013, 0);
    assert_eq!(w.frame(0.1, 0), 8);
}

#[test]
fn ph_frequency_resets_the_accumulator_and_keeps_e_and_c() {
    let log = Log::default();
    let mut w = world(&log);
    let before = *w.params();
    w.set_step(0.005, 0.0123);
    assert_eq!(w.params().step, 0.005);
    assert!(
        (w.accumulator() - 0.0023).abs() < 1e-6,
        "{}",
        w.accumulator()
    );
    assert_eq!((w.params().erp, w.params().cfm), (before.erp, before.cfm));
    assert_eq!(w.frame(0.0078, 0), 2);
}

#[test]
fn ph_iterations_reaches_the_solver() {
    let log = Log::default();
    let mut w = world(&log);
    w.set_iterations(18);
    w.frame(0.01, 0);
    assert!(log.borrow().contains(&"solve 18".to_owned()));
}

#[test]
fn one_step_runs_in_the_order_of_section_3() {
    let log = Log::default();
    let mut w = world(&log);
    let id = w.add(Fake::new("a", Kind::Shell, &log));
    w.activate(id).unwrap();
    w.add_updater(Box::new(FakeUpdater(log.clone())));
    let times = Rc::new(RefCell::new(Vec::new()));
    let t = times.clone();
    w.set_hook(Some(Box::new(move |s, e| t.borrow_mut().push((s, e)))));
    w.set_step(0.005, 0.0);
    assert_eq!(w.frame(0.01, 1000), 2);
    let step = [
        "collide 1",
        "a pre",
        "updater pre",
        "solve 20",
        "a post",
        "updater post",
        "drop",
    ];
    let expected: Vec<String> = step
        .iter()
        .chain(step.iter())
        .map(|s| s.to_string())
        .collect();
    assert_eq!(*log.borrow(), expected);
    // 1000·h = 5 rounds to 5 ms per step.
    assert_eq!(*times.borrow(), vec![(1000, 1005), (1005, 1010)]);
}

#[test]
fn per_step_actions_run_their_steps_and_leave() {
    let log = Log::default();
    let mut w = world(&log);
    let id = w.add(Fake::new("a", Kind::Shell, &log));
    w.activate(id).unwrap();
    // 25 ms at h = 0.01: whole part of 2.5 = 2 steps.
    w.constant_force(id, [0.0, 1.0, 0.0], 3.0, 25);
    w.frame(0.05, 0);
    assert_eq!(w.get(id).unwrap().forces, vec![[0.0, 3.0, 0.0]; 2]);
    // Without the shell nothing happens.
    w.remove(id);
    w.constant_force(id, [1.0, 0.0, 0.0], 1.0, 1000);
    w.frame(0.01, 0);
}

#[test]
fn removing_a_shell_removes_its_actions() {
    let log = Log::default();
    let mut w = world(&log);
    let id = w.add(Fake::new("a", Kind::Shell, &log));
    w.activate(id).unwrap();
    w.constant_force(id, [0.0, 1.0, 0.0], 1.0, 1000);
    let o = w.remove(id).unwrap();
    assert!(o.forces.is_empty());
    w.frame(0.01, 0);
}

#[test]
fn a_resting_shell_is_handled_at_the_eleventh_rest_pass() {
    let log = Log::default();
    let mut w = world(&log);
    let mut shell = Fake::new("a", Kind::Shell, &log);
    shell.resting = true;
    let id = w.add(shell);
    w.activate(id).unwrap();
    // Step 1 makes the first rest pass (before the shell is listed).
    w.step(0, 0);
    assert_eq!(w.resting_count(id), Some(RESTING_COUNT));
    let clears = |log: &Log| log.borrow().iter().filter(|s| *s == "a clear").count();
    // Rest passes at steps 65, 129, …: the 11th after listing is step 1 + 11·64.
    for _ in 0..10 * 64 {
        w.step(0, 0);
    }
    assert_eq!(w.resting_count(id), Some(0));
    assert_eq!(clears(&log), 0);
    // Awake from now on, so the next pass delists it for good.
    w.get_mut(id).unwrap().resting = false;
    for _ in 0..64 {
        w.step(0, 0);
    }
    assert_eq!(clears(&log), 1);
    assert_eq!(w.resting_count(id), None);
}

#[test]
fn activation_takes_a_shell_off_the_resting_list() {
    let log = Log::default();
    let mut w = world(&log);
    let mut shell = Fake::new("a", Kind::Shell, &log);
    shell.resting = true;
    let id = w.add(shell);
    w.activate(id).unwrap();
    w.step(0, 0);
    assert!(w.resting_count(id).is_some());
    w.activate(id).unwrap();
    assert_eq!(w.resting_count(id), None);
}

#[test]
fn below_the_floor_a_shell_stops_and_a_character_is_told() {
    let log = Log::default();
    let mut w = world(&log);
    let mut shell = Fake::new("s", Kind::Shell, &log);
    shell.height = -42.5;
    let mut character = Fake::new("c", Kind::Character, &log);
    character.height = -50.0;
    let mut above = Fake::new("u", Kind::Shell, &log);
    above.height = -41.9;
    let (s, c, u) = (w.add(shell), w.add(character), w.add(above));
    for id in [s, c, u] {
        w.activate(id).unwrap();
    }
    w.step(0, 0);
    assert!(!w.is_active(s));
    assert!(w.is_active(c) && w.is_active(u));
    assert!(log.borrow().contains(&"c below".to_owned()));
}

#[test]
fn freezing_sets_everything_aside_and_steps_only_what_is_put_back() {
    let log = Log::default();
    let mut w = world(&log);
    let a = w.add(Fake::new("a", Kind::Shell, &log));
    let b = w.add(Fake::new("b", Kind::Shell, &log));
    w.activate(a).unwrap();
    w.activate(b).unwrap();
    w.add_updater(Box::new(FakeUpdater(log.clone())));
    w.freeze().unwrap();
    assert_eq!(*log.borrow(), ["a aside", "b aside"]);
    log.borrow_mut().clear();
    assert_eq!(w.frame(0.05, 0), 0);
    assert_eq!(w.accumulator(), 0.0);
    w.activate(b).unwrap();
    w.step(0, 0);
    assert_eq!(
        *log.borrow(),
        ["collide 1", "b pre", "solve 20", "b post", "drop"]
    );
    log.borrow_mut().clear();
    w.unfreeze().unwrap();
    assert_eq!(*log.borrow(), ["a restore"]);
    assert!(w.is_active(a) && w.is_active(b));
}

#[test]
fn the_fatal_errors_of_section_5() {
    let log = Log::default();
    let mut w = world(&log);
    assert_eq!(w.unfreeze(), Err(Fatal("is not freezed!!!")));
    let a = w.add(Fake::new("a", Kind::Shell, &log));
    w.activate(a).unwrap();
    w.freeze().unwrap();
    assert_eq!(w.freeze(), Err(Fatal("already freezed!!!")));
    assert_eq!(
        w.deactivate(a),
        Err(Fatal(
            "can not deactivate physics shell when ph world is freezed!!!"
        ))
    );
    w.unfreeze().unwrap();
    w.deactivate(a).unwrap();
    assert!(!w.is_active(a));
    let mut none = Fake::new("n", Kind::Shell, &log);
    none.geometry = false;
    let n = w.add(none);
    assert_eq!(
        w.activate(n),
        Err(Fatal("trying to activate destroyed or not created object!"))
    );
    w.stepping = true;
    assert_eq!(
        w.deactivate(a),
        Err(Fatal(
            "can not deactivate physics shell during physics processing!!!"
        ))
    );
}

#[test]
fn deactivating_a_set_aside_shell_is_fatal() {
    let log = Log::default();
    let mut w = world(&log);
    let a = w.add(Fake::new("a", Kind::Shell, &log));
    w.activate(a).unwrap();
    w.freeze().unwrap();
    // Unfrozen state check comes first; a set-aside object after unfreezing
    // can only arise through a procedure, so set it by hand.
    w.frozen = false;
    assert_eq!(
        w.deactivate(a),
        Err(Fatal("can not deactivate freezed !!!"))
    );
}

thread_local! {
    static TICKS: std::cell::Cell<f64> = const { std::cell::Cell::new(0.0) };
}

/// A counter advancing 1 ms per reading.
fn tick() -> f64 {
    TICKS.with(|t| {
        t.set(t.get() + 1.0);
        t.get()
    })
}

#[test]
fn the_timers_measure_the_frame_and_its_steps() {
    let log = Log::default();
    let mut w = world(&log);
    w.set_perf_counter(tick);
    // Three steps: the collider and solver timers read twice per step,
    // the physics timer around them.
    assert_eq!(w.frame(0.035, 0), 3);
    let t = w.take_timers();
    assert_eq!((t.collider, t.solver, t.steps), (3.0, 3.0, 3));
    assert_eq!(t.physics, 13.0);
    // Taken: they restart; a frame without steps measures only itself.
    assert_eq!(w.frame(0.001, 0), 0);
    let t = w.take_timers();
    assert_eq!(
        (t.physics, t.collider, t.solver, t.steps),
        (1.0, 0.0, 0.0, 0)
    );
}
