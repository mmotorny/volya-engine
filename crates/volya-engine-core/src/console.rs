// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The console's commands (docs/behavior/console-commands.md): how a line
//! becomes a command, every command's kind, value, status and help text,
//! `help`, and the user configuration file that stores the values and the
//! key bindings (docs/behavior/options-screens.md sections 11 and 12).
//!
//! The console keeps its log lines (console-commands.md's "prints") for
//! whoever shows them. Of the actions it runs those whose effect is the
//! console's own: `help`, the key bindings (options-screens.md 11.5),
//! `bind_console` and `load_last_save <name>`; the others are its
//! caller's ([`Exec::Action`]). Setting `_preset` runs the preset's script
//! (options-screens.md section 8).

mod commands;
mod value;

pub use commands::{
    Bare, COMMANDS, CommandDef, DIFFICULTIES, GAME_MODES, Kind, PRESET_FILES, PRESETS, RENDERERS,
    Saved,
};
pub use value::{Value, help_text, parse_mode, real_status};

use volya_engine_formats::cp1251;

/// Who may use an action (options-screens.md 11.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    /// 1: any game.
    Any,
    /// 3: single player only.
    Single,
    /// 5: multiplayer only.
    Multi,
}

/// The 77 actions in their fixed order (options-screens.md 11.1).
pub const ACTIONS: &[(&str, ActionKind)] = {
    use ActionKind::{Any as A, Multi as M, Single as S};
    &[
        ("left", A),
        ("right", A),
        ("up", A),
        ("down", A),
        ("jump", A),
        ("crouch", A),
        ("crouch_toggle", A),
        ("accel", A),
        ("sprint_toggle", A),
        ("forward", A),
        ("back", A),
        ("lstrafe", A),
        ("rstrafe", A),
        ("llookout", A),
        ("rlookout", A),
        ("turn_engine", S),
        ("cam_1", A),
        ("cam_2", A),
        ("cam_3", A),
        ("cam_4", A),
        ("cam_zoom_in", A),
        ("cam_zoom_out", A),
        ("torch", A),
        ("night_vision", A),
        ("wpn_1", A),
        ("wpn_2", A),
        ("wpn_3", A),
        ("wpn_4", A),
        ("wpn_5", A),
        ("wpn_6", A),
        ("artefact", M),
        ("wpn_next", A),
        ("wpn_fire", A),
        ("wpn_zoom", A),
        ("wpn_zoom_inc", A),
        ("wpn_zoom_dec", A),
        ("wpn_reload", A),
        ("wpn_func", A),
        ("wpn_firemode_prev", A),
        ("wpn_firemode_next", A),
        ("pause", A),
        ("drop", A),
        ("use", A),
        ("scores", A),
        ("chat", M),
        ("chat_team", M),
        ("screenshot", A),
        ("quit", A),
        ("console", A),
        ("inventory", A),
        ("buy_menu", M),
        ("skin_menu", M),
        ("team_menu", M),
        ("active_jobs", S),
        ("map", A),
        ("contacts", S),
        ("ext_1", A),
        ("vote_begin", M),
        ("vote", M),
        ("vote_yes", M),
        ("vote_no", M),
        ("next_slot", A),
        ("prev_slot", A),
        ("speech_menu_0", M),
        ("speech_menu_1", M),
        ("speech_menu_2", M),
        ("speech_menu_3", M),
        ("speech_menu_4", M),
        ("speech_menu_5", M),
        ("speech_menu_6", M),
        ("speech_menu_7", M),
        ("speech_menu_8", M),
        ("speech_menu_9", M),
        ("use_bandage", S),
        ("use_medkit", S),
        ("quick_save", S),
        ("quick_load", S),
    ]
};

