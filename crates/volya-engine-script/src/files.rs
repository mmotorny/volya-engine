// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Configuration and files (script API F,
//! docs/behavior/script-api-configuration-and-files.md; section numbers are
//! its): the configuration objects `ini_file` (`system_ini`, `game_ini`,
//! `ini_file(name)`, `create_ini_file`) and their readers, the token
//! classes `token`, `token_list` and `rtoken_list`, the file system
//! `getFS()` with `fs_file`, `FS_file_list`, `FS_file_list_ex` and
//! `FS_item`, and `command_line` and `user_name`.
//!
//! What scripts read and change is [`FileSystem`]: the path aliases, the
//! index of files ([`FileIndex`]: the archive tree under the loose files the
//! engine adds, and what scripts changed), the engine's configurations and
//! the local time zone. Configurations load with
//! [`volya_engine_core::config`] over the index. Changes to the disk are
//! kept for the platform ([`FileIndex::take_disk_changes`]).
//! [`Scripts::register_configuration_and_files`] registers the names on a
//! game script state. See docs/architecture/scripts.md, Configuration and
//! files.

pub mod index;

use std::cell::RefCell;
use std::rc::Rc;

use volya_engine_core::config::{self, Config, PathAliases, Section, value};
use volya_engine_formats::cp1251;

use crate::classes::{EngineClass, ObjectMaker, Operand};
use crate::loader::{Scripts, Trees};
use crate::object_factory::{self, ObjectFactory};
use crate::services::{EngineServices, Fatal, report_fatal};
use crate::values::{Vector, to_bits32};
use crate::{Error, Value};

pub use index::{DiskChange, FileEntry, FileIndex, LOOSE, LooseFile, LooseFiles};

/// The alias configuration files are found under (2.2, 2.3).
pub const GAME_CONFIG: &str = "$game_config$";

/// The buffer full paths are built in (2.2, 5.1): a path this long or
/// longer is fatal.
pub const PATH_BUFFER: usize = 520;

/// The buffer `r_string_wq` copies a value into (3.2).
pub const UNQUOTE_BUFFER: usize = 512;

/// The buffer the lowercased section name is copied into (review gap 2).
pub const SECTION_BUFFER: usize = 256;

/// The file-list flags (5.1).
pub const FS_LIST_FILES: i32 = 1;
pub const FS_LIST_FOLDERS: i32 = 2;
pub const FS_CLAMP_EXT: i32 = 4;
pub const FS_ROOT_ONLY: i32 = 8;

/// `user_name()` (6): the fixed user name of the game's log file, as a
/// browser has no Windows account (#646).
pub const USER_NAME: &str = "player";

/// A moment in local time, as the C library's broken-down time gives it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CivilTime {
    pub year: i64,
    /// 1 … 12.
    pub month: u32,
    /// 1 … 31.
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub second: u32,
    /// 0 is Sunday.
    pub weekday: u32,
}

impl CivilTime {
    /// The UTC time `seconds` after 1970-01-01 00:00:00 UTC.
    pub fn utc(seconds: i64) -> Self {
        let days = seconds.div_euclid(86_400);
        let rest = seconds.rem_euclid(86_400) as u32;
        // Days to a civil date (the proleptic Gregorian calendar).
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        let year = yoe + era * 400 + i64::from(month <= 2);
        CivilTime {
            year,
            month,
            day,
            hour: rest / 3600,
            minute: rest / 60 % 60,
            second: rest % 60,
            weekday: (days + 4).rem_euclid(7) as u32,
        }
    }

    /// The C library's `asctime` text (5.3): `Www Mmm DD hh:mm:ss YYYY` and
    /// a line feed, the day in two digits as the spec's example has it.
    pub fn asctime(self) -> String {
        const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        const MONTHS: [&str; 12] = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        format!(
            "{} {} {:02} {:02}:{:02}:{:02} {}\n",
            DAYS[self.weekday as usize % 7],
            MONTHS[(self.month as usize + 11) % 12],
            self.day,
            self.hour,
            self.minute,
            self.second,
            self.year
        )
    }

    /// `ModifDigitOnly()`'s text (5.8): `DD:MM:YYYY hh:mm`.
    pub fn digits(self) -> String {
        format!(
            "{:02}:{:02}:{:4} {:02}:{:02}",
            self.day, self.month, self.year, self.hour, self.minute
        )
    }
}

/// The local time zone: the local time of a moment, seconds since
/// 1970-01-01 UTC.
pub type LocalTime = Box<dyn Fn(i64) -> CivilTime>;

/// The engine state configuration and files read and change.
pub struct FileSystem {
    /// The path aliases (LTX 10); `append_path` adds to them.
    pub aliases: PathAliases,
    pub index: FileIndex,
    /// `system_ini()`: `config\system.ltx` (2.1).
    pub system: Rc<Config>,
    /// `game_ini()`: `config\game.ltx` (2.1).
    pub game: Rc<Config>,
    /// `command_line()` (6): Volya Engine has no command line, so empty
    /// unless the engine sets one.
    pub command_line: String,
    /// `user_name()` (6).
    pub user_name: String,
    /// The local time zone (5.3, 5.8): UTC unless the platform gives its
    /// own.
    pub local_time: LocalTime,
}

impl FileSystem {
    /// The file system over the archive tree `trees` lends, with `aliases`
    /// and the engine's configurations `system` and `game`.
    pub fn new(
        trees: Rc<dyn Trees>,
        aliases: PathAliases,
        system: Rc<Config>,
        game: Rc<Config>,
    ) -> Self {
        FileSystem {
            aliases,
            index: FileIndex::new(trees),
            system,
            game,
            command_line: String::new(),
            user_name: USER_NAME.to_owned(),
            local_time: Box::new(CivilTime::utc),
        }
    }

    fn local(&self, seconds: u32) -> CivilTime {
        (self.local_time)(i64::from(seconds))
    }
}

