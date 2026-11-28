// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/script-api-sound.md on every target, one check per
//! section, over sound files of our own (their names and lengths: the
//! files' decoding is the sound system's).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use volya_engine_core::config::PathAliases;
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::sound::system::PlayFlags;
use volya_engine_formats::ltx;
use volya_engine_formats::model::transform;

use super::dialogs_and_trade::data;
use super::ensure;
use super::files::{FSGAME, Tree};
use super::services::run;
use crate::dialogs::{DialogsAndTrade, Event};
use crate::files::FileSystem;
use crate::game_object::{GameObjects, ObjectClass};
use crate::loader::{Prefix, Scripts};
use crate::services::EngineServices;
use crate::sound::{FILE_TYPE, Request, SND_TYPES, ScriptSounds, SoundFile, SoundFiles};
use crate::story::Story;
use crate::{Error, ErrorKind, Value};

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("1 registered", registered),
    ("2.1 construction", construction),
    ("2.3 play and play_at_pos", play),
    ("2.4 play_no_feedback", play_no_feedback),
    ("2.5 stop", stop),
    ("2.6 playing and length", playing_and_length),
    ("2.7 positions", positions),
    ("2.8 properties", properties),
    ("2.9 collection", collection),
    ("3 snd_type", snd_type),
    ("4 sound_params and SoundInfo", records),
    ("5.1 the effects factor", effects_factor),
    ("5.2 prefetch_sound", prefetch_sound),
    ("5.3 iterate_sounds", iterate_sounds),
    ("6.2 add_sound", add_sound),
    ("6.3 play_sound", play_sound),
    ("6.4 masks and counts", masks_and_counts),
    ("6.5 prefixes", prefixes),
    ("6.6 the trader's external sound", external_sound),
    ("7 hearing and remembered sounds", hearing),
    ("9 worked examples", worked_examples),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| f().err().map(|e| format!("sound {what}: {e}")))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

const PLAYER: u16 = 0;
const STALKER: u16 = 30;
const DOG: u16 = 40;
const TRADER: u16 = 60;
const BOX: u16 = 70;

/// Our sound files: name as `$game_sounds$` holds it, and length.
const FILES: &[(&str, u64)] = &[
    ("door\\open", 1500),
    ("a", 100),
    ("a0", 200),
    ("a2", 300),
    ("b1", 400),
    ("talk\\hello", 1000),
    ("talk\\hello0", 1100),
    ("talk\\hello1", 1200),
    ("talk\\bye", 900),
    ("broken", 0),
];

/// [`SoundFiles`] over [`FILES`]: `broken` exists and fails to load, as a
/// file that isn't Ogg Vorbis (S1 5).
struct OurFiles;

impl SoundFiles for OurFiles {
    fn exists(&self, name: &[u8]) -> bool {
        FILES.iter().any(|(n, _)| n.as_bytes() == name)
    }

    fn load(&mut self, name: &[u8], _: Option<&str>) -> Result<SoundFile, String> {
        let name = String::from_utf8_lossy(name).to_lowercase();
        let name = name.rsplit_once('.').map_or(name.as_str(), |(n, _)| n);
        match FILES.iter().find(|(n, _)| *n == name) {
            Some(("broken", _)) => Err("broken.ogg: not a valid Ogg Vorbis stream".into()),
            Some(&(_, ms)) => Ok(SoundFile {
                duration_ms: ms,
                min_distance: 2.0,
                max_distance: 40.0,
            }),
            None => Err(format!("{name}.ogg: cannot open sound file")),
        }
    }
}

const SYSTEM: &str = "[trade]\r\nbuy_price_factor_hostile = 1\r\nbuy_price_factor_friendly = 1\r\n\
sell_price_factor_hostile = 1\r\nsell_price_factor_friendly = 1\r\n\
[stalker]\r\nsound_threshold = 0.25\r\n";

struct State {
    s: Scripts,
    sounds: Rc<RefCell<ScriptSounds>>,
    dialogs: Rc<RefCell<DialogsAndTrade>>,
    services: Rc<RefCell<EngineServices>>,
}

fn message(e: Error) -> String {
    e.message
}

