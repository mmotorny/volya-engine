// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Finding and loading the game's script files
//! (docs/behavior/script-loading.md section 2): a script name's file in
//! `$game_scripts$`, the chunk the runtime compiles for it (the namespace
//! prefix), and a [`Scripts`] state that loads namespaces at startup, on
//! request and on access to an undefined global, as the original does.
//!
//! Loading runs a script's top level; nothing else of the game is called
//! here (the class registrators of `script.ltx` need the engine's API).

use std::cell::RefCell;
use std::io;
use std::rc::Rc;

use volya_engine_core::config::{self, PathAliases};
use volya_engine_core::vfs::{self, Vfs};
use volya_engine_formats::cp1251;
use volya_engine_formats::ltx::value;

use crate::{Error, Lua, Value};

/// The path alias of the scripts' directory (2.1).
pub const GAME_SCRIPTS: &str = "$game_scripts$";

/// The extension of every script file (2.1).
pub const EXTENSION: &str = ".script";

/// The name whose file is loaded into the globals, without a prefix (2.1).
pub const GLOBALS: &str = "_G";

/// Which namespace prefix chunks get (2.3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Prefix {
    /// Reads of names a namespace lacks fall back to the globals.
    #[default]
    Default,
    /// The `-_g` command-line option: no fallback, and the namespace has a
    /// field `_G` holding the globals.
    G,
}

impl Prefix {
    /// [`Prefix::G`] if the lowercased command line contains `-_g` (2.3).
    pub fn from_command_line(command_line: &str) -> Self {
        if command_line.to_ascii_lowercase().contains("-_g") {
            Prefix::G
        } else {
            Prefix::Default
        }
    }

    fn template(self) -> &'static str {
        match self {
            Prefix::Default => {
                "local function script_name() return \"%s\" end local this = {} %s this %s \
                 setmetatable(this, {__index = _G}) setfenv(1, this) \t\t"
            }
            Prefix::G => {
                "local function script_name() return \"%s\" end local this = {} this._G = _G \
                 %s this %s setfenv(1, this) \t\t"
            }
        }
    }
}

/// The path of script `name`'s file, `$game_scripts$<name>.script` (2.1),
/// lowercased as [`PathAliases`] paths are (lookups are case-insensitive).
pub fn script_path(aliases: &PathAliases, name: &str) -> io::Result<String> {
    aliases.path(GAME_SCRIPTS, &format!("{name}{EXTENSION}"))
}

/// The chunk the runtime compiles for script `name` (bytes, as Lua strings
/// are) whose file holds `file`: the file's bytes unchanged, after the
/// namespace prefix unless `name` is `_G` (2.3). `None` if a component of
/// `name` is empty: such a load fails without a message.
pub fn chunk(name: &[u8], file: &[u8], prefix: Prefix) -> Option<Vec<u8>> {
    if name == GLOBALS.as_bytes() {
        return Some(file.to_vec());
    }
    let components: Vec<&[u8]> = name.split(|&b| b == b'.').collect();
    if components.iter().any(|c| c.is_empty()) {
        return None;
    }
    let mut opening = [components[0], b"="].concat();
    for c in &components[1..] {
        opening.extend_from_slice(&[b"{", *c, b"="].concat());
    }
    let closing = b"}".repeat(components.len() - 1);
    let mut parts = prefix.template().split("%s");
    let mut text = Vec::with_capacity(file.len() + 200);
    for value in [name, &opening, &closing] {
        text.extend_from_slice(parts.next().unwrap().as_bytes());
        text.extend_from_slice(value);
    }
    text.extend_from_slice(parts.next().unwrap().as_bytes());
    text.extend_from_slice(file);
    Some(text)
}

/// The namespace an engine-called function name `A.f` loads first: the part
/// before its last `.`; `None` for a name without one, which is looked up in
/// the globals (2.2.4).
pub fn function_namespace(function: &str) -> Option<&str> {
    function.rfind('.').map(|i| &function[..i])
}

/// The `[common]` lists of `$game_config$script.ltx` (2.2.2).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Common {
    /// Function names `X.f`, each called after loading namespace `X`.
    pub class_registrators: Vec<String>,
    /// Script names to load.
    pub scripts: Vec<String>,
}

impl Common {
    /// Reads `[common]` of `$game_config$script.ltx` from `files`.
    pub fn read(files: &dyn Vfs, aliases: &PathAliases) -> io::Result<Self> {
        let script = config::load_aliased(files, aliases, "$game_config$", "script.ltx")?;
        let list = |key| -> Vec<String> {
            let Some(Some(v)) = script.get("common", key) else {
                return Vec::new();
            };
            value::list(v)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        };
        Ok(Common {
            class_registrators: list("class_registrators"),
            scripts: list("script"),
        })
    }
}

