// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The game's configuration files loaded from a file tree
//! (docs/formats/ltx-config.md): includes resolved against a [`Vfs`]
//! (section 4), root files addressed through the path aliases of
//! `fsgame.ltx` (sections 8 and 10). Parsing is
//! [`volya_engine_formats::ltx`].

use std::collections::HashMap;
use std::io;

use volya_engine_formats::cp1251;
use volya_engine_formats::ltx::{Builder, fsgame};
pub use volya_engine_formats::ltx::{Config, Entry, Section, value};

use crate::vfs::{self, Vfs};

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// Prefixes `err` with `path`.
fn context(path: &str, err: io::Error) -> io::Error {
    io::Error::new(err.kind(), format!("{path}: {err}"))
}

/// Lowercases `path` (ASCII only, as the spec's lowercasing): game paths are
/// looked up case-insensitively.
fn lower(path: &str) -> String {
    path.to_ascii_lowercase()
}

/// The directory part of `path`: up to and including its last separator
/// (`\`, or `/` as [`Vfs`] paths use), or `""`.
fn directory(path: &str) -> &str {
    match path.rfind(['\\', '/']) {
        Some(i) => &path[..=i],
        None => "",
    }
}

/// Loads the configuration whose root file is `path` in `vfs`. Includes
/// resolve relative to the directory of the file containing them; a missing
/// include is an error, while a missing root file yields an empty
/// configuration (section 8).
pub fn load(vfs: &dyn Vfs, path: &str) -> io::Result<Config> {
    let path = lower(path);
    let text = match vfs::read(vfs, &path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Config::default()),
        Err(e) => return Err(context(&path, e)),
    };
    let mut builder = Builder::new();
    add_file(vfs, &path, &text, &mut builder)?;
    Ok(builder.finish())
}

/// Parses `text` as a configuration file whose includes resolve against
/// the directory `base` (ending in `\` or empty), as a model's
/// user data is read (torch.md 1.2).
pub fn parse_in(vfs: &dyn Vfs, text: &[u8], base: &str) -> io::Result<Config> {
    let mut builder = Builder::new();
    add_file(
        vfs,
        &format!("{}user data", lower(base)),
        text,
        &mut builder,
    )?;
    Ok(builder.finish())
}

fn add_file(vfs: &dyn Vfs, path: &str, text: &[u8], builder: &mut Builder) -> io::Result<()> {
    let dir = directory(path);
    builder
        .add_file(text, &mut |include, builder| {
            // Include paths of the game use `\` and no `..`; other forms are
            // not covered by the spec, and the VFS rejects `..`.
            let included = lower(&format!("{dir}{}", cp1251::decode(include)));
            let text = vfs::read(vfs, &included)
                .map_err(|e| context(&format!("include {included:?}"), e))?;
            add_file(vfs, &included, &text, builder)
        })
        .map_err(|e| context(path, e))
}

/// Directories named by the aliases of `fsgame.ltx` (section 10), as
/// lowercase `\`-separated paths ending in `\` (or empty), usable as [`Vfs`]
/// paths.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PathAliases {
    dirs: HashMap<String, String>,
}

/// The predefined alias of the game root directory.
pub const FS_ROOT: &str = "$fs_root$";

/// The alias of the current level's directory, changed at run time.
pub const LEVEL: &str = "$level$";

impl PathAliases {
    /// Parses `fsgame` with `$fs_root$` set to `fs_root` (`""` for the root
    /// of the [`Vfs`] holding the game tree).
    pub fn parse(fsgame: &[u8], fs_root: &str) -> io::Result<Self> {
        let mut aliases = Self::default();
        aliases.dirs.insert(FS_ROOT.into(), as_dir(&lower(fs_root)));
        for def in fsgame::parse(fsgame)? {
            // `$fs_root$` is predefined (section 10), so redefining it is a
            // duplicate like any other.
            if aliases.dirs.contains_key(&def.alias) {
                return Err(invalid(format!(
                    "fsgame.ltx is corrupted: alias {} defined twice",
                    def.alias
                )));
            }
            let root = aliases.dirs.get(&def.root).unwrap_or(&def.root);
            let dir = as_dir(&format!(
                "{root}{}",
                def.subdirectory.as_deref().unwrap_or("")
            ));
            aliases.dirs.insert(def.alias, dir);
        }
        Ok(aliases)
    }

    /// The directory of `alias`. Aliases are stored lowercased; the spec does
    /// not say how requests are compared, so we lowercase them too.
    pub fn dir(&self, alias: &str) -> io::Result<&str> {
        self.dirs
            .get(&lower(alias))
            .map(String::as_str)
            .ok_or_else(|| invalid(format!("undefined path alias {alias}")))
    }

    /// The full, lowercased path of `relative` under `alias`.
    pub fn path(&self, alias: &str, relative: &str) -> io::Result<String> {
        Ok(lower(&format!("{}{relative}", self.dir(alias)?)))
    }

    /// Sets the directory of `alias` (the engine points [`LEVEL`] at the level
    /// being loaded).
    pub fn set_dir(&mut self, alias: &str, dir: &str) {
        self.dirs.insert(lower(alias), as_dir(&lower(dir)));
    }
}