/// A state with everything up to the sound registered, `_G` loaded, a
/// global `obj(id)` giving handles, the player, a stalker, a dog, a trader
/// and a box.
fn state() -> Result<State, String> {
    let tree = Tree(
        [("gamedata/scripts/_g.script", "")]
            .iter()
            .map(|(p, t)| ((*p).to_owned(), t.as_bytes().to_vec()))
            .collect::<BTreeMap<_, _>>(),
    );
    let tree = Rc::new(tree);
    let aliases = PathAliases::parse(FSGAME, "").map_err(|e| e.to_string())?;
    let mut s = Scripts::new(tree.clone(), &aliases, Prefix::Default).map_err(message)?;
    let console = Console::new(ConsoleFiles {
        default_controls: Vec::new(),
        presets: Default::default(),
    });
    let system = ltx::parse(SYSTEM.as_bytes()).map_err(|e| e.to_string())?;
    let services = Rc::new(RefCell::new(EngineServices::new(console, system.clone())));
    s.register_engine_services(Rc::clone(&services))
        .map_err(message)?;
    let files = Rc::new(RefCell::new(FileSystem::new(
        tree.clone(),
        aliases,
        Rc::new(system.clone()),
        Rc::new(system),
    )));
    s.register_configuration_and_files(Rc::clone(&files), Rc::clone(&services))
        .map_err(message)?;
    let objects = Rc::new(RefCell::new(GameObjects::new()));
    s.register_game_object_basics(Rc::clone(&objects), Rc::clone(&services), files)
        .map_err(message)?;
    s.register_story(Rc::new(RefCell::new(Story::new())))
        .map_err(message)?;
    let dialogs = Rc::new(RefCell::new(DialogsAndTrade::new()));
    s.register_dialogs_and_trade(Rc::clone(&dialogs))
        .map_err(message)?;
    let sounds = Rc::new(RefCell::new(ScriptSounds::new()));
    sounds.borrow_mut().files = Some(Box::new(OurFiles));
    s.register_sound(Rc::clone(&sounds), tree)
        .map_err(message)?;
    let shared = Rc::clone(&objects);
    s.lua_mut()
        .register_object_function("obj", move |args| {
            let Some(Value::Number(id)) = args.first() else {
                return Ok(None);
            };
            Ok(shared.borrow().get(*id as u16).and_then(|o| o.handle()))
        })
        .map_err(message)?;
    crate::entry_points::install(s.lua_mut()).map_err(message)?;
    s.load_globals().map_err(message)?;
    for (class, mut data) in [
        (ObjectClass::Player, data(PLAYER, "actor", "O_ACTOR")),
        (
            ObjectClass::Named("CAI_Stalker"),
            data(STALKER, "stalker", "AI_STL_S"),
        ),
        (ObjectClass::Named("CAI_Dog"), data(DOG, "dog", "AI_DOG_R")),
        (
            ObjectClass::Named("CAI_Trader"),
            data(TRADER, "trader", "AI_TRADE"),
        ),
        (
            ObjectClass::Named("CPhysicObject"),
            data(BOX, "box", "O_PHYS_S"),
        ),
    ] {
        if data.id == STALKER {
            data.placement = transform([1.0, 2.0, 3.0], [0.0; 3]);
        }
        s.spawn_engine_object(class, data)
            .map_err(|e| e.error.message)?;
    }
    Ok(State {
        s,
        sounds,
        dialogs,
        services,
    })
}

const PRELUDE: &str = "local actor, stalker, dog, trader, box = \
    obj(0), obj(30), obj(40), obj(60), obj(70)\n";

fn check_lua(t: &mut State, name: &str, body: &str) -> Result<(), String> {
    run(&mut t.s, name, &format!("{PRELUDE}{body}"))
}

fn requests(t: &State) -> Vec<Request> {
    t.sounds.borrow_mut().take_requests()
}

/// The keys of the sounds made, in order.
fn keys(t: &State) -> Vec<u32> {
    let mut keys: Vec<u32> = t.sounds.borrow().sounds().map(|(k, _)| k).collect();
    keys.sort();
    keys
}

