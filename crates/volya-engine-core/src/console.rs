// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The console: the settings the options screen edits as console variables,
//! the key bindings, and the user configuration file that stores both
//! (docs/behavior/options-screens.md sections 7, 8, 11 and 12).
//!
//! A command line is `<name> <value>`: setting a variable checks the value
//! against the variable's kind and range and keeps the old value when it is
//! rejected (section 7.1); `bind`, `bind_sec`, `unbind`, `unbind_sec` and
//! `unbindall` edit the bindings (section 11.5). Setting `_preset` runs the
//! preset's script (section 8).
//!
//! Volya Engine has only the variables of the options screen, and two that
//! act without a control (`r1_tf_mipbias`, `rs_stats`). Any other
//! `<name> <value>` line (the user file sets about 200 more, and the presets
//! set some) is kept as written and saved again, so settings of systems
//! Volya Engine lacks survive a round trip (our choice; the original knows
//! each as a variable of its own). Command names are matched exactly (our
//! choice: the spec does not say); action and key names case-insensitively.

use std::collections::BTreeMap;

use volya_engine_formats::cp1251;
use volya_engine_formats::ltx::value;

/// The kind and range of a variable (section 7.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    OnOff,
    Int {
        min: i64,
        max: i64,
    },
    Real {
        min: f32,
        max: f32,
    },
    /// A name from the list; the value is its index.
    List(&'static [&'static str]),
    /// `<W>x<H>` (`vid_mode`).
    Mode,
}

/// A variable of the options screen (section 7.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VarDef {
    pub name: &'static str,
    pub kind: Kind,
    /// The built-in initial value.
    pub initial: &'static str,
}

const fn real(name: &'static str, min: f32, max: f32, initial: &'static str) -> VarDef {
    VarDef {
        name,
        kind: Kind::Real { min, max },
        initial,
    }
}

const fn int(name: &'static str, min: i64, max: i64, initial: &'static str) -> VarDef {
    VarDef {
        name,
        kind: Kind::Int { min, max },
        initial,
    }
}

const fn on_off(name: &'static str, initial: &'static str) -> VarDef {
    VarDef {
        name,
        kind: Kind::OnOff,
        initial,
    }
}

/// The renderer list of `renderer` (section 7.2).
pub const RENDERERS: &[&str] = &["renderer_r1", "renderer_r2a", "renderer_r2"];
/// The presets of `_preset`.
pub const PRESETS: &[&str] = &["Minimum", "Low", "Default", "High", "Extreme"];
/// The preset scripts' file names, `$game_config$rspec_<file>.ltx`, by
/// preset (section 8).
pub const PRESET_FILES: [&str; 5] = ["minimum", "low", "default", "high", "extreme"];
/// The difficulties of `g_game_difficulty`.
pub const DIFFICULTIES: &[&str] = &["gd_novice", "gd_stalker", "gd_veteran", "gd_master"];

