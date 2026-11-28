// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/script-api-configuration-and-files.md on every target, one
//! check per section, over [`FileSystem`] on an archive tree of our own and
//! a saved-games folder of loose files.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::io;
use std::rc::Rc;

use volya_engine_core::config::PathAliases;
use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_core::vfs::{self, DirEntry, EntryKind, Vfs, VfsFile};
use volya_engine_formats::ltx;

use super::ensure;
use super::services::run;
use crate::ErrorKind;
use crate::files::{
    CivilTime, DiskChange, FileSystem, LooseFile, LooseFiles, mask_matches, masks, read_bool,
    scan_vector,
};
use crate::loader::{Prefix, Scripts, Trees};
use crate::object_factory::{ObjectFactory, class_id};
use crate::services::EngineServices;

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("1 registered", registered),
    ("2 configuration objects", configuration_objects),
    ("3 readers", readers),
    ("4 token classes", tokens),
    ("5 file system", file_system),
    ("6 command line and user name", command_line),
    ("9 logic reads", logic_reads),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| f().err().map(|e| format!("files {what}: {e}")))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// An archive tree in memory: lowercase `/`-separated paths.
struct Tree(BTreeMap<String, Vec<u8>>);

impl Trees for Tree {
    fn with_trees(&self, f: &mut dyn FnMut(&dyn Vfs, &dyn Vfs)) -> io::Result<()> {
        f(self, self);
        Ok(())
    }
}

impl Vfs for Tree {
    fn open(&self, path: &str) -> io::Result<Box<dyn VfsFile + '_>> {
        let key = vfs::normalize(path)?.to_ascii_lowercase();
        match self.0.get(&key) {
            Some(data) => Ok(Box::new(File(io::Cursor::new(data.clone())))),
            None => Err(io::Error::new(io::ErrorKind::NotFound, key)),
        }
    }

    fn read_dir(&self, path: &str) -> io::Result<Vec<DirEntry>> {
        let key = vfs::normalize(path)?.to_ascii_lowercase();
        let prefix = if key.is_empty() {
            String::new()
        } else {
            format!("{key}/")
        };
        let mut found = BTreeMap::new();
        for (p, data) in self.0.range(prefix.clone()..) {
            let Some(rest) = p.strip_prefix(&prefix) else {
                break;
            };
            let entry = match rest.split_once('/') {
                Some((dir, _)) => DirEntry {
                    name: dir.to_owned(),
                    kind: EntryKind::Dir,
                    size: 0,
                },
                None => DirEntry {
                    name: rest.to_owned(),
                    kind: EntryKind::File,
                    size: data.len() as u64,
                },
            };
            found.entry(entry.name.clone()).or_insert(entry);
        }
        if found.is_empty() && !key.is_empty() {
            return Err(io::Error::new(io::ErrorKind::NotFound, key));
        }
        Ok(found.into_values().collect())
    }
}

struct File(io::Cursor<Vec<u8>>);

impl io::Read for File {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.read(buf)
    }
}

impl io::Seek for File {
    fn seek(&mut self, pos: io::SeekFrom) -> io::Result<u64> {
        self.0.seek(pos)
    }
}

impl VfsFile for File {
    fn size(&self) -> u64 {
        self.0.get_ref().len() as u64
    }
}

/// The saved-games folder's bytes.
struct Saves;

impl LooseFiles for Saves {
    fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        Ok(format!("bytes of {path}").into_bytes())
    }
}

const FSGAME: &[u8] = b"$app_data_root$ = true | false| {userdocs}\\Stalker-SHOC\r\n\
    $game_data$ = false| true | $fs_root$ | gamedata\\\r\n\
    $game_scripts$ = true | false| $game_data$ | scripts\\\r\n\
    $game_config$ = true | false| $game_data$ | config\\\r\n\
    $game_saves$ = true | false| $app_data_root$| savedgames\\\r\n";

/// The saved-games folder's full path.
const SAVES: &str = "{userdocs}\\stalker-shoc\\savedgames\\";

