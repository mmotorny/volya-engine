// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

use std::sync::Arc;

use super::*;
use crate::motion_playback::{ALL_PARTITIONS, Motions, STOP_AT_END};
use volya_engine_formats::skeletal_motions::{
    Motion, MotionDefinition, MotionParams, Partition, Track,
};

fn definition(name: &str, bone_or_partition: u16) -> MotionDefinition {
    MotionDefinition {
        name: name.into(),
        flags: STOP_AT_END,
        bone_or_partition,
        motion_index: 0,
        speed: 1.0,
        power: 1.0,
        accrue: 2.0,
        falloff: 2.0,
    }
}

/// Bones `root` → `head`; partition 0 (`torso`) holds root, partition 1
/// (`head`) head. `all` (all partitions) and `torso` have 16 keys, `head`
/// 31; each stops at end.
fn motions() -> Arc<Motions> {
    let defs = vec![
        definition("all", ALL_PARTITIONS),
        definition("torso", 0),
        definition("head", 1),
    ];
    let keys = [16, 16, 31];
    let motions = keys
        .iter()
        .map(|&n| Motion {
            keys: n,
            tracks: (0..2)
                .map(|_| Track {
                    rotations: vec![[0, 0, 0, 32767]].into(),
                    translations: Box::new([]),
                    size: [0.0; 3],
                    base: [0.0; 3],
                })
                .collect(),
        })
        .collect();
    let params = MotionParams {
        version: 3,
        partitions: vec![
            Partition {
                name: "torso".into(),
                bones: vec![("root".into(), 0)],
            },
            Partition {
                name: "head".into(),
                bones: vec![("head".into(), 1)],
            },
        ],
        definitions: defs,
    };
    let identity = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    Arc::new(Motions::new(
        &["root", "head"],
        &[None, Some(0)],
        &[identity; 2],
        vec![(params, motions)],
    ))
}

/// Sounds that play until [`FakeSounds::end`].
#[derive(Default)]
struct FakeSounds {
    next: usize,
    playing: Vec<SoundId>,
    started: Vec<String>,
    stopped: Vec<SoundId>,
    disabled: bool,
}

impl FakeSounds {
    fn end_all(&mut self) {
        self.playing.clear();
    }
}

impl ChannelSounds for FakeSounds {
    fn start(&mut self, name: &str) -> Result<Option<SoundId>, SoundFatal> {
        self.started.push(name.to_owned());
        if self.disabled {
            return Ok(None);
        }
        let id = SoundId::for_test(self.next);
        self.next += 1;
        self.playing.push(id);
        Ok(Some(id))
    }

    fn has_emitter(&self, sound: SoundId) -> bool {
        self.playing.contains(&sound)
    }

    fn stop(&mut self, sound: SoundId) {
        self.playing.retain(|&s| s != sound);
        self.stopped.push(sound);
    }
}

/// A trader: its model, its sounds, the clock, and what each event's
/// function calls.
struct Trader {
    model: AnimatedModel,
    sounds: FakeSounds,
    clock: Clock,
    raised: Vec<u8>,
    responses: Vec<(ChannelEvent, ChannelCall)>,
}

impl Trader {
    fn new() -> Self {
        Self {
            model: AnimatedModel::new(motions()),
            sounds: FakeSounds::default(),
            clock: Clock {
                time_global: 0,
                frame: 0,
            },
            raised: Vec::new(),
            responses: Vec::new(),
        }
    }

    fn call(&mut self, channels: &mut MotionChannels, call: ChannelCall) -> Result<(), Fatal> {
        let mut parts = TraderParts {
            model: &mut self.model,
            sounds: &mut self.sounds,
            clock: self.clock,
        };
        channels.call(&call, &mut parts)
    }

    /// `ms` of 20 ms frames: the tracks updates, their notifications, the
    /// check after each.
    fn run(&mut self, channels: &mut MotionChannels, ms: u32) {
        for _ in 0..ms / 20 {
            self.clock.time_global += 20;
            self.clock.frame += 1;
            self.model.update_tracks(self.clock);
            for n in self.model.take_notifications() {
                channels.notified(n.value);
            }
            channels.check(self).unwrap();
        }
    }

    fn take_raised(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.raised)
    }
}

impl CheckParts for Trader {
    type Error = Fatal;

    fn sounds(&mut self) -> &mut dyn ChannelSounds {
        &mut self.sounds
    }