/// A configuration object's part (2): read-only, shared by every object
/// holding it.
#[derive(Clone)]
pub struct IniFile(pub Rc<Config>);

/// A `token` (4.1). Assigning `name` copies the text (our choice: 4.1 has
/// the original keep a reference whose reading after collection is
/// unestablished).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Token {
    pub name: Option<Vec<u8>>,
    pub id: i32,
}

/// A `token_list` (4.2): its (name, id) entries.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TokenList(pub Vec<(Vec<u8>, i32)>);

/// An `rtoken_list` (4.3).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RtokenList(pub Vec<Vec<u8>>);

/// The file system object's part: the file system is [`FileSystem`].
struct FsObject;

/// An `fs_file` (5.3): a copy of the index's entry with its full path
/// (our choice: the original's is the entry itself).
#[derive(Clone, Debug, PartialEq)]
pub struct FsFile {
    pub name: String,
    pub entry: FileEntry,
}

/// An `FS_file_list` (5.6): its names, or no list.
#[derive(Clone, Debug, PartialEq)]
pub struct FileList(pub Option<Vec<Vec<u8>>>);

/// An item of an `FS_file_list_ex`, an `FS_item` (5.7, 5.8).
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub name: Vec<u8>,
    pub size: u32,
    pub modif: u32,
}

/// An `FS_file_list_ex` (5.7).
#[derive(Clone, Debug, PartialEq)]
pub struct FileListEx(pub Vec<Item>);

/// What the functions share: the file system, the services (whose console
/// logs fatal errors), the state's fatal error and its object factory.
#[derive(Clone)]
struct Context {
    files: Rc<RefCell<FileSystem>>,
    services: Rc<RefCell<EngineServices>>,
    fatal: Rc<RefCell<Option<String>>>,
    factory: Rc<RefCell<Option<ObjectFactory>>>,
}

impl Context {
    fn stopped(&self) -> Result<(), String> {
        match &*self.fatal.borrow() {
            Some(message) => Err(message.clone()),
            None => Ok(()),
        }
    }

    /// A fatal error (CL 3.8): reported and the state stopped. The error.
    fn fatal(
        &self,
        function: &str,
        line: u32,
        expression: &str,
        description: &str,
        arguments: &[&str],
    ) -> String {
        report_fatal(
            &self.services,
            &self.fatal,
            Fatal {
                function,
                file: file!(),
                line,
                expression,
                description,
                arguments,
            },
        )
    }

    /// A fatal error whose description is `description` (2.4): the
    /// expression is unestablished, ours `fatal error`.
    fn check(&self, function: &str, line: u32, description: &str) -> String {
        self.fatal(function, line, "fatal error", description, &[])
    }

    /// The run-time library's handler for a failed bounded string
    /// operation or an invalid argument (spec B 2.3): its report's rows as
    /// [`crate::services`] fills them.
    fn handler(&self, function: &str, line: u32) -> String {
        self.fatal(
            function,
            line,
            "fatal error",
            "<no expression>",
            &["error handler is invoked!"],
        )
    }

    /// A fatal error standing for a fault or crash the spec believes (our
    /// wording).
    fn fault(&self, function: &str, line: u32, what: &str) -> String {
        self.fatal(function, line, "fatal error", "<no expression>", &[what])
    }

    /// The full path of `(alias, name)` (5.1): the alias's directory and
    /// `name`, lowercased. An undefined alias is a failed check whose
    /// description is the alias (the checked condition is unestablished:
    /// ours); a path of [`PATH_BUFFER`] bytes or more is the handler's.
    fn full_path(
        &self,
        function: &str,
        line: u32,
        alias: &[u8],
        name: &[u8],
    ) -> Result<String, String> {
        let alias = cp1251::decode(alias);
        let dir = self.files.borrow().aliases.dir(&alias).map(str::to_owned);
        let dir = dir.map_err(|_| self.fatal(function, line, "alias defined", &alias, &[]))?;
        let path = format!("{dir}{}", cp1251::decode(name)).to_ascii_lowercase();
        self.limit(function, line, path)
    }

    /// A full path a member takes as given (5.1).
    // TODO(#994): gap 3, the simplest reading: these copy the path into the
    // same 520-byte buffer.
    fn given_path(&self, function: &str, line: u32, path: &[u8]) -> Result<String, String> {
        self.limit(function, line, cp1251::decode(path))
    }

    fn limit(&self, function: &str, line: u32, path: String) -> Result<String, String> {
        if text_len(&path) >= PATH_BUFFER {
            return Err(self.handler(function, line));
        }
        Ok(path)
    }

    /// `(alias, name)` or, with an empty `alias`, `name` as a full path
    /// (5.4).
    fn path_or_full(
        &self,
        function: &str,
        line: u32,
        alias: &[u8],
        name: &[u8],
    ) -> Result<String, String> {
        if alias.is_empty() {
            self.given_path(function, line, name)
        } else {
            self.full_path(function, line, alias, name)
        }
    }
}

/// `text`'s length in the game's bytes.
fn text_len(text: &str) -> usize {
    cp1251::encode(text).map_or(text.len(), |b| b.len())
}

/// A text for Lua: Windows-1251 bytes.
fn lua_text(text: &str) -> Value {
    Value::String(cp1251::encode(text).unwrap_or_else(|| text.as_bytes().to_vec()))
}

/// The text argument `args[i]` of `what` (our wording for a mismatch, the
/// binding library's rule, A).
fn text<'a>(args: &'a [Value], i: usize, what: &str) -> Result<&'a [u8], String> {
    match args.get(i) {
        Some(Value::String(s)) => Ok(s),
        _ => Err(format!("{what}: text expected")),
    }
}

fn number(args: &[Value], i: usize, what: &str) -> Result<f64, String> {
    match args.get(i) {
        Some(Value::Number(n)) => Ok(*n),
        _ => Err(format!("{what}: number expected")),
    }
}