const MIXED: &str = "[nums]\r\ns = -12abc\r\nu = -1\r\nf = 0.25f\r\nbig = 4294967297\r\n\
    b1 = on\r\nb2 = TRUE\r\nb3 = yes1\r\nb7 = truexxx\r\nnv\r\nq = \"quoted\"\r\n\
    v = 1,2.5,x\r\nv2 = 3, 4, 5\r\ncls = AI_STL_S\r\ntok = Beta\r\n\
    [Mixed]\r\nkey = 2\r\nKey = 1\r\n[child]:nums\r\nextra = 1\r\n";

const DOOR: &str = "[logic]\r\nactive = door@shut\r\n\
    [door@shut]\r\nlocked = true\r\nsnd_open_start = door_unlock\r\n\
    on_info = {+can_leave} door@closed %=play_snd(device\\servo)%\r\n\
    [door@closed]\r\n[door@open]\r\n";

/// The tree's files.
fn tree() -> Tree {
    let files: &[(&str, &str)] = &[
        ("gamedata/scripts/_g.script", ""),
        ("gamedata/config/system.ltx", "[sys]\r\nk = v\r\n"),
        ("gamedata/config/game.ltx", "[game]\r\nmode = single\r\n"),
        ("gamedata/config/mixed.ltx", MIXED),
        (
            "gamedata/config/inc/main.ltx",
            "#include \"part.ltx\"\r\n[main]\r\nx = 1\r\n",
        ),
        ("gamedata/config/inc/part.ltx", "[part]\r\ny = 2\r\n"),
        ("gamedata/config/scripts/door.ltx", DOOR),
        ("gamedata/config/dup.ltx", "[a]\r\n[a]\r\n"),
    ];
    Tree(
        files
            .iter()
            .map(|(p, t)| ((*p).to_owned(), t.as_bytes().to_vec()))
            .collect(),
    )
}

type Shared = Rc<RefCell<FileSystem>>;

/// A game script state with the services and configuration and files
/// registered, `_G` loaded, and the saved-games folder listed.
fn state() -> Result<(Scripts, Shared, Rc<RefCell<EngineServices>>), String> {
    let tree = Rc::new(tree());
    let aliases = PathAliases::parse(FSGAME, "").map_err(|e| e.to_string())?;
    let mut s = Scripts::new(tree.clone(), &aliases, Prefix::Default).map_err(|e| e.message)?;
    let console = Console::new(ConsoleFiles {
        default_controls: Vec::new(),
        presets: Default::default(),
    });
    let system = ltx::parse(b"[sys]\r\nk = v\r\n").map_err(|e| e.to_string())?;
    let game = ltx::parse(b"[game]\r\nmode = single\r\n").map_err(|e| e.to_string())?;
    let services = Rc::new(RefCell::new(EngineServices::new(console, system.clone())));
    s.register_engine_services(Rc::clone(&services))
        .map_err(|e| e.message)?;
    let mut files = FileSystem::new(tree, aliases, Rc::new(system), Rc::new(game));
    files.index.set_loose_folder(
        SAVES,
        &[
            loose("b.sav", 10, 1_000_000_000),
            loose("a.sav", 20, 2_000_000_000),
            loose("a.dds", 30, 1_500_000_000),
            loose("sub\\c.sav", 40, 1_200_000_000),
        ],
        Rc::new(Saves),
    );
    let files = Rc::new(RefCell::new(files));
    s.register_configuration_and_files(Rc::clone(&files), Rc::clone(&services))
        .map_err(|e| e.message)?;
    s.load_globals().map_err(|e| e.message)?;
    Ok((s, files, services))
}

fn loose(path: &str, size: u32, modif: u32) -> LooseFile {
    LooseFile {
        path: path.to_owned(),
        size,
        modif,
    }
}

/// `code` must stop the state with a fatal error whose report's rows
/// include `rows`, in the log.
fn fatal(code: &str, rows: &[&str]) -> Result<(), String> {
    let (mut s, _, services) = state()?;
    let result = s.exec("=fatal", format!("pcall(function() {code} end)").as_bytes());
    ensure(
        matches!(&result, Err(e) if e.kind == ErrorKind::Fatal),
        || format!("{code}: {result:?}, not fatal"),
    )?;
    let log = services.borrow().console.log().join("\n");
    for row in rows {
        ensure(log.contains(row), || {
            format!("{code}: no {row:?} in the log:\n{log}")
        })?;
    }
    let later = s.exec("=later", b"return 1");
    ensure(
        matches!(&later, Err(e) if e.kind == ErrorKind::Fatal),
        || format!("{code}: the state goes on: {later:?}"),
    )
}