/// The variables of section 7.2, with `r1_tf_mipbias` and `rs_stats`, which
/// the options screen has no control for.
pub const VARIABLES: &[VarDef] = &[
    VarDef {
        name: "renderer",
        kind: Kind::List(RENDERERS),
        initial: "renderer_r1",
    },
    VarDef {
        name: "_preset",
        kind: Kind::List(PRESETS),
        initial: "Default",
    },
    VarDef {
        name: "vid_mode",
        kind: Kind::Mode,
        initial: "1024x768",
    },
    real("rs_c_gamma", 0.5, 1.5, "1"),
    real("rs_c_contrast", 0.5, 1.5, "1"),
    real("rs_c_brightness", 0.5, 1.5, "1"),
    on_off("rs_fullscreen", "on"),
    real("rs_vis_distance", 0.4, 1.5, "1"),
    real("r__geometry_lod", 0.1, 1.2, "1"),
    real("r__detail_density", 0.2, 0.6, "0.3"),
    int("texture_lod", 0, 4, "0"),
    int("r__tf_aniso", 1, 16, "4"),
    // texture-quality-filtering.md 4.
    real("r1_tf_mipbias", -0.5, 0.5, "0"),
    int("r__supersample", 1, 4, "1"),
    on_off("r2_sun", "on"),
    on_off("r2_sun_details", "off"),
    real("r2_slight_fade", 0.02, 2.0, "1"),
    real("r2_ls_squality", 0.5, 1.0, "1"),
    on_off("ai_use_torch_dynamic_lights", "on"),
    on_off("rs_v_sync", "off"),
    on_off("rs_refresh_60hz", "off"),
    // display-mode.md A.1.
    on_off("rs_stats", "off"),
    real("snd_volume_eff", 0.0, 1.0, "1"),
    real("snd_volume_music", 0.0, 1.0, "0.7"),
    on_off("snd_efx", "on"),
    VarDef {
        name: "g_game_difficulty",
        kind: Kind::List(DIFFICULTIES),
        initial: "gd_master",
    },
    on_off("hud_crosshair", "on"),
    on_off("cl_dynamiccrosshair", "on"),
    on_off("hud_weapon", "on"),
    on_off("hud_info", "on"),
    on_off("hud_crosshair_dist", "off"),
    real("mouse_sens", 0.05, 0.6, "0.12"),
    on_off("mouse_invert", "off"),
];

/// A variable's value.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    OnOff(bool),
    Int(i64),
    Real(f32),
    /// Index into the variable's list.
    List(usize),
    Mode(u32, u32),
}

/// The tolerance of real ranges (section 7.1).
const RANGE_EPSILON: f32 = 0.000_01;

impl Value {
    /// `text` as a value of `kind`, `None` when the kind rejects it
    /// (section 7.1).
    pub fn parse(kind: Kind, text: &str) -> Option<Self> {
        match kind {
            Kind::OnOff => match text {
                "on" | "1" => Some(Self::OnOff(true)),
                "off" | "0" => Some(Self::OnOff(false)),
                _ => None,
            },
            Kind::Int { min, max } => {
                let v = value::int(text);
                (min..=max).contains(&v).then_some(Self::Int(v))
            }
            Kind::Real { min, max } => {
                let v = value::float(text);
                (v >= min - RANGE_EPSILON && v <= max + RANGE_EPSILON).then_some(Self::Real(v))
            }
            Kind::List(names) => names
                .iter()
                .position(|n| n.eq_ignore_ascii_case(text))
                .map(Self::List),
            Kind::Mode => parse_mode(text).map(|(w, h)| Self::Mode(w, h)),
        }
    }

    /// The text read back (section 7.1): `on`/`off`, decimal, `%3.5f` with
    /// trailing zeros removed, the list name, `<W>x<H>`.
    pub fn status(self, kind: Kind) -> String {
        match (self, kind) {
            (Self::OnOff(on), _) => if on { "on" } else { "off" }.to_owned(),
            (Self::Int(v), _) => v.to_string(),
            (Self::Real(v), _) => real_status(v),
            (Self::List(i), Kind::List(names)) => names.get(i).copied().unwrap_or("?").to_owned(),
            (Self::List(_), _) => "?".to_owned(),
            (Self::Mode(w, h), _) => format!("{w}x{h}"),
        }
    }

    /// The value as a number: on/off as 1/0, list entries as their index.
    pub fn number(self) -> f32 {
        match self {
            Self::OnOff(on) => f32::from(u8::from(on)),
            Self::Int(v) => v as f32,
            Self::Real(v) => v,
            Self::List(i) => i as f32,
            Self::Mode(w, _) => w as f32,
        }
    }
}

/// `%3.5f` of `v` with its trailing zeros removed: `1.00000` → `1.`,
/// `0.33330` → `0.3333`.
pub fn real_status(v: f32) -> String {
    let text = format!("{:.5}", f64::from(v));
    text.trim_end_matches('0').to_owned()
}