/// `body` stops the state with a fatal error whose message holds `text`.
fn fatal(body: &str, text: &str) -> Result<(), String> {
    let mut t = state()?;
    let e =
        t.s.exec("=fatal", format!("{PRELUDE}{body}").as_bytes())
            .err();
    ensure(
        e.as_ref()
            .is_some_and(|e| e.kind == ErrorKind::Fatal && e.message.contains(text)),
        || format!("{body}: {e:?}"),
    )
}

/// 1.1, 1.2: the classes, their constants and the functions.
pub fn registered() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "1",
        r#"
        eq(sound_object.s3d, 0, "s3d") eq(sound_object.s2d, 2, "s2d")
        eq(sound_object.looped, 1, "looped")
        eq(snd_type.no_sound, 0, "no_sound") eq(snd_type.talk, 16384, "talk")
        raises(function() return sound_params() end, "", "sound_params constructed")
        raises(function() return SoundInfo() end, "", "SoundInfo constructed")
        for _, n in ipairs({ "add_sound", "play_sound", "remove_sound", "set_sound_mask",
          "active_sound_count", "sound_prefix", "external_sound_start", "external_sound_stop",
          "sound_voice_prefix", "set_sound_threshold", "restore_sound_threshold",
          "memory_sound_objects", "get_sound_info" }) do
          eq(type(game_object[n]), "function", n)
        end
        for _, n in ipairs({ "set_snd_volume", "get_snd_volume", "prefetch_sound",
          "iterate_sounds" }) do
          eq(type(level[n]), "function", n)
        end
        "#,
    )
}

/// 2.1: a sound when the file exists, in the effects category with type
/// 0 or the type given; none otherwise; a file that fails to load fatal.
pub fn construction() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "2.1",
        r#"
        s1 = sound_object("door\\open")
        s2 = sound_object("a", snd_type.weapon_shoot)
        s3 = sound_object("missing")
        eq(s3:playing(), false, "no sound: not playing")
        s3:play(actor)
        raises(function() return sound_object(1) end, "", "a number name")
        "#,
    )?;
    let sounds = t.sounds.borrow();
    let mut made: Vec<(Vec<u8>, i32)> = sounds
        .sounds()
        .map(|(_, s)| (s.name.clone(), s.ai_type))
        .collect();
    made.sort();
    ensure(
        made == [
            (b"a".to_vec(), 0x8020_0000_u32 as i32),
            (b"door\\open".to_vec(), 0),
        ],
        || format!("sounds made {made:?}"),
    )?;
    drop(sounds);
    let r = requests(&t);
    ensure(
        r.len() == 2 && r.iter().all(|r| matches!(r, Request::Create { .. })),
        || format!("only the two creations: {r:?}"),
    )?;
    fatal(r#"sound_object("broken")"#, "not a valid Ogg Vorbis stream")
}

/// 2.3: the six forms, the owner, a restart keeping the emitter.
pub fn play() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "2.3",
        r#"
        s = sound_object("door\\open")
        s:play(actor)
        s:play(stalker, 0.5)
        s:play(stalker, 0.25, sound_object.s2d + sound_object.looped)
        p = sound_object("a")
        p:play_at_pos(dog, vector():set(1, 2, 3))
        p:play_at_pos(dog, vector():set(4, 5, 6), 2)
        p:play_at_pos(dog, vector():set(7, 8, 9), 1.5, 7)
        eq(s:playing(), true, "playing")
        local v = p:get_position()
        eq(v.x, 7, "x") eq(v.z, 9, "z")
        raises(function() s:play(nil) end, "game object expected", "nil owner")
        "#,
    )?;
    let [s, p] = keys(&t)[..] else {
        return Err("two sounds".into());
    };
    let flags = |looped, two_d, delay| PlayFlags {
        looped,
        two_d,
        delay,
    };
    let play = |key, flags, position, owner| Request::Play {
        key,
        flags,
        position,
        owner,
    };
    let got: Vec<Request> = requests(&t)
        .into_iter()
        .filter(|r| !matches!(r, Request::Create { .. }))
        .collect();
    let want = [
        play(s, flags(false, false, 0.0), None, PLAYER),
        play(s, flags(false, false, 0.5), None, STALKER),
        play(s, flags(true, true, 0.25), None, STALKER),
        play(p, flags(false, false, 0.0), Some([1.0, 2.0, 3.0]), DOG),
        play(p, flags(false, false, 2.0), Some([4.0, 5.0, 6.0]), DOG),
        // Flags 7: bits 1 and 2 only.
        play(p, flags(true, true, 1.5), Some([7.0, 8.0, 9.0]), DOG),
    ];
    ensure(got == want, || format!("requests {got:?}"))?;
    // The restart keeps the first play's looped flag (S2 3.2).
    let e = t.sounds.borrow().emitter(s);
    ensure(e.is_some_and(|e| !e.looped && e.two_d), || {
        format!("restarted emitter {e:?}")
    })
}