/// The rows of the run-time library's handler (spec B 2.3).
const HANDLER: &[&str] = &["[error]Arguments     : error handler is invoked!"];

/// Section 1: what is registered.
pub fn registered() -> Result<(), String> {
    let (mut s, _, _) = state()?;
    run(
        &mut s,
        "1",
        r##"
for _, f in ipairs({"system_ini", "game_ini", "create_ini_file", "getFS", "command_line",
    "user_name"}) do
  eq(type(_G[f]), "function", f)
end
local class_mt = getmetatable(ini_file)
for _, c in ipairs({"ini_file", "token", "token_list", "rtoken_list", "FS", "fs_file",
    "FS_file_list", "FS_file_list_ex", "FS_item"}) do
  eq(getmetatable(_G[c]) == class_mt, true, c .. " is a class")
end
local ini = system_ini()
for _, m in ipairs({"section_exist", "line_exist", "line_count", "r_string", "r_string_wq",
    "r_s32", "r_u32", "r_float", "r_bool", "r_vector", "r_clsid", "r_token", "r_line"}) do
  eq(type(ini[m]), "function", "ini_file:" .. m)
end
local fs = getFS()
for _, m in ipairs({"path_exist", "update_path", "get_path", "append_path", "exist",
    "file_length", "get_file_age", "get_file_age_str", "file_delete", "file_rename",
    "file_copy", "dir_delete", "r_open", "r_close", "w_open", "w_close", "file_list_open",
    "file_list_open_ex"}) do
  eq(type(fs[m]), "function", "FS:" .. m)
end
local item = fs:file_list_open_ex("$game_saves$", FS.FS_ListFiles, ""):GetAt(0)
for _, m in ipairs({"NameFull", "NameShort", "Size", "Modif", "ModifDigitOnly"}) do
  eq(type(item[m]), "function", "FS_item:" .. m)
end
"##,
    )
}

/// Section 2: `system_ini`, `game_ini`, `ini_file`, `create_ini_file` and
/// the reading errors.
pub fn configuration_objects() -> Result<(), String> {
    let (mut s, _, _) = state()?;
    run(
        &mut s,
        "2",
        r##"
-- 2.1
eq(rawequal(system_ini(), system_ini()), true, "the same system_ini")
eq(rawequal(system_ini(), game_ini()), false, "two configurations")
eq(system_ini():r_string("sys", "k"), "v", "system.ltx")
eq(game_ini():r_string("game", "mode"), "single", "game.ltx")
-- 2.2
local mixed = ini_file("mixed.ltx")
eq(mixed:section_exist("nums"), true, "ini_file(name) reads $game_config$name")
eq(ini_file("MIXED.LTX"):section_exist("nums"), true, "the path lowercased")
local missing = ini_file("absent.ltx")
eq(missing:section_exist("nums"), false, "a missing file: no sections")
eq(missing:line_exist("nums", "s"), false, "a missing file: no lines")
local main = ini_file("inc\\main.ltx")
eq(main:r_string("part", "y"), "2", "includes against the file's own folder")
eq(main:r_string("main", "x"), "1", "the root's own section")
eq(rawequal(ini_file("mixed.ltx"), mixed), false, "a new object per call")
-- 2.3
local made = create_ini_file("[a]\r\nx = 1\r\n#include \"inc\\part.ltx\"\r\n")
eq(made:r_string("a", "x"), "1", "create_ini_file's text")
eq(made:r_string("part", "y"), "2", "its includes against $game_config$")
local cut = create_ini_file("[a]\r\n\0[b]\r\n")
eq(cut:section_exist("a"), true, "up to the first zero byte")
eq(cut:section_exist("b"), false, "nothing after it")
"##,
    )?;
    fatal("ini_file('dup.ltx')", &["duplicate section"])?;
    fatal("create_ini_file('#include \"none.ltx\"')", &["none.ltx"])?;
    let long = "x".repeat(520 - "gamedata\\config\\".len());
    fatal(&format!("ini_file('{long}')"), HANDLER)?;
    // 2.4.
    fatal(
        "ini_file('mixed.ltx'):r_string('absent', 'k')",
        &["[error]Description   : Can't open section 'absent'"],
    )?;
    fatal(
        "ini_file('mixed.ltx'):line_count('Absent')",
        &["Can't open section 'Absent'"],
    )?;
    fatal(
        "ini_file('mixed.ltx'):r_s32('nums', 'zz')",
        &["[error]Description   : Can't find variable zz in [nums]"],
    )?;
    fatal(
        "ini_file('mixed.ltx'):r_line('absent', 0, '', '')",
        &["Can't open section"],
    )
}

