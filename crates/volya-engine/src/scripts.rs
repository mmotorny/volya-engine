// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The game's scripts from program start to a New Game's first frame
//! (docs/behavior/script-api-engine-entry-points.md section 3; section
//! numbers are its): startup at program start, then the engine's calls
//! into scripts placed in the New Game's loading captions
//! (docs/behavior/loading-screen.md section 5) whose work they belong to.
//! See docs/architecture/scripts.md, Entry points.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::rc::Rc;
use std::sync::Arc;

use crate::characters::Characters;
use crate::core::character::hits::{Hit, HitType};
use crate::core::collision::CollisionWorld;
use crate::core::collision::geometry::{self, Vec3};
use crate::core::config::{self, Config, PathAliases};
use crate::core::dialogs::Dialogs;
use crate::core::scripted_motion_channels::{ChannelCall, ChannelEvent, ChannelSounds};
use crate::core::simulation::ACTOR_ID;
use crate::core::vfs::Vfs;
use crate::core::zones;
use crate::data::GameData;
use crate::level::LevelScreen;
use crate::level_physics::{LevelPhysics, ScriptWorld};
use crate::loading_screen::{GAME_TYPE, Loading, Stage};
use crate::objects::{LoadedModel, Names};
use crate::script::classes::OverrideFailure;
use crate::script::dialogs::{Event as DialogEvent, SharedDialog, TraderMotion, offer_talk};
use crate::script::entry_points::{GameScripts, Startup};
use crate::script::files::FileSystem;
use crate::script::game_object::callbacks::ObjectEvent;
use crate::script::game_object::object_binder::update_delta;
use crate::script::game_object::physics::{LevelQueries, PhysicsEvent, PhysicsShell};
use crate::script::game_object::{
    EngineState, Event, GameObjects, ObjectClass, Physics, Restrictor, SpawnData, Visual,
};
use crate::script::loader::{Prefix, Trees};
use crate::script::object_factory::{CreatedObject, GameTypeKind, Sides};
use crate::script::services::EngineServices;
use crate::script::simulation::Event as SimulationEvent;
use crate::script::story::{Icon, SharedInfoPortions, Story};
use crate::script::window_files::WindowFiles;
use crate::script::{Error, ErrorKind};
use volya_engine_formats::cp1251;
use volya_engine_formats::dialog_files::Dialog;
use volya_engine_formats::model;
use volya_engine_formats::object::{Kind, Object, ObjectRecord, Shape};
use volya_engine_formats::patrol_path::PatrolPaths;
use volya_engine_formats::ui::Resolution;

/// The server options of a New Game from the menu, `start
/// server(all/single/alife/new)` (level-loading.md section 2.1).
pub const SERVER_OPTIONS: &str = "all/single/alife/new";

/// The precache phase the client's synchronising asks for (3.3 step
/// 11.6; loading-screen.md's 30 frames).
pub const PRECACHE_FRAMES: u32 = 30;

/// Program start (3.1 step 2): the engine state the script interface
/// reads, and startup on the first game state, over the game's trees.
pub fn start(
    data: &GameData,
    trees: Rc<dyn Trees>,
    resolution: Resolution,
) -> io::Result<GameScripts> {
    let mut game = Err(io::Error::other("the game's files are unavailable"));
    trees.with_trees(&mut |tree, _| {
        game = config::load_aliased(tree, &data.aliases, "$game_config$", "game.ltx");
    })?;
    let system = Rc::new(data.system.clone());
    // TODO(#941): the services' console is the app's.
    let services = Rc::new(RefCell::new(EngineServices::new(
        data.console(),
        data.system.clone(),
    )));
    // TODO(#1003): the loose folders, the local time zone and the disk
    // changes from the platform.
    let files = FileSystem::new(
        Rc::clone(&trees),
        data.aliases.clone(),
        system,
        Rc::new(game?),
    );
    // Volya Engine has no command line: the default prefix, and the
    // rebuild is never kept (`-keep_lua`).
    let prefix = Prefix::from_command_line(&files.command_line);
    GameScripts::start(Startup {
        trees,
        aliases: data.aliases.clone(),
        prefix,
        services,
        files: Rc::new(RefCell::new(files)),
        // The level's objects as scripts see them, fed by the level
        // ([`client_spawning`], [`level_frame`]).
        objects: Rc::new(RefCell::new(GameObjects::new())),
        // The story's information portions are the level's, shared at
        // spawning ([`client_spawning`]).
        story: Rc::new(RefCell::new(story(data))),
        dialogs: Rc::default(),
        // The game's clocks as scripts read them, fed by the level
        // ([`client_spawning`], [`level_frame`]).
        times: Rc::default(),
        sounds: Rc::default(),
        simulation: Rc::default(),
        window_files: Rc::new(RefCell::new(window_files(data, resolution))),
    })
    .map_err(fatal)
}

/// What script API G3's names read: the texture registry the main menu
/// filled (5.3), the named colours, the string table, the UI fonts (created
/// with the title menu's, before scripts start) and the resolution of the
/// `_16` rule, which [`set_resolution`] keeps current.
fn window_files(data: &GameData, resolution: Resolution) -> WindowFiles {
    WindowFiles {
        resolution,
        colors: data.colors.clone(),
        textures: data.descriptions.clone(),
        strings: Some(Arc::clone(&data.strings)),
        fonts_created: true,
        windows: Vec::new(),
    }
}

/// The output resolution changed: the `_16` rule of script API G3's
/// readers follows it (ui-layout.md 1.1).
pub fn set_resolution(scripts: &GameScripts, resolution: Resolution) {
    scripts.window_files().borrow_mut().resolution = resolution;
}

/// The story state before a game: the interface's texture descriptions
/// for `set_icon_name` (script API P 3.3).
fn story(data: &GameData) -> Story {
    let descriptions = data.descriptions.clone();
    let mut story = Story::new();
    story.textures = Some(Rc::new(move |id: &[u8]| {
        let d = descriptions.get(&cp1251::decode(id))?;
        Some(Icon {
            texture: cp1251::encode(&d.file).unwrap_or_else(|| d.file.as_bytes().to_vec()),
            rect: [d.rect.x, d.rect.y, d.rect.width, d.rect.height],
        })
    }));
    story
}

/// A script error's report: the original's fatal error, the game's end.
pub fn fatal(e: Error) -> io::Error {
    io::Error::other(e.message)
}