/// `dir` with a `\` appended if non-empty and not ending in one.
fn as_dir(dir: &str) -> String {
    if dir.is_empty() || dir.ends_with('\\') {
        dir.to_owned()
    } else {
        format!("{dir}\\")
    }
}

/// Loads the configuration whose root file is `relative` under `alias`.
pub fn load_aliased(
    vfs: &dyn Vfs,
    aliases: &PathAliases,
    alias: &str,
    relative: &str,
) -> io::Result<Config> {
    load(vfs, &aliases.path(alias, relative)?)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::vfs::{DirEntry, VfsFile};

    struct Files(HashMap<&'static str, &'static [u8]>);

    struct MemFile(Cursor<&'static [u8]>);
    impl io::Read for MemFile {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.0.read(buf)
        }
    }
    impl io::Seek for MemFile {
        fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
            self.0.seek(pos)
        }
    }
    impl VfsFile for MemFile {
        fn size(&self) -> u64 {
            self.0.get_ref().len() as u64
        }
    }

    impl Vfs for Files {
        fn open(&self, path: &str) -> io::Result<Box<dyn VfsFile + '_>> {
            let path = vfs::normalize(path)?;
            match self.0.get(path.as_str()) {
                Some(data) => Ok(Box::new(MemFile(Cursor::new(data)))),
                None => Err(io::Error::new(io::ErrorKind::NotFound, path)),
            }
        }
        fn read_dir(&self, _: &str) -> io::Result<Vec<DirEntry>> {
            unimplemented!()
        }
    }

    fn tree() -> Files {
        Files(HashMap::from([
            (
                "gamedata/config/system.ltx",
                &b"#include \"creatures\\monsters.ltx\"\r\n[root]:m_flesh\r\n"[..],
            ),
            (
                "gamedata/config/creatures/monsters.ltx",
                b"#include \"M_flesh.ltx\"\r\n[monsters]\r\n",
            ),
            (
                "gamedata/config/creatures/m_flesh.ltx",
                b"[m_flesh]\r\nk = v\r\n",
            ),
            ("gamedata/config/bad.ltx", b"#include \"missing.ltx\"\r\n"),
            ("gamedata/config/up.ltx", b"#include \"..\\system.ltx\"\r\n"),
        ]))
    }

    #[test]
    fn resolves_nested_includes_relative_to_their_file() {
        let config = load(&tree(), "gamedata\\config\\System.ltx").unwrap();
        let names: Vec<&str> = config.sections().map(Section::name).collect();
        assert_eq!(names, ["m_flesh", "monsters", "root"]);
        assert_eq!(config.get("root", "k"), Some(Some("v")));
    }

    #[test]
    fn missing_root_is_empty_but_missing_include_fails() {
        assert!(
            load(&tree(), "gamedata/config/none.ltx")
                .unwrap()
                .is_empty()
        );
        let err = load(&tree(), "gamedata/config/bad.ltx").unwrap_err();
        assert!(err.to_string().contains("missing.ltx"), "{err}");
        assert!(load(&tree(), "gamedata/config/up.ltx").is_err());
    }

    const FSGAME: &[u8] = b"$app_data_root$ = true | false| {userdocs}\\Stalker-SHOC\r\n\
        $game_data$ =     false| true | $fs_root$      | gamedata\\\r\n\
        $game_levels$ =   true | false| $game_data$    | levels\\\r\n\
        $game_config$ =   true | false| $game_data$    | config\\\r\n\
        $level$ =         false| false| $game_levels$\r\n\
        $game_saves$ =    true | false| $app_data_root$| savedgames\\\r\n";

    #[test]
    fn resolves_aliases() {
        let aliases = PathAliases::parse(FSGAME, "").unwrap();
        assert_eq!(aliases.dir("$game_config$").unwrap(), "gamedata\\config\\");
        assert_eq!(aliases.dir("$level$").unwrap(), "gamedata\\levels\\");
        assert_eq!(
            aliases.dir("$game_saves$").unwrap(),
            "{userdocs}\\stalker-shoc\\savedgames\\"
        );
        assert_eq!(
            aliases.path("$game_config$", "System.ltx").unwrap(),
            "gamedata\\config\\system.ltx"
        );
        assert!(aliases.dir("$undefined$").is_err());
        let rooted = PathAliases::parse(FSGAME, "C:\\Game").unwrap();
        assert_eq!(rooted.dir("$game_data$").unwrap(), "c:\\game\\gamedata\\");
        let config = load_aliased(&tree(), &aliases, "$game_config$", "system.ltx").unwrap();
        assert_eq!(config.len(), 3);
    }

    #[test]
    fn rejects_duplicate_aliases() {
        let twice = b"$a$ = true|true|x\r\n$A$ = true|true|y\r\n";
        assert!(PathAliases::parse(twice, "").is_err());
        assert!(PathAliases::parse(b"$fs_root$ = true|true|x\r\n", "").is_err());
    }

    #[test]
    fn sets_level_directory() {
        let mut aliases = PathAliases::parse(FSGAME, "").unwrap();
        aliases.set_dir(LEVEL, "gamedata\\levels\\l01_escape");
        assert_eq!(
            aliases.path(LEVEL, "level.ltx").unwrap(),
            "gamedata\\levels\\l01_escape\\level.ltx"
        );
    }
}
