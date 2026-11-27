// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The console's actions that are the game's (console-scope.md section 6,
//! the specifications its column *Spec* names): whoever executes a line (a
//! bound key, a typed line) leaves its action with the console
//! ([`Exec::Action`](crate::core::console::Exec::Action)), and the update
//! runs them in order once the frame's phase has run
//! ([`App::run_console_actions`]).

use crate::audio::AudioOut;
use crate::core::console::{ActionLine, DIFFICULTIES};
use crate::core::diagnostics;
use crate::core::input::Input;
use crate::title_menu::TitleMenu;

use super::{App, NewGame, Phase, display_mode_request, supersample, video_mode};

/// The multiplayer actions (console-scope.md section 6, specs 9b and 9c).
const MULTIPLAYER: &[&str] = &[
    "beclient",
    "beserver",
    "chat",
    "cl_voteno",
    "cl_votestart",
    "cl_voteyes",
    "g_kill",
    "g_restart",
    "g_restart_fast",
    "g_swapteams",
    "get_server_address",
    "net_cl_clearstats",
    "net_cl_resync",
    "net_compressor_status",
    "net_sv_clearstats",
    "ra",
    "sv_addmap",
    "sv_banplayer",
    "sv_banplayer_ip",
    "sv_changegametype",
    "sv_changelevel",
    "sv_changelevelgametype",
    "sv_dump_online_statistics",
    "sv_listmaps",
    "sv_listplayers",
    "sv_listplayers_banned",
    "sv_kick",
    "sv_nextanomalyset",
    "sv_nextmap",
    "sv_prevmap",
    "sv_return_to_base",
    "sv_setenvtime",
    "sv_setweather",
    "sv_startteammoney",
    "sv_statistic_save",
    "sv_status",
    "sv_unbanplayer_ip",
    "sv_votestop",
];

/// What `start`'s argument asks for (steering-the-original.md 3.2).
#[derive(Debug, PartialEq, Eq)]
enum Start {
    /// `all/single/alife/new`: the New Game.
    NewGame,
    /// `<save>/single/alife/load`.
    Load,
    /// No client options and no `single`: the line is logged.
    NoClient,
    /// Any other form (TODO(#657): unspecified; #658: multiplayer).
    Other,
}

/// The text between `<word>(` and the next `)`, empty without one.
fn options<'a>(argument: &'a str, word: &str) -> &'a str {
    let Some(at) = argument.find(&format!("{word}(")) else {
        return "";
    };
    let rest = &argument[at + word.len() + 1..];
    rest.find(')').map_or(rest, |end| &rest[..end])
}

/// Steering-the-original.md 3.2: the server's options are `/`-separated
/// fields; without client options a single-player server's become
/// `localhost`.
fn start(argument: &str) -> Start {
    let server = options(argument, "server");
    let client = options(argument, "client");
    let fields: Vec<&str> = server.split('/').collect();
    let single = fields.contains(&"single");
    if client.is_empty() && !single {
        return Start::NoClient;
    }
    if single && fields.contains(&"alife") {
        if fields.contains(&"new") && fields[0] == "all" {
            return Start::NewGame;
        }
        if fields.contains(&"load") {
            return Start::Load;
        }
    }
    Start::Other
}

impl App {
    /// Runs the actions the console's lines left this frame
    /// ([`crate::core::console::Console::take_actions`]), in order. Lines
    /// typed in the console window, once there is one (#631), reach them
    /// the same way: entered with
    /// [`Console::enter`](crate::core::console::Console::enter), their
    /// actions run here.
    pub(super) fn run_console_actions(&mut self, input: &Input, audio: &mut dyn AudioOut) {
        for line in self.console.take_actions() {
            // A fatal error or a quit ends what the frame still had to run.
            if self.failed.is_some() || self.quit {
                return;
            }
            self.run_action(&line, input, audio);
        }
    }