/// The engine's calls into scripts in the work of New Game caption
/// `stage`, made before the caption's own work (3.3):
///
/// - `st_server_starting` (steps 8 and 9.1 to 9.3): the server's game
///   object, the game-type function asked on the server side in the
///   state of the latest startup;
/// - `st_loading_alife_simulator` (9.4): the simulation created, with the
///   rebuild and the start callback;
/// - `st_client_connecting_to` (11.2): the client's game object, the
///   game-type function asked on the client side in the new state;
/// - `st_client_spawning` (11.4): none before the caption's work; the
///   level's client objects come after it ([`client_spawning`]);
/// - `st_client_synchronising` (11.6): the precache phase.
///
/// Which caption each step shows under is our placement: the specs give
/// the stages' order, not the captions' boundaries within them.
pub fn stage(scripts: &mut GameScripts, stage: Stage, data: &GameData) -> Result<(), Error> {
    match stage {
        Stage::ServerStarting => {
            scripts.create_game_object(GAME_TYPE, true)?;
            // TODO(#997): gap 3, where the game-type script list is made:
            // as here (9.3), from `script.ltx [single] script`, empty in
            // this edition, so no coroutine.
        }
        Stage::LoadingAlifeSimulator => scripts.create_simulation(SERVER_OPTIONS, &data.system)?,
        // The New Game's records (offline 8.1) are the simulation's
        // (TODO(#950): their script server classes through the factory).
        Stage::ClientConnectingTo => {
            scripts.create_game_object(GAME_TYPE, false)?;
        }
        Stage::ClientSynchronising => scripts
            .services()
            .borrow_mut()
            .device
            .request_precache(PRECACHE_FRAMES),
        _ => {}
    }
    Ok(())
}

/// A New Game starts (3.2 step 7): the game type the services report, and
/// the level start's loading phase opened (spec B 4.1, 4.5).
pub fn new_game(scripts: &GameScripts) {
    let mut services = scripts.services().borrow_mut();
    services.game_started(SERVER_OPTIONS);
    services.open_loading_phase();
}

/// The level ends (the game is left for the menu, or a New Game starts
/// over it): every object the scripts still see is destroyed, its binder
/// getting `net_destroy()` (spec L 5.6; TODO(#1019): gap 2, the simplest
/// reading), the game's story forgotten, then the level's call list
/// emptied, none of it called (spec O 6.5), the server objects the
/// objects spawned from released (script API J), the default trade
/// parameters dropped (script API R 5.2), the game clock gone (script
/// API H) and the creatures' sound state forgotten (script API T). A
/// script error is fatal.
pub fn level_ended(scripts: &mut GameScripts) -> Result<(), Error> {
    let ids: Vec<u16> = scripts.objects().borrow().iter().map(|o| o.id).collect();
    for id in ids {
        let destroyed = scripts.scripts().destroy_object(id);
        destroyed.map_err(|f| override_fatal(scripts, f))?;
    }
    // Script API Y's world and level go with it: no physics world until
    // the next level's (2.2).
    {
        let physics = scripts.scripts().physics_state()?;
        let mut p = physics.borrow_mut();
        p.world = None;
        p.level = None;
        p.shells.clear();
        p.current_holder = None;
        p.exploding.clear();
        p.take_events();
    }
    scripts.story().borrow_mut().new_game();
    // Script API H: no game clock until the next level's.
    scripts.times().borrow_mut().level_ended();
    // The creatures' sound state goes with them (script API T 6, 7).
    scripts.sounds().borrow_mut().creatures = Default::default();
    scripts.scripts().clear_level_calls()?;
    // Script API K: `alife()` is nil until the next level's simulation.
    scripts.simulation().borrow_mut().end();
    scripts.scripts().release_server_objects()?;
    scripts.scripts().destroy_default_trade()
}

/// The New Game's load ended, with its level or not (3.3 step 12): the
/// loading phase closed.
pub fn load_ended(scripts: &GameScripts) {
    scripts.services().borrow_mut().close_loading_phase();
}

/// An object of the level as the scripts' spawning takes it (script API M
/// 3, 4, 5.4, 6.2): its record, its visual loaded with the level (with its
/// owner's names), its shell's physics part and the shell as script API
/// Y's members see it; whether it is a door under the door scheme
/// ([`crate::level_doors`]).
pub struct ClientObject<'a> {
    pub id: u16,
    pub record: ObjectRecord,
    pub model: Option<(&'a LoadedModel, &'a Names)>,
    pub physics: Option<Physics>,
    pub shell: Option<Rc<PhysicsShell>>,
    pub door: bool,
}

/// What the level's spawning gives the scripts (3.3 step 11.4): the
/// objects online at the start, byte 11 of each level vertex (M 4), the
/// game time and its factor, the level's physics world, collision and
/// patrol paths (script API Y) and the game's information portions
/// (script API P).
pub struct ClientSpawning<'a> {
    pub objects: Vec<ClientObject<'a>>,
    pub vertex_lights: Vec<u8>,
    pub game_time: u64,
    pub time_factor: f32,
    pub physics: Rc<RefCell<LevelPhysics>>,
    pub collision: Arc<CollisionWorld>,
    pub paths: PatrolPaths,
    pub info_portions: Option<SharedInfoPortions>,
}

/// `st_client_spawning` (3.3 step 11.4), after the caption's own work
/// (our placement: the objects' visuals and shells are made by it): the
/// level vertices' light bytes, the game's information portions shared
/// with the story (script API P), and the objects online at the start,
/// each created through the factory and spawned (spec C 6, spec M 3, spec
/// L 5.1); the server time the engine clock `now` (ms).
pub fn client_spawning(
    scripts: &mut GameScripts,
    loading: &mut Loading,
    tree: &dyn Vfs,
    data: &GameData,
    now: u64,
) -> Result<(), Error> {
    let mut characters = Characters::new(tree, data);
    // Script API K: `alife()` is the level's simulation from here on, its
    // binders' calls among the first to reach it.
    loading.share_simulation(Rc::clone(scripts.simulation()));
    let spawning = loading.client_objects().map_err(io_fatal)?;
    {
        let mut o = scripts.objects().borrow_mut();
        o.level_vertex_lights = spawning.vertex_lights;
        o.server_time = now as u32;
        o.game_time = spawning.game_time;
    }
    // Script API H's clocks, before the objects spawn.
    (scripts.times().borrow_mut()).level_frame(spawning.game_time, spawning.time_factor);
    {
        // Script API Y's engine side: the level's physics world and its
        // queries, before the objects spawn. The player is in no vehicle
        // (TODO(#1057): the vehicles' driving), so it keeps no holder.
        let physics = scripts.scripts().physics_state()?;
        let mut p = physics.borrow_mut();
        p.world = Some(Box::new(ScriptWorld(spawning.physics)));
        p.level = Some(Box::new(ScriptLevel {
            collision: spawning.collision,
            paths: spawning.paths,
        }));
        p.current_holder = None;
    }
    scripts.story().borrow_mut().info_portions = spawning.info_portions;
    scripts.scripts().exec("=fake_player", FAKE_PLAYER)?;
    scripts
        .scripts()
        .exec("=fake_buy_supplies", FAKE_BUY_SUPPLIES)?;
    for object in spawning.objects {
        spawn_client_object(scripts, &data.system, &mut characters, object)?;
    }
    Ok(())
}