/// 2.4: an emitter of no sound object.
pub fn play_no_feedback() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "2.4",
        r#"
        s = sound_object("a")
        s:play_no_feedback(actor, sound_object.s2d, 0.5, vector():set(1, 0, 0), 0.7)
        eq(s:playing(), false, "the object's sound has no emitter")
        "#,
    )?;
    let got = requests(&t);
    let want = Request::PlayIndependent {
        key: keys(&t)[0],
        flags: PlayFlags {
            looped: false,
            two_d: true,
            delay: 0.5,
        },
        position: [1.0, 0.0, 0.0],
        volume: 0.7,
        owner: PLAYER,
    };
    ensure(got.get(1) == Some(&want), || format!("requests {got:?}"))
}

/// 2.5: at once, with fade; nothing without an emitter.
pub fn stop() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "2.5",
        r#"
        s = sound_object("a")
        s:stop() s:stop_deffered()
        s:play(actor) s:stop_deffered()
        eq(s:playing(), true, "fading")
        s:stop()
        eq(s:playing(), false, "stopped")
        "#,
    )?;
    let key = keys(&t)[0];
    let got: Vec<Request> = requests(&t).into_iter().skip(2).collect();
    ensure(
        got == [Request::StopWithFade(key), Request::Stop(key)],
        || format!("requests {got:?}"),
    )
}

/// 2.6: an emitter in every phase; the duration in ms; no sound fatal.
pub fn playing_and_length() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "2.6",
        r#"
        s = sound_object("door\\open")
        eq(s:length(), 1500, "length")
        eq(s:playing(), false, "before")
        s:play(actor, 10)
        eq(s:playing(), true, "waiting for its delay")
        "#,
    )?;
    fatal(r#"sound_object("missing"):length()"#, "no sound")
}

/// 2.7: (0, 0, 0) without an emitter; setting then is fatal.
pub fn positions() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "2.7",
        r#"
        s = sound_object("a")
        local v = s:get_position()
        eq(v.x + v.y + v.z, 0, "no emitter")
        s:play(actor)
        s:set_position(vector():set(3, 2, 1))
        eq(s:get_position().x, 3, "set")
        "#,
    )?;
    let key = keys(&t)[0];
    let got = requests(&t);
    ensure(
        got.last() == Some(&Request::SetPosition(key, [3.0, 2.0, 1.0])),
        || format!("requests {got:?}"),
    )?;
    fatal(r#"sound_object("a"):set_position(vector())"#, "no emitter")
}

/// 2.8: the emitter's controls; a new emitter's; assigning without one.
pub fn properties() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "2.8",
        r#"
        s = sound_object("a")
        s.volume = 0.5
        s:play(actor)
        eq(s.volume, 1, "a new emitter's volume") eq(s.frequency, 1, "frequency")
        eq(s.min_distance, 2, "the file's min") eq(s.max_distance, 40, "max")
        s.volume = 0.25 s.frequency = 2 s.min_distance = 3 s.max_distance = 50
        eq(s.volume, 0.25, "volume") eq(s.min_distance, 3, "min")
        eq(s.max_distance, 50, "max set")
        "#,
    )?;
    let key = keys(&t)[0];
    let got: Vec<Request> = requests(&t).into_iter().skip(2).collect();
    ensure(
        got == [
            Request::SetVolume(key, 0.25),
            Request::SetFrequency(key, 2.0),
            Request::SetRange(key, 3.0, 40.0),
            Request::SetRange(key, 3.0, 50.0),
        ],
        || format!("requests {got:?}"),
    )?;
    fatal(r#"local _ = sound_object("a").volume"#, "no emitter")
}

