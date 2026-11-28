// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Finding and loading the game's script files
//! (docs/behavior/script-loading.md section 2): a script name's file in
//! `$game_scripts$`, the chunk the runtime compiles for it (the namespace
//! prefix), and a [`Scripts`] state that loads namespaces at startup, on
//! request and on access to an undefined global, as the original does.
//!
//! Loading runs a script's top level; the class registrators of
//! `script.ltx` are called by [`crate::object_factory`].

use std::cell::RefCell;
use std::io;
use std::rc::Rc;

use volya_engine_core::config::{self, PathAliases};
use volya_engine_core::vfs::{self, Vfs};
use volya_engine_formats::cp1251;
use volya_engine_formats::ltx::value;

use crate::classes::{Conversion, EngineClass, EnginePart, OverrideFailure};
use crate::object_factory::{self, ObjectFactory};
use crate::{Error, ErrorKind, Lua, Value};

/// The path alias of the scripts' directory (2.1).
pub const GAME_SCRIPTS: &str = "$game_scripts$";

/// The extension of every script file (2.1).
pub const EXTENSION: &str = ".script";

/// The name whose file is loaded into the globals, without a prefix (2.1).
/// Compared case-sensitively (our choice).
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
    /// `game_type_clsid_factory`: the game-type function's name
    /// (docs/behavior/script-api-engine-entry-points.md 2.2).
    pub game_type_function: Option<String>,
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
            game_type_function: script
                .get("common", "game_type_clsid_factory")
                .flatten()
                .map(|v| v.trim().to_owned()),
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
    /// The file's path couldn't be made, or the file exists but couldn't be
    /// read (the trees unmounted, for one).
    Read,
    /// The chunk doesn't compile; the namespace doesn't exist.
    Compile,
    /// Running the chunk raised; the namespace exists with what it defined.
    Run,
    /// The name is in use by a value that is neither nil nor a table: the
    /// original stops the game (2.2), and the state stops (see [`Scripts`]).
    Fatal,
}

/// The game's file trees, lent for each read as the platforms lend them
/// (the web platform's `with_trees`): the combined archive tree, then the game
/// root with its loose files. Implementations own or borrow nothing they
/// can't keep, as [`Scripts`] holds them for its lifetime.
pub trait Trees {
    /// Runs `f` on the trees; `Err` when they are unavailable (unmounted).
    fn with_trees(&self, f: &mut dyn FnMut(&dyn Vfs, &dyn Vfs)) -> io::Result<()>;
}

/// `text` as the game's bytes (Windows-1251, as configuration files are).
fn lua_string(text: &str) -> Value {
    Value::String(cp1251::encode(text).unwrap_or_else(|| text.as_bytes().to_vec()))
}

/// The registry key of the loader's entry function (loader.lua: ENTRY).
const ENTRY: &str = "volya.script_loader";

/// Why a chunk wasn't read, for Lua (`missing`, `unreadable`, `invalid`),
/// and a failure to keep.
type NotRead = (&'static str, Option<String>);

/// Reads script `name`'s chunk: the chunk and its name, or why not.
fn read_chunk(
    trees: &dyn Trees,
    aliases: &PathAliases,
    prefix: Prefix,
    bytes: &[u8],
) -> Result<(Vec<u8>, String), NotRead> {
    let name = cp1251::decode(bytes);
    let path = script_path(aliases, &name).map_err(|e| ("unreadable", Some(e.to_string())))?;
    // Scripts are files of the archive tree (2.1; db-archive.md precedence).
    let mut file = Err(io::Error::other("the game's files are unavailable"));
    let lent = trees.with_trees(&mut |tree, _root| file = vfs::read(tree, &path));
    let file = match lent.and(file) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(("missing", None)),
        Err(e) => return Err(("unreadable", Some(format!("{path}: {e}")))),
    };
    // Our choice: the chunk is named after the path as [`PathAliases`]
    // makes it (lowercase, relative to the game root), not the original's.
    let chunk = chunk(bytes, &file, prefix).ok_or(("invalid", None))?;
    Ok((chunk, format!("@{path}")))
}