/// The scripts' part of a level frame, after the level's update: the
/// server and game times and the game clock (script API H); the player's placement, its feet and its body's
/// heading (our reading of M 4); unless the game is `paused`, each
/// object's scheduled update ([`scheduled_updates`]); the objects that
/// went offline or were removed released and destroyed (L 5.5), those
/// that came online spawned (M 2.1); the player's use action on the object
/// it targeted (M 6.3), which raises its `use_object` (spec O 4, id 15),
/// and its talk offer ([`offer_used_talk`]);
/// the events the level's systems raised on its objects (spec O 4: the
/// player's death, the items used up on it, taken and dropped); the
/// level's call list updated, after the physics steps and the objects'
/// updates (O 6.1: the door scheme's fast call); the story's frame
/// ([`story_frame`]); then what the members asked of the engine
/// ([`apply_events`]; `level.set_time_factor`'s factors set on the game
/// clock, H 4.2). No system of the app perceives sounds yet, so none
/// is queued (O 4, id 17).
pub fn level_frame(
    scripts: &mut GameScripts,
    level: &mut LevelScreen,
    tree: &dyn Vfs,
    data: &GameData,
    now: u64,
    paused: bool,
) -> Result<(), Error> {
    {
        let mut o = scripts.objects().borrow_mut();
        o.server_time = now as u32;
        o.game_time = level.clock().time();
        if let Some(player) = o.get_mut(ACTOR_ID) {
            let p = level.player();
            player.placement = model::transform(p.state().position, [0.0, -p.heading(), 0.0]);
        }
    }
    let clock = level.clock();
    (scripts.times().borrow_mut()).level_frame(clock.time(), clock.time_factor());
    // Script API K's brains move offline characters by the game-time
    // chunk's factor.
    scripts.simulation().borrow_mut().normal_time_factor = clock.game_time().normal_time_factor;
    if !paused {
        scheduled_updates(scripts, now as u32)?;
    }
    let changes = level.take_script_changes();
    // The scripts' objects among them (one left out of the scripts' spawning
    // has none).
    let queue: Vec<u16> = {
        let o = scripts.objects().borrow();
        let queued = changes.offline.iter().chain(&changes.removed);
        queued.copied().filter(|&id| o.get(id).is_some()).collect()
    };
    if !queue.is_empty() {
        let released = scripts.scripts().release_destroyed(&queue);
        released.map_err(|f| override_fatal(scripts, f))?;
        let physics = scripts.scripts().physics_state()?;
        for id in &queue {
            physics.borrow_mut().shells.remove(id);
        }
    }
    for &id in &changes.online {
        if let Some(object) = level.client_object(id) {
            let mut characters = Characters::new(tree, data);
            spawn_client_object(scripts, &data.system, &mut characters, object)?;
        }
    }
    if let Some(target) = level.take_used()
        && scripts.objects().borrow().get(target).is_some()
    {
        // The engine's own handling that follows a usable object's (taking
        // hold of it, 6.3) is not made here; the door's use is the level's.
        let used = scripts.scripts().player_use(target, ACTOR_ID);
        used.map_err(|f| override_fatal(scripts, f))?;
        if offer_used_talk(&mut scripts.objects().borrow_mut(), target) {
            let shown = DialogEvent::TalkWindowShown { partner: target };
            level.queue_dialog_events([shown]);
        }
    }
    for (receiver, event) in level.take_object_events() {
        scripts.scripts().raise(receiver, &event)?;
    }
    scripts.scripts().update_level_calls()?;
    story_frame(scripts, level)?;
    apply_events(scripts, level);
    apply_physics_events(scripts, level)?;
    apply_simulation_events(scripts, level);
    for factor in scripts.times().borrow_mut().take_time_factors() {
        level.set_time_factor(factor);
    }
    Ok(())
}

/// What script API K's members asked of the engine: `release` of an
/// online object destroys it, which then leaves the simulation (7.3,
/// believed; at once, our choice: its client object goes with the next
/// frame's changes); an object `create` made in an online parent is already
/// registered online in it, and items in a parent have no client object
/// of their own here, so nothing more is done (7.1).
fn apply_simulation_events(scripts: &GameScripts, level: &mut LevelScreen) {
    let events = scripts.simulation().borrow_mut().take_events();
    for event in events {
        match event {
            SimulationEvent::Destroy { id, .. } => level.release_object(id),
            SimulationEvent::SpawnedOnline { .. } => {}
        }
    }
}

/// The scheduled updates of a level frame at server time `now` (spec L
/// 5.2): every object not marked for destruction (talk.md 5.1 for the
/// characters; our reading for the others), in id
/// order, gets its binder's `update(delta)`, `delta` the time since its
/// previous update ([`update_delta`]). The scheduler's own rules are
/// unestablished (L 5.2): our choice, every object every frame, the
/// maximum scheduling interval taken as 0 (so at most 1000 ms), and none
/// while the game is paused. The engine's own updates are the level's. A
/// script error is fatal.
fn scheduled_updates(scripts: &mut GameScripts, now: u32) -> Result<(), Error> {
    let due: Vec<(u16, u32)> = {
        let mut o = scripts.objects().borrow_mut();
        let ids: Vec<u16> = o.iter().filter(|o| !o.destroying).map(|o| o.id).collect();
        ids.into_iter()
            .filter_map(|id| {
                let object = o.get_mut(id)?;
                let elapsed = now.wrapping_sub(object.updated_at);
                object.updated_at = now;
                Some((id, update_delta(elapsed, 0)))
            })
            .collect()
    };
    for (id, delta) in due {
        // An object an earlier update destroyed gets none.
        let gone = scripts
            .objects()
            .borrow()
            .get(id)
            .is_none_or(|o| o.destroying);
        if gone {
            continue;
        }
        // The engine's own part first: an object under script control runs
        // its action queue (script API V 2.4, believed its scheduled
        // update).
        scripts.scripts().run_action_queue(id)?;
        let updated = scripts.scripts().scheduled_update(id, delta);
        updated.map_err(|f| override_fatal(scripts, f))?;
    }
    Ok(())
}