/// 2.9: a collected object's sound is discarded.
pub fn collection() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "2.9",
        r#"
        sound_object("a"):play_at_pos(actor, vector())
        collectgarbage() collectgarbage()
        "#,
    )?;
    let got = requests(&t);
    ensure(
        matches!(got.last(), Some(Request::Discard(_))) && keys(&t).is_empty(),
        || format!("requests {got:?}"),
    )
}

/// 3: the 46 constants, the top-bit ones negative.
pub fn snd_type() -> Result<(), String> {
    let mut t = state()?;
    let mut body = String::new();
    for (name, value) in SND_TYPES {
        body.push_str(&format!(
            "eq(snd_type.{name}, {}, \"{name}\")\n",
            value as i32
        ));
    }
    body.push_str("eq(snd_type.weapon, -2147483648, \"weapon\")\n");
    body.push_str("eq(snd_type.weapon_shoot, -2145386496, \"weapon_shoot\")\n");
    check_lua(&mut t, "3", &body)?;
    ensure(SND_TYPES.len() == 46, || "46 constants".into())
}

/// 4, 7.4: the records' fields.
pub fn records() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "4",
        r#"
        local i = stalker:get_sound_info()
        eq(i.who, nil, "who") eq(i.power, 0, "power") eq(i.time, 0, "time")
        eq(i.danger, 0, "danger") eq(i.position.x, 0, "position")
        i.power = 0.5 i.who = dog i.position = vector():set(1, 2, 3)
        eq(i.power, 0.5, "power set") eq(i.who:id(), 40, "who set")
        eq(i.position.y, 2, "position set")
        eq(stalker:get_sound_info().power, 0, "a copy")
        "#,
    )
}

/// 5.1: *F* kept to 0 … 1.
pub fn effects_factor() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "5.1",
        r#"
        eq(level.get_snd_volume(), 1, "initial")
        level.set_snd_volume(2) eq(level.get_snd_volume(), 1, "kept to 1")
        level.set_snd_volume(-1) eq(level.get_snd_volume(), 0, "kept to 0")
        level.set_snd_volume(0.5) eq(level.get_snd_volume(), 0.5, "half")
        "#,
    )?;
    let got = requests(&t);
    ensure(
        got == [
            Request::SetEffectsFactor(1.0),
            Request::SetEffectsFactor(0.0),
            Request::SetEffectsFactor(0.5),
        ],
        || format!("requests {got:?}"),
    )
}

/// 5.2: lowered, cut at its last `.`, once per level; missing fatal.
pub fn prefetch_sound() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "5.2",
        r#"
        level.prefetch_sound("Talk\\Hello.ogg")
        level.prefetch_sound("talk\\hello")
        "#,
    )?;
    let got = requests(&t);
    ensure(got == [Request::Prefetch("talk\\hello".into())], || {
        format!("requests {got:?}")
    })?;
    fatal(r#"level.prefetch_sound("missing")"#, "cannot open")
}

/// 5.3: the whole list for an item's own file, then its numbered ones;
/// both forms; an error in `f` fatal.
pub fn iterate_sounds() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "5.3",
        r#"
        local got = {}
        level.iterate_sounds("a, b", 3, function(n) table.insert(got, n) end)
        eq(table.concat(got, "|"), "a, b|a0|a2|b1", "unbound")
        got = {}
        level.iterate_sounds("a", 1, "bound", function(o, n) table.insert(got, o .. n) end)
        eq(table.concat(got, "|"), "bounda|bounda0", "bound")
        level.iterate_sounds("a", 1, 5)
        "#,
    )?;
    ensure(requests(&t).is_empty(), || "plays nothing".into())?;
    fatal(
        r#"level.iterate_sounds("a", 0, function() error("boom", 0) end)"#,
        "LUA error: boom",
    )
}