/// A boolean argument; a number counts as true unless 0 (`dir_delete`'s
/// `remove_files`, `file_rename`'s `overwrite`).
fn flag(args: &[Value], i: usize, what: &str) -> Result<bool, String> {
    match args.get(i) {
        Some(Value::Boolean(b)) => Ok(*b),
        Some(Value::Number(n)) => Ok(*n != 0.0),
        _ => Err(format!("{what}: boolean expected")),
    }
}

/// An index argument by the E8 conversion.
fn index(args: &[Value], i: usize, what: &str) -> Result<i32, String> {
    number(args, i, what).map(to_bits32)
}

fn no_form(what: &str) -> String {
    format!("{what}: no form of it takes these arguments")
}

// --- Configuration objects (2, 3) ---------------------------------------------

/// Loads a configuration from the index: the root file at full path
/// `path` (missing: no sections, LTX 8), or `text` with includes under
/// `base`. A failure is fatal (LTX 11), its message the argument of a fatal
/// message (our choice of rows).
fn load_config(
    c: &Context,
    function: &str,
    source: Result<&str, (&[u8], &str)>,
) -> Result<Config, String> {
    let files = c.files.borrow();
    let tree = index::IndexVfs(&files.index);
    let loaded = match source {
        Ok(path) => config::load(&tree, path),
        Err((text, base)) => config::parse_in(&tree, text, base),
    };
    drop(files);
    loaded.map_err(|e| {
        c.fatal(
            function,
            line!(),
            "fatal error",
            "<no expression>",
            &[&e.to_string()],
        )
    })
}

/// The section a reader needs (2.4, 3): looked up by its lowercased name;
/// a missing one is fatal.
fn section<'a>(
    c: &Context,
    config: &'a Config,
    name: &[u8],
    function: &str,
) -> Result<Section<'a>, String> {
    // TODO(#994): gap 2, the simplest reading: a name of 256 bytes or more
    // fails the bounded copy (spec B 2.3) before the lookup.
    if name.len() >= SECTION_BUFFER {
        return Err(c.handler(function, line!()));
    }
    let text = cp1251::decode(name);
    config
        .section(&text)
        .ok_or_else(|| c.check(function, line!(), &format!("Can't open section '{text}'")))
}

/// The stored value of `key` in `section` (`None`: no value); a missing
/// section or key is fatal (2.4).
fn stored<'a>(
    c: &Context,
    config: &'a Config,
    args: &[Value],
    function: &str,
) -> Result<Option<&'a str>, String> {
    let name = text(args, 0, function)?;
    let key = cp1251::decode(text(args, 1, function)?);
    let found = section(c, config, name, function)?;
    found.get(&key).ok_or_else(|| {
        c.check(
            function,
            line!(),
            &format!("Can't find variable {key} in [{}]", cp1251::decode(name)),
        )
    })
}

/// A value a number reader needs: no value is the handler's (3.3).
fn some_value<'a>(c: &Context, value: Option<&'a str>, function: &str) -> Result<&'a str, String> {
    value.ok_or_else(|| c.handler(function, line!()))
}

/// `sscanf(value, "%f,%f,%f")` (3.5): reading stops at the first component
/// that doesn't parse; it and those after it are 0.
pub fn scan_vector(value: &str) -> [f32; 3] {
    let mut out = [0.0; 3];
    let mut rest = value;
    for (i, slot) in out.iter_mut().enumerate() {
        if i > 0 {
            match rest.strip_prefix(',') {
                Some(r) => rest = r,
                None => break,
            }
        }
        match value::scan_double(rest) {
            Some((v, r)) => {
                *slot = v as f32;
                rest = r;
            }
            None => break,
        }
    }
    out
}

/// `r_bool` (3.4): the first 7 bytes lowercased are `on`, `yes`, `true` or
/// `1` and the value ends there.
pub fn read_bool(value: &str) -> bool {
    text_len(value) < 7 && value::bool(value)
}