/// The story's part of a level frame (script API P): the simulation's
/// story registry; what the engine's own gives and the returning messages
/// ask of the scripts (`IP` 3.3, 3.4); the player's task update (5.1:
/// every level frame, our choice where its rate is unestablished); the
/// story's frame (queued news, sequences); then its callbacks raised on
/// the player's object (spec O); and the use tips the scripts set, for the
/// HUD's hint (P 8, use-action.md 2.2).
fn story_frame(scripts: &mut GameScripts, level: &mut LevelScreen) -> Result<(), Error> {
    {
        let mut story = scripts.story().borrow_mut();
        let shared = level.shared_simulation().borrow();
        let registry = shared.simulation.as_ref().map(|s| s.story_registry());
        if story.story_registry.as_ref() != registry {
            story.story_registry = registry.cloned();
        }
    }
    let changed = scripts.story().borrow().portions_changed();
    let s = scripts.scripts();
    if changed {
        s.info_portions_changed()?;
    }
    s.update_player_tasks()?;
    s.story_frame()?;
    // What else the story asks of the engine's other parts, which have no
    // receiver yet: the dialogue window's rebuild mark and the talk
    // window's lines (TODO(#993)); the PDA's changed mark (TODO(#179));
    // news (TODO(#176)); the difficulty applied to the player
    // (TODO(#652)); sequences (TODO(#1062)); `article_info`, whose group
    // and name are the articles' (TODO(#1088)).
    let _unreceived = s.raise_story_callbacks()?;
    level.set_tip_texts(&scripts.story().borrow().tip_texts);
    Ok(())
}

/// The use action's talk with the living on `target` (use-action.md 3
/// steps 6 and 7, 6.2; script API R 4.2): when its usable flag is set, it
/// is a character with health above 0 and the player isn't talking, it is
/// offered the talk ([`offer_talk`]). Whether it accepted: showing the
/// talk window is then the talk's. The search window of the dead (6.1) is
/// not made here.
pub fn offer_used_talk(objects: &mut GameObjects, target: u16) -> bool {
    let Some(t) = objects.get(target) else {
        return false;
    };
    let alive = t.entity.as_ref().is_some_and(|e| e.health > 0.0);
    if !t.usable || t.owner.is_none() || !alive {
        return false;
    }
    let player = objects.get(ACTOR_ID).and_then(|o| o.owner.as_ref());
    if player.is_some_and(|o| o.talk.talking) {
        return false;
    }
    offer_talk(objects, ACTOR_ID, target) == Some(true)
}

/// The dialogs as the talk takes them (`dialog-files` 3.2; script API R
/// 2.1, 3.5): each read by id the first time ([`Dialogs`]), then built
/// once with [`init_dialog`](crate::script::loader::Scripts::init_dialog)
/// (a dialog without a phrase list gets its phrases from its
/// `init_func`), then shared. The built dialogs live as long as their
/// owner keeps them. TODO(#1075): the talk loads its dialogs here.
/// TODO(#907): DG-9 states when generated dialogs are built.
#[derive(Default)]
pub struct ScriptDialogs {
    built: HashMap<String, SharedDialog>,
}

impl ScriptDialogs {
    pub fn new() -> Self {
        Self::default()
    }

    /// The dialog `id`, read from `tree` through `dialogs` and built the
    /// first time. A read failure or a script error is fatal.
    pub fn load(
        &mut self,
        scripts: &mut GameScripts,
        dialogs: &mut Dialogs,
        tree: &dyn Vfs,
        aliases: &PathAliases,
        id: &str,
    ) -> Result<SharedDialog, Error> {
        self.load_with(
            id,
            || dialogs.load(tree, aliases, id).map_err(io_fatal),
            |d| scripts.scripts().init_dialog(d),
        )
    }

    fn load_with(
        &mut self,
        id: &str,
        read: impl FnOnce() -> Result<Arc<Dialog>, Error>,
        build: impl FnOnce(Dialog) -> Result<SharedDialog, Error>,
    ) -> Result<SharedDialog, Error> {
        if let Some(dialog) = self.built.get(id) {
            return Ok(Rc::clone(dialog));
        }
        let dialog = build(Dialog::clone(&*read()?))?;
        self.built.insert(id.to_owned(), Rc::clone(&dialog));
        Ok(dialog)
    }
}

/// The traders' motion channels after the scripts' part of a level frame
/// (docs/behavior/scripted-motion-channels.md): the calls the trader
/// members made since (script API R 6, T 6.6), then each trader's check, its
/// events raised on the trader (spec O 4, ids 30 to 32) and the calls
/// their functions made made at once (4.2). A script error is fatal.
pub fn motion_channels_frame(
    scripts: &mut GameScripts,
    level: &mut LevelScreen,
    tree: &dyn Vfs,
    data: &GameData,
    sounds: &mut dyn ChannelSounds,
) -> Result<(), Error> {
    let events = scripts.dialogs().borrow_mut().take_trader_events(None);
    for (trader, call) in events.into_iter().filter_map(channel_call) {
        level.motion_channel_call(tree, data, trader, &call, sounds);
    }
    let mut raise = |trader: u16, event: ChannelEvent| {
        let event = match event {
            ChannelEvent::GlobalAnimRequest => ObjectEvent::TraderGlobalAnimRequest,
            ChannelEvent::HeadAnimRequest => ObjectEvent::TraderHeadAnimRequest,
            ChannelEvent::SoundEnd => ObjectEvent::TraderSoundEnd,
        };
        let raised = scripts.scripts().raise(trader, &event);
        raised?;
        let events = scripts
            .dialogs()
            .borrow_mut()
            .take_trader_events(Some(trader));
        Ok(events
            .into_iter()
            .filter_map(channel_call)
            .map(|(_, c)| c)
            .collect())
    };
    level.motion_channels_check(tree, data, sounds, &mut raise)
}

