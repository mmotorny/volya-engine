// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The trader's scripted motion channels on a New Game's level, from the
//! Russian edition's data (BYOA): docs/behavior/scripted-motion-channels.md's
//! worked example (section 6, the start chain up to `new_start_intro3`),
//! with the scheme's responses made by the test, and a cycle the model
//! lacks. Skips when `VOLYA_GOG_INSTALLER` is unset.

#![cfg(not(target_arch = "wasm32"))]

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use volya_engine::core::audio::Category;
use volya_engine::core::scripted_motion_channels::{
    ChannelCall, ChannelEvent, SystemSounds, TRADER_CLASS,
};
use volya_engine::core::sound::system::{NoDevice, SoundSystem};
use volya_engine::core::vfs::db::DbVfs;
use volya_engine::core::vfs::{StdVfs, Vfs};
use volya_engine::data::GameData;
use volya_engine::level::LevelScreen;
use volya_engine_formats::ui::Resolution;

const MONOLOG: &str = "characters_voice\\scenario\\trader\\trader_monolog1";

/// The level with the trader, a sound system, and the time.
struct Run<'a> {
    tree: &'a dyn Vfs,
    data: &'a GameData,
    screen: LevelScreen,
    sounds: SoundSystem,
    trader: u16,
    now: u64,
    /// The events raised, with the frame's time.
    raised: Vec<(u64, ChannelEvent)>,
}

impl Run<'_> {
    fn call(&mut self, call: ChannelCall) {
        let mut sounds = SystemSounds {
            system: Some(&mut self.sounds),
            tree: self.tree,
            aliases: &self.data.aliases,
        };
        (self.screen).motion_channel_call(self.tree, self.data, self.trader, &call, &mut sounds);
    }

    /// 20 ms frames until `until` ms: the level's update, the sounds', the
    /// check; `respond` gives the calls of an event's function.
    fn run(&mut self, until: u64, respond: &dyn Fn(ChannelEvent) -> Vec<ChannelCall>) {
        while self.now < until {
            self.now += 20;
            self.screen.set_global_clock(self.now);
            self.screen.update(self.now, &Default::default());
            self.sounds.update(self.now, &mut NoDevice);
            let mut sounds = SystemSounds {
                system: Some(&mut self.sounds),
                tree: self.tree,
                aliases: &self.data.aliases,
            };
            let (now, raised) = (self.now, &mut self.raised);
            let trader = self.trader;
            let mut raise = |id: u16, event| {
                assert_eq!(id, trader);
                raised.push((now, event));
                Ok::<_, ()>(respond(event))
            };
            (self.screen)
                .motion_channels_check(self.tree, self.data, &mut sounds, &mut raise)
                .unwrap();
            assert_eq!(self.screen.take_fatal(), None);
        }
    }

    fn take_raised(&mut self) -> Vec<(u64, ChannelEvent)> {
        std::mem::take(&mut self.raised)
    }

    fn instance(&self) -> usize {
        (self.screen.world().objects.instances.iter())
            .position(|i| i.id == self.trader)
            .unwrap()
    }
}