    fn raise(&mut self, event: ChannelEvent, channels: &mut MotionChannels) -> Result<(), Fatal> {
        self.raised.push(event.id());
        let calls: Vec<ChannelCall> = (self.responses.iter())
            .filter(|(e, _)| *e == event)
            .map(|(_, c)| c.clone())
            .collect();
        for c in calls {
            self.call(channels, c)?;
        }
        Ok(())
    }
}

fn global(name: &str) -> ChannelCall {
    ChannelCall::SetGlobalAnim(name.into())
}

fn sound(name: &str, head: &str) -> ChannelCall {
    ChannelCall::SetSound {
        sound: name.into(),
        head_anim: head.into(),
    }
}

#[test]
fn before_any_global_motion_event_30_comes_at_every_check() {
    let (mut t, mut c) = (Trader::new(), MotionChannels::new());
    assert!(c.global.ended() && c.head.ended() && c.sound.is_none());
    t.run(&mut c, 60);
    // 3: the head channel isn't marked; no sound, no event 31.
    assert_eq!(t.take_raised(), [30, 30, 30]);
    assert_eq!(c, MotionChannels::new());
}

#[test]
fn a_cycle_the_model_lacks_is_fatal_after_the_name_is_kept() {
    let (mut t, mut c) = (Trader::new(), MotionChannels::new());
    let e = t.call(&mut c, global("nothing")).unwrap_err();
    assert_eq!(e, Fatal::NoCycle("nothing".into()));
    assert_eq!(e.to_string(), "! MODEL: can't find cycle: nothing");
    assert_eq!(
        e.check(),
        Some(("motion_ID.valid()", "! MODEL: can't find cycle: "))
    );
    assert_eq!(c.global.name.as_deref(), Some(&b"nothing"[..]));
    assert!(c.global.ended());
}

#[test]
fn a_global_motion_ends_with_its_notification() {
    let (mut t, mut c) = (Trader::new(), MotionChannels::new());
    t.call(&mut c, global("all")).unwrap();
    assert_eq!(c.global.motion, t.model.motions().cycle("all"));
    // On both partitions, channel 0, notifying, mixing.
    let started: Vec<_> = t.model.playbacks().map(|(_, p)| p.clone()).collect();
    assert_eq!(started.len(), 2);
    assert!(
        started
            .iter()
            .all(|p| p.notification == Some(GLOBAL_NOTIFICATION)
                && p.channel == 0
                && p.weight < 1e-6)
    );
    // The end is 0.5 s of motion time (16 keys less one), at a speed a
    // little below 1: no event before.
    t.run(&mut c, 460);
    assert!(t.take_raised().is_empty());
    t.run(&mut c, 60);
    assert!(c.global.ended());
    assert_eq!(t.take_raised(), [30]);
    // It stays ended: event 30 at every check, the head marked each time.
    t.run(&mut c, 40);
    assert_eq!(t.take_raised(), [30, 30]);
}

#[test]
fn the_sound_and_its_head_motion() {
    let (mut t, mut c) = (Trader::new(), MotionChannels::new());
    t.call(&mut c, global("torso")).unwrap();
    t.call(&mut c, sound("trader\\monolog", "head")).unwrap();
    assert_eq!(t.sounds.started, ["trader\\monolog"]);
    assert_eq!(c.head.motion, t.model.motions().cycle("head"));
    assert!(c.sound.is_some_and(|s| s.0.is_some()));
    // `torso` ends at about 0.5 s: event 30 marks the head ended, and the
    // sound plays, so event 31 follows in the same check.
    t.run(&mut c, 520);
    assert_eq!(t.take_raised(), [30, 31]);
    assert!(c.head.ended());
    // The head motion's own end wouldn't notify now: it is ended already.
    t.run(&mut c, 20);
    assert_eq!(t.take_raised(), [30, 31]);
    // The sound's end: event 32 once, the sound discarded.
    t.sounds.end_all();
    t.run(&mut c, 40);
    assert_eq!(t.take_raised(), [32, 30, 30]);
    assert_eq!(c.sound, None);
    assert_eq!(t.sounds.stopped.len(), 1);
}

#[test]
fn a_new_sound_stops_the_old_one() {
    let (mut t, mut c) = (Trader::new(), MotionChannels::new());
    t.call(&mut c, sound("a", "head")).unwrap();
    t.call(&mut c, sound("b", "head")).unwrap();
    assert_eq!(t.sounds.stopped, [SoundId::for_test(0)]);
    assert_eq!(t.sounds.playing, [SoundId::for_test(1)]);
}