/// `ini_file` (2.2, 3).
fn ini_class(ctx: &Context) -> EngineClass {
    let c = ctx.clone();
    let mut class = EngineClass::new("ini_file")
        .constructor(
            1,
            |args| matches!(args.first(), Some(Value::String(_))).then_some(0),
            move |_, args| {
                c.stopped()?;
                let name = text(args, 0, "ini_file")?;
                let path =
                    c.full_path("CInifile::CInifile", line!(), GAME_CONFIG.as_bytes(), name)?;
                Ok(IniFile(Rc::new(load_config(
                    &c,
                    "CInifile::Load",
                    Ok(&path),
                )?)))
            },
        )
        .constructible();

    // 3.1.
    let c = ctx.clone();
    class = class.method("section_exist", move |ini: &mut IniFile, args| {
        c.stopped()?;
        let name = cp1251::decode(text(args, 0, "ini_file.section_exist")?);
        Ok(vec![ini.0.contains(&name).into()])
    });
    let c = ctx.clone();
    class = class.method("line_exist", move |ini: &mut IniFile, args| {
        c.stopped()?;
        let what = "ini_file.line_exist";
        let name = text(args, 0, what)?;
        let key = cp1251::decode(text(args, 1, what)?);
        if !ini.0.contains(&cp1251::decode(name)) {
            return Ok(vec![false.into()]);
        }
        let found = section(&c, &ini.0, name, "CInifile::line_exist")?;
        Ok(vec![found.contains(&key).into()])
    });
    let c = ctx.clone();
    class = class.method("line_count", move |ini: &mut IniFile, args| {
        c.stopped()?;
        let name = text(args, 0, "ini_file.line_count")?;
        let found = section(&c, &ini.0, name, "CInifile::line_count")?;
        Ok(vec![(found.len() as f64).into()])
    });

    // 3.2.
    let c = ctx.clone();
    class = class.method("r_string", move |ini: &mut IniFile, args| {
        c.stopped()?;
        let value = stored(&c, &ini.0, args, "CInifile::r_string")?;
        Ok(vec![value.map_or(Value::Nil, lua_text)])
    });
    let c = ctx.clone();
    class = class.method("r_string_wq", move |ini: &mut IniFile, args| {
        c.stopped()?;
        let function = "CInifile::r_string_wb";
        let Some(value) = stored(&c, &ini.0, args, function)? else {
            return Ok(vec![Value::Nil]);
        };
        if text_len(value) >= UNQUOTE_BUFFER {
            return Err(c.handler(function, line!()));
        }
        Ok(vec![lua_text(value::unquote(value))])
    });

    // 3.3.
    for (name, signed) in [("r_s32", true), ("r_u32", false)] {
        let c = ctx.clone();
        class = class.method(name, move |ini: &mut IniFile, args| {
            c.stopped()?;
            let function = "CInifile::r_s32";
            let value = some_value(&c, stored(&c, &ini.0, args, function)?, function)?;
            // TODO(#994): gap 1, an integer outside the signed 32-bit range:
            // the simplest reading, its low 32 bits.
            let bits = value::int(value) as i32;
            let n = if signed {
                f64::from(bits)
            } else {
                f64::from(bits as u32)
            };
            Ok(vec![n.into()])
        });
    }
    let c = ctx.clone();
    class = class.method("r_float", move |ini: &mut IniFile, args| {
        c.stopped()?;
        let function = "CInifile::r_float";
        let value = some_value(&c, stored(&c, &ini.0, args, function)?, function)?;
        Ok(vec![f64::from(value::float(value)).into()])
    });

    // 3.4.
    let c = ctx.clone();
    class = class.method("r_bool", move |ini: &mut IniFile, args| {
        c.stopped()?;
        let function = "CInifile::r_bool";
        match stored(&c, &ini.0, args, function)? {
            Some(value) => Ok(vec![read_bool(value).into()]),
            // Believed a fault (3.4): ours, a fatal error.
            None => Err(c.fault(function, line!(), "r_bool of an entry with no value")),
        }
    });

    // 3.5.
    let c = ctx.clone();
    class = class.method_with_objects("r_vector", move |ini: &mut IniFile, objects, args| {
        c.stopped()?;
        let function = "CInifile::r_fvector3";
        let value = some_value(&c, stored(&c, &ini.0, args, function)?, function)?;
        let [x, y, z] = scan_vector(value);
        Ok(vec![objects.new_object("vector", Vector::new(x, y, z))])
    });

    // 3.6.
    let c = ctx.clone();
    class = class.method("r_clsid", move |ini: &mut IniFile, args| {
        c.stopped()?;
        let function = "CInifile::r_clsid";
        let value = some_value(&c, stored(&c, &ini.0, args, function)?, function)?;
        let bytes = cp1251::encode(value).unwrap_or_else(|| value.as_bytes().to_vec());
        let id = object_factory::class_id(&bytes);
        let n = c
            .factory
            .borrow_mut()
            .get_or_insert_with(ObjectFactory::new)
            .number(id);
        Ok(vec![(n as f64).into()])
    });
    let c = ctx.clone();
    class = class.method_with_objects("r_token", move |ini: &mut IniFile, objects, args| {
        c.stopped()?;
        let function = "CInifile::r_token";
        let list = match args.get(2).and_then(|v| objects.get::<TokenList>(v)) {
            Some(Operand::Other(list)) => list,
            _ => return Err(no_form("ini_file.r_token")),
        };
        let value = stored(&c, &ini.0, args, function)?;
        if list.0.is_empty() {
            return Ok(vec![0.0.into()]);
        }
        let value = some_value(&c, value, function)?;
        let bytes = cp1251::encode(value).unwrap_or_default();
        let id = list
            .0
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(&bytes))
            .map_or(0, |(_, id)| *id);
        Ok(vec![f64::from(id).into()])
    });

    // 3.7.
    let c = ctx.clone();
    class.method("r_line", move |ini: &mut IniFile, args| {
        c.stopped()?;
        let what = "ini_file.r_line";
        let name = text(args, 0, what)?;
        let i = index(args, 1, what)?;
        text(args, 2, what)?;
        text(args, 3, what)?;
        let found = section(&c, &ini.0, name, "CInifile::r_line")?;
        let entry = usize::try_from(i).ok().and_then(|i| found.entries().nth(i));
        Ok(match entry {
            Some(e) => vec![
                true.into(),
                lua_text(e.key()),
                lua_text(e.value().unwrap_or("")),
            ],
            None => vec![false.into(), "".into(), "".into()],
        })
    })
}

// --- Token classes (4) ---------------------------------------------------------