/// 6.2: the collection's sounds with the prefix; once per id; fighters.
pub fn add_sound() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "6.2",
        r#"
        stalker:sound_prefix("talk\\")
        eq(stalker:add_sound("hello, bye", 2, snd_type.talk, 2, 1, 7), 4, "sounds")
        eq(stalker:add_sound("bye", 0, snd_type.talk, 2, 1, 7), 0, "id taken")
        eq(dog:add_sound("a", 3, 0, 1, 1, 1, "bip01_spine"), 3, "no prefix")
        eq(trader:add_sound("hello", 2, 0, 1, 1, 1), 0, "not a fighter")
        "#,
    )?;
    let s = t.sounds.borrow();
    let c = s
        .creatures
        .players
        .get(&STALKER)
        .and_then(|p| p.collections.get(&7));
    let names: Vec<String> = c.map_or(Vec::new(), |c| {
        c.sounds
            .iter()
            .map(|n| String::from_utf8_lossy(n).into_owned())
            .collect()
    });
    ensure(
        names == ["talk\\hello", "talk\\hello0", "talk\\hello1", "talk\\bye"]
            && c.is_some_and(|c| c.bone == b"bip01_head" && c.priority == 2 && c.mask == 1),
        || format!("collection {names:?}"),
    )
}

/// 6.3: who may play, the queued sound, the times and the play.
pub fn play_sound() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "6.3",
        r#"
        stalker:sound_prefix("talk\\")
        stalker:add_sound("bye", 0, snd_type.talk, 2, 1, 7)
        stalker:add_sound("hello", 0, 0, 3, 2, 8)
        stalker:play_sound(7)
        eq(stalker:active_sound_count(), 1, "queued and playing")
        stalker:play_sound(7)
        eq(stalker:active_sound_count(), 1, "equal priority refused")
        stalker:play_sound(8, 1000, 500)
        eq(stalker:active_sound_count(), 1, "another mask: queued, not started")
        eq(stalker:active_sound_count(true), 1, "one emitter")
        dog:play_sound(7)
        "#,
    )?;
    let s = t.sounds.borrow();
    let q = s.creatures.players.get(&STALKER).map(|p| p.queued.clone());
    let q = q.unwrap_or_default();
    // The second queued sound starts 500 … 999 ms on and lasts its length.
    let ok = q.len() == 2
        && (q[0].start, q[0].stop) == (0, 900)
        && (500..1000).contains(&q[1].start)
        && q[1].stop == q[1].start + 1000;
    ensure(ok, || format!("queued {q:?}"))?;
    let made: Vec<i32> = q
        .iter()
        .filter_map(|q| s.sound(q.key).map(|x| x.ai_type))
        .collect();
    ensure(made == [FILE_TYPE, FILE_TYPE], || {
        format!("the file's own type: {made:?}")
    })?;
    let key = q[0].key;
    drop(s);
    let got = requests(&t);
    let played = got.iter().any(|r| {
        *r == Request::Play {
            key,
            flags: PlayFlags::default(),
            position: Some([1.0, 2.0, 3.0]),
            owner: STALKER,
        }
    });
    ensure(played, || format!("played at the object: {got:?}"))?;
    ensure(
        !got.iter()
            .any(|r| matches!(r, Request::Play { key: k, .. } if *k == q[1].key)),
        || "the later one is not started".into(),
    )
}

/// 6.4: the mask blocks and removes; counts; a removed collection.
pub fn masks_and_counts() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "6.4",
        r#"
        stalker:sound_prefix("talk\\")
        stalker:add_sound("bye", 0, 0, 2, 1, 7)
        stalker:play_sound(7)
        stalker:set_sound_mask(1)
        eq(stalker:active_sound_count(), 0, "removed by the mask")
        stalker:play_sound(7)
        eq(stalker:active_sound_count(), 0, "blocked")
        stalker:set_sound_mask(0)
        stalker:play_sound(7)
        eq(stalker:active_sound_count(), 1, "unblocked")
        stalker:remove_sound(7)
        eq(stalker:active_sound_count(), 1, "the queued sound stays")
        eq(trader:active_sound_count(), -1, "not a fighter")
        eq(box:active_sound_count(true), -1, "not a fighter either")
        "#,
    )?;
    let got = requests(&t);
    ensure(
        got.iter()
            .filter(|r| matches!(r, Request::Discard(_)))
            .count()
            == 1,
        || format!("the masked sound discarded: {got:?}"),
    )
}