#[test]
fn calls_of_an_events_function_take_effect_before_the_next_step() {
    let (mut t, mut c) = (Trader::new(), MotionChannels::new());
    t.call(&mut c, global("torso")).unwrap();
    t.call(&mut c, sound("s", "head")).unwrap();
    t.responses = vec![
        (ChannelEvent::GlobalAnimRequest, global("torso")),
        (
            ChannelEvent::HeadAnimRequest,
            ChannelCall::SetHeadAnim("head".into()),
        ),
    ];
    t.run(&mut c, 520);
    // A global motion started by event 30's function doesn't keep the head
    // channel from being marked just after; 31 follows and restarts it.
    assert_eq!(t.take_raised(), [30, 31]);
    assert!(!c.global.ended() && !c.head.ended());
    t.run(&mut c, 20);
    assert!(t.take_raised().is_empty());
}

#[test]
fn a_sound_started_by_event_32s_function_is_stopped_at_once() {
    let (mut t, mut c) = (Trader::new(), MotionChannels::new());
    t.call(&mut c, global("all")).unwrap();
    t.call(&mut c, sound("s", "head")).unwrap();
    t.responses = vec![(ChannelEvent::SoundEnd, sound("again", "head"))];
    t.sounds.end_all();
    t.run(&mut c, 20);
    assert_eq!(t.take_raised(), [32]);
    assert_eq!(t.sounds.started, ["s", "again"]);
    assert_eq!(c.sound, None);
    assert!(t.sounds.playing.is_empty());
}

#[test]
fn external_sound_start_asks_for_a_head_motion() {
    let (mut t, mut c) = (Trader::new(), MotionChannels::new());
    t.call(&mut c, global("all")).unwrap();
    t.call(&mut c, ChannelCall::SetHeadAnim("head".into()))
        .unwrap();
    t.call(&mut c, ChannelCall::ExternalSoundStart("tutorial".into()))
        .unwrap();
    assert!(c.head.ended());
    t.run(&mut c, 20);
    assert_eq!(t.take_raised(), [31]);
    // `external_sound_stop` stops it; the channels are unchanged.
    t.call(&mut c, ChannelCall::ExternalSoundStop).unwrap();
    assert_eq!(c.sound, None);
    assert_eq!(t.sounds.stopped.len(), 1);
    t.call(&mut c, ChannelCall::ExternalSoundStop).unwrap();
    assert_eq!(t.sounds.stopped.len(), 1);
    t.run(&mut c, 20);
    assert!(t.take_raised().is_empty());
}

#[test]
fn without_sound_data_the_sound_ends_at_the_next_check() {
    let (mut t, mut c) = (Trader::new(), MotionChannels::new());
    t.sounds.disabled = true;
    t.call(&mut c, global("all")).unwrap();
    t.call(&mut c, sound("s", "head")).unwrap();
    assert_eq!(c.sound, Some(ChannelSound(None)));
    t.run(&mut c, 20);
    assert_eq!(t.take_raised(), [32]);
    assert_eq!(c.sound, None);
}

#[test]
fn a_spawning_clears_the_record_without_stopping_the_sound() {
    let (mut t, mut c) = (Trader::new(), MotionChannels::new());
    t.call(&mut c, global("all")).unwrap();
    t.call(&mut c, sound("s", "head")).unwrap();
    c.reinitialise();
    assert_eq!(c, MotionChannels::new());
    assert!(t.sounds.stopped.is_empty());
    assert_eq!(t.sounds.playing.len(), 1);
}

#[test]
fn a_global_motion_on_all_partitions_leaves_the_head_motion_unreported() {
    // 2.3: the head motion fades out without notifying.
    let (mut t, mut c) = (Trader::new(), MotionChannels::new());
    t.call(&mut c, ChannelCall::SetHeadAnim("head".into()))
        .unwrap();
    t.call(&mut c, global("all")).unwrap();
    let head = (t.model.playbacks())
        .find(|(_, p)| p.notification == Some(HEAD_NOTIFICATION))
        .map(|(_, p)| p.fade);
    assert_eq!(head, Some(crate::motion_playback::Fade::Out));
    t.run(&mut c, 400);
    assert!(!c.head.ended());
    assert!(t.take_raised().is_empty());
    // Event 30 after `all`'s end marks it ended; no sound, no 31.
    t.run(&mut c, 120);
    assert_eq!(t.take_raised(), [30]);
    assert!(c.head.ended());
}