fn token_classes() -> [EngineClass; 3] {
    let token = EngineClass::new("token")
        .constructor(0, |_| Some(0), |_, _| Ok(Token::default()))
        .constructible()
        .writable_property(
            "name",
            |t: &Token| t.name.clone().map_or(Value::Nil, Value::String),
            |t: &mut Token, v| {
                t.name = match v {
                    Value::String(s) => Some(s.clone()),
                    Value::Nil => None,
                    _ => return Err("token.name: text expected".into()),
                };
                Ok(())
            },
        )
        .writable_property(
            "id",
            |t: &Token| f64::from(t.id).into(),
            |t: &mut Token, v| {
                t.id = to_bits32(v.as_number().ok_or("token.id: number expected")?);
                Ok(())
            },
        );

    let list = EngineClass::new("token_list")
        .constructor(0, |_| Some(0), |_, _| Ok(TokenList::default()))
        .constructible()
        .method("add", |l: &mut TokenList, args| {
            let name = text(args, 0, "token_list.add")?.to_vec();
            let id = index(args, 1, "token_list.add")?;
            l.0.push((name, id));
            Ok(Vec::new())
        })
        .method("remove", |l: &mut TokenList, args| {
            let name = text(args, 0, "token_list.remove")?;
            match l.0.iter().position(|(n, _)| n == name) {
                Some(i) => {
                    l.0.remove(i);
                    Ok(Vec::new())
                }
                // Believed memory corruption or a crash (4.2): ours, an
                // error.
                None => Err("token_list.remove: no entry has that name".into()),
            }
        })
        .method("clear", |l: &mut TokenList, _| {
            l.0.clear();
            Ok(Vec::new())
        })
        .method("id", |l: &mut TokenList, args| {
            let name = text(args, 0, "token_list.id")?;
            // Unestablished for an absent name (4.2): ours, 0.
            let id = l.0.iter().find(|(n, _)| n == name).map_or(0, |(_, id)| *id);
            Ok(vec![f64::from(id).into()])
        })
        .method("name", |l: &mut TokenList, args| {
            let id = index(args, 0, "token_list.name")?;
            // Unestablished for an absent id (4.2): ours, nil.
            let name = l.0.iter().find(|(_, i)| *i == id).map(|(n, _)| n.clone());
            Ok(vec![name.map_or(Value::Nil, Value::String)])
        });

    // 4.3: `i` by the E8 conversion, read as unsigned.
    let position = |args: &[Value], what: &str| index(args, 0, what).map(|i| i as u32 as usize);
    let rlist = EngineClass::new("rtoken_list")
        .constructor(0, |_| Some(0), |_, _| Ok(RtokenList::default()))
        .constructible()
        .method("add", |l: &mut RtokenList, args| {
            l.0.push(text(args, 0, "rtoken_list.add")?.to_vec());
            Ok(Vec::new())
        })
        .method("remove", move |l: &mut RtokenList, args| {
            let i = position(args, "rtoken_list.remove")?;
            if i < l.0.len() {
                l.0.remove(i);
            }
            Ok(Vec::new())
        })
        .method("clear", |l: &mut RtokenList, _| {
            l.0.clear();
            Ok(Vec::new())
        })
        .method("count", |l: &mut RtokenList, _| {
            Ok(vec![(l.0.len() as f64).into()])
        })
        .method("get", move |l: &mut RtokenList, args| {
            let i = position(args, "rtoken_list.get")?;
            Ok(vec![l.0.get(i).cloned().map_or(Value::Nil, Value::String)])
        });
    [token, list, rlist]
}

// --- The file system (5) -------------------------------------------------------

/// The names `file_list_open` gives for the index below `folder` (5.6 step
/// 3, 5.7 step 3): (name, entry) pairs in the index's order; files only
/// when `accept` takes their name.
fn listed(
    below: Vec<(String, FileEntry)>,
    folder: &str,
    flags: i32,
    accept: &dyn Fn(&[u8]) -> bool,
) -> Vec<(Vec<u8>, FileEntry)> {
    let mut out = Vec::new();
    for (path, entry) in below {
        let rest = &path[folder.len()..];
        let name = cp1251::encode(rest).unwrap_or_else(|| rest.as_bytes().to_vec());
        if let Some(inner) = rest.strip_suffix('\\') {
            if flags & FS_LIST_FOLDERS == 0 || (flags & FS_ROOT_ONLY != 0 && inner.contains('\\')) {
                continue;
            }
            out.push((name, entry));
        } else {
            if flags & FS_LIST_FILES == 0
                || (flags & FS_ROOT_ONLY != 0 && rest.contains('\\'))
                || !accept(&name)
            {
                continue;
            }
            let mut name = name;
            if flags & FS_CLAMP_EXT != 0
                && let Some(dot) = name.iter().rposition(|&b| b == b'.')
            {
                name.truncate(dot);
            }
            out.push((name, entry));
        }
    }
    out
}

/// Whether `name` matches `mask` (5.7 step 2): `*` any run of bytes, `?`
/// any one byte, any other byte only itself.
pub fn mask_matches(mask: &[u8], name: &[u8]) -> bool {
    match mask.split_first() {
        None => name.is_empty(),
        Some((b'*', rest)) => (0..=name.len()).any(|i| mask_matches(rest, &name[i..])),
        Some((&m, rest)) => match name.split_first() {
            Some((&n, tail)) if m == b'?' || m == n => mask_matches(rest, tail),
            _ => false,
        },
    }
}

/// The masks of `file_list_open_ex` (5.7 step 2).
pub fn masks(mask: &[u8]) -> Vec<Vec<u8>> {
    mask.split(|&b| b == b',')
        .map(|p| {
            let start = p.iter().position(|&b| b > 0x20).unwrap_or(p.len());
            let end = p.iter().rposition(|&b| b > 0x20).map_or(start, |e| e + 1);
            p[start..end.max(start)].to_vec()
        })
        .filter(|p| !p.is_empty())
        .collect()
}

/// `FS_file_list_ex:Sort(mode)` (5.7).
pub fn sort_items(items: &mut [Item], mode: i32) {
    match mode {
        0 => items.sort_unstable_by(|a, b| a.name.cmp(&b.name)),
        1 => items.sort_unstable_by(|a, b| b.name.cmp(&a.name)),
        2 => items.sort_unstable_by_key(|i| i.size),
        3 => items.sort_unstable_by_key(|i| std::cmp::Reverse(i.size)),
        4 => items.sort_unstable_by_key(|i| i.modif),
        5 => items.sort_unstable_by_key(|i| std::cmp::Reverse(i.modif)),
        _ => {}
    }
}