/// A game script state: a [`Lua`] state with the loader installed (2.2):
/// undefined globals load the script of their name, `prefetch(N)` loads N,
/// and [`Scripts::load_globals`] and [`Scripts::load_namespace`] load what
/// the engine asks for.
///
/// The fatal error (2.2) stops the game in the original; Lua 5.1 can't
/// unwind past a script's `pcall`, so (our choice) the call during which it
/// happened returns [`ErrorKind::Fatal`] whatever scripts did with it, and
/// so does every later call: the state is stopped. In Lua, every later load
/// raises it again.
///
/// Holds its [`Trees`] for its lifetime: drop it before the platform
/// unmounts the game's files (reads after that fail as
/// [`FailureKind::Read`]).
pub struct Scripts {
    lua: Lua,
    failures: Rc<RefCell<Vec<Failure>>>,
    /// The fatal error's message, once raised.
    pub(crate) fatal: Rc<RefCell<Option<String>>>,
    /// The object factory, made on first use ([`crate::object_factory`]).
    pub(crate) factory: Rc<RefCell<Option<ObjectFactory>>>,
    /// The game object basics, once registered ([`crate::game_object`]).
    pub(crate) game_object_state: Option<crate::game_object::Registered>,
    /// The story state, once registered ([`crate::story`]).
    pub(crate) story_state: Option<crate::story::Registered>,
    /// The dialogs and trade, once registered ([`crate::dialogs`]).
    pub(crate) dialogs_state: Option<crate::dialogs::Registered>,
    /// The sound, once registered ([`crate::sound`]).
    pub(crate) sound_state: Option<crate::sound::Registered>,
    /// The simulation registry, once registered ([`crate::simulation`]).
    pub(crate) simulation_state: Option<crate::simulation::Registered>,
    /// The simulation objects, once registered
    /// ([`crate::simulation_objects`]).
    pub(crate) simulation_objects_state: Option<crate::simulation_objects::Registered>,
    /// The scripted actions, once registered ([`crate::scripted_actions`]).
    pub(crate) scripted_actions_state: Option<crate::scripted_actions::Registered>,
    /// The inventory, once registered ([`crate::inventory`]).
    pub(crate) inventory_state: Option<crate::inventory::Registered>,
    /// The level, once registered ([`crate::level`]).
    pub(crate) level_state: Option<crate::level::Registered>,
}