    fn run_action(&mut self, line: &ActionLine, input: &Input, audio: &mut dyn AudioOut) {
        let argument = line.argument.as_str();
        match line.name {
            // Steering-the-original.md section 7: the game is left, then
            // the program ends, as the menu's quit does: a normal exit
            // (console-log.md 3.3, console-diagnostics.md 2.5,
            // options-screens.md 12.3).
            "quit" => {
                self.console.device_destroyed();
                self.console.log_mut().save();
                self.save = Some(self.console.save());
                self.quit = true;
            }
            // Section 7: only the game is left.
            "disconnect" => self.disconnect(input, audio),
            // Save-load-screens.md section 2: alone it toggles, `on` or
            // `1` opens, `off` or `0` closes, anything else opens.
            "main_menu" => {
                let open = match argument {
                    "" => None,
                    "off" | "0" => Some(false),
                    _ => Some(true),
                };
                self.main_menu(open, input, audio);
            }
            "start" => match start(argument) {
                Start::NewGame => self.start_new_game(input, audio),
                Start::NoClient => self.console.log_mut().message(&format!(
                    "! Can't start game without client. Arguments: '{argument}'."
                )),
                // TODO(#75): loading a save.
                Start::Load => {}
                // TODO(#657, #658): the other forms.
                Start::Other => {}
            },
            // Options-screens.md 12.3: no name, the user configuration file.
            // TODO(#655): the other names.
            "cfg_save" if config_name(argument) == "user.ltx" => {
                self.save = Some(self.console.save());
            }
            // TODO(#655): a named configuration file.
            "cfg_save" | "cfg_load" => {}
            "vid_restart" => self.video_restart(),
            // Sound-positional.md section 2: the next sound frame restarts
            // the sound system with `snd_targets`.
            "snd_restart" => self.sound_voices_set = false,
            // TODO(#75): saving and loading games.
            "save" | "load" | "load_last_save" => {}
            // TODO(#660).
            "screenshot" => {}
            // TODO(#659).
            "demo_play" | "demo_record" | "demo_record_take_point" | "demo_record_clear_points" => {
            }
            // Console-misc-settings.md 8: the original's patch service is
            // not reachable from the web build, so the query is never sent.
            "check_for_updates" => self
                .console
                .log_mut()
                .message("! Unable to send query for patch!"),
            // TODO(#658).
            name if MULTIPLAYER.contains(&name) => {}
            name => tracing::warn!(name, "console action without a handler"),
        }
    }

    /// `disconnect` (steering-the-original.md 7): the game ends; with no
    /// level the title screen's menu opens. Without a game it does nothing
    /// (our reading).
    fn disconnect(&mut self, input: &Input, audio: &mut dyn AudioOut) {
        let title = self.opening(false);
        let Some(Phase::Level {
            menu,
            intro,
            overlay,
            ..
        }) = &mut self.phase
        else {
            return;
        };
        diagnostics::end("quit", "quit to the title menu");
        if let Some(m) = menu {
            m.stop(audio);
        }
        intro.stop(audio);
        if let Some(o) = overlay.take() {
            o.end(audio);
        }
        self.phase = Some(Phase::Menu(TitleMenu::open(
            &self.data, self.clock, input, audio, title,
        )));
        self.screens.menu_opened();
    }

    /// `main_menu` (title-menu.md 2, save-load-screens.md 2): `open`, or
    /// with `None` the other way round. Over a level the in-game menu opens
    /// or closes; asked to open under the loading image or the intro, it
    /// opens once the controls are live, as the quit key's (#96).
    fn main_menu(&mut self, open: Option<bool>, input: &Input, audio: &mut dyn AudioOut) {
        let in_game = self.opening(true);
        match &mut self.phase {
            Some(Phase::Level {
                menu,
                overlay,
                intro,
                ..
            }) => match (menu.is_some(), open.unwrap_or(menu.is_none())) {
                (false, true) if overlay.is_some() || intro.player.is_some() => {
                    self.menu_asked = true;
                }
                (false, true) => {
                    *menu = Some(Box::new(TitleMenu::open(
                        &self.data, self.clock, input, audio, in_game,
                    )));
                    self.screens.menu_opened();
                    self.unlock_opened = None;
                }
                // `main_menu off`: the menu screen is discarded, which stops
                // its music; the pause is released.
                (true, false) => {
                    if let Some(m) = menu {
                        m.stop(audio);
                    }
                    *menu = None;
                }
                _ => {}
            },
            // TODO(#656): closing the title menu with no level.
            Some(Phase::Menu(_)) => {}
            // No menu opens over the logos or the loading (our reading:
            // no line runs there).
            _ => {}
        }
    }