fn fs_class(ctx: &Context) -> EngineClass {
    let mut class = EngineClass::new("FS")
        .constant("FS_ListFiles", FS_LIST_FILES)
        .constant("FS_ListFolders", FS_LIST_FOLDERS)
        .constant("FS_ClampExt", FS_CLAMP_EXT)
        .constant("FS_RootOnly", FS_ROOT_ONLY);
    for (i, name) in [
        "FS_sort_by_name_up",
        "FS_sort_by_name_down",
        "FS_sort_by_size_up",
        "FS_sort_by_size_down",
        "FS_sort_by_modif_up",
        "FS_sort_by_modif_down",
    ]
    .into_iter()
    .enumerate()
    {
        class = class.constant(name, i as i32);
    }

    // 5.2.
    let c = ctx.clone();
    class = class.method("path_exist", move |_: &mut FsObject, args| {
        c.stopped()?;
        let alias = cp1251::decode(text(args, 0, "FS.path_exist")?);
        Ok(vec![c.files.borrow().aliases.dir(&alias).is_ok().into()])
    });
    let c = ctx.clone();
    class = class.method("update_path", move |_: &mut FsObject, args| {
        c.stopped()?;
        let what = "FS.update_path";
        let path = c.full_path(
            "CLocatorAPI::update_path",
            line!(),
            text(args, 0, what)?,
            text(args, 1, what)?,
        )?;
        Ok(vec![lua_text(&path)])
    });
    // What scripts receive for a path object is unestablished (5.2, 10):
    // ours, nil.
    let c = ctx.clone();
    class = class.method("get_path", move |_: &mut FsObject, args| {
        c.stopped()?;
        c.full_path(
            "CLocatorAPI::get_path",
            line!(),
            text(args, 0, "FS.get_path")?,
            b"",
        )?;
        Ok(vec![Value::Nil])
    });
    let c = ctx.clone();
    class = class.method("append_path", move |_: &mut FsObject, args| {
        c.stopped()?;
        let what = "FS.append_path";
        let alias = cp1251::decode(text(args, 0, what)?);
        let root = cp1251::decode(text(args, 1, what)?);
        let add = cp1251::decode(text(args, 2, what)?);
        let mut files = c.files.borrow_mut();
        // `root` an alias names its directory, as in `fsgame.ltx` (LTX 10;
        // our reading); an alias defined already is redefined (ours, 10).
        let root = files.aliases.dir(&root).map_or(root.clone(), str::to_owned);
        files.aliases.set_dir(&alias, &format!("{root}{add}"));
        Ok(vec![Value::Nil])
    });

    // 5.3.
    let c = ctx.clone();
    class = class.method_with_objects("exist", move |_: &mut FsObject, objects, args| {
        c.stopped()?;
        let what = "FS.exist";
        let path = match args.len() {
            1 => c.given_path("CLocatorAPI::exist", line!(), text(args, 0, what)?)?,
            2 => c.full_path(
                "CLocatorAPI::exist",
                line!(),
                text(args, 0, what)?,
                text(args, 1, what)?,
            )?,
            _ => return Err(no_form(what)),
        };
        let entry = c.files.borrow().index.entry(&path);
        Ok(vec![entry.map_or(Value::Nil, |entry| {
            objects.new_object("fs_file", FsFile { name: path, entry })
        })])
    });
    let c = ctx.clone();
    class = class.method("file_length", move |_: &mut FsObject, args| {
        c.stopped()?;
        let path = c.given_path(
            "CLocatorAPI::file_length",
            line!(),
            text(args, 0, "FS.file_length")?,
        )?;
        let entry = c.files.borrow().index.entry(&path);
        Ok(vec![entry.map_or(-1.0, |e| f64::from(e.size_real)).into()])
    });
    let c = ctx.clone();
    class = class.method("get_file_age", move |_: &mut FsObject, args| {
        c.stopped()?;
        let path = c.given_path(
            "CLocatorAPI::get_file_age",
            line!(),
            text(args, 0, "FS.get_file_age")?,
        )?;
        Ok(vec![f64::from(age(&c, &path)).into()])
    });
    let c = ctx.clone();
    class = class.method("get_file_age_str", move |_: &mut FsObject, args| {
        c.stopped()?;
        let path = c.given_path(
            "CLocatorAPI::get_file_age",
            line!(),
            text(args, 0, "FS.get_file_age_str")?,
        )?;
        let age = age(&c, &path);
        Ok(vec![lua_text(&c.files.borrow().local(age).asctime())])
    });

    // 5.4.
    let c = ctx.clone();
    class = class.method("file_delete", move |_: &mut FsObject, args| {
        c.stopped()?;
        let what = "FS.file_delete";
        let function = "CLocatorAPI::file_delete";
        let path = match args.len() {
            1 => c.given_path(function, line!(), text(args, 0, what)?)?,
            2 => c.path_or_full(
                function,
                line!(),
                text(args, 0, what)?,
                text(args, 1, what)?,
            )?,
            _ => return Err(no_form(what)),
        };
        c.files.borrow_mut().index.delete(&path);
        Ok(Vec::new())
    });
    let c = ctx.clone();
    class = class.method("file_rename", move |_: &mut FsObject, args| {
        c.stopped()?;
        let what = "FS.file_rename";
        let function = "CLocatorAPI::file_rename";
        let from = c.given_path(function, line!(), text(args, 0, what)?)?;
        let to = c.given_path(function, line!(), text(args, 1, what)?)?;
        let overwrite = flag(args, 2, what)?;
        c.files.borrow_mut().index.rename(&from, &to, overwrite);
        Ok(Vec::new())
    });
    let c = ctx.clone();
    class = class.method("file_copy", move |_: &mut FsObject, args| {
        c.stopped()?;
        let what = "FS.file_copy";
        let function = "CLocatorAPI::file_copy";
        let from = c.given_path(function, line!(), text(args, 0, what)?)?;
        let to = c.given_path(function, line!(), text(args, 1, what)?)?;
        let copied = c.files.borrow_mut().index.copy(&from, &to);
        // A failed read of an indexed file stops the game (DB 9).
        copied.map_err(|e| c.fault(function, line!(), &format!("{from}: {e}")))?;
        Ok(Vec::new())
    });
    let c = ctx.clone();
    class = class.method("dir_delete", move |_: &mut FsObject, args| {
        c.stopped()?;
        let what = "FS.dir_delete";
        let function = "CLocatorAPI::dir_delete";
        let (path, remove) = match args.len() {
            2 => (
                c.given_path(function, line!(), text(args, 0, what)?)?,
                flag(args, 1, what)?,
            ),
            // TODO(#994): gap 5, the simplest reading: an empty alias uses
            // `name` as a full path, as `file_delete` does.
            3 => (
                c.path_or_full(
                    function,
                    line!(),
                    text(args, 0, what)?,
                    text(args, 1, what)?,
                )?,
                flag(args, 2, what)?,
            ),
            _ => return Err(no_form(what)),
        };
        c.files.borrow_mut().index.delete_folder(&path, remove);
        Ok(Vec::new())
    });

    // 5.5: no shipped script calls these.
    let c = ctx.clone();
    class = class.method("r_open", move |_: &mut FsObject, args| {
        c.stopped()?;
        let what = "FS.r_open";
        let function = "CLocatorAPI::r_open";
        let path = match args.len() {
            1 => c.given_path(function, line!(), text(args, 0, what)?)?,
            2 => c.full_path(
                function,
                line!(),
                text(args, 0, what)?,
                text(args, 1, what)?,
            )?,
            _ => return Err(no_form(what)),
        };
        if c.files.borrow().index.entry(&path).is_none() {
            return Ok(vec![Value::Nil]);
        }
        // TODO(#1002): the reader of script API I.
        Err(format!(
            "{what}: data-stream readers (script API I) aren't available"
        ))
    });
    class = class.method("r_close", |_: &mut FsObject, _| {
        // TODO(#1002): the reader of script API I.
        Err("FS.r_close: data-stream readers (script API I) aren't available".to_owned())
    });
    let c = ctx.clone();
    class = class.method("w_open", move |_: &mut FsObject, args| {
        c.stopped()?;
        match args.len() {
            // Believed a crash (5.5): ours, a fatal error.
            1 => Err(c.fault("CLocatorAPI::w_close", line!(), "w_open with one argument")),
            // What scripts receive is unestablished (5.5): ours, an error.
            _ => Err("FS.w_open: writers aren't available to scripts".to_owned()),
        }
    });
    class = class.method("w_close", |_: &mut FsObject, _| {
        Err("FS.w_close: writers aren't available to scripts".to_owned())
    });

    // 5.6.
    let c = ctx.clone();
    class = class.method_with_objects("file_list_open", move |_: &mut FsObject, objects, args| {
        c.stopped()?;
        let what = "FS.file_list_open";
        let function = "CLocatorAPI::file_list_open";
        let (folder, flags) = match args.len() {
            2 => {
                let alias = text(args, 0, what)?;
                let dir = c
                    .files
                    .borrow()
                    .aliases
                    .dir(&cp1251::decode(alias))
                    .map(str::to_owned);
                let folder = match dir {
                    Ok(dir) => c.limit(function, line!(), dir)?,
                    Err(_) => c.given_path(function, line!(), alias)?,
                };
                (folder, index(args, 1, what)?)
            }
            3 => {
                let alias = match args.first() {
                    Some(Value::String(s)) if !s.is_empty() => s.as_slice(),
                    Some(Value::String(_) | Value::Nil) => {
                        // A failed check (5.6): its condition's text is
                        // unestablished, ours.
                        return Err(c.fatal(function, line!(), "assertion failed", "alias", &[]));
                    }
                    _ => return Err(no_form(what)),
                };
                (
                    c.full_path(function, line!(), alias, text(args, 1, what)?)?,
                    index(args, 2, what)?,
                )
            }
            _ => return Err(no_form(what)),
        };
        let below = c.files.borrow().index.below(&folder);
        let names = below.map(|below| {
            listed(below, &folder, flags, &|_| true)
                .into_iter()
                .map(|(name, _)| name)
                .collect()
        });
        Ok(vec![objects.new_object("FS_file_list", FileList(names))])
    });

    // 5.7.
    let c = ctx.clone();
    class.method_with_objects(
        "file_list_open_ex",
        move |_: &mut FsObject, objects, args| {
            c.stopped()?;
            let what = "FS.file_list_open_ex";
            let function = "CLocatorAPI::file_list_open_ex";
            let alias = text(args, 0, what)?;
            let flags = index(args, 1, what)?;
            let mask = text(args, 2, what)?;
            // Our choice for step 1: the index's loose files are what the
            // platform last listed ([`FileIndex::set_loose_folder`]).
            let folder = c.full_path(function, line!(), alias, b"")?;
            let masks = masks(mask);
            let accept =
                |name: &[u8]| masks.is_empty() || masks.iter().any(|m| mask_matches(m, name));
            let below = c.files.borrow().index.below(&folder).unwrap_or_default();
            let mut items: Vec<Item> = listed(below, &folder, flags, &accept)
                .into_iter()
                .map(|(name, e)| Item {
                    name,
                    size: e.size_real,
                    modif: e.modif,
                })
                .collect();
            items.sort_by(|a, b| a.name.cmp(&b.name));
            Ok(vec![
                objects.new_object("FS_file_list_ex", FileListEx(items)),
            ])
        },
    )
}