/// The 129 key names with their codes (options-screens.md 11.2).
pub const KEYS: &[(&str, u16)] = &[
    ("kESCAPE", 1),
    ("k1", 2),
    ("k2", 3),
    ("k3", 4),
    ("k4", 5),
    ("k5", 6),
    ("k6", 7),
    ("k7", 8),
    ("k8", 9),
    ("k9", 10),
    ("k0", 11),
    ("kMINUS", 12),
    ("kEQUALS", 13),
    ("kBACK", 14),
    ("kTAB", 15),
    ("kQ", 16),
    ("kW", 17),
    ("kE", 18),
    ("kR", 19),
    ("kT", 20),
    ("kY", 21),
    ("kU", 22),
    ("kI", 23),
    ("kO", 24),
    ("kP", 25),
    ("kLBRACKET", 26),
    ("kRBRACKET", 27),
    ("kRETURN", 28),
    ("kLCONTROL", 29),
    ("kA", 30),
    ("kS", 31),
    ("kD", 32),
    ("kF", 33),
    ("kG", 34),
    ("kH", 35),
    ("kJ", 36),
    ("kK", 37),
    ("kL", 38),
    ("kSEMICOLON", 39),
    ("kAPOSTROPHE", 40),
    ("kGRAVE", 41),
    ("kLSHIFT", 42),
    ("kBACKSLASH", 43),
    ("kZ", 44),
    ("kX", 45),
    ("kC", 46),
    ("kV", 47),
    ("kB", 48),
    ("kN", 49),
    ("kM", 50),
    ("kCOMMA", 51),
    ("kPERIOD", 52),
    ("kSLASH", 53),
    ("kRSHIFT", 54),
    ("kMULTIPLY", 55),
    ("kLMENU", 56),
    ("kSPACE", 57),
    ("kCAPITAL", 58),
    ("kF1", 59),
    ("kF2", 60),
    ("kF3", 61),
    ("kF4", 62),
    ("kF5", 63),
    ("kF6", 64),
    ("kF7", 65),
    ("kF8", 66),
    ("kF9", 67),
    ("kF10", 68),
    ("kNUMLOCK", 69),
    ("kSCROLL", 70),
    ("kNUMPAD7", 71),
    ("kNUMPAD8", 72),
    ("kNUMPAD9", 73),
    ("kSUBTRACT", 74),
    ("kNUMPAD4", 75),
    ("kNUMPAD5", 76),
    ("kNUMPAD6", 77),
    ("kADD", 78),
    ("kNUMPAD1", 79),
    ("kNUMPAD2", 80),
    ("kNUMPAD3", 81),
    ("kNUMPAD0", 82),
    ("kDECIMAL", 83),
    ("kF11", 87),
    ("kF12", 88),
    ("kF13", 100),
    ("kF14", 101),
    ("kF15", 102),
    ("kKANA", 112),
    ("kCONVERT", 121),
    ("kNOCONVERT", 123),
    ("kYEN", 125),
    ("kNUMPADEQUALS", 141),
    ("kCIRCUMFLEX", 144),
    ("kAT", 145),
    ("kCOLON", 146),
    ("kUNDERLINE", 147),
    ("kKANJI", 148),
    ("kSTOP", 149),
    ("kAX", 150),
    ("kUNLABELED", 151),
    ("kNUMPADENTER", 156),
    ("kRCONTROL", 157),
    ("kNUMPADCOMMA", 179),
    ("kDIVIDE", 181),
    ("kSYSRQ", 183),
    ("kRMENU", 184),
    ("kHOME", 199),
    ("kUP", 200),
    ("kPRIOR", 201),
    ("kLEFT", 203),
    ("kRIGHT", 205),
    ("kEND", 207),
    ("kDOWN", 208),
    ("kNEXT", 209),
    ("kINSERT", 210),
    ("kDELETE", 211),
    ("kLWIN", 219),
    ("kRWIN", 220),
    ("kAPPS", 221),
    ("kPAUSE", 197),
    ("mouse1", 337),
    ("mouse2", 338),
    ("mouse3", 339),
    ("mouse4", 340),
    ("mouse5", 341),
    ("mouse6", 342),
    ("mouse7", 343),
    ("mouse8", 344),
];

/// The index of action `name` in [`ACTIONS`] (case-insensitive).
pub fn action_index(name: &str) -> Option<usize> {
    ACTIONS
        .iter()
        .position(|(a, _)| a.eq_ignore_ascii_case(name))
}

