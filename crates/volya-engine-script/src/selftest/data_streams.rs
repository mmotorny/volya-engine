// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/script-api-data-streams.md on every target, one check per
//! section: `net_packet`, `reader` and `ClientID` from scripts, and the
//! engine's packets and readers scripts use in place.

use std::cell::RefCell;
use std::rc::Rc;

use volya_engine_core::console::{Console, ConsoleFiles};
use volya_engine_formats::ltx;

use super::ensure;
use super::loading::scripts_of;
use super::services::run;
use crate::data_streams::{
    INV_255, INV_65535, INV_TWO_PI, NetPacket, Reader, SDIR_MIN, TWO_PI, direction_code,
    direction_of_code, packet_floor, quantised, scaled_direction,
};
use crate::loader::{Prefix, Scripts};
use crate::services::EngineServices;
use crate::values::Vector;
use crate::{ErrorKind, Value};

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("1 registered", registered),
    ("2.1 which form runs", forms),
    ("2.2 argument conversions", conversions),
    ("2.3 results", results),
    ("3.1 the packet", packet),
    ("3.2 writes", writes),
    ("3.3 reads", reads),
    ("3.4 positions and chunks", positions),
    ("3.5 quantised values and angles", quantised_values),
    ("3.6 directions", directions),
    ("4 reader", reader),
    ("5 ClientID", client_id),
    ("6 engine streams", engine_streams),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| f().err().map(|e| format!("data streams {what}: {e}")))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

type Shared = Rc<RefCell<EngineServices>>;

/// A game script state with the services, the value types and the data
/// streams registered and `_G` loaded; the global `test_reader()` gives an
/// engine reader over `block`.
fn state(block: &[u8]) -> Result<(Scripts, Shared), String> {
    let (mut s, _) = scripts_of(&[("_g.script", "")], Prefix::Default)?;
    let console = Console::new(ConsoleFiles {
        default_controls: Vec::new(),
        presets: Default::default(),
    });
    let system = ltx::parse(b"").map_err(|e| e.to_string())?;
    let services = Rc::new(RefCell::new(EngineServices::new(console, system)));
    s.register_engine_services(Rc::clone(&services))
        .map_err(|e| e.message)?;
    s.register_data_streams(Rc::clone(&services))
        .map_err(|e| e.message)?;
    let part = s
        .new_reader(Reader::over(block.to_vec()))
        .map_err(|e| e.message)?;
    s.lua_mut()
        .register_object_function("test_reader", move |_| Ok(Some(part)))
        .map_err(|e| e.message)?;
    s.load_globals().map_err(|e| e.message)?;
    Ok((s, services))
}

fn check_code(name: &str, block: &[u8], body: &str) -> Result<(), String> {
    let (mut s, _) = state(block)?;
    run(&mut s, name, body)
}

/// `code` must stop the state with a fatal error whose report includes
/// `row`, in the log.
fn fatal(code: &str, row: &str) -> Result<(), String> {
    let (mut s, services) = state(&[1, 2, 3])?;
    let result = s.exec("=fatal", format!("pcall(function() {code} end)").as_bytes());
    ensure(
        matches!(&result, Err(e) if e.kind == ErrorKind::Fatal),
        || format!("{code}: {result:?}, not fatal"),
    )?;
    let log = services.borrow().console.log().join("\n");
    ensure(log.contains(row), || {
        format!("{code}: no {row:?} in the log:\n{log}")
    })
}

const OUTSIDE: &str = "outside its storage";
const NO_STORAGE: &str = "no storage";
const NO_64: &str = "64-bit integer";