/// `get_file_age` (5.3): the entry's `modif`, or 4294967295.
fn age(c: &Context, path: &str) -> u32 {
    c.files
        .borrow()
        .index
        .entry(path)
        .map_or(u32::MAX, |e| e.modif)
}

fn fs_file_class() -> EngineClass {
    type Field = fn(&FileEntry) -> u32;
    let fields: [(&str, Field); 5] = [
        ("vfs", |e| e.vfs),
        ("ptr", |e| e.ptr),
        ("size_real", |e| e.size_real),
        ("size_compressed", |e| e.size_compressed),
        ("modif", |e| e.modif),
    ];
    let mut class = EngineClass::new("fs_file").property("name", |f: &FsFile| lua_text(&f.name));
    for (name, read) in fields {
        class = class.property(name, move |f: &FsFile| f64::from(read(&f.entry)).into());
    }
    class
}

fn file_list_classes(ctx: &Context) -> [EngineClass; 3] {
    // 5.6.
    let c = ctx.clone();
    let no_list = move |function: &str| {
        // Believed a fault (5.6): ours, a fatal error.
        c.fault(function, line!(), "a file list holding no list")
    };
    let size_fault = no_list.clone();
    let list = EngineClass::new("FS_file_list")
        .method("Size", move |l: &mut FileList, _| match &l.0 {
            Some(names) => Ok(vec![(names.len() as f64).into()]),
            None => Err(size_fault("FS_file_list::Size")),
        })
        .method("GetAt", move |l: &mut FileList, args| {
            let i = index(args, 0, "FS_file_list.GetAt")?;
            let Some(names) = &l.0 else {
                return Err(no_list("FS_file_list::GetAt"));
            };
            // What scripts see of the out-of-range exception is
            // unestablished (5.6, 10): ours, an error with its text.
            usize::try_from(i)
                .ok()
                .and_then(|i| names.get(i))
                .map(|n| vec![Value::String(n.clone())])
                .ok_or_else(|| "invalid vector<T> subscript".to_owned())
        })
        .method("Free", |l: &mut FileList, _| {
            // After it, every member is unestablished (5.6): ours, as a list
            // holding no list.
            l.0 = None;
            Ok(Vec::new())
        });

    // 5.7.
    let c = ctx.clone();
    let list_ex = EngineClass::new("FS_file_list_ex")
        .method("Size", |l: &mut FileListEx, _| {
            Ok(vec![(l.0.len() as f64).into()])
        })
        .method_with_objects("GetAt", move |l: &mut FileListEx, objects, args| {
            let i = index(args, 0, "FS_file_list_ex.GetAt")?;
            match usize::try_from(i).ok().and_then(|i| l.0.get(i)) {
                Some(item) => Ok(vec![objects.new_object("FS_item", item.clone())]),
                // Believed garbage or a crash (5.7): ours, a fatal error.
                None => Err(c.fault(
                    "FS_file_list_ex::GetAt",
                    line!(),
                    "an item outside the list",
                )),
            }
        })
        .method("Sort", |l: &mut FileListEx, args| {
            sort_items(&mut l.0, index(args, 0, "FS_file_list_ex.Sort")?);
            Ok(Vec::new())
        });

    // 5.8.
    let mut item = EngineClass::new("FS_item");
    for name in ["NameFull", "NameShort"] {
        item = item.method(name, |i: &mut Item, _| {
            Ok(vec![Value::String(i.name.clone())])
        });
    }
    item = item.method("Size", |i: &mut Item, _| Ok(vec![f64::from(i.size).into()]));
    let c = ctx.clone();
    item = item.method("Modif", move |i: &mut Item, _| {
        Ok(vec![lua_text(&c.files.borrow().local(i.modif).asctime())])
    });
    let c = ctx.clone();
    item = item.method("ModifDigitOnly", move |i: &mut Item, _| {
        Ok(vec![lua_text(&c.files.borrow().local(i.modif).digits())])
    });
    [list, list_ex, item]
}