/// `<W>x<H>`, two decimal integers (section 7.2). That neither may be 0 is
/// our choice (nothing can be drawn at a zero size).
pub fn parse_mode(text: &str) -> Option<(u32, u32)> {
    let (w, h) = text.split_once('x')?;
    let number = |s: &str| {
        (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse::<u32>().ok())
            .flatten()
            .filter(|&n| n > 0)
    };
    Some((number(w)?, number(h)?))
}

/// Who may use an action (section 11.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    /// 1: any game.
    Any,
    /// 3: single player only.
    Single,
    /// 5: multiplayer only.
    Multi,
}

/// The 77 actions in their fixed order (section 11.1).
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

/// The 129 key names with their codes (section 11.2).
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
/// only (section 11.4).
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
    /// A variable or binding command took effect.
    Done,
    /// A known command whose value was rejected: nothing changed.
    Rejected,
    /// Not a command of Volya Engine's: kept as written.
    Kept,
    /// Not a command at all (an empty line, a name alone).
    Ignored,
}

/// Commands the specs name that are not variables: they have no status,
/// so the user file never stores them (section 12.3), and a line of one in
/// a hand-edited file is not kept. Running them is the console's caller's
/// business (none runs from the user file here).
pub const COMMANDS: &[&str] = &[
    "cfg_load",
    "cfg_save",
    "vid_restart",
    "snd_restart",
    "check_for_updates",
    // docs/behavior/level-loading.md section 2.1.
    "disconnect",
    "start",
    "main_menu",
];

/// The console's state.
#[derive(Clone, Debug)]
pub struct Console {
    files: ConsoleFiles,
    values: Vec<Value>,
    /// Other `<name> <value>` lines, by name: the value as written.
    kept: BTreeMap<String, String>,
    /// Per action, the primary and secondary key (indices into [`KEYS`]).
    slots: Vec<[Option<usize>; 2]>,
    /// `bind_console` lines: the command line and the key's code.
    console_keys: Vec<(String, u16)>,
    /// A preset's script is running.
    in_preset: bool,
    /// Counts the commands that took effect.
    generation: u64,
    /// Per variable, its executions: with an argument, accepted or not,
    /// and of the bare name, which reads its status
    /// (texture-quality-filtering.md 6.1).
    executions: Vec<u64>,
}

impl PartialEq for Console {
    /// Equal settings and bindings, however they were reached.
    fn eq(&self, o: &Self) -> bool {
        self.files == o.files
            && self.values == o.values
            && self.kept == o.kept
            && self.slots == o.slots
            && self.console_keys == o.console_keys
    }
}

impl Console {
    /// The variables at their built-in values and no bindings.
    pub fn new(files: ConsoleFiles) -> Self {
        let values = VARIABLES
            .iter()
            .map(|d| Value::parse(d.kind, d.initial).expect("built-in value"))
            .collect();
        Self {
            files,
            values,
            kept: BTreeMap::new(),
            slots: vec![[None; 2]; ACTIONS.len()],
            console_keys: Vec::new(),
            in_preset: false,
            generation: 0,
            executions: vec![0; VARIABLES.len()],
        }
    }

    /// How often variable `name` was executed: with an argument (accepted
    /// or not) or alone, which reads its status (0 for other names).
    pub fn executions(&self, name: &str) -> u64 {
        VARIABLES
            .iter()
            .position(|d| d.name == name)
            .map_or(0, |i| self.executions[i])
    }

    /// Changes whenever a command takes effect (cheap change detection).
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// As the game starts (section 12.2): `unbindall`, then the user
    /// configuration file `user` as a script.
    pub fn startup(files: ConsoleFiles, user: &[u8]) -> Self {
        let mut console = Self::new(files);
        console.exec("unbindall");
        console.exec_script(user);
        console
    }

    /// Runs a script: every line (CR, LF or CR LF; at most 1023 bytes) as a
    /// command (section 12.2).
    pub fn exec_script(&mut self, script: &[u8]) {
        for line in script.split(|&b| b == b'\r' || b == b'\n') {
            let line = &line[..line.len().min(1023)];
            self.exec(&cp1251::decode(line));
        }
    }