/// Section 1: the three classes and their members.
pub fn registered() -> Result<(), String> {
    check_code(
        "1",
        &[],
        r##"
local packet_names = {"w_begin", "w_tell", "w_chunk_open8", "w_chunk_close8", "w_chunk_open16",
  "w_chunk_close16", "w_u8", "w_u16", "w_u24", "w_u32", "w_s16", "w_s32", "w_u64", "w_s64",
  "w_float", "w_bool", "w_stringZ", "w_vec3", "w_matrix", "w_clientID", "w_float_q8",
  "w_float_q16", "w_angle8", "w_angle16", "w_dir", "w_sdir", "r_begin", "r_tell", "r_seek",
  "r_advance", "r_elapsed", "r_eof", "r_u8", "r_u16", "r_u24", "r_u32", "r_s8", "r_s16", "r_s32",
  "r_u64", "r_s64", "r_float", "r_bool", "r_stringZ", "r_clientID", "r_vec3", "r_matrix",
  "r_float_q8", "r_float_q16", "r_angle8", "r_angle16", "r_dir", "r_sdir"}
eq(#packet_names, 53, "53 names of net_packet")
local p = net_packet()
for _, n in ipairs(packet_names) do eq(type(p[n]), "function", "net_packet." .. n) end
local reader_names = {"r_tell", "r_seek", "r_advance", "r_elapsed", "r_eof", "r_u8", "r_u16",
  "r_u32", "r_s8", "r_s16", "r_s32", "r_u64", "r_s64", "r_float", "r_bool", "r_stringZ", "r_vec3",
  "r_dir", "r_sdir", "r_float_q8", "r_float_q16", "r_angle8", "r_angle16"}
eq(#reader_names, 23, "23 names of reader")
local r = test_reader()
for _, n in ipairs(reader_names) do eq(type(r[n]), "function", "reader." .. n) end
eq(r.r_u24, nil, "reader has no r_u24")
eq(r.w_u8, nil, "reader has no writes")
raises(function() return reader() end, "", "no reader constructor")
local id = ClientID()
eq(type(id.value), "function", "ClientID.value")
eq(type(id.set), "function", "ClientID.set")
eq(id == ClientID(), true, "ClientID's ==")
"##,
    )
}

/// Section 2.1: one-form methods are called whatever is passed; the two
/// forms by the number of values; no reference form ever matches.
pub fn forms() -> Result<(), String> {
    check_code(
        "2.1",
        &[7, 0, 0, 0],
        r##"
local p = net_packet()
p:w_u8()
eq(p:w_tell(), 1, "a missing number argument converts as 0")
p:w_u8(5, 6, 7)
eq(p:w_tell(), 2, "extra arguments are ignored")
eq(p:r_u8(), 0, "the missing argument wrote 0")
eq(p:r_u8(), 5, "the first argument wrote 5")
for _, n in ipairs({"r_u8", "r_s8", "r_u16", "r_s16", "r_u24", "r_u32", "r_s32", "r_u64", "r_s64",
    "r_float"}) do
  raises(function() return p[n](p, 1) end, "no form", "net_packet." .. n .. "(number)")
  raises(function() return p[n](p, nil) end, "no form", "net_packet." .. n .. "(nil)")
  raises(function() return p[n](p, 1, 2) end, "no form", "net_packet." .. n .. "(1, 2)")
end
local r = test_reader()
for _, n in ipairs({"r_u8", "r_s8", "r_u16", "r_s16", "r_u32", "r_s32", "r_u64", "r_s64",
    "r_float"}) do
  raises(function() return r[n](r, 1) end, "no form", "reader." .. n .. "(number)")
end
eq(r:r_tell(), 0, "no failed form read")
eq(r:r_u32(), 7, "the value form")
"##,
    )
}

/// Section 2.2: the conversions of the arguments.
pub fn conversions() -> Result<(), String> {
    check_code(
        "2.2",
        &[],
        r##"
local function written(write, read, v)
  local p = net_packet()
  p[write](p, v)
  return p[read](p)
end
-- To u8
eq(written("w_u8", "r_u8", 261), 5, "To u8: low 8 bits")
eq(written("w_u8", "r_u8", -1), 255, "To u8 of -1")
eq(written("w_u8", "r_u8", 2.9), 2, "To u8 truncates")
eq(written("w_u8", "r_u8", -2.9), 254, "To u8 truncates toward zero")
eq(written("w_u8", "r_u8", "7"), 7, "To u8 of a numeric text")
eq(written("w_u8", "r_u8", {}), 0, "To u8 of a table")
eq(written("w_u8", "r_u8", 2^31 + 1), 0, "To u8 out of range")
eq(written("w_u8", "r_u8", 0/0), 0, "To u8 of NaN")
-- To u16
eq(written("w_u16", "r_u16", 65537), 1, "To u16: low 16 bits")
eq(written("w_u16", "r_u16", -2), 65534, "To u16 of -2")
eq(written("w_u16", "r_u16", 2^31), 0, "To u16 out of range")
-- To u32
eq(written("w_u32", "r_u32", -1), 4294967295, "To u32 of -1")
eq(written("w_u32", "r_u32", 2^32 + 3), 3, "To u32: low 32 bits")
eq(written("w_u32", "r_u32", 2^63), 0, "To u32 of 2^63")
eq(written("w_u32", "r_u32", 0/0), 0, "To u32 of NaN")
eq(written("w_u24", "r_u24", 2^24 + 9), 9, "w_u24: low 24 bits")
-- To s32
eq(written("w_s32", "r_s32", -5.5), -5, "To s32 truncates")
eq(written("w_s32", "r_s32", 2^31), -2^31, "To s32 out of range")
eq(written("w_s32", "r_s32", 0/0), -2^31, "To s32 of NaN")
eq(written("w_s16", "r_s16", 32768), -32768, "w_s16: low 16 bits")
eq(written("w_s16", "r_s16", 2^31), 0, "w_s16 of -2^31's low bits")
-- To f32
eq(written("w_float", "r_float", 0.1), 0.100000001490116119384765625, "To f32 rounds")
eq(written("w_float", "r_float", 1e39), math.huge, "To f32 overflows")
eq(written("w_float", "r_float", "2.5"), 2.5, "To f32 of a numeric text")
-- Boolean
eq(written("w_bool", "r_bool", nil), false, "w_bool(nil)")
eq(written("w_bool", "r_bool", false), false, "w_bool(false)")
eq(written("w_bool", "r_bool", 0), true, "w_bool(0)")
eq(written("w_bool", "r_bool", ""), true, "w_bool('')")
-- Text
eq(written("w_stringZ", "r_stringZ", "ab\0cd"), "ab", "w_stringZ: up to the zero byte")
eq(written("w_stringZ", "r_stringZ", 1.5), "1.5", "w_stringZ of a number")
eq(written("w_stringZ", "r_stringZ", 1/3), "0.33333333333333", "w_stringZ of a number: %.14g")
"##,
    )?;
    fatal("net_packet():w_stringZ({})", "no text")?;
    fatal("net_packet():w_stringZ()", "no text")?;
    fatal("net_packet():w_vec3(nil)", NO_STORAGE)?;
    fatal("net_packet():w_vec3(matrix())", NO_STORAGE)?;
    fatal("net_packet():w_matrix(vector())", NO_STORAGE)?;
    fatal("net_packet():w_clientID(5)", NO_STORAGE)?;
    fatal("net_packet():w_u64(1)", NO_64)?;
    fatal("net_packet():w_s64(1)", NO_64)
}

/// Section 2.3: what the reads give.
pub fn results() -> Result<(), String> {
    check_code(
        "2.3",
        &[1],
        r##"
local p = net_packet()
for _ = 1, 8 do p:w_u8(255) end
p:w_float(-1.5)
eq(p:r_u8(), 255, "r_u8 unsigned")
p:r_seek(0)
eq(p:r_s8(), -1, "r_s8 signed")
p:r_seek(0)
eq(p:r_u16(), 65535, "r_u16")
p:r_seek(0)
eq(p:r_s16(), -1, "r_s16")
p:r_seek(0)
eq(p:r_u24(), 2^24 - 1, "r_u24")
p:r_seek(0)
eq(p:r_u32(), 2^32 - 1, "r_u32")
p:r_seek(0)
eq(p:r_s32(), -1, "r_s32")
p:r_seek(8)
eq(p:r_float(), -1.5, "r_float")
eq(type(p:r_eof()), "boolean", "r_eof gives a boolean")
eq(p:r_tell(), 12, "r_tell")
eq(p:w_tell(), 12, "w_tell")
eq(p:r_elapsed(), 0, "r_elapsed")
local r = test_reader()
eq(r:r_tell(), 0, "reader r_tell")
eq(r:r_u8(), 1, "reader r_u8")
"##,
    )?;
    fatal("net_packet():r_u64()", NO_64)?;
    fatal("net_packet():r_s64()", NO_64)?;
    fatal("test_reader():r_u64()", OUTSIDE)?;
    // The 8 bytes are read before the failure.
    let (mut s, _) = state(&[0; 9])?;
    let result = s.exec("=u64", b"local r = test_reader() pcall(r.r_u64, r)");
    ensure(
        matches!(result, Err(e) if e.kind == ErrorKind::Fatal),
        || "reader r_u64 not fatal".into(),
    )
}

/// Section 3.1: a new packet, reads past the written part, the buffer's
/// end.
pub fn packet() -> Result<(), String> {
    check_code(
        "3.1",
        &[],
        r##"
local p = net_packet()
eq(p:w_tell(), 0, "a new packet's write position")
eq(p:r_tell(), 0, "a new packet's read position")
eq(p:r_eof(), true, "a new packet is at its end")
eq(p:r_u32(), 0, "a new packet's bytes are 0")
eq(p:r_tell(), 4, "reading past the write position advances")
eq(p:r_stringZ(), "", "the empty text of a 0 byte")
p:w_u16(513)
p:r_seek(0)
eq(p:r_u8(), 1, "little-endian")
p:r_seek(8188)
eq(p:r_u32(), 0, "the buffer's last bytes")
p:w_begin(0)
for _ = 1, 4095 do p:w_u16(65535) end
eq(p:w_tell(), 8192, "8192 bytes written")
"##,
    )?;
    fatal("local p = net_packet() p:r_seek(8189) p:r_u32()", OUTSIDE)?;
    fatal(
        "local p = net_packet() for _ = 1, 8193 do p:w_u8(1) end",
        OUTSIDE,
    )?;
    fatal("local p = net_packet() p:r_seek(-1) p:r_u8()", OUTSIDE)?;
    // r_stringZ searches to the buffer's end.
    fatal(
        "local p = net_packet() for _ = 1, 8192 do p:w_u8(65) end p:r_stringZ()",
        OUTSIDE,
    )
}

/// Section 3.2: the bytes each write stores.
pub fn writes() -> Result<(), String> {
    check_code(
        "3.2",
        &[],
        r##"
local p = net_packet()
local function sizes(write, v, n)
  local before = p:w_tell()
  p[write](p, v)
  eq(p:w_tell() - before, n, write .. " stores " .. n .. " bytes")
end
sizes("w_u8", 1, 1) sizes("w_u16", 1, 2) sizes("w_s16", 1, 2) sizes("w_u24", 1, 3)
sizes("w_u32", 1, 4) sizes("w_s32", 1, 4) sizes("w_float", 1, 4) sizes("w_bool", true, 1)
sizes("w_stringZ", "abc", 4) sizes("w_vec3", vector():set(1, 2, 3), 12)
sizes("w_matrix", matrix():identity(), 48) sizes("w_clientID", ClientID(), 4)
sizes("w_dir", vector():set(0, 0, 1), 2) sizes("w_sdir", vector():set(0, 0, 1), 6)
sizes("w_angle8", 1, 1) sizes("w_angle16", 1, 2)
local before = p:w_tell()
p:w_float_q8(1, 0, 2)
eq(p:w_tell() - before, 1, "w_float_q8 stores 1 byte")
p:w_float_q16(1, 0, 2)
eq(p:w_tell() - before, 3, "w_float_q16 stores 2 bytes")
-- The values stored.
local q = net_packet()
q:w_vec3(vector():set(1.5, -2, 0.1))
q:r_seek(0)
eq(q:r_float(), 1.5, "w_vec3 x") eq(q:r_float(), -2, "w_vec3 y")
eq(q:r_float(), 0.100000001490116119384765625, "w_vec3 z as f32")
local m = matrix():set(vector():set(1, 2, 3), vector():set(5, 6, 7), vector():set(9, 10, 11),
  vector():set(13, 14, 15))
m._14_ = 4 m._24_ = 8 m._34_ = 12 m._44_ = 16
q:w_begin(0)
q:w_matrix(m)
q:r_seek(2)
local stored = {}
for i = 1, 12 do stored[i] = q:r_float() end
eq(table.concat(stored, ","), "1,2,3,5,6,7,9,10,11,13,14,15", "w_matrix: columns 0-2 of rows 0-3")
local id = ClientID()
id:set(70000)
q:w_clientID(id)
q:r_seek(50)
eq(q:r_u32(), 70000, "w_clientID stores the value")
q:w_bool(5)
q:r_seek(54)
eq(q:r_u8(), 1, "w_bool stores 1")
"##,
    )
}

/// Section 3.3: what the reads take and give.
pub fn reads() -> Result<(), String> {
    check_code(
        "3.3",
        &[],
        r##"
local p = net_packet()
p:w_vec3(vector():set(1, 2, 3))
p:w_vec3(vector():set(4, 5, 6))
local v = vector():set(7, 8, 9)
local got = p:r_vec3(v)
eq(got.x + got.y * 10 + got.z * 100, 321, "r_vec3 gives a new vector")
eq(v.x + v.y * 10 + v.z * 100, 987, "r_vec3 leaves its argument")
eq(rawequal(got, v), false, "a new vector")
local w = p:r_vec3()
eq(w.x, 4, "r_vec3 without an argument")
eq(p:r_tell(), 24, "12 bytes each")
local m = matrix():set(vector():set(1, 2, 3), vector():set(5, 6, 7), vector():set(9, 10, 11),
  vector():set(13, 14, 15))
m._14_ = 4 m._24_ = 8 m._34_ = 12 m._44_ = 16
p:w_matrix(m)
local n = matrix()
n._14_ = 9 n._24_ = 9 n._34_ = 9 n._44_ = 9
eq(p:r_matrix(n), nil, "r_matrix gives nothing")
eq(n.i.x + n.i.y + n.i.z, 6, "r_matrix row 0")
eq(n.c.x + n.c.y + n.c.z, 42, "r_matrix row 3")
eq(n._14_, 0, "element 3 = 0") eq(n._24_, 0, "element 7 = 0") eq(n._34_, 0, "element 11 = 0")
eq(n._44_, 1, "element 15 = 1")
local id = ClientID()
id:set(12345)
p:w_clientID(id)
local back = p:r_clientID()
eq(back:value(), 12345, "r_clientID")
eq(back == id, true, "the same value")
eq(rawequal(back, id), false, "a new ClientID")
p:w_stringZ("text")
eq(p:r_stringZ(), "text", "r_stringZ")
eq(p:r_tell(), p:w_tell(), "past the zero byte")
"##,
    )?;
    fatal("net_packet():r_matrix(nil)", NO_STORAGE)?;
    fatal("net_packet():r_dir(nil)", NO_STORAGE)?;
    for name in [
        "r_float_q8",
        "r_float_q16",
        "r_angle8",
        "r_angle16",
        "r_begin",
    ] {
        fatal(&format!("net_packet():{name}(0, 0, 1)"), NO_STORAGE)?;
    }
    Ok(())
}

/// Section 3.4: positions and chunks.
pub fn positions() -> Result<(), String> {
    check_code(
        "3.4",
        &[],
        r##"
local p = net_packet()
p:w_u32(1) p:w_u32(2)
p:w_begin(258)
eq(p:w_tell(), 2, "w_begin: position 0, then the type")
p:r_seek(0)
eq(p:r_u16(), 258, "w_begin's type")
p:r_seek(1.9)
eq(p:r_tell(), 1, "r_seek To u32")
p:r_advance(-1)
eq(p:r_tell(), 0, "r_advance(-1) moves back")
p:r_advance(-1)
eq(p:r_tell(), 2^32 - 1, "modulo 2^32")
eq(p:r_elapsed(), 3, "r_elapsed modulo 2^32")
p:r_seek(3)
eq(p:r_elapsed(), 2^32 - 1, "a read position past the write position")
eq(p:r_eof(), true, "r_eof past the end")
p:r_seek(2)
eq(p:r_eof(), true, "r_eof at the end")
p:r_seek(1)
eq(p:r_eof(), false, "r_eof before the end")
-- Chunks: the length of what follows the marked position.
p:w_begin(0)
p:w_u8(0)
p:w_u32(7) p:w_u8(1)
p:w_chunk_close8(2)
eq(p:w_tell(), 8, "w_chunk_close8 leaves the write position")
p:r_seek(2)
eq(p:r_u8(), 5, "w_chunk_close8: write position - pos - 1")
p:w_u16(0)
p:w_stringZ("abc")
p:w_chunk_close16(8)
p:r_seek(8)
eq(p:r_u16(), 4, "w_chunk_close16: write position - pos - 2")
p:w_chunk_close8(14)
p:r_seek(14)
eq(p:r_u8(), 255, "low 8 bits of -1")
"##,
    )?;
    fatal("net_packet():w_chunk_open8(0)", NO_STORAGE)?;
    fatal("net_packet():w_chunk_open16(0)", NO_STORAGE)?;
    fatal("net_packet():w_chunk_close8(8192)", OUTSIDE)?;
    fatal("net_packet():w_chunk_close16(8191)", OUTSIDE)
}

/// The quantised writes recomputed in plain f32 (no subnormals arise for
/// the cases here).
fn q16_of(v: f32, min: f32, max: f32) -> u16 {
    let f = (v - min) / (max - min) * 65535.0 + 0.5;
    f.floor() as i64 as u16
}

/// Section 3.5: the packet's floor, the quantised writes, the angles and
/// the reader's reads.
pub fn quantised_values() -> Result<(), String> {
    ensure(TWO_PI.to_bits() == 0x40C9_0FDB, || "2π".into())?;
    ensure(INV_TWO_PI.to_bits() == 0x3E22_F983, || "1/2π".into())?;
    ensure(f32::from_bits(INV_65535) == 1.525_902_2e-5, || {
        "1/65535".into()
    })?;
    ensure(f32::from_bits(INV_255) == 0.003_921_567, || "1/255".into())?;
    let examples: [(f32, i32); 15] = [
        (1.5, 1),
        (-1.5, -2),
        (0.0, 0),
        (-0.0, -1),
        (-1.0, -1),
        (2_147_483_000.0, 2_147_483_008),
        (2_147_483_648.0, i32::MIN),
        (3.0e9, (3_000_000_000i64 - (1 << 32)) as i32),
        (4_294_967_296.0, 1),
        (1.0e10, 2),
        (-3.0e9, 1_294_967_296),
        (f32::INFINITY, 1),
        (f32::NEG_INFINITY, -1),
        (f32::NAN, 1),
        (-f32::NAN, 1),
    ];
    for (f, want) in examples {
        let got = packet_floor(f);
        ensure(got == want, || {
            format!("the packet's floor of {f}: {got}, not {want}")
        })?;
    }
    // min = max: +∞, −∞ and NaN.
    for (v, q16, q8) in [(2.0, 1, 1), (0.0, 65535, 255), (1.0, 1, 1)] {
        let a = quantised(v, 1.0, 1.0, 65535.0) as u16;
        let b = quantised(v, 1.0, 1.0, 255.0) as u8;
        ensure(a == q16 && b == q8, || {
            format!("min = max, v = {v}: {a}, {b}, not {q16}, {q8}")
        })?;
    }
    // In range, recomputed; outside, wrapping.
    let mut seed = 0x1234_5678u32;
    for _ in 0..200 {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let v = (seed >> 8) as f32 / 16_777_216.0 * 20.0 - 5.0;
        let got = quantised(v, -5.0, 15.0, 65535.0) as u16;
        let want = q16_of(v, -5.0, 15.0);
        ensure(got == want, || {
            format!("w_float_q16({v}, -5, 15): {got}, not {want}")
        })?;
    }
    // 131070.5: the low 16 bits of 131070.
    ensure(quantised(2.0, 0.0, 1.0, 65535.0) as u16 == 65534, || {
        "w_float_q16 past max wraps".into()
    })?;
    let (mut s, _) = state(&[])?;
    let results = s
        .exec(
            "=3.5",
            br#"
local p = net_packet()
p:w_float_q16(0.5, 0, 1) p:w_float_q8(0.5, 0, 1)
p:w_angle16(1) p:w_angle8(1)
p:w_angle16(-1) p:w_angle16(1 + 2 * math.pi) p:w_angle16(0/0)
return p:r_u16(), p:r_u8(), p:r_u16(), p:r_u8(), p:r_u16(), p:r_u16(), p:r_u16()
"#,
        )
        .map_err(|e| e.message)?;
    let numbers: Vec<f64> = results.iter().filter_map(Value::as_number).collect();
    let angle16 = |a: f32| f64::from(q16_of(a, 0.0, TWO_PI));
    // −1 reduced: t = −1 × (1/2π), i = ⌈t⌉ = 0, t + 1, times 2π.
    let t = -INV_TWO_PI;
    let reduced = (t + 1.0) * TWO_PI;
    let t2 = (1.0 + 2.0 * std::f64::consts::PI) as f32 * INV_TWO_PI;
    let reduced2 = (t2 - t2.floor()) * TWO_PI;
    let want = [
        32768.0,
        128.0,
        angle16(1.0),
        f64::from(((1.0 / TWO_PI) * 255.0 + 0.5).floor()),
        angle16(reduced),
        angle16(reduced2),
        // TODO(#1240): a NaN angle, the simplest reading (NaN stores 1).
        1.0,
    ];
    ensure(numbers == want, || {
        format!("quantised writes: {numbers:?}, not {want:?}")
    })?;
    // The reader's reads (x87 at 24 bits; the values here are exact in
    // f32 operations too, but for the rounding of each step).
    check_code(
        "3.5 reader",
        &[0xFF, 0xFF, 0, 0, 0xFF, 0, 0x80, 0x80],
        r##"
local r = test_reader()
eq(r:r_float_q16(-1, 1), 1, "r_float_q16 of 65535")
eq(r:r_float_q16(2, 4), 2, "r_float_q16 of 0")
eq(math.abs(r:r_float_q8(0, 10) - 10) < 1e-5, true, "r_float_q8 of 255")
eq(r:r_angle8(), 0, "r_angle8 of 0")
local a = r:r_angle16()
eq(math.abs(a - 32896 / 65535 * 2 * math.pi) < 1e-6, true, "r_angle16")
eq(r:r_tell(), 8, "2, 2, 1, 1, 2 bytes")
"##,
    )
}

/// Section 3.6: the direction code and its decoding.
pub fn directions() -> Result<(), String> {
    let v = Vector::new;
    for (d, want) in [
        (v(0.0, 0.0, 1.0), 0u16),
        (v(1.0, 0.0, 0.0), 255),
        (v(0.0, 1.0, 0.0), 126),
        (v(0.0, 0.0, -1.0), 8192),
        (v(1.0, 1.0, 1.0), 5418),
        (v(-1.0, 2.0, -3.0), 43690),
        (v(0.0, 0.0, 0.0), 129),
    ] {
        let got = direction_code(d);
        ensure(got == want, || {
            format!("the code of {d:?}: {got}, not {want}")
        })?;
    }
    for (code, want) in [
        (0u16, v(0.0, 0.0, 1.0)),
        (255, v(1.0, 0.0, 0.0)),
        (126, v(0.0, 1.0, 0.0)),
        (8192, v(0.0, 0.0, -1.0)),
        (5418, v(0.577_350_26, 0.577_350_26, 0.577_350_26)),
    ] {
        let got = direction_of_code(code);
        ensure(got == want, || {
            format!("decoding {code}: {got:?}, not {want:?}")
        })?;
    }
    let got = direction_of_code(129);
    ensure(
        (got.x - 0.008_064).abs() < 1e-6
            && (got.y - 0.008_064).abs() < 1e-6
            && (got.z - 0.999_935).abs() < 1e-6,
        || format!("decoding 129: {got:?}"),
    )?;
    ensure(
        direction_of_code(8192).z.to_bits() == 0xBF80_0000
            && direction_of_code(1 << 15).x.to_bits() == 0x8000_0000,
        || "a set sign bit on 0 gives -0".into(),
    )?;
    ensure(f64::from(SDIR_MIN) == 1.000_000_011_686_097_4e-7, || {
        "w_sdir's threshold".into()
    })?;
    let (d, l) = scaled_direction(v(3.0, 0.0, 4.0));
    ensure(l == 5.0 && d == v(0.6, 0.0, 0.8), || {
        format!("w_sdir of (3, 0, 4): {d:?}, {l}")
    })?;
    let (d, l) = scaled_direction(v(1e-8, 0.0, 0.0));
    ensure(l == 0.0 && d == v(0.0, 0.0, 1.0), || {
        format!("w_sdir of a short vector: {d:?}, {l}")
    })?;
    check_code(
        "3.6",
        &[0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40],
        r##"
local p = net_packet()
p:w_dir(vector():set(2, 2, 2))
p:w_sdir(vector():set(0, 3, 0))
p:w_sdir(vector())
eq(p:w_tell(), 14, "2 + 6 + 6 bytes")
p:r_seek(0)
eq(p:r_u16(), 5418, "w_dir stores the code")
p:r_seek(0)
local v = vector():set(9, 9, 9)
eq(p:r_dir(v), nil, "r_dir gives nothing")
eq(v.x, 0.57735025882720947265625, "r_dir fills its argument")
p:r_sdir(v)
eq(v.x == 0 and v.y == 3 and v.z == 0, true, "r_sdir: the direction times the length")
p:r_seek(8)
eq(p:r_u16(), 0, "the zero vector's direction (0, 0, 1)")
eq(p:r_float(), 0, "and length 0")
local r = test_reader()
local w = vector()
r:r_dir(w)
eq(w.x == 1 and w.y == 0 and w.z == 0, true, "reader r_dir")
r:r_sdir(w)
eq(w.x == 0 and w.y == 0 and w.z == 2, true, "reader r_sdir")
"##,
    )?;
    fatal("net_packet():w_dir(nil)", NO_STORAGE)?;
    fatal("net_packet():w_sdir(1)", NO_STORAGE)?;
    fatal("test_reader():r_dir(nil)", NO_STORAGE)
}

/// Section 4: the reader.
pub fn reader() -> Result<(), String> {
    let mut block = vec![0xFE, 0xFF, b'h', b'i', 0];
    for x in [1.0f32, 2.0, 3.0] {
        block.extend(x.to_le_bytes());
    }
    check_code(
        "4",
        &block,
        r##"
local r = test_reader()
eq(r:r_elapsed(), 17, "size - position")
eq(r:r_s16(), -2, "r_s16")
eq(r:r_stringZ(), "hi", "r_stringZ")
local v = vector()
eq(r:r_vec3(v), nil, "r_vec3 gives nothing")
eq(v.x + v.y * 10 + v.z * 100, 321, "r_vec3 fills its argument")
eq(r:r_eof(), true, "r_eof at the end")
eq(r:r_elapsed(), 0, "nothing left")
r:r_seek(-3.5)
eq(r:r_tell(), -3, "r_seek To s32: a signed position")
eq(r:r_elapsed(), 20, "size - position")
eq(r:r_eof(), false, "r_eof before the start")
r:r_advance(3)
eq(r:r_tell(), 0, "r_advance")
r:r_advance(2^31)
eq(r:r_tell(), -2^31, "r_advance To s32 of 2^31")
r:r_seek(16)
eq(r:r_eof(), false, "one byte left")
r:r_advance(1)
eq(r:r_eof(), true, "size - position < 1")
r:r_advance(1)
eq(r:r_elapsed(), -1, "past the end, negative")
eq(r:r_eof(), true, "past the end")
"##,
    )?;
    fatal("local r = test_reader() r:r_seek(2) r:r_u16()", OUTSIDE)?;
    fatal("local r = test_reader() r:r_seek(-1) r:r_u8()", OUTSIDE)?;
    fatal("local r = test_reader() r:r_stringZ()", OUTSIDE)?;
    fatal("test_reader():r_vec3(nil)", OUTSIDE)
}

/// Section 5: `ClientID`.
pub fn client_id() -> Result<(), String> {
    check_code(
        "5",
        &[],
        r##"
local id = ClientID()
eq(id:value(), 0, "ClientID() holds 0")
eq(id:set(2^32 + 5), nil, "set gives nothing")
eq(id:value(), 5, "set To u32")
id:set(-1)
eq(id:value(), 2^32 - 1, "value in [0, 2^32)")
local other = ClientID()
eq(id == other, false, "different values")
other:set(-1)
eq(id == other, true, "equal values")
"##,
    )
}

/// Sections 3.1 and 6: an engine packet a script reads and writes in place,
/// and its release.
pub fn engine_streams() -> Result<(), String> {
    let (mut s, _) = state(&[])?;
    let mut held = NetPacket::holding(&[1, 0, 2, 0, 0]).ok_or("the bytes fit")?;
    held.read_position = 2;
    let part = s.new_packet(held).map_err(|e| e.message)?;
    s.exec(
        "=reads",
        b"function test_read(p, size) return p:r_u8(), p:r_bool(), p:r_bool(), p:r_eof(), size end\n\
          function test_write(p) p:w_u8(2) p:w_bool(false) end",
    )
    .map_err(|e| e.message)?;
    let results = s
        .call("test_read", &[Value::Object(part), Value::Number(5.0)])
        .map_err(|e| e.message)?;
    let want = [
        2.0.into(),
        false.into(),
        false.into(),
        true.into(),
        5.0.into(),
    ];
    ensure(results == want, || format!("the reads: {results:?}"))?;
    let positions = s.with_part(part, |p: &mut NetPacket| {
        (p.read_position, p.write_position)
    });
    ensure(positions == Some((5, 5)), || {
        format!("the engine's packet after the reads: {positions:?}")
    })?;
    s.call("test_write", &[Value::Object(part)])
        .map_err(|e| e.message)?;
    let written = s.with_part(part, |p: &mut NetPacket| p.written().to_vec());
    ensure(written == Some(vec![1, 0, 2, 0, 0, 2, 0]), || {
        format!("the engine's packet after the writes: {written:?}")
    })?;
    ensure(s.release_stream(part) == Ok(true), || "released".into())?;
    ensure(
        s.call("test_write", &[Value::Object(part)]).is_err(),
        || "a released packet's members fail".into(),
    )?;
    ensure(NetPacket::holding(&[0; 8193]).is_none(), || {
        "a message longer than the buffer".into()
    })
}