    /// `start server(all/single/alife/new) client(localhost)`
    /// (steering-the-original.md 3.2, level-loading.md 2.1): the New Game
    /// at the difficulty `g_game_difficulty` holds, its menu closed. A game
    /// running is left first (TODO(#657): unspecified; the menu's New Game
    /// runs `disconnect` first).
    fn start_new_game(&mut self, input: &Input, audio: &mut dyn AudioOut) {
        self.disconnect(input, audio);
        let status = self.console.status("g_game_difficulty");
        let Some(&(difficulty, _)) = DIFFICULTIES
            .iter()
            .find(|(name, _)| Some(*name) == status.as_deref())
        else {
            return;
        };
        let Some(Phase::Menu(menu)) = &mut self.phase else {
            return;
        };
        menu.stop(audio);
        let Some(Phase::Menu(menu)) = self.phase.take() else {
            unreachable!()
        };
        self.phase = Some(Phase::Loading {
            difficulty,
            stages: NewGame::Closing(Box::new(menu)),
        });
        // The device is reset as the menu goes (texture-quality-filtering.md
        // 8.1).
        self.samplers.reset(&self.console);
        self.loads += 1;
    }

    /// `vid_restart` (options-screens.md 5.4, display-mode.md A.9): the
    /// video mode, supersampling and full screen applied, the device reset
    /// (texture-quality-filtering.md 7.3).
    fn video_restart(&mut self) {
        self.video = video_mode(&self.console);
        self.supersample = supersample(&self.console);
        self.full_screen_request = display_mode_request(&self.console, self.page_full_screen);
        self.samplers.reset(&self.console);
        match &mut self.phase {
            Some(Phase::Menu(menu)) => menu.resized(&self.data, self.video),
            Some(Phase::Level { menu: Some(m), .. }) => m.resized(&self.data, self.video),
            _ => {}
        }
    }
}

/// The file `cfg_save <name>` writes, as options-screens.md 12.3 names it
/// in `$app_data_root$`: no name the user configuration file; else the
/// extension replaced by `.ltx`.
fn config_name(name: &str) -> String {
    if name.is_empty() {
        return "user.ltx".to_owned();
    }
    let stem = name.rfind('.').map_or(name, |at| &name[..at]);
    format!("{stem}.ltx")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::console::{COMMANDS, Console, ConsoleFiles, Exec, Kind};

    #[test]
    fn start_reads_the_server_and_client_options() {
        let new = "server(all/single/alife/new) client(localhost)";
        assert_eq!(start(new), Start::NewGame);
        assert_eq!(start("server(all/single/alife/new)"), Start::NewGame);
        assert_eq!(start("server(x/single/alife/load)"), Start::Load);
        assert_eq!(start("server(all/alife/new)"), Start::NoClient);
        assert_eq!(start(""), Start::NoClient);
        assert_eq!(start("server(l01_escape/single)"), Start::Other);
    }

    #[test]
    fn config_names_take_the_ltx_extension() {
        assert_eq!(config_name(""), "user.ltx");
        assert_eq!(config_name("user"), "user.ltx");
        assert_eq!(config_name("user.cfg"), "user.ltx");
        assert_eq!(config_name("other"), "other.ltx");
    }

    /// Every action the console leaves to its caller has a handler.
    #[test]
    fn every_action_left_to_the_caller_is_known() {
        let known = [
            "quit",
            "disconnect",
            "main_menu",
            "start",
            "cfg_save",
            "cfg_load",
            "vid_restart",
            "snd_restart",
            "save",
            "load",
            "load_last_save",
            "screenshot",
            "demo_play",
            "demo_record",
            "check_for_updates",
        ];
        for def in COMMANDS {
            if !matches!(def.kind, Kind::Action(_)) || def.designer() {
                continue;
            }
            let mut console = Console::new(ConsoleFiles::default());
            console.exec("load_last_save x");
            let left = [format!("{} x", def.name), def.name.to_owned()]
                .iter()
                .any(|line| console.exec(line) == Exec::Action);
            assert!(
                !left || known.contains(&def.name) || MULTIPLAYER.contains(&def.name),
                "{}",
                def.name
            );
        }
    }
}