/// The index of key `name` in [`KEYS`] (case-insensitive).
pub fn key_index(name: &str) -> Option<usize> {
    KEYS.iter().position(|(k, _)| k.eq_ignore_ascii_case(name))
}

/// Whether a key may be bound to both actions `a` and `b` (indices into
/// [`ACTIONS`]): when one is single-player only and the other multiplayer
/// only (options-screens.md 11.4).
pub fn may_share(a: usize, b: usize) -> bool {
    use ActionKind::{Multi, Single};
    matches!(
        (ACTIONS[a].1, ACTIONS[b].1),
        (Single, Multi) | (Multi, Single)
    )
}

/// The game files the console runs scripts from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConsoleFiles {
    /// `$game_config$default_controls.ltx`, run by `unbindall`.
    pub default_controls: Vec<u8>,
    /// `$game_config$rspec_<file>.ltx` by preset ([`PRESET_FILES`]); empty
    /// when missing.
    pub presets: [Vec<u8>; 5],
}

/// What [`Console::exec`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exec {
    /// A value or a binding was stored.
    Done,
    /// A value was rejected or an action's argument named nothing: nothing
    /// changed.
    Rejected,
    /// The bare name printed the status, or `help` its listing.
    Shown,
    /// An action whose effect is the console's caller's (its own
    /// specification, docs/behavior/console-scope.md section 6).
    Action,
    /// No such command: `! Unknown command:  <name>` was printed.
    Unknown,
    /// An empty line, or `name` without a game (console-commands.md 4.3).
    Ignored,
}

/// `load_last_save <name>` keeps at most this many characters
/// (steering-the-original.md 3.2.1).
const LAST_SAVE_LENGTH: usize = 519;

/// The console's state.
#[derive(Clone, Debug)]
pub struct Console {
    files: ConsoleFiles,
    /// Per command of [`COMMANDS`], its value.
    values: Vec<Value>,
    /// Per action, the primary and secondary key (indices into [`KEYS`]).
    slots: Vec<[Option<usize>; 2]>,
    /// `bind_console` lines: the command line and the key's code.
    console_keys: Vec<(String, u16)>,
    /// The name `load_last_save <name>` remembered.
    last_save: Option<String>,
    /// Counts the commands that took effect.
    generation: u64,
    /// Per command, its executions: with an argument, accepted or not, and
    /// of the bare name (texture-quality-filtering.md 6.1).
    executions: Vec<u64>,
    /// The lines printed, oldest first.
    log: Vec<String>,
    /// The user configuration file runs at startup.
    starting: bool,
    /// Why the program ends (console-commands.md section 5), once it must.
    fatal: Option<String>,
}

impl PartialEq for Console {
    /// Equal values and bindings, however they were reached.
    fn eq(&self, o: &Self) -> bool {
        self.files == o.files
            && self.values == o.values
            && self.slots == o.slots
            && self.console_keys == o.console_keys
            && self.last_save == o.last_save
    }
}

/// The index in [`COMMANDS`] of the registered command `name` (byte
/// comparison, case-sensitive).
fn registered(name: &str) -> Option<usize> {
    let i = COMMANDS
        .binary_search_by(|d| d.name.as_bytes().cmp(name.as_bytes()))
        .ok()?;
    (!COMMANDS[i].designer()).then_some(i)
}

/// A line as section 2 step 1 makes it: tabs and line feeds become spaces,
/// runs of spaces one space, a leading space is dropped.
fn normalize(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    for c in line.chars() {
        let c = if matches!(c, '\t' | '\n') { ' ' } else { c };
        if c == ' ' && (out.is_empty() || out.ends_with(' ')) {
            continue;
        }
        out.push(c);
    }
    out
}

impl Console {
    /// Every command at its built-in value and no bindings.
    pub fn new(files: ConsoleFiles) -> Self {
        let values = COMMANDS
            .iter()
            .map(|d| match d.kind {
                Kind::Action(_) | Kind::Name => Value::None,
                kind => Value::parse(kind, d.initial).expect("built-in value"),
            })
            .collect();
        Self {
            files,
            values,
            slots: vec![[None; 2]; ACTIONS.len()],
            console_keys: Vec::new(),
            last_save: None,
            generation: 0,
            executions: vec![0; COMMANDS.len()],
            log: Vec::new(),
            starting: false,
            fatal: None,
        }
    }