    /// Runs one command line.
    pub fn exec(&mut self, line: &str) -> Exec {
        let result = self.run(line);
        if matches!(result, Exec::Done | Exec::Kept) {
            self.generation += 1;
        }
        result
    }

    fn run(&mut self, line: &str) -> Exec {
        let line = line.trim();
        let (name, rest) = match line.split_once(char::is_whitespace) {
            Some((n, r)) => (n, r.trim()),
            None => (line, ""),
        };
        if name.is_empty() {
            return Exec::Ignored;
        }
        let words: Vec<&str> = rest.split_whitespace().collect();
        match name {
            "unbindall" => {
                self.slots.fill([None; 2]);
                self.console_keys.clear();
                let defaults = std::mem::take(&mut self.files.default_controls);
                self.exec_script(&defaults);
                self.files.default_controls = defaults;
                return Exec::Done;
            }
            "bind" | "bind_sec" => {
                let (Some(action), Some(key)) = (words.first(), words.get(1)) else {
                    return Exec::Rejected;
                };
                let (Some(a), Some(k)) = (action_index(action), key_index(key)) else {
                    return Exec::Rejected;
                };
                self.bind(a, usize::from(name == "bind_sec"), Some(k));
                return Exec::Done;
            }
            "unbind" | "unbind_sec" => {
                let Some(a) = words.first().and_then(|a| action_index(a)) else {
                    return Exec::Rejected;
                };
                self.slots[a][usize::from(name == "unbind_sec")] = None;
                return Exec::Done;
            }
            "bind_console" => {
                // The last word is the key (section 12.3).
                let Some((command, key)) = rest.rsplit_once(char::is_whitespace) else {
                    return Exec::Rejected;
                };
                let Some(k) = key_index(key) else {
                    return Exec::Rejected;
                };
                let code = KEYS[k].1;
                self.console_keys.retain(|(_, c)| *c != code);
                let at = self.console_keys.partition_point(|(_, c)| *c < code);
                self.console_keys
                    .insert(at, (format!("{} {}", command.trim(), KEYS[k].0), code));
                return Exec::Done;
            }
            _ => {}
        }
        if COMMANDS.contains(&name) {
            return Exec::Ignored;
        }
        let Some(i) = VARIABLES.iter().position(|d| d.name == name) else {
            if rest.is_empty() {
                return Exec::Ignored;
            }
            // A preset sets other variables only for the session: it sets
            // them again at the next start (our choice; the shipped file
            // stores none of those it alone sets).
            if !self.in_preset || self.kept.contains_key(name) {
                self.kept.insert(name.to_owned(), rest.to_owned());
            }
            return Exec::Kept;
        };
        self.executions[i] += 1;
        if rest.is_empty() {
            return Exec::Ignored;
        }
        let def = VARIABLES[i];
        let value = match def.kind {
            // A list value is one name.
            Kind::List(_) | Kind::Mode => words.first().copied().unwrap_or(""),
            _ => rest,
        };
        let parsed = Value::parse(def.kind, value);
        if let Some(v) = parsed {
            self.values[i] = v;
        }
        if name == "_preset" {
            // Section 8: the current value's script runs whether or not the
            // new name was accepted.
            if let Value::List(p) = self.values[i] {
                let script = std::mem::take(&mut self.files.presets[p]);
                self.in_preset = true;
                self.exec_script(&script);
                self.in_preset = false;
                self.files.presets[p] = script;
            }
        }
        if parsed.is_some() {
            Exec::Done
        } else {
            Exec::Rejected
        }
    }

    /// Sets slot `slot` of action `action` to key `key`; the key then leaves
    /// both slots of every other action that may not share it (section
    /// 11.5).
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

    /// The value of variable `name`.
    pub fn value(&self, name: &str) -> Option<Value> {
        let i = VARIABLES.iter().position(|d| d.name == name)?;
        Some(self.values[i])
    }

    /// The definition of variable `name`.
    pub fn def(name: &str) -> Option<VarDef> {
        VARIABLES.iter().find(|d| d.name == name).copied()
    }