/// The call of a trader member's event (script API R 6, T 6.6), with its
/// trader.
fn channel_call(event: DialogEvent) -> Option<(u16, ChannelCall)> {
    match event {
        DialogEvent::TraderMotion {
            trader,
            motion,
            name,
        } => Some((
            trader,
            match motion {
                TraderMotion::Body => ChannelCall::SetGlobalAnim(name),
                TraderMotion::Head => ChannelCall::SetHeadAnim(name),
            },
        )),
        DialogEvent::TraderSound {
            trader,
            sound,
            head_motion,
        } => Some((
            trader,
            ChannelCall::SetSound {
                sound,
                head_anim: head_motion,
            },
        )),
        // Script API T 6.6.
        DialogEvent::ExternalSoundStart { trader, sound } => {
            Some((trader, ChannelCall::ExternalSoundStart(sound)))
        }
        DialogEvent::ExternalSoundStop { trader } => Some((trader, ChannelCall::ExternalSoundStop)),
        _ => None,
    }
}

/// Spawns `object` in the scripts (spec C 6.1, 6.2; spec M 3, 10.2; spec
/// L 5.1): its client object created through the factory for its
/// section's class id; a script entry's object spawns through its
/// `net_Spawn`, an engine entry's as the engine's own kind (the player's
/// for `O_ACTOR` in the single-player game type). Its visual, physics part
/// and restrictor shape, and a character's start dialog (CH-7 6.1), are
/// given as it is made ([`engine_state`]). A spawning that fails is the
/// scripts' to report (L 5.1); a script error is fatal.
pub fn spawn_client_object(
    scripts: &mut GameScripts,
    system: &Config,
    characters: &mut Characters,
    object: ClientObject,
) -> Result<(), Error> {
    if !binding_runs(system, &object) {
        return Ok(());
    }
    let mut data = SpawnData::from_record(object.id, &object.record, system).map_err(io_fatal)?;
    // Script API J: the simulation object it spawns from, `net_Spawn`'s
    // and the binder's `data`.
    let server = scripts.scripts().server_object_for(system, &object.record);
    data.server = Some(server.map_err(|f| override_fatal(scripts, f))?);
    let state = engine_state(characters, &object).map_err(io_fatal)?;
    scripts.objects().borrow_mut().prepare(object.id, state);
    // The character's voice prefix (script API T 6.5).
    let voice = characters.voice_prefix(&object.record).map_err(io_fatal)?;
    {
        let mut sounds = scripts.sounds().borrow_mut();
        let creatures = &mut sounds.creatures;
        creatures.forget(object.id);
        if !voice.is_empty() {
            let voice = cp1251::encode(&voice).unwrap_or_else(|| voice.into_bytes());
            creatures.voice_prefixes.insert(object.id, voice);
        }
    }
    // Its shell as script API Y sees it, before its binder asks for it.
    if let Some(shell) = object.shell {
        let physics = scripts.scripts().physics_state()?;
        (physics.borrow_mut().shells).insert(object.id, PhysicsShell::clone(&shell));
    }
    let created = scripts.scripts().create_client_object(data.class_id)?;
    let spawned = match created.object {
        CreatedObject::Script(client) => scripts.scripts().spawn(client, data).map(|_| ()),
        CreatedObject::Engine { entry_id, sides } => {
            let game_type = scripts.services().borrow().game_type as u32;
            let class = match sides {
                Sides::ByGameType if GameTypeKind::of(game_type) == GameTypeKind::ZeroOrOne => {
                    ObjectClass::Player
                }
                _ => ObjectClass::Unnamed(entry_id),
            };
            // TODO(#1120): the player bound by the fake of its binding.
            let binding = (class == ObjectClass::Player).then_some(FAKE_PLAYER_BINDING);
            (scripts.scripts())
                .spawn_engine_object_with_binding(class, data, binding)
                .map(|_| ())
        }
    };
    spawned.map_err(|f| override_fatal(scripts, f))
}

/// What the engine gives `object` as it spawns in the scripts (spec M 3):
/// its visual, physics part and restrictor shape, and its character
/// information's start dialog (docs/behavior/character-talk-state.md 6.2),
/// which a stalker or trader takes as its start dialog and default start
/// dialog (6.1).
pub fn engine_state(characters: &mut Characters, object: &ClientObject) -> io::Result<EngineState> {
    let start_dialog = characters.start_dialog(&object.record)?;
    Ok(EngineState {
        visual: object.model.map(|(m, names)| visual_of(m, names)),
        physics: object.physics,
        restrictor: restrictor_of(&object.record),
        start_dialog: cp1251::encode(&start_dialog).unwrap_or_else(|| start_dialog.into_bytes()),
    })
}

/// Whether `object` spawns in the scripts: one without a binding function
/// (spec L 3.1; an empty value finds none), a door under the door scheme,
/// whose binder (`bind_physic_object.init`, L 8) runs on the script
/// interface implemented so far, the trader, whose binder
/// (`bind_monster.bind`) runs his scheme `mob_trader`, or the player, under
/// the fake of its binding (TODO(#1120)). TODO(#1077): every object, once
/// the interface the other binders call is implemented.
fn binding_runs(system: &Config, object: &ClientObject) -> bool {
    let section = object.record.header.section.as_str();
    match system.get(section, "script_binding") {
        None | Some(None) => true,
        Some(Some(name)) => {
            name.trim().is_empty()
                || object.door
                || object.id == ACTOR_ID
                || object.record.kind() == Kind::Trader
        }
    }
}

