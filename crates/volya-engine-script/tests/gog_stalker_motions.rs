// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The Russian edition's stalker motions, checked against
//! docs/behavior/stalker-motions.md (cited by section): an `esc_lager`
//! campfire stalker spawned with the shipped scripts and its model's
//! motions, whose script animations take over its model. Skips when
//! `VOLYA_GOG_INSTALLER` is unset.

#[path = "support/campfire.rs"]
mod campfire;
#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;
#[path = "support/shipped_scripts.rs"]
mod shipped_scripts;

use std::sync::Arc;

use volya_engine_core::motion_playback::{AnimatedModel, Clock, Fade, MotionId, Motions};
use volya_engine_core::stalker_motions::{Slot, StalkerModel};
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::vfs::{Vfs, VfsFile, read};
use volya_engine_formats::model::{Mat4, Model};
use volya_engine_formats::object::ObjectRecord;
use volya_engine_formats::skeletal_motions::{self, ModelMotions};
use volya_engine_script::Value;
use volya_engine_script::game_object::EngineState;
use volya_engine_script::loader::Scripts;

type Tree = DbVfs<Box<dyn VfsFile + 'static>>;

/// The motions of the model `visual` (skeletal-motions 1): its motion
/// files under `meshes\`, in order.
fn motions(tree: &Tree, visual: &str) -> Arc<Motions> {
    let path = format!("gamedata\\meshes\\{visual}.ogf").to_lowercase();
    let model = Model::parse(&read(tree, &path).unwrap()).unwrap();
    let names: Vec<&str> = model.bones.iter().map(|b| b.name.as_str()).collect();
    let parents: Vec<Option<usize>> = model.bones.iter().map(|b| b.parent).collect();
    let binds: Vec<Mat4> = model.bind_pose().unwrap();
    let mut file = tree.open(&path).unwrap();
    let ModelMotions::Files(items) =
        skeletal_motions::read_model_motions(&mut file, &path).unwrap()
    else {
        panic!("{visual}: no motion files");
    };
    let sets = items
        .iter()
        .map(|item| {
            let p = format!("gamedata\\meshes\\{item}.omf").to_lowercase();
            let mut f = tree.open(&p).unwrap();
            let set = skeletal_motions::read_motion_file(&mut f, &p).unwrap();
            let count = set.params.definitions.len();
            let decoded = skeletal_motions::read_motions(&mut f, count, names.len(), &p).unwrap();
            (set.params, decoded)
        })
        .collect();
    Arc::new(Motions::new(&names, &parents, &binds, sets))
}

/// What the engine gives the stalker: its model's motions (3.1) and its
/// specific character's `crouch_type` (the novices' −1, section 10).
fn prepare(tree: &Tree, record: &ObjectRecord, model: &mut Option<Arc<Motions>>) -> EngineState {
    let visual = record.object.visual().unwrap().model.as_str().to_owned();
    eprintln!("its model: {visual}");
    let m = motions(tree, &visual);
    *model = Some(Arc::clone(&m));
    EngineState {
        stalker_model: Some(StalkerModel::new(m)),
        crouch_type: -1,
        ..EngineState::default()
    }
}

fn clock(ms: u32) -> Clock {
    Clock {
        time_global: ms,
        frame: ms / 20,
    }
}

/// The motions fading in on `partition`.
fn playing(model: &AnimatedModel, partition: usize) -> Vec<MotionId> {
    (model.partition_playbacks(partition))
        .filter(|p| p.fade == Fade::In)
        .map(|p| p.motion)
        .collect()
}

/// The stalker spawned, its model's motions and an animated model of them.
struct Campfire<'a> {
    s: &'a mut Scripts,
    stalker: u16,
    motions: Arc<Motions>,
    animated: AnimatedModel,
    /// `time_global()`, advanced by 20 ms frames.
    t: u32,
}

impl Campfire<'_> {
    fn cycle(&self, name: &str) -> MotionId {
        self.motions.cycle(name).unwrap()
    }

    /// Lua `code` run with `o` the stalker and `m` its state manager:
    /// its results.
    fn exec(&mut self, code: &str) -> Vec<Value> {
        let stalker = self.stalker;
        let code = format!(
            "local m = db.storage[{stalker}].state_mgr\nlocal o = m.planner.object\n{code}"
        );
        self.s.exec("=campfire", code.as_bytes()).unwrap()
    }

    /// `animation_count()` after `code`.
    fn count_after(&mut self, code: &str) -> usize {
        match self.exec(&format!("{code}\nreturn o:animation_count()"))[..] {
            [Value::Number(n)] => n as usize,
            ref got => panic!("{got:?}"),
        }
    }

    /// One frame: the model's tracks updated, then the stalker's motion
    /// update.
    fn frame(&mut self) {
        let clock = clock(self.t);
        self.animated.update_tracks(clock);
        let out = (self.s)
            .update_stalker_motions(self.stalker, &mut self.animated, clock, &mut || 0)
            .unwrap();
        assert!(out.is_some(), "a living stalker");
    }
}