/// 6.5: the sound prefix and the voice prefix.
pub fn prefixes() -> Result<(), String> {
    let mut t = state()?;
    (t.sounds.borrow_mut().creatures.voice_prefixes).insert(STALKER, b"voice\\".to_vec());
    check_lua(
        &mut t,
        "6.5",
        r#"
        eq(stalker:sound_prefix(), nil, "none")
        stalker:sound_prefix("x\\") eq(stalker:sound_prefix(), "x\\", "set")
        trader:sound_prefix("y") eq(trader:sound_prefix(), nil, "not a fighter")
        eq(stalker:sound_voice_prefix(), "voice\\", "snd_config")
        eq(trader:sound_voice_prefix(), "", "none given")
        eq(box:sound_voice_prefix(), nil, "not an inventory owner")
        "#,
    )
}

/// 6.6: the trader's external sound as TC2 2.5, 2.6; others nothing.
pub fn external_sound() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "6.6",
        r#"
        trader:external_sound_start("trader_tutorial") trader:external_sound_stop()
        stalker:external_sound_start("x") stalker:external_sound_stop()
        "#,
    )?;
    let got = t.dialogs.borrow_mut().take_events();
    ensure(
        got == [
            Event::ExternalSoundStart {
                trader: TRADER,
                sound: b"trader_tutorial".to_vec(),
            },
            Event::ExternalSoundStop { trader: TRADER },
        ],
        || format!("events {got:?}"),
    )
}

/// 7: the thresholds, the remembered sounds.
pub fn hearing() -> Result<(), String> {
    let mut t = state()?;
    check_lua(
        &mut t,
        "7",
        r#"
        stalker:set_sound_threshold(0.5)
        dog:restore_sound_threshold()
        local n = 0
        for s in stalker:memory_sound_objects() do n = n + 1 end
        eq(n, 0, "nothing remembered")
        trader:set_sound_threshold(1)
        "#,
    )?;
    let s = t.sounds.borrow();
    let h = &s.creatures.hearing;
    let stalker = h.get(&STALKER).map(|h| (h.current, h.base));
    let dog = h.get(&DOG).map(|h| (h.current, h.base));
    ensure(
        stalker == Some((0.5, 0.25)) && dog == Some((0.05, 0.05)) && !h.contains_key(&TRADER),
        || format!("thresholds {stalker:?} {dog:?}"),
    )?;
    drop(s);
    fatal("local _ = trader:memory_sound_objects()", "not a fighter")
}

/// 9: `sr_sound2d`'s 500 ms as 0.5 s; 6.3's worked example of priorities.
pub fn worked_examples() -> Result<(), String> {
    let mut t = state()?;
    t.services.borrow_mut().device.frame(100, 0.0, false);
    check_lua(
        &mut t,
        "9",
        r#"
        local d = 500
        snd = sound_object("a")
        snd:play_at_pos(actor, vector():set(1, 1, 1), d / 1000.0, sound_object.s3d)
        stalker:sound_prefix("talk\\")
        stalker:add_sound("bye", 0, snd_type.talk, 2, 1, 1)
        stalker:add_sound("hello", 0, snd_type.talk, 1, 1, 2)
        stalker:play_sound(1)
        stalker:play_sound(1)
        eq(stalker:active_sound_count(), 1, "refused")
        stalker:play_sound(2)
        eq(stalker:active_sound_count(), 1, "replaced")
        "#,
    )?;
    let got = requests(&t);
    let delayed = got.iter().any(|r| {
        matches!(r, Request::Play { flags, position: Some([1.0, 1.0, 1.0]), owner: PLAYER, .. }
            if *flags == PlayFlags { looped: false, two_d: false, delay: 0.5 })
    });
    ensure(delayed, || format!("0.5 s: {got:?}"))?;
    let s = t.sounds.borrow();
    let q = s.creatures.players.get(&STALKER).map(|p| p.queued.clone());
    let q = q.unwrap_or_default();
    ensure(
        q.len() == 1 && q[0].priority == 1 && (q[0].start, q[0].stop) == (100, 1100),
        || format!("queued {q:?}"),
    )?;
    let discarded = got
        .iter()
        .filter(|r| matches!(r, Request::Discard(_)))
        .count();
    ensure(discarded == 1, || format!("the first stopped: {got:?}"))
}