/// Applies the events the scripts' members raised (spec M): the player's
/// forced placement and camera angles (4), hits on the player (5.6,
/// player-hits.md) and the visibility switch (6.5). A death event's
/// handling (5.2) is the death's (#248); a stalker's wounded flag has no
/// behaviour to update, and an explosion is spec Y's
/// ([`apply_physics_events`]). The dialogs and trade's events (script API
/// R) are queued for the talk ([`LevelScreen::take_dialog_events`]), but
/// the trader members', which are [`motion_channels_frame`]'s.
pub fn apply_events(scripts: &mut GameScripts, level: &mut LevelScreen) {
    // The trader members' events are the motion channels'
    // ([`motion_channels_frame`]).
    level.queue_dialog_events(scripts.dialogs().borrow_mut().take_other_events());
    let events = scripts.objects().borrow_mut().take_events();
    for event in events {
        match event {
            Event::ForcePlacement { object, placement } if object == ACTOR_ID => {
                let [x, y, z, _] = placement[3];
                level.force_player_position([x, y, z]);
            }
            Event::CameraAngles { object, angles } if object == ACTOR_ID => {
                level.set_camera_angles(angles);
            }
            Event::Hit(hit) if hit.target == ACTOR_ID => {
                // An unknown hit type, and a bone the player's model lacks,
                // are the hit handling's fatal errors (player-hits.md 2, 4).
                let kind = u32::try_from(hit.hit_type)
                    .map_err(|_| io::Error::other("hit type"))
                    .and_then(HitType::new);
                match kind {
                    Ok(kind) => {
                        let _ = level.hit(Hit {
                            kind,
                            power: hit.power,
                            bone: Some(hit.bone),
                            armour_piercing: hit.armour_piercing.unwrap_or(0.0),
                        });
                    }
                    Err(_) => level
                        .record_fatal(format!("Unknown/unregistered hit type {}", hit.hit_type)),
                }
            }
            Event::Visibility { object, hidden } => level.set_object_hidden(object, hidden),
            // What the player does with it is unestablished, and it holds
            // no weapon in hand (TODO(#1134)).
            Event::HideWeapon { .. } => {}
            _ => {}
        }
    }
}

/// Applies what script API Y's members asked of the engine
/// ([`PhysicsState::take_events`](crate::script::game_object::physics::PhysicsState::take_events)),
/// in order: a lamp's switching (9, hanging-lamps.md 5.1). What the
/// explode event does is unestablished (2.1, TODO(#1084)); the car's and
/// helicopter's effects are the vehicles' (TODO(#1057)).
pub fn apply_physics_events(
    scripts: &mut GameScripts,
    level: &mut LevelScreen,
) -> Result<(), Error> {
    let events = scripts
        .scripts()
        .physics_state()?
        .borrow_mut()
        .take_events();
    for event in events {
        if let PhysicsEvent::LampSwitched { object, on } = event {
            level.switch_lamp(object, on);
        }
    }
    Ok(())
}

/// The level's queries script API Y's members make, over its collision
/// mesh (believed the level's static geometry) and the game's patrol paths
/// (spec N).
struct ScriptLevel {
    collision: Arc<CollisionWorld>,
    paths: PatrolPaths,
}

/// The explosive's ray down (2.1, believed below it): its length is
/// unestablished, ours that of `GetRealAltitude` (8.2).
const EXPLOSION_RAY: f32 = 1000.0;

const DOWN: Vec3 = [0.0, -1.0, 0.0];

impl ScriptLevel {
    /// The nearest triangle the segment from `from` along the unit `dir`
    /// for `length` meets, at its distance.
    fn nearest(&self, from: Vec3, dir: Vec3, length: f32) -> Option<(f32, u32)> {
        let c = &self.collision;
        c.ray_nearest(from, dir, length, |t| {
            geometry::ray_triangle_distance(from, dir, &c.corners(t))
                .filter(|d| (0.0..=length).contains(d))
        })
    }
}

impl LevelQueries for ScriptLevel {
    /// The normal of the level's triangle below `position`, turned to face
    /// it (our choice).
    fn explosion_normal(&self, position: Vec3) -> Option<Vec3> {
        let (_, t) = self.nearest(position, DOWN, EXPLOSION_RAY)?;
        let [a, b, c] = self.collision.corners(t);
        let n = geometry::normalize(geometry::cross(geometry::sub(b, a), geometry::sub(c, a)))?;
        Some(if geometry::dot(n, DOWN) > 0.0 {
            n.map(|v| -v)
        } else {
            n
        })
    }

    fn ray_blocked(&self, from: Vec3, to: Vec3) -> bool {
        let d = geometry::sub(to, from);
        let length = geometry::dot(d, d).sqrt();
        match geometry::normalize(d) {
            Some(dir) => self.collision.ray_hits(from, dir, length),
            None => false,
        }
    }

    fn distance_below(&self, from: Vec3, range: f32) -> Option<f32> {
        self.nearest(from, DOWN, range).map(|(d, _)| d)
    }

    fn level_box_top(&self) -> f32 {
        self.collision.bbox_max[1]
    }

    fn patrol_point(&self, path: &[u8], point: u32) -> Option<Vec3> {
        let path = self.paths.get(&cp1251::decode(path))?;
        path.points.get(point as usize).map(|p| p.position)
    }
}

/// The fake of the player's binding (TODO(#1120)): `bind_stalker.actor_init`
/// can't run yet (#1077), so the player's object is bound by
/// [`FAKE_PLAYER_BINDING`] instead, whose binder only registers the player
/// with the shipped `db.add_actor` as it spawns and `db.del_actor` as it is
/// destroyed, as the shipped binder does: so `db.actor` exists, which the
/// physics-object binder waits for to start an object's logic (script API L
/// 8; the door scheme).
const FAKE_PLAYER: &[u8] = br#"
class "__volya_fake_player_binder" (object_binder)
function __volya_fake_player_binder:__init(obj) super(obj) end
function __volya_fake_player_binder:net_spawn(data)
  if not object_binder.net_spawn(self, data) then return false end
  db.add_actor(self.object)
  return true
end
function __volya_fake_player_binder:net_destroy()
  db.del_actor(self.object)
  object_binder.net_destroy(self)
end
function __volya_fake_player_binding(obj)
  obj:bind_object(__volya_fake_player_binder(obj))
end
"#;

/// FAKE(#1148): script API S's `buy_supplies(ini, section)` (5.6), which
/// the shipped `trade_manager.update` calls on the trader from his binder's
/// update, restocks nothing: the owner's items and the server's creation of
/// items aren't implemented (#1110).
const FAKE_BUY_SUPPLIES: &[u8] = br#"
function game_object.buy_supplies(self, ini, section) end
"#;

/// The binding function of [`FAKE_PLAYER`] (TODO(#1120)).
const FAKE_PLAYER_BINDING: &[u8] = b"__volya_fake_player_binding";