    /// How often command `name` was executed: with an argument (accepted
    /// or not) or alone (0 for other names).
    pub fn executions(&self, name: &str) -> u64 {
        registered(name).map_or(0, |i| self.executions[i])
    }

    /// Changes whenever a command takes effect (cheap change detection).
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The lines the commands printed, oldest first.
    pub fn log(&self) -> &[String] {
        &self.log
    }

    /// Why the program ends, once a command made it end: `help` executed
    /// from the user configuration file at startup (console-commands.md
    /// section 5, measured as a crash of the original).
    pub fn fatal(&self) -> Option<&str> {
        self.fatal.as_deref()
    }

    /// As the game starts (console-commands.md section 1): of the user
    /// configuration file `user` the lines that start with the bytes
    /// `renderer `, then `unbindall`, then the whole file as a script
    /// (options-screens.md section 12.2). The first execution's
    /// `Executing config-script` lines are `cfg_load`'s (console-scope.md
    /// 5.6), not printed here.
    pub fn startup(files: ConsoleFiles, user: &[u8]) -> Self {
        let mut console = Self::new(files);
        for line in user.split(|&b| b == b'\r' || b == b'\n') {
            if line.starts_with(b"renderer ") {
                let line = &line[..line.len().min(1023)];
                console.exec(&cp1251::decode(line));
            }
        }
        console.starting = true;
        console.exec("unbindall");
        console.exec_script(user);
        console.starting = false;
        console
    }

    /// Runs a script: every line (CR, LF or CR LF; at most 1023 bytes) as a
    /// command (options-screens.md section 12.2).
    pub fn exec_script(&mut self, script: &[u8]) {
        for line in script.split(|&b| b == b'\r' || b == b'\n') {
            if self.fatal.is_some() {
                return;
            }
            let line = &line[..line.len().min(1023)];
            self.exec(&cp1251::decode(line));
        }
    }

    /// Runs one line the programs or a script execute.
    pub fn exec(&mut self, line: &str) -> Exec {
        self.execute(line, false)
    }

    /// Runs one line typed in the console and entered: as [`Self::exec`],
    /// after printing `~ <line>` (section 2, step 2).
    pub fn enter(&mut self, line: &str) -> Exec {
        self.execute(line, true)
    }

    fn execute(&mut self, line: &str, typed: bool) -> Exec {
        let result = self.run(line, typed);
        if result == Exec::Done {
            self.generation += 1;
        }
        result
    }

    fn print(&mut self, line: String) {
        self.log.push(line);
    }

    /// Section 2.
    fn run(&mut self, line: &str, typed: bool) -> Exec {
        let line = normalize(line);
        if line.is_empty() {
            return Exec::Ignored;
        }
        if typed {
            self.print(format!("~ {line}"));
        }
        let (name, argument) = match line.split_once(' ') {
            Some((n, a)) => (n, a.strip_suffix(' ').unwrap_or(a)),
            None => (line.as_str(), ""),
        };
        let Some(i) = registered(name) else {
            // The message ends in a space and the name follows one more.
            self.print(format!("! Unknown command:  {name}"));
            return Exec::Unknown;
        };
        let def = COMMANDS[i];
        // Only the bytes `A` … `Z` change.
        let argument = if def.keeps_case() {
            argument.to_owned()
        } else {
            argument.to_ascii_lowercase()
        };
        self.executions[i] += 1;
        if argument.is_empty() && def.bare == Bare::Shows {
            let status = self.values[i].status(def.kind);
            self.print(format!("- {name} {status}"));
            return Exec::Shown;
        }
        self.command(i, &argument)
    }