/// Section 3: the readers.
pub fn readers() -> Result<(), String> {
    ensure(
        read_bool("on") && read_bool("TRUE") && read_bool("1"),
        || "r_bool's true words".into(),
    )?;
    ensure(
        !read_bool("yes1") && !read_bool("truexxx") && !read_bool(""),
        || "r_bool's other values".into(),
    )?;
    ensure(scan_vector("1,2.5,x") == [1.0, 2.5, 0.0], || {
        format!("%f,%f,%f: {:?}", scan_vector("1,2.5,x"))
    })?;
    ensure(scan_vector("1;2,3") == [1.0, 0.0, 0.0], || {
        "a component after a non-comma".into()
    })?;
    let (mut s, _, _) = state()?;
    let mut factory = ObjectFactory::new();
    let clsid = factory.number(class_id(b"AI_STL_S"));
    run(
        &mut s,
        "3",
        &format!(
            r##"
local ini = ini_file("mixed.ltx")
-- 3.1
eq(ini:section_exist("nums"), true, "section_exist")
eq(ini:section_exist("NUMS"), false, "section_exist doesn't lowercase")
eq(ini:section_exist("mixed"), true, "names stored lowercase")
eq(ini:line_exist("NUMS", "s"), false, "line_exist after section_exist")
eq(ini:line_exist("nums", "S"), false, "keys exactly")
eq(ini:line_exist("nums", "s"), true, "line_exist")
eq(ini:line_exist("nums", "nv"), true, "a key without a value")
eq(ini:line_count("nums"), 14, "line_count")
eq(ini:line_count("NUMS"), 14, "line_count lowercases")
eq(ini:line_count("child"), 15, "inherited entries")
-- 3.2
eq(ini:r_string("nums", "q"), '"quoted"', "r_string keeps quotes")
eq(ini:r_string_wq("nums", "q"), "quoted", "r_string_wq")
eq(ini:r_string("nums", "nv"), nil, "r_string of no value")
eq(ini:r_string_wq("nums", "nv"), nil, "r_string_wq of no value")
eq(ini:r_string("nums", "v2"), "3,4,5", "the stored value")
-- 3.3
eq(ini:r_s32("nums", "s"), -12, "r_s32 like atoi")
eq(ini:r_u32("nums", "u"), 4294967295, "r_u32 unsigned")
eq(ini:r_s32("nums", "u"), -1, "r_s32 signed")
eq(ini:r_s32("nums", "big"), 1, "the low 32 bits (TODO(#994) gap 1)")
eq(ini:r_float("nums", "f"), 0.25, "r_float like atof")
eq(ini:r_s32("child", "s"), -12, "an inherited entry")
-- 3.4
eq(ini:r_bool("nums", "b1"), true, "on")
eq(ini:r_bool("nums", "b2"), true, "TRUE")
eq(ini:r_bool("nums", "b3"), false, "yes1")
eq(ini:r_bool("nums", "b7"), false, "7 bytes")
-- 3.5
local v = ini:r_vector("nums", "v")
eq(v.x, 1, "r_vector x") eq(v.y, 2.5, "r_vector y") eq(v.z, 0, "r_vector stops")
local w = ini:r_vector("nums", "v2")
eq(w.z, 5, "r_vector z")
eq(rawequal(ini:r_vector("nums", "v2"), w), false, "a new vector")
-- 3.6
eq(ini:r_clsid("nums", "cls"), {clsid}, "r_clsid's number")
local list = token_list()
eq(ini:r_token("nums", "tok", list), 0, "an empty list")
eq(ini:r_token("nums", "nv", list), 0, "an empty list reads no value")
list:add("alpha", 1) list:add("beta", 2) list:add("BETA", 3)
eq(ini:r_token("nums", "tok", list), 2, "r_token ignores case, the first match")
eq(ini:r_token("nums", "s", list), 0, "no match")
-- 3.7
local ok, k, val = ini:r_line("mixed", 0, "", "")
eq(ok, true, "r_line 0") eq(k, "Key", "sorted by key bytes") eq(val, "1", "its value")
ok, k, val = ini:r_line("mixed", 1, "", "")
eq(k, "key", "r_line 1")
ok, k, val = ini:r_line("mixed", 0.6, "", "")
eq(k, "Key", "the E8 conversion")
ok, k, val = ini:r_line("nums", 7, "", "")
eq(k, "nv", "no value's key") eq(val, "", "no value reads empty")
ok, k, val = ini:r_line("mixed", 2, "", "")
eq(ok, false, "past the end") eq(k, "", "no key") eq(val, "", "no value")
eq((ini:r_line("mixed", -1, "", "")), false, "a negative index")
raises(function() ini:r_line("mixed", 0) end, "", "r_line needs four arguments")
"##
        ),
    )?;
    fatal("ini_file('mixed.ltx'):r_s32('nums', 'nv')", HANDLER)?;
    fatal("ini_file('mixed.ltx'):r_float('nums', 'nv')", HANDLER)?;
    fatal("ini_file('mixed.ltx'):r_vector('nums', 'nv')", HANDLER)?;
    fatal("ini_file('mixed.ltx'):r_bool('nums', 'nv')", &["no value"])?;
    // TODO(#994): gap 2, a section name of 256 bytes or more.
    let long = "s".repeat(256);
    fatal(
        &format!("ini_file('mixed.ltx'):line_count('{long}')"),
        HANDLER,
    )
}