impl Scripts {
    /// A state reading script files from `trees` (paths as `aliases`
    /// resolves them).
    pub fn new(trees: Rc<dyn Trees>, aliases: &PathAliases, prefix: Prefix) -> Result<Self, Error> {
        let mut lua = Lua::new()?;
        // The binding library, opened before `_G` loads (classes spec 1.1).
        lua.open_classes()?;
        // The value types and bit functions (script API E).
        crate::values::register(&mut lua)?;
        let failures = Rc::new(RefCell::new(Vec::new()));
        let fatal = Rc::new(RefCell::new(None));
        let aliases = aliases.clone();
        let read_failures = Rc::clone(&failures);
        lua.register("__volya_script_chunk", move |args| {
            let [Value::String(bytes)] = args else {
                return Err(format!("script name expected, got {args:?}"));
            };
            match read_chunk(&*trees, &aliases, prefix, bytes) {
                Ok((chunk, name)) => Ok(vec![Value::String(chunk), name.as_str().into()]),
                Err((why, message)) => {
                    if let Some(message) = message {
                        read_failures.borrow_mut().push(Failure {
                            name: cp1251::decode(bytes),
                            kind: FailureKind::Read,
                            message,
                        });
                    }
                    Ok(vec![Value::Nil, why.into()])
                }
            }
        })?;
        let reported = Rc::clone(&failures);
        let reported_fatal = Rc::clone(&fatal);
        lua.register("__volya_script_report", move |args| {
            let text = |v: &Value| match v {
                Value::String(s) => cp1251::decode(s),
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
            if kind == FailureKind::Fatal {
                *reported_fatal.borrow_mut() = Some(text(message));
            }
            reported.borrow_mut().push(Failure {
                name: text(name),
                kind,
                message: text(message),
            });
            Ok(Vec::new())
        })?;
        lua.register_kept_factory("__volya_kept")?;
        lua.exec_kept("=script_loader", include_bytes!("loader.lua"), ENTRY)?;
        let factory = Rc::new(RefCell::new(None));
        object_factory::install(&mut lua, Rc::clone(&factory))?;
        Ok(Scripts {
            lua,
            failures,
            fatal,
            factory,
            game_object_state: None,
            story_state: None,
            dialogs_state: None,
            sound_state: None,
            simulation_state: None,
            simulation_objects_state: None,
            scripted_actions_state: None,
            inventory_state: None,
            level_state: None,
        })
    }

    /// Runs `f` on the state unless it is stopped; the fatal error if it
    /// was raised before or during `f`.
    pub(crate) fn guarded<T>(
        &mut self,
        f: impl FnOnce(&mut Lua) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let stopped = |fatal: &RefCell<Option<String>>| {
            fatal.borrow().clone().map(|message| Error {
                kind: ErrorKind::Fatal,
                message,
            })
        };
        if let Some(e) = stopped(&self.fatal) {
            return Err(e);
        }
        let result = f(&mut self.lua);
        match stopped(&self.fatal) {
            Some(e) => Err(e),
            None => result,
        }
    }

    /// Startup (2.2.1): loads `_G`, `$game_scripts$_G.script`, into the
    /// globals, whether or not it is already loaded. The engine calls this
    /// after registering its script classes.
    pub fn load_globals(&mut self) -> Result<(), Error> {
        self.guarded(|lua| lua.call_kept(ENTRY, &["globals".into()]).map(drop))
    }

    /// Loads namespace `name` unless it is already loaded, as for an
    /// engine-called function's namespace or `prefetch` (2.2.4, 2.2.5). A
    /// missing file loads nothing; other failures are kept
    /// ([`Scripts::take_failures`]). `Err` only for the fatal error.
    pub fn load_namespace(&mut self, name: &str) -> Result<(), Error> {
        self.guarded(|lua| {
            lua.call_kept(ENTRY, &["namespace".into(), lua_string(name)])
                .map(drop)
        })
    }

    /// [`Lua::exec`] on the state (which may load scripts).
    pub fn exec(&mut self, name: &str, code: &[u8]) -> Result<Vec<Value>, Error> {
        self.guarded(|lua| lua.exec(name, code))
    }

    /// [`Lua::call`] on the state (which may load scripts).
    pub fn call(&mut self, function: &str, args: &[Value]) -> Result<Vec<Value>, Error> {
        self.guarded(|lua| lua.call(function, args))
    }

    /// [`Lua::register_class`] on the state: the engine registers its
    /// classes before [`Scripts::load_globals`].
    pub fn register_class(&mut self, class: EngineClass) -> Result<(), Error> {
        self.guarded(|lua| lua.register_class(class))
    }

    /// [`Lua::dispatch`] on the state (which may load scripts). A failing
    /// override stops the state as the loader's fatal error does: the
    /// original stops the game (classes spec 5.3).
    pub fn dispatch(
        &mut self,
        part: EnginePart,
        method: &str,
        args: &[Value],
        conversion: Conversion,
    ) -> Result<Value, OverrideFailure> {
        let result = self.guarded(|lua| Ok(lua.dispatch(part, method, args, conversion)))?;
        if let Err(failure) = &result
            && failure.error.kind == ErrorKind::Fatal
        {
            *self.fatal.borrow_mut() = Some(failure.error.message.clone());
        }
        result
    }

    /// [`Lua::with_part`] on the state.
    pub fn with_part<T: std::any::Any, R>(
        &self,
        part: EnginePart,
        f: impl FnOnce(&mut T) -> R,
    ) -> Option<R> {
        self.lua.with_part(part, f)
    }

    /// The state, for the engine's registrations ([`crate::services`]).
    pub(crate) fn lua_mut(&mut self) -> &mut Lua {
        &mut self.lua
    }

    /// The fatal error's message once raised: an engine function setting it
    /// stops the state as the loader's fatal error does.
    pub(crate) fn fatal_flag(&self) -> Rc<RefCell<Option<String>>> {
        Rc::clone(&self.fatal)
    }

    /// The failures since the last call, oldest first.
    pub fn take_failures(&mut self) -> Vec<Failure> {
        std::mem::take(&mut self.failures.borrow_mut())
    }
}