/// 3.3, 8, 11, 12 step 3: an `esc_lager` stalker's script animations, as
/// the shipped state manager and the scripts' calls queue them, take over
/// its legs and torso, the head going on with its own.
#[test]
fn the_scripts_sit_animation_plays_on_its_model() {
    let mut model = None;
    let Some((mut scripts, stalker, spawned)) =
        campfire::started_with(None, |tree, record| prepare(tree, record, &mut model))
    else {
        return;
    };
    let s = scripts.scripts();
    assert_eq!(spawned, Ok(()), "spawning; {:?}", s.take_failures());
    let motions = model.unwrap();
    // The partitions `legs`, `torso`, `head` (2.3).
    assert_eq!(motions.partition_count(), 3);
    // `state_mgr_goap` set callback 29 (11.1).
    assert!(s.has_callback(stalker, 29).unwrap(), "callback 29");
    let mut c = Campfire {
        s,
        stalker,
        animated: AnimatedModel::new(Arc::clone(&motions)),
        motions,
        t: 1_000,
    };
    sitting_down(&mut c);
    the_queue(&mut c);
    let objects = scripts.objects().borrow();
    let parts = objects.get(stalker).unwrap().movement.stalker.as_ref();
    let parts = parts.unwrap();
    assert_eq!(parts.motions.slot(Slot::Script), (None, None));
}

/// 11.2, 12 step 3: the state manager's pose change from `stand` to
/// `sit_knee` queues `idle_0_to_sit_1` and `sit_1_idle_0`; the pose
/// settles once the transfer (2.501 s, ending 1/30 s early) has ended.
fn sitting_down(c: &mut Campfire) {
    assert_eq!(c.count_after("m.animstate:set_state('sit_knee')"), 2);
    let (into, sit) = (c.cycle("idle_0_to_sit_1"), c.cycle("sit_1_idle_0"));
    let head = c.cycle("head_idle_0");
    let start = c.t;
    loop {
        c.frame();
        if c.t == start {
            // The mask leaves the head out (3.1, 3.4).
            assert_eq!(playing(&c.animated, 0), [into]);
            assert_eq!(playing(&c.animated, 1), [into]);
            assert_eq!(playing(&c.animated, 2), [head]);
        }
        if c.count_after("m.animstate:update()") == 1 {
            break;
        }
        c.t += 20;
        assert!(c.t - start < 3_000, "the transfer ends");
    }
    let elapsed = c.t - start;
    eprintln!("the pose change took {elapsed} ms");
    assert!((2_400..=2_520).contains(&elapsed), "{elapsed} ms");
    assert_eq!(playing(&c.animated, 0), [sit]);
    assert_eq!(playing(&c.animated, 1), [sit]);
    assert_eq!(playing(&c.animated, 2), [head]);
    // The looping idle plays on while it stays queued (8.2).
    for _ in 0..500 {
        c.t += 20;
        c.frame();
    }
    assert_eq!(c.count_after(""), 1);
    assert_eq!(playing(&c.animated, 0), [sit]);
}

/// 8.1 – 8.3: the scripts' own queue: a name the model lacks appends
/// nothing; each end calls callback 29 at the next update; cleared, the
/// normal selection takes the legs and torso back.
fn the_queue(c: &mut Campfire) {
    let code = "ended = 0\n\
        o:set_callback(callback.script_animation, function() ended = ended + 1 end)\n\
        o:clear_animations()\n\
        o:add_animation('sit_1_to_idle_0', true, false)\n\
        o:add_animation('no_such_motion', true, false)\n\
        o:add_animation('idle_0_to_sit_1', true, false)";
    assert_eq!(c.count_after(code), 2);
    let (out, into) = (c.cycle("sit_1_to_idle_0"), c.cycle("idle_0_to_sit_1"));
    c.t += 20;
    c.frame();
    assert_eq!(playing(&c.animated, 0), [out]);
    let ended = |c: &mut Campfire| c.exec("return ended")[0].as_number().unwrap() as u32;
    let start = c.t;
    while ended(c) == 0 {
        c.t += 20;
        assert!(c.t - start < 5_000, "the first ends");
        c.frame();
    }
    eprintln!("the first ended after {} ms", c.t - start);
    assert_eq!(c.count_after(""), 1);
    assert_eq!(playing(&c.animated, 0), [into]);
    while ended(c) == 1 {
        c.t += 20;
        assert!(c.t - start < 10_000, "the second ends");
        c.frame();
    }
    assert_eq!(c.count_after(""), 0);
    // The normal selection: still, standing, in the danger state the
    // movement leaves it in (TODO(#1129)), the legs `norm_idle_0` (7.3).
    c.t += 20;
    c.frame();
    assert_eq!(playing(&c.animated, 0), [c.cycle("norm_idle_0")]);
}