    /// The status of variable `name`, or the kept value of another name.
    pub fn status(&self, name: &str) -> Option<String> {
        match VARIABLES.iter().position(|d| d.name == name) {
            Some(i) => Some(self.values[i].status(VARIABLES[i].kind)),
            None => self.kept.get(name).cloned(),
        }
    }

    /// `name`'s value as on/off (false when not on/off).
    pub fn on(&self, name: &str) -> bool {
        self.value(name) == Some(Value::OnOff(true))
    }

    /// `name`'s value as a number (0 when unknown).
    pub fn number(&self, name: &str) -> f32 {
        self.value(name).map_or(0.0, Value::number)
    }

    /// The primary and secondary key names of action `action` (an index
    /// into [`ACTIONS`]).
    pub fn keys(&self, action: usize) -> [Option<&'static str>; 2] {
        self.slots[action].map(|k| k.map(|k| KEYS[k].0))
    }

    /// `vid_mode`.
    pub fn video_mode(&self) -> (u32, u32) {
        match self.value("vid_mode") {
            Some(Value::Mode(w, h)) => (w, h),
            _ => (1024, 768),
        }
    }

    /// The user configuration file `cfg_save` writes (section 12.3): every
    /// command's line in ascending byte order of the command names, CR LF;
    /// at `bind` `unbindall` and the primary keys, at `bind_sec` the
    /// secondary ones (both in action order), at `bind_console` its lines
    /// (LF only, by key code).
    pub fn save(&self) -> Vec<u8> {
        let mut entries: Vec<(&str, String)> = Vec::new();
        for (d, v) in VARIABLES.iter().zip(&self.values) {
            entries.push((d.name, format!("{} {}\r\n", d.name, v.status(d.kind))));
        }
        for (name, value) in &self.kept {
            entries.push((name, format!("{name} {value}\r\n")));
        }
        let slot = |slot: usize, command: &str| -> String {
            ACTIONS
                .iter()
                .zip(&self.slots)
                .filter_map(|((a, _), s)| {
                    s[slot].map(|k| format!("{command} {a} {}\r\n", KEYS[k].0))
                })
                .collect()
        };
        entries.push(("bind", format!("unbindall\r\n{}", slot(0, "bind"))));
        entries.push(("bind_sec", slot(1, "bind_sec")));
        entries.push((
            "bind_console",
            self.console_keys
                .iter()
                .map(|(line, _)| format!("bind_console {line}\n"))
                .collect(),
        ));
        entries.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
        let text: String = entries.into_iter().map(|(_, t)| t).collect();
        cp1251::encode(&text).unwrap_or_else(|| text.into_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULTS: &[u8] =
        b"bind quit kESCAPE\r\nbind forward kW\r\nbind jump kSPACE\r\nbind vote kF6\r\nbind quick_save kF6";

    fn files() -> ConsoleFiles {
        let mut presets: [Vec<u8>; 5] = Default::default();
        presets[1] = b"r__geometry_lod .9\r\nr__tf_aniso 2\r\nph_iterations 18\r\n".to_vec();
        presets[4] = b"r__tf_aniso 16\n".to_vec();
        ConsoleFiles {
            default_controls: DEFAULTS.to_vec(),
            presets,
        }
    }

    #[test]
    fn counts() {
        assert_eq!(ACTIONS.len(), 77);
        assert_eq!(KEYS.len(), 129);
        // Section 7.2's 31, `r1_tf_mipbias` and `rs_stats`.
        assert_eq!(VARIABLES.len(), 33);
    }

    #[test]
    fn kinds_accept_reject_and_read_back() {
        let mut c = Console::new(files());
        // on/off: exact, case-sensitive.
        assert_eq!(c.exec("rs_fullscreen 0"), Exec::Done);
        assert_eq!(c.status("rs_fullscreen").unwrap(), "off");
        assert_eq!(c.exec("rs_fullscreen On"), Exec::Rejected);
        assert_eq!(c.exec("rs_fullscreen 1"), Exec::Done);
        assert_eq!(c.status("rs_fullscreen").unwrap(), "on");
        // Integers: atoi, in range.
        assert_eq!(c.exec("r__tf_aniso 17"), Exec::Rejected);
        assert_eq!(c.exec("r__tf_aniso 8.9"), Exec::Done);
        assert_eq!(c.status("r__tf_aniso").unwrap(), "8");
        // Reals: atof within the range ± 0.00001; %3.5f without trailing 0s.
        assert_eq!(c.status("rs_c_gamma").unwrap(), "1.");
        assert_eq!(c.exec("r2_slight_fade 0.3333"), Exec::Done);
        assert_eq!(c.status("r2_slight_fade").unwrap(), "0.3333");
        assert_eq!(c.exec("snd_volume_eff 1.000005"), Exec::Done);
        assert_eq!(c.exec("snd_volume_eff 1.1"), Exec::Rejected);
        assert_eq!(c.exec("snd_volume_eff 0"), Exec::Done);
        assert_eq!(c.status("snd_volume_eff").unwrap(), "0.");
        assert_eq!(c.status("mouse_sens").unwrap(), "0.12");
        // Lists: case-insensitive names, stored by their own spelling.
        assert_eq!(c.exec("g_game_difficulty GD_VETERAN"), Exec::Done);
        assert_eq!(c.status("g_game_difficulty").unwrap(), "gd_veteran");
        assert_eq!(c.exec("renderer renderer_r3"), Exec::Rejected);
        // Modes: two decimal integers.
        assert_eq!(c.exec("vid_mode 1920x1080"), Exec::Done);
        assert_eq!(c.video_mode(), (1920, 1080));
        for bad in ["1920", "1920x", "x1080", "19a0x1080", "0x768"] {
            assert_eq!(c.exec(&format!("vid_mode {bad}")), Exec::Rejected, "{bad}");
        }
        assert_eq!(c.video_mode(), (1920, 1080));
    }

    #[test]
    fn a_preset_runs_its_script_even_when_the_name_is_rejected() {
        let mut c = Console::new(files());
        assert_eq!(c.exec("_preset low"), Exec::Done);
        assert_eq!(c.status("_preset").unwrap(), "Low");
        assert_eq!(c.status("r__geometry_lod").unwrap(), "0.9");
        assert_eq!(c.number("r__tf_aniso"), 2.0);
        assert_eq!(c.status("ph_iterations"), None, "only kept lines change");
        c.exec("ph_iterations 10");
        c.exec("_preset Low");
        assert_eq!(c.status("ph_iterations").unwrap(), "18");
        c.exec("r__tf_aniso 9");
        assert_eq!(c.exec("_preset Maximum"), Exec::Rejected);
        assert_eq!(c.status("_preset").unwrap(), "Low");
        assert_eq!(c.number("r__tf_aniso"), 2.0, "Low's script ran again");
        c.exec("_preset Extreme");
        assert_eq!(c.number("r__tf_aniso"), 16.0);
    }

    #[test]
    fn binding_takes_the_key_from_other_actions_except_single_and_multi() {
        let mut c = Console::startup(files(), b"");
        let k = |c: &Console, a: &str| c.keys(action_index(a).unwrap());
        assert_eq!(k(&c, "forward"), [Some("kW"), None]);
        // kF6 is on both vote (multiplayer) and quick_save (single player).
        assert_eq!(k(&c, "vote"), [Some("kF6"), None]);
        assert_eq!(k(&c, "quick_save"), [Some("kF6"), None]);
        c.exec("bind_sec forward kspace");
        assert_eq!(k(&c, "forward"), [Some("kW"), Some("kSPACE")]);
        assert_eq!(k(&c, "jump"), [None, None], "taken from jump");
        // The same action keeps both slots.
        c.exec("bind FORWARD kSPACE");
        assert_eq!(k(&c, "forward"), [Some("kSPACE"), Some("kSPACE")]);
        c.exec("bind jump kF6");
        assert_eq!(k(&c, "vote"), [None, None]);
        assert_eq!(k(&c, "quick_save"), [None, None]);
        c.exec("unbind_sec forward");
        assert_eq!(k(&c, "forward"), [Some("kSPACE"), None]);
        // Unknown names do nothing.
        assert_eq!(c.exec("bind nothing kW"), Exec::Rejected);
        assert_eq!(c.exec("bind jump kNOTHING"), Exec::Rejected);
        // unbindall loads the defaults.
        c.exec("unbindall");
        assert_eq!(k(&c, "forward"), [Some("kW"), None]);
        assert_eq!(k(&c, "jump"), [Some("kSPACE"), None]);
    }

    #[test]
    fn the_user_file_round_trips_in_byte_order() {
        let user = b"_preset Extreme\r\nai_use_torch_dynamic_lights on\r\nunbindall\r\nbind left kLEFT\r\nbind quit kESCAPE\r\nbind_console disconnect kF10\nbind_console quit kF9\nbind_sec jump kRCONTROL\r\ng_always_run on\r\nmouse_sens 0.3\r\nph_iterations 18\r\nr2_slight_fade 0.3333\r\nvid_mode 1280x1024\r\n";
        let mut c = Console::startup(files(), user);
        // Commands without a status are neither run here nor stored.
        assert_eq!(c.exec("cfg_load other.ltx"), Exec::Ignored);
        assert_eq!(
            c.exec("start server(all/single/alife/new) client(localhost)"),
            Exec::Ignored
        );
        assert_eq!(c.number("r__tf_aniso"), 16.0, "the preset ran");
        let saved = String::from_utf8(c.save()).unwrap();
        let lines: Vec<&str> = saved.split_inclusive('\n').collect();
        // Every command once, in byte order of names; `_` sorts first.
        assert_eq!(lines[0], "_preset Extreme\r\n");
        assert_eq!(lines[1], "ai_use_torch_dynamic_lights on\r\n");
        let at = |text: &str| {
            lines
                .iter()
                .position(|l| *l == text)
                .unwrap_or_else(|| panic!("{text:?}"))
        };
        let unbindall = at("unbindall\r\n");
        assert_eq!(lines[unbindall + 1], "bind left kLEFT\r\n");
        assert_eq!(
            lines[unbindall + 2],
            "bind jump kSPACE\r\n",
            "defaults, in action order"
        );
        assert_eq!(lines[unbindall + 3], "bind forward kW\r\n");
        assert!(at("bind quit kESCAPE\r\n") > unbindall);
        // bind < bind_console < bind_sec; console keys by code (F9 67, F10 68).
        assert_eq!(
            lines[at("bind_console quit kF9\n") + 1],
            "bind_console disconnect kF10\n"
        );
        assert!(at("bind_console quit kF9\n") > at("bind quit kESCAPE\r\n"));
        assert!(at("bind_sec jump kRCONTROL\r\n") > at("bind_console disconnect kF10\n"));
        assert!(at("g_always_run on\r\n") > at("bind_sec jump kRCONTROL\r\n"));
        assert!(saved.contains("mouse_sens 0.3\r\n"));
        assert!(saved.contains("r2_slight_fade 0.3333\r\n"));
        assert!(saved.contains("vid_mode 1280x1024\r\n"));
        assert!(saved.contains("ph_iterations 18\r\n"));
        // Variables the file did not set are written at their values.
        assert!(saved.contains("rs_c_gamma 1.\r\n"));
        let mut names: Vec<&str> = lines
            .iter()
            .filter(|l| !l.starts_with("bind") && !l.starts_with("unbindall"))
            .map(|l| l.split(' ').next().unwrap())
            .collect();
        let sorted = {
            let mut s = names.clone();
            s.sort();
            s
        };
        assert_eq!(names, sorted);
        names.dedup();
        assert_eq!(names.len(), sorted.len(), "each once");
        // Reading what was saved gives the same console.
        assert_eq!(Console::startup(files(), saved.as_bytes()), c);
    }
}