/// Section 4: `token`, `token_list`, `rtoken_list`.
pub fn tokens() -> Result<(), String> {
    let (mut s, _, _) = state()?;
    run(
        &mut s,
        "4",
        r##"
local t = token()
eq(t.name, nil, "a new token's name") eq(t.id, 0, "its id")
t.name = "n" t.id = 7.4
eq(t.name, "n", "name assigned") eq(t.id, 7, "id by the E8 conversion")
local l = token_list()
l:add("a", 1) l:add("b", 2) l:add("a", 3)
eq(l:id("a"), 1, "the first entry of a name") eq(l:name(2), "b", "the first of an id")
eq(l:id("A"), 0, "names exactly")
l:remove("a")
eq(l:id("a"), 3, "remove takes the first")
raises(function() l:remove("zz") end, "no entry", "remove of an absent name")
l:clear()
eq(l:name(2), nil, "clear")
local r = rtoken_list()
r:add("x") r:add("y") r:add("z")
eq(r:count(), 3, "count") eq(r:get(1), "y", "get") eq(r:get(3), nil, "get past the end")
eq(r:get(-1), nil, "a negative index is never below the count")
r:remove(0)
eq(r:get(0), "y", "remove moves later entries up")
r:remove(5) r:remove(-1)
eq(r:count(), 2, "remove out of range does nothing")
r:clear()
eq(r:count(), 0, "clear")
"##,
    )
}