/// What went wrong loading a script. The original discards these without a
/// message (2.4) and so do scripts; they are kept for diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub name: String,
    pub kind: FailureKind,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    /// The file exists but couldn't be read.
    Read,
    /// The chunk doesn't compile; the namespace doesn't exist.
    Compile,
    /// Running the chunk raised; the namespace exists with what it defined.
    Run,
    /// The name is in use by a value that is neither nil nor a table: the
    /// original stops the game (2.2). Raised in Lua too.
    Fatal,
}

/// `text` as the game's bytes (Windows-1251, as configuration files are).
fn lua_string(text: &str) -> Value {
    Value::String(cp1251::encode(text).unwrap_or_else(|| text.as_bytes().to_vec()))
}

/// The registry key of the loader's entry function (loader.lua).
const ENTRY: &str = "volya.script_loader";

/// A game script state: a [`Lua`] state with the loader installed (2.2):
/// undefined globals load the script of their name, `prefetch(N)` loads N,
/// and [`Scripts::load_globals`] and [`Scripts::load_namespace`] load what
/// the engine asks for.
pub struct Scripts {
    lua: Lua,
    failures: Rc<RefCell<Vec<Failure>>>,
}

impl Scripts {
    /// A state reading script files from `files` (the game's file tree,
    /// paths as [`PathAliases`] resolves them).
    pub fn new(files: Rc<dyn Vfs>, aliases: &PathAliases, prefix: Prefix) -> Result<Self, Error> {
        let mut lua = Lua::new()?;
        let failures = Rc::new(RefCell::new(Vec::new()));
        let aliases = aliases.clone();
        let read_failures = Rc::clone(&failures);
        lua.register("__volya_script_chunk", move |args| {
            let [Value::String(bytes)] = args else {
                return Err(format!("script name expected, got {args:?}"));
            };
            let name = cp1251::decode(bytes);
            let not_loaded = |why: &str| Ok(vec![Value::Nil, why.into()]);
            let path = match script_path(&aliases, &name) {
                Ok(path) => path,
                Err(e) => return Err(e.to_string()),
            };
            let file = match vfs::read(&*files, &path) {
                Ok(file) => file,
                Err(e) if e.kind() == io::ErrorKind::NotFound => return not_loaded("missing"),
                Err(e) => {
                    read_failures.borrow_mut().push(Failure {
                        name,
                        kind: FailureKind::Read,
                        message: format!("{path}: {e}"),
                    });
                    return not_loaded("unreadable");
                }
            };
            match chunk(bytes, &file, prefix) {
                Some(chunk) => Ok(vec![
                    Value::String(chunk),
                    format!("@{path}").as_str().into(),
                ]),
                None => not_loaded("invalid"),
            }
        })?;
        let reported = Rc::clone(&failures);
        lua.register("__volya_script_report", move |args| {
            let text = |v: &Value| match v {
                Value::String(s) => String::from_utf8_lossy(s).into_owned(),
                v => format!("{v:?}"),
            };
            let [kind, name, message] = args else {
                return Err(format!("failure expected, got {args:?}"));
            };
            let kind = match kind.as_str() {
                Some("compile") => FailureKind::Compile,
                Some("run") => FailureKind::Run,
                Some("fatal") => FailureKind::Fatal,
                _ => return Err(format!("unknown failure {kind:?}")),
            };
            reported.borrow_mut().push(Failure {
                name: text(name),
                kind,
                message: text(message),
            });
            Ok(Vec::new())
        })?;
        lua.exec_kept("=script_loader", include_bytes!("loader.lua"), ENTRY)?;
        Ok(Scripts { lua, failures })
    }

    /// Startup (2.2.1): loads `_G`, `$game_scripts$_G.script`, into the
    /// globals, whether or not it is already loaded. The engine calls this
    /// after registering its script classes.
    pub fn load_globals(&mut self) -> Result<(), Error> {
        self.lua.call_kept(ENTRY, &["globals".into()]).map(drop)
    }

    /// Loads namespace `name` unless it is already loaded, as for an
    /// engine-called function's namespace or `prefetch` (2.2.4, 2.2.5). A
    /// missing file loads nothing. `Err` only for the fatal error.
    pub fn load_namespace(&mut self, name: &str) -> Result<(), Error> {
        self.lua
            .call_kept(ENTRY, &["namespace".into(), lua_string(name)])
            .map(drop)
    }

    /// The state, for running and calling scripts.
    pub fn lua(&mut self) -> &mut Lua {
        &mut self.lua
    }

    /// The failures since the last call, oldest first.
    pub fn take_failures(&mut self) -> Vec<Failure> {
        std::mem::take(&mut self.failures.borrow_mut())
    }
}
