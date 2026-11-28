// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The game's scripts from program start to a New Game's first frame
//! (docs/behavior/script-api-engine-entry-points.md section 3; section
//! numbers are its): startup at program start, then the engine's calls
//! into scripts placed in the New Game's loading captions
//! (docs/behavior/loading-screen.md section 5) whose work they belong to.
//! See docs/architecture/scripts.md, Entry points.

use std::cell::RefCell;
use std::io;
use std::rc::Rc;

use crate::core::config;
use crate::data::GameData;
use crate::loading_screen::{GAME_TYPE, Stage};
use crate::script::Error;
use crate::script::entry_points::{GameScripts, Startup};
use crate::script::files::FileSystem;
use crate::script::game_object::GameObjects;
use crate::script::loader::{Prefix, Trees};
use crate::script::services::EngineServices;

/// The server options of a New Game from the menu, `start
/// server(all/single/alife/new)` (level-loading.md section 2.1).
pub const SERVER_OPTIONS: &str = "all/single/alife/new";

/// The precache phase the client's synchronising asks for (3.3 step
/// 11.6; loading-screen.md's 30 frames).
pub const PRECACHE_FRAMES: u32 = 30;

/// Program start (3.1 step 2): the engine state the script interface
/// reads, and startup on the first game state, over the game's trees.
pub fn start(data: &GameData, trees: Rc<dyn Trees>) -> io::Result<GameScripts> {
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
        // TODO(#1030): the app's objects as scripts see them.
        objects: Rc::new(RefCell::new(GameObjects::new())),
    })
    .map_err(fatal)
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
        // TODO(#1027): under `st_client_spawning` (11.4), the level's client
        // objects through the factory and their binding functions (spec L).
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

/// The New Game's load ended, with its level or not (3.3 step 12): the
/// loading phase closed.
pub fn load_ended(scripts: &GameScripts) {
    scripts.services().borrow_mut().close_loading_phase();
}