/// The visual of `model` as the place members read it (M 4): its header
/// sphere's centre and its bones at their bind matrices, the root first.
/// It has no `IRender_Visual` for `CGameObject:Visual()`: no script class
/// the shipped game instantiates reads it (M 10.2).
fn visual_of(model: &LoadedModel, names: &Names) -> Visual {
    Visual {
        part: None,
        centre: model.header.sphere_centre,
        bones: (0..model.bone_count())
            .map(|i| {
                let name = names.get(model.bone_name(i));
                let bytes = cp1251::encode(name).unwrap_or_else(|| name.as_bytes().to_vec());
                (bytes, model.bind(i))
            })
            .collect(),
        root: 0,
    }
}

/// The shapes of a record that has a Shape block (sav.md 7.4.2).
fn shapes_of(record: &ObjectRecord) -> Option<&[Shape]> {
    Some(match &record.object {
        Object::Zone(o) => &o.shapes,
        Object::Anomaly(o) => &o.anomaly.zone.shapes,
        Object::VisualAnomaly(o) => &o.anomaly.zone.shapes,
        Object::TorridZone(o) => &o.zone.shapes,
        Object::LevelChanger(o) => &o.shapes,
        Object::Restrictor(o) => &o.shapes,
        Object::SmartZone(o) => &o.restrictor.shapes,
        Object::TeamBase(o) => &o.restrictor.shapes,
        Object::Climbable(o) => &o.shapes,
        _ => return None,
    })
}

/// A restrictor's shape in level space (M 6.2, whose derivation is the
/// engine's: our reading): each of the record's shapes placed by its
/// placement, a box as its 6 face planes (its axes span it about its
/// centre, as the zones read it), the bounding sphere the zones' shape
/// sphere placed.
fn restrictor_of(record: &ObjectRecord) -> Option<Restrictor> {
    let h = &record.header;
    Some(placed(
        shapes_of(record)?,
        &model::transform(h.position, h.angles),
    ))
}

/// `shapes` placed by `x` as [`restrictor_of`] places them.
fn placed(shapes: &[Shape], x: &model::Mat4) -> Restrictor {
    let point = |p: [f32; 3]| model::transform_point(x, p);
    let direction = |d: [f32; 3]| -> [f32; 3] {
        std::array::from_fn(|c| d[0] * x[0][c] + d[1] * x[1][c] + d[2] * x[2][c])
    };
    let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let (centre, radius) = zones::shape_sphere(shapes);
    let mut r = Restrictor {
        bound: (point(centre), radius),
        ..Default::default()
    };
    for shape in shapes {
        match *shape {
            Shape::Sphere { centre, radius } => r.spheres.push((point(centre), radius)),
            Shape::Box([a, b, c, o]) => {
                let o = point(o);
                let mut planes = [([0.0; 3], 0.0); 6];
                for (k, axis) in [a, b, c].into_iter().enumerate() {
                    let u = direction(axis);
                    let l = dot(u, u).sqrt();
                    let n = if l > 0.0 { u.map(|v| v / l) } else { [0.0; 3] };
                    let m = dot(n, o);
                    planes[2 * k] = (n, -m - l * 0.5);
                    planes[2 * k + 1] = (n.map(|v| -v), m - l * 0.5);
                }
                r.boxes.push(planes);
            }
        }
    }
    r
}

/// A load's error, fatal as the original's.
fn io_fatal(e: io::Error) -> Error {
    Error {
        kind: ErrorKind::Fatal,
        message: e.to_string(),
    }
}