/// The global functions of 2.1, 2.3 and 6, and `getFS`.
fn register_functions(scripts: &mut Scripts, ctx: &Context) -> Result<(), Error> {
    let lua = scripts.lua_mut();
    let (system, game) = {
        let files = ctx.files.borrow();
        (Rc::clone(&files.system), Rc::clone(&files.game))
    };
    let system = lua.new_engine_object("ini_file", |_| IniFile(system))?;
    let game = lua.new_engine_object("ini_file", |_| IniFile(game))?;
    lua.register_object_function("system_ini", move |_| Ok(Some(system)))?;
    lua.register_object_function("game_ini", move |_| Ok(Some(game)))?;
    let c = ctx.clone();
    lua.register_with_objects("create_ini_file", move |maker: &ObjectMaker, args| {
        c.stopped()?;
        let text = text(args, 0, "create_ini_file")?;
        let text = &text[..text.iter().position(|&b| b == 0).unwrap_or(text.len())];
        let base = c.full_path("create_ini_file", line!(), GAME_CONFIG.as_bytes(), b"")?;
        let config = load_config(&c, "CInifile::Load", Err((text, &base)))?;
        Ok(vec![maker.new_object("ini_file", IniFile(Rc::new(config)))])
    })?;
    let fs = lua.new_engine_object("FS", |_| FsObject)?;
    lua.register_object_function("getFS", move |_| Ok(Some(fs)))?;
    let c = ctx.clone();
    lua.register("command_line", move |_| {
        c.stopped()?;
        Ok(vec![lua_text(
            &c.files.borrow().command_line.to_ascii_lowercase(),
        )])
    })?;
    let c = ctx.clone();
    lua.register("user_name", move |_| {
        c.stopped()?;
        Ok(vec![lua_text(&c.files.borrow().user_name)])
    })?;
    Ok(())
}

impl Scripts {
    /// Registers configuration and files (1) over `files`, fatal errors
    /// reported to the console of `services`: the engine registers them
    /// before [`Scripts::load_globals`]. The configuration objects of
    /// `system_ini()` and `game_ini()` and the file system object are made
    /// here.
    pub fn register_configuration_and_files(
        &mut self,
        files: Rc<RefCell<FileSystem>>,
        services: Rc<RefCell<EngineServices>>,
    ) -> Result<(), Error> {
        let ctx = Context {
            files,
            services,
            fatal: self.fatal_flag(),
            factory: Rc::clone(&self.factory),
        };
        self.register_class(ini_class(&ctx))?;
        for class in token_classes() {
            self.register_class(class)?;
        }
        self.register_class(fs_file_class())?;
        for class in file_list_classes(&ctx) {
            self.register_class(class)?;
        }
        self.register_class(fs_class(&ctx))?;
        register_functions(self, &ctx)
    }
}