    /// Gives `argument` to command `i` (section 3, 4).
    fn command(&mut self, i: usize, argument: &str) -> Exec {
        let def = COMMANDS[i];
        let parsed = match def.kind {
            Kind::Action(_) => return self.action(def.name, argument),
            // Only acts while a game with a player exists; Volya Engine's
            // console has none (4.3).
            Kind::Name => return Exec::Ignored,
            kind => Value::parse(kind, argument),
        };
        let result = match parsed {
            Some(v) => {
                self.values[i] = v;
                Exec::Done
            }
            None if def.kind == Kind::Mode => {
                self.print(format!("! Wrong video mode [{argument}]"));
                Exec::Rejected
            }
            None => {
                self.print(format!("~ Invalid syntax in call to '{}'", def.name));
                self.print(format!(
                    "~ Valid arguments: {}",
                    help_text(def.name, def.kind)
                ));
                Exec::Rejected
            }
        };
        if def.name == "_preset" {
            // The current value's script runs whether or not the new name
            // was accepted (options-screens.md section 8).
            if let Value::List(p) = self.values[i] {
                let p = p as usize;
                let script = std::mem::take(&mut self.files.presets[p]);
                self.exec_script(&script);
                self.files.presets[p] = script;
            }
        }
        result
    }

    /// The actions whose effect is the console's own.
    fn action(&mut self, name: &str, argument: &str) -> Exec {
        let words: Vec<&str> = argument.split(' ').filter(|w| !w.is_empty()).collect();
        match name {
            "help" => {
                self.help();
                Exec::Shown
            }
            "unbindall" => {
                self.slots.fill([None; 2]);
                self.console_keys.clear();
                let defaults = std::mem::take(&mut self.files.default_controls);
                self.exec_script(&defaults);
                self.files.default_controls = defaults;
                Exec::Done
            }
            "bind" | "bind_sec" => {
                let (Some(action), Some(key)) = (words.first(), words.get(1)) else {
                    return Exec::Rejected;
                };
                let (Some(a), Some(k)) = (action_index(action), key_index(key)) else {
                    return Exec::Rejected;
                };
                self.bind(a, usize::from(name == "bind_sec"), Some(k));
                Exec::Done
            }
            "unbind" | "unbind_sec" => {
                let Some(a) = words.first().and_then(|a| action_index(a)) else {
                    return Exec::Rejected;
                };
                self.slots[a][usize::from(name == "unbind_sec")] = None;
                Exec::Done
            }
            "bind_console" => {
                // The last word is the key (options-screens.md 12.3).
                let Some((command, key)) = argument.rsplit_once(' ') else {
                    return Exec::Rejected;
                };
                let Some(k) = key_index(key) else {
                    return Exec::Rejected;
                };
                let code = KEYS[k].1;
                self.console_keys.retain(|(_, c)| *c != code);
                let at = self.console_keys.partition_point(|(_, c)| *c < code);
                self.console_keys
                    .insert(at, (format!("{command} {}", KEYS[k].0), code));
                Exec::Done
            }
            // With a name it only remembers it (steering-the-original.md
            // 3.2.1); without one it loads, which is the caller's.
            "load_last_save" if !argument.is_empty() => {
                self.last_save = Some(argument.chars().take(LAST_SAVE_LENGTH).collect());
                Exec::Done
            }
            _ => Exec::Action,
        }
    }

    /// `help`'s listing (section 5). From the user configuration file at
    /// startup it stops before `name`, whose status reads the game objects,
    /// and the program ends.
    fn help(&mut self) {
        self.print("- --- Command listing: start ---".to_owned());
        for (i, def) in COMMANDS.iter().enumerate() {
            if def.designer() {
                continue;
            }
            if self.starting && def.kind == Kind::Name {
                self.fatal = Some(
                    "`help` in the user configuration file: the command listing failed at `name`"
                        .to_owned(),
                );
                return;
            }
            let line = format!(
                "{:<20} ({:<10}) --- {}",
                def.name,
                self.values[i].status(def.kind),
                help_text(def.name, def.kind)
            );
            self.print(line);
        }
        self.print("- --- Command listing: end ----".to_owned());
    }