/// An override's failure (spec A 5.3): its log line written, then the
/// game's end.
fn override_fatal(scripts: &GameScripts, failure: OverrideFailure) -> Error {
    if let Some(line) = &failure.log {
        scripts
            .services()
            .borrow_mut()
            .console
            .log_mut()
            .message(line);
    }
    failure.error
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::script::game_object::GameObject;

    /// Whether the sphere (`p`, `radius`) is inside `r` (M 6.2).
    fn inside(r: &Restrictor, p: [f32; 3], radius: f32) -> bool {
        let d =
            |a: [f32; 3], b: [f32; 3]| (0..3).map(|k| (a[k] - b[k]).powi(2)).sum::<f32>().sqrt();
        if d(p, r.bound.0) >= r.bound.1 + radius {
            return false;
        }
        r.spheres.iter().any(|&(c, s)| d(p, c) < s + radius)
            || (r.boxes.iter()).any(|b| {
                b.iter()
                    .all(|(n, o)| n[0] * p[0] + n[1] * p[1] + n[2] * p[2] + o <= radius)
            })
    }

    #[test]
    fn the_level_queries_read_the_collision_mesh_and_patrol_paths() {
        use crate::core::collision::{Cform, CformTriangle};
        use volya_engine_formats::gamemtl::GameMaterial;
        use volya_engine_formats::patrol_path::{PathPoint, PatrolPath};

        // A floor at y = 0 of two triangles, one of them wound downward,
        // and a wall at x = 5.
        let cform = Cform {
            bbox_min: [-10.0, -1.0, -10.0],
            bbox_max: [10.0, 7.0, 10.0],
            vertices: vec![
                [-10.0, 0.0, -10.0],
                [-10.0, 0.0, 10.0],
                [10.0, 0.0, 10.0],
                [10.0, 0.0, -10.0],
                [5.0, 0.0, -10.0],
                [5.0, 0.0, 10.0],
                [5.0, 5.0, 0.0],
            ],
            triangles: [[0, 1, 2], [0, 3, 2], [4, 5, 6]]
                .map(|vertices| CformTriangle {
                    vertices,
                    attributes: 0,
                })
                .to_vec(),
        };
        let collision = CollisionWorld::new(
            cform,
            vec![GameMaterial {
                id: 0,
                ..GameMaterial::default()
            }],
        )
        .unwrap();
        let point = |name: &str, position| PathPoint {
            name: name.into(),
            position,
            flags: 0,
            level_vertex: 0,
            game_vertex: 0,
        };
        let paths = PatrolPaths::from_paths(vec![PatrolPath {
            name: "heli_path".into(),
            points: vec![point("p0", [1.0, 30.0, 2.0]), point("p1", [3.0, 30.0, 4.0])],
            links: Vec::new(),
        }]);
        let level = ScriptLevel {
            collision: Arc::new(collision),
            paths,
        };
        let near = |a: Option<f32>, b: f32| a.is_some_and(|a| (a - b).abs() < 1e-4);
        assert!(near(level.distance_below([-5.0, 3.0, 5.0], 10.0), 3.0));
        assert!(near(level.distance_below([0.0, 3.0, -5.0], 10.0), 3.0));
        assert_eq!(level.distance_below([0.0, 3.0, 0.0], 2.0), None);
        assert_eq!(level.distance_below([0.0, 3.0, 50.0], 10.0), None);
        // Both floor triangles' normals face up, toward the ray's origin.
        for at in [[-5.0, 3.0, 5.0], [0.0, 3.0, -5.0]] {
            let n = level.explosion_normal(at).unwrap();
            assert!((n[1] - 1.0).abs() < 1e-6, "{n:?}");
        }
        assert_eq!(level.explosion_normal([0.0, 3.0, 50.0]), None);
        assert!(level.ray_blocked([0.0, 1.0, 0.0], [8.0, 1.0, 0.0]));
        assert!(!level.ray_blocked([0.0, 1.0, 0.0], [4.0, 1.0, 0.0]));
        assert!(!level.ray_blocked([0.0, 1.0, 0.0], [0.0, 1.0, 0.0]));
        assert_eq!(level.level_box_top(), 7.0);
        assert_eq!(level.patrol_point(b"heli_path", 1), Some([3.0, 30.0, 4.0]));
        assert_eq!(level.patrol_point(b"heli_path", 2), None);
        assert_eq!(level.patrol_point(b"other", 0), None);
    }

    #[test]
    fn a_restrictors_shapes_are_placed_in_level_space() {
        // A 2 × 4 × 6 box about (1, 0, 0) and a sphere of radius 1 at
        // (0, 0, 5), turned a quarter about y and moved to (10, 0, 0).
        let shapes = [
            Shape::Box([
                [2.0, 0.0, 0.0],
                [0.0, 4.0, 0.0],
                [0.0, 0.0, 6.0],
                [1.0, 0.0, 0.0],
            ]),
            Shape::Sphere {
                centre: [0.0, 0.0, 5.0],
                radius: 1.0,
            },
        ];
        let x = model::transform([10.0, 0.0, 0.0], [0.0, std::f32::consts::FRAC_PI_2, 0.0]);
        let r = placed(&shapes, &x);
        let box_centre = model::transform_point(&x, [1.0, 0.0, 0.0]);
        let sphere_centre = model::transform_point(&x, [0.0, 0.0, 5.0]);
        assert_eq!(r.boxes.len(), 1);
        assert_eq!(r.spheres, [(sphere_centre, 1.0)]);
        assert!(inside(&r, box_centre, 0.001));
        assert!(inside(&r, sphere_centre, 0.001));
        // The box's x half-extent, 1 m, now lies along the turned axis.
        let along = |t: f32| model::transform_point(&x, [1.0 + t, 0.0, 0.0]);
        assert!(inside(&r, along(0.9), 0.001));
        assert!(!inside(&r, along(1.2), 0.001));
        assert!(inside(&r, along(1.2), 0.3));
        assert!(!inside(&r, [10.0, 50.0, 0.0], 0.001));
    }

    fn object(id: u16, class: ObjectClass) -> GameObject {
        let data = SpawnData {
            id,
            name: Vec::new(),
            section: b"section".to_vec(),
            class_id: 0,
            story_id: -1,
            custom_data: Vec::new(),
            parent: None,
            placement: model::transform([0.0; 3], [0.0; 3]),
            level_vertex: 0,
            game_vertex: 0,
            health: None,
            server: None,
        };
        GameObject::new(class, &data, 0)
    }

    const TRADER: u16 = 7;

    fn talkers(enabled: bool) -> GameObjects {
        let mut objects = GameObjects::new();
        objects.insert(object(ACTOR_ID, ObjectClass::Player));
        let mut trader = object(TRADER, ObjectClass::Named("CAI_Trader"));
        trader.owner.as_mut().unwrap().talk.talk_enabled = enabled;
        objects.insert(trader);
        objects.insert(object(8, ObjectClass::Named("CAI_Dog")));
        objects
    }

    fn talking(objects: &GameObjects, id: u16) -> (bool, Option<u16>) {
        let talk = &objects.get(id).unwrap().owner.as_ref().unwrap().talk;
        (talk.talking, talk.partner)
    }

    #[test]
    fn the_use_action_offers_a_talk_to_the_living() {
        let mut objects = talkers(true);
        assert!(offer_used_talk(&mut objects, TRADER));
        assert_eq!(talking(&objects, TRADER), (true, Some(ACTOR_ID)));
        assert_eq!(talking(&objects, ACTOR_ID), (true, Some(TRADER)));
        // While the player talks, nothing happens.
        assert!(!offer_used_talk(&mut objects, TRADER));
        // A disabled talk, a monster, a missing object.
        let mut objects = talkers(false);
        assert!(!offer_used_talk(&mut objects, TRADER));
        assert!(!offer_used_talk(&mut objects, 8));
        assert!(!offer_used_talk(&mut objects, 9));
        assert_eq!(talking(&objects, ACTOR_ID), (false, None));
    }

    #[test]
    fn the_use_action_offers_no_talk_to_the_dead_or_unusable() {
        let mut objects = talkers(true);
        objects.get_mut(TRADER).unwrap().usable = false;
        assert!(!offer_used_talk(&mut objects, TRADER));
        let mut objects = talkers(true);
        let entity = objects.get_mut(TRADER).and_then(|o| o.entity.as_mut());
        entity.unwrap().health = 0.0;
        assert!(!offer_used_talk(&mut objects, TRADER));
        assert_eq!(talking(&objects, TRADER), (false, None));
    }

    #[test]
    fn a_dialog_is_read_and_built_once() {
        let dialog = Dialog {
            id: "generated".into(),
            priority: 0,
            caption: None,
            lists: Default::default(),
            body: volya_engine_formats::dialog_files::Body::InitFunc("m.init".into()),
        };
        let mut dialogs = ScriptDialogs::new();
        let (reads, builds) = (RefCell::new(0), RefCell::new(0));
        let load = |dialogs: &mut ScriptDialogs| {
            dialogs.load_with(
                "generated",
                || {
                    *reads.borrow_mut() += 1;
                    Ok(Arc::new(dialog.clone()))
                },
                |d| {
                    *builds.borrow_mut() += 1;
                    Ok(Rc::new(RefCell::new(d)))
                },
            )
        };
        let first = load(&mut dialogs).unwrap();
        let again = load(&mut dialogs).unwrap();
        assert!(Rc::ptr_eq(&first, &again));
        assert_eq!((*reads.borrow(), *builds.borrow()), (1, 1));
        assert_eq!(first.borrow().id, "generated");
    }
}