/// Section 5: the file system.
pub fn file_system() -> Result<(), String> {
    ensure(
        mask_matches(b"*.sav", b"a.sav") && !mask_matches(b"*.sav", b"a.SAV"),
        || "masks: * and case".into(),
    )?;
    ensure(
        mask_matches(b"?.s*", b"a.sav") && !mask_matches(b"?.sav", b"ab.sav"),
        || "masks: ?".into(),
    )?;
    ensure(
        masks(b" *.sav , ,\t*.dds") == [b"*.sav".to_vec(), b"*.dds".to_vec()],
        || format!("masks: {:?}", masks(b" *.sav , ,\t*.dds")),
    )?;
    let (mut s, files, _) = state()?;
    let saves = SAVES.replace('\\', "\\\\");
    run(
        &mut s,
        "5",
        &format!(
            r##"
local fs = getFS()
local saves = "{saves}"
-- 5.1
eq(rawequal(getFS(), fs), true, "the same file system object")
eq(FS.FS_ListFiles, 1, "FS_ListFiles") eq(FS.FS_ListFolders, 2, "FS_ListFolders")
eq(FS.FS_ClampExt, 4, "FS_ClampExt") eq(FS.FS_RootOnly, 8, "FS_RootOnly")
eq(FS.FS_sort_by_name_up, 0, "by name up") eq(FS.FS_sort_by_modif_down, 5, "by modif down")
-- 5.2
eq(fs:path_exist("$game_config$"), true, "path_exist") eq(fs:path_exist("$nope$"), false, "no alias")
eq(fs:update_path("$game_config$", "Inc\\A.LTX"), "gamedata\\config\\inc\\a.ltx", "update_path")
eq(fs:update_path("$game_saves$", ""), saves, "the saved-games folder")
-- 5.3
local f = fs:exist("$game_config$", "Mixed.ltx")
eq(f.name, "gamedata\\config\\mixed.ltx", "fs_file name") eq(f.vfs, 0, "an archive member's vfs")
eq(f.size_real, {mixed}, "size_real") eq(f.size_compressed, {mixed}, "size_compressed")
eq(fs:exist("gamedata\\config\\mixed.ltx") ~= nil, true, "exist(path)")
eq(fs:exist("gamedata\\config\\Mixed.ltx"), nil, "a path as given, not lowercased")
eq(fs:exist("gamedata/config/mixed.ltx"), nil, "the index's separators")
eq(fs:exist("$game_config$", "none.ltx"), nil, "nothing")
eq(fs:exist("$game_config$", "inc\\").name, "gamedata\\config\\inc\\", "a folder entry")
local sav = fs:exist("$game_saves$", "a.sav")
eq(sav.vfs, 4294967295, "a loose file's vfs") eq(sav.ptr, 0, "its ptr")
eq(sav.size_real, 20, "its size") eq(sav.modif, 2000000000, "its modif")
eq(fs:file_length(saves .. "a.sav"), 20, "file_length")
eq(fs:file_length(saves .. "none"), -1, "file_length of nothing")
eq(fs:get_file_age(saves .. "a.sav"), 2000000000, "get_file_age")
eq(fs:get_file_age(saves .. "none"), 4294967295, "get_file_age of nothing")
eq(fs:get_file_age_str(saves .. "a.sav"), "Wed May 18 03:33:20 2033\n", "get_file_age_str")
eq(fs:get_file_age_str(saves .. "none"), "Sun Feb 07 06:28:15 2106\n", "of nothing")
-- 5.6
local function names(list)
  local t = {{}}
  for i = 0, list:Size() - 1 do t[#t + 1] = list:GetAt(i) end
  return table.concat(t, " ")
end
eq(names(fs:file_list_open("$game_saves$", FS.FS_ListFiles)), "a.dds a.sav b.sav sub\\c.sav",
  "files in byte order")
eq(names(fs:file_list_open("$game_saves$", bit_or(FS.FS_ListFiles, FS.FS_RootOnly))),
  "a.dds a.sav b.sav", "FS_RootOnly")
eq(names(fs:file_list_open("$game_saves$", FS.FS_ListFolders)), "sub\\", "folders")
eq(names(fs:file_list_open("$game_saves$", bit_or(FS.FS_ListFiles, FS.FS_ClampExt))),
  "a a b sub\\c", "FS_ClampExt")
eq(names(fs:file_list_open("$game_saves$", "sub\\", FS.FS_ListFiles)), "c.sav", "(alias, name)")
eq(names(fs:file_list_open(saves, FS.FS_ListFiles)), "a.dds a.sav b.sav sub\\c.sav",
  "a full path for no alias")
local list = fs:file_list_open("$game_saves$", FS.FS_ListFiles)
raises(function() list:GetAt(4) end, "invalid vector<T> subscript", "GetAt out of range")
list:Free()
-- 5.7, 5.8
local ex = fs:file_list_open_ex("$game_saves$", bit_or(FS.FS_ListFiles, FS.FS_RootOnly), "*.sav")
eq(ex:Size(), 2, "masks") eq(ex:GetAt(0):NameFull(), "a.sav", "by name")
ex:Sort(FS.FS_sort_by_modif_down)
eq(ex:GetAt(0):NameFull(), "a.sav", "newest first")
ex:Sort(FS.FS_sort_by_modif_up)
local item = ex:GetAt(0)
eq(item:NameFull(), "b.sav", "oldest first") eq(item:NameShort(), "b.sav", "NameShort")
eq(item:Size(), 10, "Size") eq(item:Modif(), "Sun Sep 09 01:46:40 2001\n", "Modif")
eq(item:ModifDigitOnly(), "09:09:2001 01:46", "ModifDigitOnly")
ex:Sort(FS.FS_sort_by_size_down) eq(ex:GetAt(0):Size(), 20, "by size down")
ex:Sort(FS.FS_sort_by_name_down) eq(ex:GetAt(0):NameFull(), "b.sav", "by name down")
ex:Sort(9) eq(ex:GetAt(0):NameFull(), "b.sav", "another mode leaves the list")
eq(fs:file_list_open_ex("$game_saves$", FS.FS_ListFiles, " *.dds , ?.sav"):Size(), 3, "mask list")
eq(fs:file_list_open_ex("$game_saves$", FS.FS_ListFiles, ""):Size(), 4, "no mask: every file")
eq(fs:file_list_open_ex("$game_saves$", bit_or(FS.FS_ListFiles, FS.FS_ClampExt), "*.sav")
  :GetAt(0):NameFull(), "a", "FS_ClampExt after the masks")
-- 5.4
fs:file_delete("$game_saves$", "b.sav")
eq(fs:exist("$game_saves$", "b.sav"), nil, "file_delete")
fs:file_delete("", saves .. "a.dds")
eq(fs:exist(saves .. "a.dds"), nil, "file_delete of a full path")
fs:file_delete("$game_config$", "mixed.ltx")
eq(fs:exist("$game_config$", "mixed.ltx"), nil, "an archive member's entry")
eq(ini_file("mixed.ltx"):section_exist("nums"), false, "no longer in the index")
fs:file_rename(saves .. "a.sav", saves .. "Renamed.SAV", false)
eq(fs:exist(saves .. "a.sav"), nil, "file_rename from") eq(fs:exist(saves .. "renamed.sav").size_real, 20, "to, lowercased")
fs:file_rename(saves .. "sub\\c.sav", saves .. "renamed.sav", false)
eq(fs:exist(saves .. "sub\\c.sav") ~= nil, true, "no overwrite")
fs:file_copy(saves .. "renamed.sav", saves .. "copy.sav")
fs:dir_delete(saves .. "sub\\", 0)
eq(fs:exist(saves .. "sub\\c.sav") ~= nil, true, "dir_delete keeps a folder with files")
fs:dir_delete("$game_saves$", "", 1)
eq(fs:exist(saves .. "sub\\c.sav"), nil, "dir_delete removes files")
eq(fs:exist(saves .. "renamed.sav"), nil, "every file below")
fs:dir_delete(saves .. "none\\", 1)
-- 5.5
eq(fs:r_open("$game_saves$", "none"), nil, "r_open of nothing")
raises(function() fs:w_open("$game_saves$", "x") end, "writers", "w_open")
"##,
            mixed = MIXED.len()
        ),
    )?;
    let changes = files.borrow_mut().index.take_disk_changes();
    let full = |p: &str| format!("{SAVES}{p}");
    let want = [
        DiskChange::Delete(full("b.sav")),
        DiskChange::Delete(full("a.dds")),
        DiskChange::Rename {
            from: full("a.sav"),
            to: full("renamed.sav"),
        },
        DiskChange::Write {
            path: full("copy.sav"),
            bytes: format!("bytes of {}", full("renamed.sav")).into_bytes(),
        },
        DiskChange::Delete(full("renamed.sav")),
        DiskChange::Delete(full("sub\\c.sav")),
        DiskChange::RemoveFolder(full("sub\\")),
    ];
    ensure(changes == want, || format!("disk changes: {changes:#?}"))?;
    fatal(
        "getFS():update_path('$nope$', 'x')",
        &["[error]Description   : $nope$"],
    )?;
    fatal(
        "getFS():file_list_open('$nope$', 'x', 1)",
        &["[error]Description   : $nope$"],
    )?;
    fatal("getFS():file_list_open('$nope$', 1):Size()", &["no list"])?;
    fatal(
        "getFS():file_list_open_ex('$game_saves$', 1, ''):GetAt(9)",
        &["outside the list"],
    )?;
    fatal("getFS():w_open('x')", &["w_open"])?;
    // TODO(#994): gap 3, one path of 520 bytes or more.
    fatal(&format!("getFS():exist('{}')", "p".repeat(520)), HANDLER)
}

/// Section 6: `command_line()` and `user_name()`; the times of 5.3 and 5.8.
pub fn command_line() -> Result<(), String> {
    let example = CivilTime::utc(315_626_635);
    ensure(example.asctime() == "Wed Jan 02 02:03:55 1980\n", || {
        format!("asctime: {:?}", example.asctime())
    })?;
    ensure(
        CivilTime::utc(1_204_726_020).digits() == "05:03:2008 14:07",
        || format!("digits: {:?}", CivilTime::utc(1_204_726_020).digits()),
    )?;
    let (mut s, files, _) = state()?;
    run(
        &mut s,
        "6a",
        r##"eq(command_line(), "", "no command line") eq(user_name(), "player", "user_name")"##,
    )?;
    files.borrow_mut().command_line = "C:\\Game\\XR_3DA.EXE -Nointro".into();
    files.borrow_mut().local_time = Box::new(|t| CivilTime::utc(t + 3600));
    run(
        &mut s,
        "6b",
        r##"
eq(command_line(), "c:\\game\\xr_3da.exe -nointro", "lowercased")
local item = getFS():file_list_open_ex("$game_saves$", 1, "b.sav"):GetAt(0)
eq(item:ModifDigitOnly(), "09:09:2001 02:46", "the local time zone")
"##,
    )
}

/// Section 9: the logic reads of a door through `section_exist`,
/// `line_exist`, `r_string` and `r_bool`, as the logic scripts make them.
pub fn logic_reads() -> Result<(), String> {
    let (mut s, _, _) = state()?;
    run(
        &mut s,
        "9",
        r##"
local function cfg_get_string(ini, section, key)
  if ini:section_exist(section) and ini:line_exist(section, key) then return ini:r_string(section, key) end
end
local function cfg_get_bool(ini, section, key)
  if ini:section_exist(section) and ini:line_exist(section, key) then return ini:r_bool(section, key) end
  return false
end
local custom = create_ini_file("[logic]\r\ncfg = scripts\\door.ltx\r\n[collide]\r\nignore_static\r\n")
eq(custom:section_exist("animation"), false, "no animation")
eq(custom:section_exist("drop_box"), false, "no drop_box")
local cfg = cfg_get_string(custom, "logic", "cfg")
eq(cfg, "scripts\\door.ltx", "[logic] cfg")
eq(custom:r_string("collide", "ignore_static"), nil, "no value")
local ini = ini_file(cfg)
for _, name in ipairs({"logic", "door@shut", "door@closed", "door@open"}) do
  eq(ini:section_exist(name), true, name)
end
local active = cfg_get_string(ini, "logic", "active")
eq(active, "door@shut", "active")
eq(cfg_get_bool(ini, active, "locked"), true, "locked")
eq(cfg_get_string(ini, active, "snd_open_start"), "door_unlock", "snd_open_start")
eq(cfg_get_string(ini, active, "on_info"), "{+can_leave}door@closed%=play_snd(device\\servo)%",
  "on_info, spaces removed")
for _, key in ipairs({"closed", "no_force", "show_tips", "tip_open", "snd_close_start", "on_use"}) do
  eq(ini:line_exist(active, key), false, key .. " is absent")
end
"##,
    )
}