fn with_level(test: impl FnOnce(Run<'_>)) {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let tree = DbVfs::open_game(&root, Some("mods")).unwrap();
    let data = GameData::read(&tree, None, Resolution::new(1024, 768)).unwrap();
    let screen = LevelScreen::load(&tree, &data, 0).unwrap();
    let trader = (screen.world().objects.instances.iter())
        .find(|i| i.name == "escape_trader")
        .unwrap();
    assert_eq!(trader.class_id, TRADER_CLASS);
    let trader = trader.id;
    let mut sounds = SoundSystem::new(16);
    sounds.set_volume(Category::Effects, 1.0);
    test(Run {
        tree: &tree,
        data: &data,
        screen,
        sounds,
        trader,
        now: 0,
        raised: Vec::new(),
    });
}

fn nothing(_: ChannelEvent) -> Vec<ChannelCall> {
    Vec::new()
}

fn global(name: &str) -> ChannelCall {
    ChannelCall::SetGlobalAnim(name.into())
}

#[test]
fn the_start_chain() {
    use ChannelEvent::*;
    with_level(|mut run| {
        // 1. Spawning: the trader online at the start has his record, both
        // channels ended; each check raises event 30 to an empty entry.
        let k = run.instance();
        let record = run.screen.motion_channels().record(k).unwrap();
        assert!(record.global.ended() && record.head.ended() && record.sound.is_none());
        assert_eq!(run.screen.motion_channels().traders(), [k]);
        run.run(60, &nothing);
        assert_eq!(
            run.take_raised(),
            [
                (20, GlobalAnimRequest),
                (40, GlobalAnimRequest),
                (60, GlobalAnimRequest)
            ]
        );
        // Nothing plays on his model before the first start (3).
        assert!(run.screen.motions().model(k).is_none());

        // 3. `new_start_intro1`: `spinka_idle` on all partitions, 1.667 s;
        // its partitions notify at about 1.64 s.
        let start = run.now;
        run.call(global("spinka_idle"));
        run.run(start + 1600, &nothing);
        assert!(run.take_raised().is_empty());
        run.run(start + 1700, &nothing);
        let raised = run.take_raised();
        let first = raised[0].0 - start;
        assert!((1620..=1680).contains(&first), "{first}");
        assert!(raised.iter().all(|&(_, e)| e == GlobalAnimRequest));
        let record = run.screen.motion_channels().record(k).unwrap();
        assert!(record.global.ended() && record.head.ended());

        // 4. `new_start_intro2`: `vstuplenie` (`torso`) and, with the
        // monologue, `vstuplenie_head` (`head`); both end at about 23.36 s
        // in the same tracks update, the sound (26,369 ms) still plays.
        let start = run.now;
        run.call(global("vstuplenie"));
        run.call(ChannelCall::SetSound {
            sound: MONOLOG.into(),
            head_anim: "vstuplenie_head".into(),
        });
        let record = run.screen.motion_channels().record(k).unwrap();
        assert!(!record.global.ended() && !record.head.ended());
        let sound = record.sound.unwrap().0.unwrap();
        assert!(run.sounds.has_emitter(sound));
        let head = |e: ChannelEvent| match e {
            HeadAnimRequest => vec![ChannelCall::SetHeadAnim("vstuplenie_head".into())],
            _ => Vec::new(),
        };
        run.run(start + 23_300, &head);
        assert!(run.take_raised().is_empty());
        run.run(start + 23_400, &head);
        let raised = run.take_raised();
        let first = raised[0].0 - start;
        assert!((23_320..=23_400).contains(&first), "{first}");
        // Event 30, the head marked ended, event 31 (`vstuplenie_head`
        // again), at every check.
        let checks = raised.len() / 2;
        assert!(checks >= 2);
        for pair in raised.chunks(2) {
            assert_eq!([pair[0].1, pair[1].1], [GlobalAnimRequest, HeadAnimRequest]);
            assert_eq!(pair[0].0, pair[1].0);
        }
        // The sound's end: event 32 once, then no sound and no event 31.
        run.run(start + 26_300, &head);
        assert!(!run.take_raised().iter().any(|&(_, e)| e == SoundEnd));
        run.run(start + 26_500, &head);
        let raised = run.take_raised();
        let ends: Vec<u64> = (raised.iter())
            .filter(|&&(_, e)| e == SoundEnd)
            .map(|&(t, _)| t - start)
            .collect();
        assert_eq!(ends.len(), 1);
        assert!((26_369..=26_420).contains(&ends[0]), "{ends:?}");
        let after: Vec<_> = raised
            .iter()
            .filter(|&&(t, _)| t - start > ends[0])
            .collect();
        assert!(after.iter().all(|&&(_, e)| e == GlobalAnimRequest));
        let record = run.screen.motion_channels().record(k).unwrap();
        assert!(record.sound.is_none());
        assert!(!run.sounds.sound_ids().any(|s| s == sound));
    });
}

#[test]
fn a_cycle_the_model_lacks_is_fatal() {
    with_level(|mut run| {
        run.call(global("no such cycle"));
        assert_eq!(
            run.screen.take_fatal().as_deref(),
            Some("! MODEL: can't find cycle: no such cycle")
        );
    });
}