    /// Sets slot `slot` of action `action` to key `key`; the key then leaves
    /// both slots of every other action that may not share it
    /// (options-screens.md section 11.5).
    fn bind(&mut self, action: usize, slot: usize, key: Option<usize>) {
        self.slots[action][slot] = key;
        let Some(key) = key else { return };
        for (other, slots) in self.slots.iter_mut().enumerate() {
            if other == action || may_share(action, other) {
                continue;
            }
            for s in slots {
                if *s == Some(key) {
                    *s = None;
                }
            }
        }
    }

    /// The value of command `name`.
    pub fn value(&self, name: &str) -> Option<Value> {
        registered(name).map(|i| self.values[i].clone())
    }

    /// The definition of command `name`.
    pub fn def(name: &str) -> Option<CommandDef> {
        registered(name).map(|i| COMMANDS[i])
    }

    /// The status of command `name` (empty for commands without a value).
    pub fn status(&self, name: &str) -> Option<String> {
        registered(name).map(|i| self.values[i].status(COMMANDS[i].kind))
    }

    /// `name`'s value as on/off (false when not on/off).
    pub fn on(&self, name: &str) -> bool {
        self.value(name) == Some(Value::OnOff(true))
    }

    /// `name`'s value as a number (0 when unknown).
    pub fn number(&self, name: &str) -> f32 {
        self.value(name).map_or(0.0, |v| v.number())
    }

    /// The name `load_last_save <name>` remembered.
    pub fn last_save(&self) -> Option<&str> {
        self.last_save.as_deref()
    }

    /// The primary and secondary key names of action `action` (an index
    /// into [`ACTIONS`]).
    pub fn keys(&self, action: usize) -> [Option<&'static str>; 2] {
        self.slots[action].map(|k| k.map(|k| KEYS[k].0))
    }

    /// `vid_mode`. Which sizes take effect is display-mode.md's; one below
    /// 1 is used as 1 (our choice: nothing can be drawn at a zero size).
    pub fn video_mode(&self) -> (u32, u32) {
        match self.value("vid_mode") {
            Some(Value::Mode(w, h)) => (w.max(1) as u32, h.max(1) as u32),
            _ => (1024, 768),
        }
    }

    /// The user configuration file `cfg_save` writes (console-commands.md
    /// section 6): each command's lines in name order, CR LF; at `bind`
    /// `unbindall` and the primary keys, at `bind_sec` the secondary ones
    /// (both in action order), at `bind_console` its lines (LF only, by key
    /// code; options-screens.md 12.3).
    pub fn save(&self) -> Vec<u8> {
        let mut text = String::new();
        let slot = |text: &mut String, slot: usize, command: &str| {
            for ((a, _), s) in ACTIONS.iter().zip(&self.slots) {
                if let Some(k) = s[slot] {
                    text.push_str(&format!("{command} {a} {}\r\n", KEYS[k].0));
                }
            }
        };
        for (def, v) in COMMANDS.iter().zip(&self.values) {
            if def.designer() {
                continue;
            }
            match (def.saved, def.name) {
                (Saved::No, _) => {}
                (Saved::Own, "bind") => {
                    text.push_str("unbindall\r\n");
                    slot(&mut text, 0, "bind");
                }
                (Saved::Own, "bind_sec") => slot(&mut text, 1, "bind_sec"),
                (Saved::Own, "bind_console") => {
                    for (line, _) in &self.console_keys {
                        text.push_str(&format!("bind_console {line}\n"));
                    }
                }
                (Saved::Own, "load_last_save") => {
                    if let Some(name) = &self.last_save {
                        text.push_str(&format!("load_last_save {name}\r\n"));
                    }
                }
                // `renderer` is left out only under `-r2`; Volya Engine has
                // no command line. The other commands: when the status is
                // not empty.
                (Saved::Own | Saved::Yes, name) => {
                    let status = v.status(def.kind);
                    if !status.is_empty() {
                        text.push_str(&format!("{name} {status}\r\n"));
                    }
                }
            }
        }
        cp1251::encode(&text).unwrap_or_else(|| text.into_bytes())
    }
}

#[cfg(test)]
mod tests;
