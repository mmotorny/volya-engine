// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Script API I, data streams (docs/behavior/script-api-data-streams.md;
//! section numbers are its): the classes `net_packet` (a packet scripts
//! write to and read from, [`NetPacket`]), `reader` (a read-only stream
//! over a block of bytes, [`Reader`]) and `ClientID` ([`ClientId`]),
//! registered by [`Scripts::register_data_streams`].
//!
//! The engine hands scripts its own streams ([`Scripts::new_packet`],
//! [`Scripts::new_reader`]): engine objects scripts use in place, which the
//! engine reads back with [`Scripts::with_part`] and releases with
//! [`Scripts::release_stream`]. See docs/architecture/scripts.md, Data
//! streams.

use std::cell::RefCell;
use std::rc::Rc;

use crate::classes::{EngineClass, EnginePart, Objects, Operand};
use crate::loader::Scripts;
use crate::services::{EngineServices, Fatal, report_fatal};
use crate::values::{
    Matrix, Sse, Vector, X, lt, lua_number, to_bits32, to_f32, to_flags16, to_flags32, to_int32,
};
use crate::{Error, Value, number_text};

/// The packet's buffer, in bytes (3.1).
pub const PACKET_BUFFER: usize = 8192;

/// The class names (1).
pub const PACKET: &str = "net_packet";
pub const READER: &str = "reader";
pub const CLIENT_ID: &str = "ClientID";

/// 2π, the f32 6.2831855 (3.5).
pub const TWO_PI: f32 = 6.283_185_5;
/// 1/2π, the f32 0.15915494 (3.5).
pub const INV_TWO_PI: f32 = 0.159_154_94;
/// 1/65535 and 1/255, the f32 constants `0x37800080` and `0x3b80807d` (3.5).
pub const INV_65535: u32 = 0x3780_0080;
pub const INV_255: u32 = 0x3B80_807D;
/// `w_sdir`'s threshold, the f32 1.0000000116860974·10⁻⁷ (3.6).
pub const SDIR_MIN: f32 = 1.0e-7;

/// The sign bit of an f32.
const SIGN: u32 = 0x8000_0000;

/// The elements of a matrix a packet stores, in order (3.2, 3.3): rows 0
/// to 3, columns 0 to 2.
const MATRIX_ELEMENTS: [usize; 12] = [0, 1, 2, 4, 5, 6, 8, 9, 10, 12, 13, 14];

// --- The streams ----------------------------------------------------------

/// A `net_packet` (3.1): a buffer of [`PACKET_BUFFER`] bytes, a write
/// position and a read position. No read or write checks a position
/// against the other or the buffer's size; one reaching past the buffer is
/// believed to corrupt memory, here a fatal error (our choice).
#[derive(Clone)]
pub struct NetPacket {
    buffer: Box<[u8; PACKET_BUFFER]>,
    pub write_position: u32,
    pub read_position: u32,
}

impl Default for NetPacket {
    fn default() -> Self {
        Self::new()
    }
}

impl NetPacket {
    /// `net_packet()` (3.1): every byte 0, both positions 0.
    pub fn new() -> Self {
        NetPacket {
            buffer: Box::new([0; PACKET_BUFFER]),
            write_position: 0,
            read_position: 0,
        }
    }

    /// A packet holding `bytes` from its start, written up to their end and
    /// read from 0: the engine's packet holding a message it hands a script
    /// (a record's spawn packet, its message type at 0; 8). `None` if they
    /// don't fit the buffer.
    pub fn holding(bytes: &[u8]) -> Option<Self> {
        let mut packet = Self::new();
        packet.buffer.get_mut(..bytes.len())?.copy_from_slice(bytes);
        packet.write_position = bytes.len() as u32;
        Some(packet)
    }

    /// The whole buffer.
    pub fn buffer(&self) -> &[u8; PACKET_BUFFER] {
        &self.buffer
    }

    /// The bytes from the start to the write position (to the buffer's end
    /// if it is past it).
    pub fn written(&self) -> &[u8] {
        let end = (self.write_position as usize).min(PACKET_BUFFER);
        &self.buffer[..end]
    }

    /// Stores `bytes` at the write position and advances it (3.2).
    fn put(&mut self, bytes: &[u8]) -> Option<()> {
        let at = self.write_position as usize;
        let end = at.checked_add(bytes.len())?;
        self.buffer.get_mut(at..end)?.copy_from_slice(bytes);
        self.write_position = self.write_position.wrapping_add(bytes.len() as u32);
        Some(())
    }

    /// Stores `bytes` at byte `at`, the write position unchanged (3.4).
    fn store(&mut self, at: u32, bytes: &[u8]) -> Option<()> {
        let at = at as usize;
        let end = at.checked_add(bytes.len())?;
        self.buffer.get_mut(at..end)?.copy_from_slice(bytes);
        Some(())
    }
}

/// A `reader` (4): a stream over a block of bytes it does not own, with a
/// signed 32-bit position. A read reaching outside the block is believed
/// to read other data or fail, here a fatal error (our choice).
#[derive(Clone)]
pub struct Reader {
    data: Rc<[u8]>,
    pub position: i32,
}

impl Reader {
    /// A reader over `data` at its start.
    pub fn over(data: impl Into<Rc<[u8]>>) -> Self {
        Reader {
            data: data.into(),
            position: 0,
        }
    }

    /// The block's size.
    pub fn size(&self) -> i32 {
        self.data.len() as i32
    }
}

/// A `ClientID` (5): one `u32`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClientId(pub u32);

/// What the reads of both streams share: bytes taken at the read position,
/// which then advances by their number (3.3, 4).
trait Stream: 'static + Clone {
    const CLASS: &'static str;

    fn take<const N: usize>(&mut self) -> Option<[u8; N]>;

    /// The bytes up to the first zero byte, the position advanced past it;
    /// the search is limited by nothing but the end of the storage.
    fn text(&mut self) -> Option<Vec<u8>>;
}

impl Stream for NetPacket {
    const CLASS: &'static str = PACKET;

    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let at = self.read_position as usize;
        let bytes = self.buffer.get(at..at.checked_add(N)?)?.try_into().ok()?;
        self.read_position = self.read_position.wrapping_add(N as u32);
        Some(bytes)
    }

    fn text(&mut self) -> Option<Vec<u8>> {
        let rest = self.buffer.get(self.read_position as usize..)?;
        let len = rest.iter().position(|&b| b == 0)?;
        let text = rest[..len].to_vec();
        self.read_position = self.read_position.wrapping_add(len as u32 + 1);
        Some(text)
    }
}

impl Stream for Reader {
    const CLASS: &'static str = READER;

    fn take<const N: usize>(&mut self) -> Option<[u8; N]> {
        let at = usize::try_from(self.position).ok()?;
        let bytes = self.data.get(at..at.checked_add(N)?)?.try_into().ok()?;
        self.position = self.position.wrapping_add(N as i32);
        Some(bytes)
    }

    fn text(&mut self) -> Option<Vec<u8>> {
        let rest = self.data.get(usize::try_from(self.position).ok()?..)?;
        let len = rest.iter().position(|&b| b == 0)?;
        let text = rest[..len].to_vec();
        self.position = self.position.wrapping_add(len as i32 + 1);
        Some(text)
    }
}

// --- Conversions (2.2) ----------------------------------------------------

/// To u8 (2.2): truncated to a 32-bit integer, its low 8 bits; NaN or out
/// of range 0.
pub fn to_u8(n: f64) -> u8 {
    to_int32(n).map_or(0, |i| i as u8)
}

/// To u16 (2.2), as E's "To flags" for `flags16`.
pub fn to_u16(n: f64) -> u16 {
    to_flags16(n)
}

/// To u32 (2.2), as E's "To flags" for `flags32`.
pub fn to_u32(n: f64) -> u32 {
    to_flags32(n)
}

/// To s32 (2.2): the SSE2 conversion (that the run-time library's flag
/// for it is set is believed), NaN or out of range −2³¹.
pub fn to_s32(n: f64) -> i32 {
    to_bits32(n)
}

// --- Quantised values, angles and directions (3.5, 3.6) -------------------

/// The packet's floor (3.5): ⌊f⌋ for every finite f with |f| < 2³¹ but −0,
/// which gives −1; NaN and +∞ give 1, −∞ gives −1; f in [2³¹, 2³²) gives
/// ⌊f⌋ − 2³², and in (−2³², −2³¹] (the spec's −3·10⁹) ⌊f⌋ + 2³².
pub fn packet_floor(f: f32) -> i32 {
    if f.is_nan() {
        return 1;
    }
    if f == 0.0 && f.is_sign_negative() {
        return -1;
    }
    if f.abs() < 4_294_967_296.0 {
        return f64::from(f).floor() as i64 as i32;
    }
    // Beyond, the spec gives examples only (2³² → 1, 10¹⁰ → 2, ±∞ → ±1).
    // Our reading, which gives them: the significand as the top bits of a
    // 32-bit integer shifted right by (158 − e) modulo 32, e the biased
    // exponent, with the sign.
    let bits = f.abs().to_bits();
    let e = (bits >> 23) & 0xFF;
    let significand = ((bits & 0x7F_FFFF) | 0x80_0000) << 8;
    let r = (significand >> ((158 + 256 - e) % 32)) as i32;
    if f < 0.0 { r.wrapping_neg() } else { r }
}

/// The integer `w_float_q16` (`steps` 65535) and `w_float_q8` (255) store
/// the low bits of (3.5).
pub fn quantised(v: f32, min: f32, max: f32, steps: f32) -> i32 {
    let f = (Sse(v) - Sse(min)) / (Sse(max) - Sse(min)) * Sse(steps) + Sse(0.5);
    packet_floor(f.0)
}

/// An angle reduced to one turn as `w_angle16` and `w_angle8` do (3.5).
pub fn one_turn(a: f32) -> f32 {
    if !(lt(a, 0.0) || lt(TWO_PI, a) || a.is_nan()) {
        return a;
    }
    let t = Sse(a) * Sse(INV_TWO_PI);
    // TODO(#1240): the review's gap leaves i unstated for a NaN t and for
    // |t| ≥ 2³¹; the simplest reading: ⌊t⌋ and ⌈t⌉ as they are (a NaN i,
    // and t itself, an integer, beyond).
    let i = if lt(0.0, t.0) {
        t.0.floor()
    } else if t.0.to_bits() == SIGN {
        1.0
    } else {
        t.0.ceil()
    };
    let mut rest = t - Sse(i);
    if lt(rest.0, 0.0) {
        rest = rest + Sse(1.0);
    }
    (rest * Sse(TWO_PI)).0
}

/// The direction code of `v` (3.6).
pub fn direction_code(v: Vector) -> u16 {
    let sign = |x: f32, bit: u16| if x.is_sign_negative() { 1 << bit } else { 0 };
    let signs = sign(v.x, 15) | sign(v.y, 14) | sign(v.z, 13);
    let (x, y, z) = (Sse(v.x.abs()), Sse(v.y.abs()), Sse(v.z.abs()));
    let s = (z + y) + x;
    let k = Sse(126.0) / s;
    let mut cx = packet_floor((x * k).0);
    let mut cy = packet_floor((y * k).0);
    if cx >= 64 {
        cx = 127i32.wrapping_sub(cx);
        cy = 127i32.wrapping_sub(cy);
    }
    (cx.wrapping_mul(128).wrapping_add(cy) as u16).wrapping_add(signs)
}

/// X, Y and Z of code bits 0–12 (3.6): X bits 7–12, Y bits 0–6, folded
/// when X + Y ≥ 127, Z = 126 − X − Y.
fn direction_integers(i: u16) -> [i32; 3] {
    let mut x = i32::from((i >> 7) & 0x3F);
    let mut y = i32::from(i & 0x7F);
    if x + y >= 127 {
        x = 127 - x;
        y = 127 - y;
    }
    [x, y, 126 - x - y]
}

/// Entry `i` of the decoding table (3.6): 1/√(X² + Y² + Z²) computed at the
/// x87 unit's precision when the table is built, believed 53 bits (f64),
/// stored as f32.
pub fn direction_scale(i: u16) -> f32 {
    let [x, y, z] = direction_integers(i % 8192).map(f64::from);
    (1.0 / (x * x + y * y + z * z).sqrt()) as f32
}

/// The vector of direction code `c` (3.6).
pub fn direction_of_code(c: u16) -> Vector {
    let t = Sse(direction_scale(c));
    let [x, y, z] = direction_integers(c % 8192).map(|n| (Sse(n as f32) * t).0);
    let signed = |v: f32, bit: u16| {
        if c & (1 << bit) != 0 {
            f32::from_bits(v.to_bits() | SIGN)
        } else {
            v
        }
    };
    Vector::new(signed(x, 15), signed(y, 14), signed(z, 13))
}

/// The direction and length `w_sdir` stores (3.6).
pub fn scaled_direction(v: Vector) -> (Vector, f32) {
    let square = |x: f32| Sse(x) * Sse(x);
    let sum = (square(v.x) + square(v.y)) + square(v.z);
    let root = X::of(sum.0).sqrt();
    // Compared before the root is rounded to f32.
    if root.0 <= f64::from(SDIR_MIN) {
        return (Vector::new(0.0, 0.0, 1.0), 0.0);
    }
    let length = root.store();
    let u = Sse(1.0) / Sse(length);
    let direction = Vector::new((Sse(v.x) * u).0, (Sse(v.y) * u).0, (Sse(v.z) * u).0);
    (direction, length)
}

/// `w_sdir`'s vector read back (3.6): the decoding times the length.
fn scaled(code: u16, length: f32) -> Vector {
    let d = direction_of_code(code);
    let l = Sse(length);
    Vector::new((Sse(d.x) * l).0, (Sse(d.y) * l).0, (Sse(d.z) * l).0)
}

/// The quantised and angle reads of a `reader` (3.5, 4), in the x87 unit
/// (E's rule for X[…]), stored as f32.
pub fn float_q16(q: u16, min: f32, max: f32) -> f32 {
    let q = X(f64::from(q));
    ((X::of(max) - X::of(min)) * q * X::of(f32::from_bits(INV_65535)) + X::of(min)).store()
}

pub fn float_q8(q: u8, min: f32, max: f32) -> f32 {
    let q = X(f64::from(q));
    (q * X::of(f32::from_bits(INV_255)) * (X::of(max) - X::of(min)) + X::of(min)).store()
}

pub fn angle16(q: u16) -> f32 {
    (X(f64::from(q)) * X::of(TWO_PI) * X::of(f32::from_bits(INV_65535))).store()
}

pub fn angle8(q: u8) -> f32 {
    (X(f64::from(q)) * X::of(f32::from_bits(INV_255)) * X::of(TWO_PI)).store()
}

// --- Registration -----------------------------------------------------------

#[derive(Clone)]
struct Context {
    services: Rc<RefCell<EngineServices>>,
    fatal: Rc<RefCell<Option<String>>>,
}

/// A read or write reaching outside the stream's storage (3.1, 4; our
/// wording).
const OUTSIDE: &str = "a stream read or write outside its storage";
/// A value given where the method stores through a reference no script can
/// pass, or takes an object that isn't one (2.1, 2.2, 3.3; our wording).
const NO_STORAGE: &str = "no storage behind a stream method's argument";

impl Context {
    fn stopped(&self) -> Result<(), String> {
        match &*self.fatal.borrow() {
            Some(message) => Err(message.clone()),
            None => Ok(()),
        }
    }

    /// A fatal error standing for an access violation the spec believes
    /// (our wording).
    fn crash(&self, function: &str, line: u32, text: &str) -> String {
        report_fatal(
            &self.services,
            &self.fatal,
            Fatal {
                function,
                file: file!(),
                line,
                expression: "fatal error",
                description: "<no expression>",
                arguments: &[text],
            },
        )
    }
}

fn arg(args: &[Value], i: usize) -> &Value {
    args.get(i).unwrap_or(&Value::Nil)
}

/// A number argument (2.2): Lua's conversion to a number, 0 for no value
/// or one that isn't one.
fn number(args: &[Value], i: usize) -> f64 {
    lua_number(arg(args, i))
}

fn f32_arg(args: &[Value], i: usize) -> f32 {
    to_f32(number(args, i))
}

/// Only the value form of a two-form read matches (2.1): the call passes
/// nothing but the object. What follows otherwise depends on A; ours, a
/// raised error.
fn value_form(args: &[Value], what: &str) -> Result<(), String> {
    if args.is_empty() {
        Ok(())
    } else {
        Err(format!("{what}: no form of it takes these arguments"))
    }
}

/// A read of `N` bytes made a value (2.3).
fn read_method<T: Stream, const N: usize>(
    class: EngineClass,
    c: &Context,
    name: &'static str,
    two_forms: bool,
    value: fn([u8; N]) -> Value,
) -> EngineClass {
    let c = c.clone();
    let what = format!("{}.{name}", T::CLASS);
    class.method(name, move |s: &mut T, args| {
        c.stopped()?;
        if two_forms {
            value_form(args, &what)?;
        }
        let bytes = s
            .take::<N>()
            .ok_or_else(|| c.crash(&what, line!(), OUTSIDE))?;
        Ok(vec![value(bytes)])
    })
}

fn num(x: f64) -> Value {
    Value::Number(x)
}

/// The reads both streams have (3.3, 4).
fn common_reads<T: Stream>(mut class: EngineClass, c: &Context) -> EngineClass {
    class = read_method::<T, 1>(class, c, "r_u8", true, |b| num(f64::from(b[0])));
    class = read_method::<T, 1>(class, c, "r_s8", true, |b| num(f64::from(b[0] as i8)));
    class = read_method::<T, 2>(class, c, "r_u16", true, |b| {
        num(f64::from(u16::from_le_bytes(b)))
    });
    class = read_method::<T, 2>(class, c, "r_s16", true, |b| {
        num(f64::from(i16::from_le_bytes(b)))
    });
    class = read_method::<T, 4>(class, c, "r_u32", true, |b| {
        num(f64::from(u32::from_le_bytes(b)))
    });
    class = read_method::<T, 4>(class, c, "r_s32", true, |b| {
        num(f64::from(i32::from_le_bytes(b)))
    });
    class = read_method::<T, 4>(class, c, "r_float", true, |b| {
        num(f64::from(f32::from_le_bytes(b)))
    });
    class = read_method::<T, 1>(class, c, "r_bool", false, |b| Value::Boolean(b[0] != 0));
    // 2.3: no 64-bit integer class is registered; the 8 bytes are read,
    // then the result is believed to fail.
    for name in ["r_u64", "r_s64"] {
        let c = c.clone();
        let what = format!("{}.{name}", T::CLASS);
        class = class.method(name, move |s: &mut T, args| {
            c.stopped()?;
            value_form(args, &what)?;
            s.take::<8>()
                .ok_or_else(|| c.crash(&what, line!(), OUTSIDE))?;
            Err(c.crash(&what, line!(), "no class of a 64-bit integer"))
        });
    }
    let cc = c.clone();
    let what = format!("{}.r_stringZ", T::CLASS);
    class = class.method("r_stringZ", move |s: &mut T, _| {
        cc.stopped()?;
        let text = s.text().ok_or_else(|| cc.crash(&what, line!(), OUTSIDE))?;
        Ok(vec![Value::String(text)])
    });
    // 3.6: fill the vector passed.
    let cc = c.clone();
    let what = format!("{}.r_dir", T::CLASS);
    class = class.method_with_objects("r_dir", move |s: &mut T, o, args| {
        cc.stopped()?;
        let code = s
            .take::<2>()
            .ok_or_else(|| cc.crash(&what, line!(), OUTSIDE))?;
        fill(
            &cc,
            &what,
            o,
            arg(args, 0),
            direction_of_code(u16::from_le_bytes(code)),
        )
    });
    let cc = c.clone();
    let what = format!("{}.r_sdir", T::CLASS);
    class = class.method_with_objects("r_sdir", move |s: &mut T, o, args| {
        cc.stopped()?;
        let code = s
            .take::<2>()
            .ok_or_else(|| cc.crash(&what, line!(), OUTSIDE))?;
        let length = s
            .take::<4>()
            .ok_or_else(|| cc.crash(&what, line!(), OUTSIDE))?;
        let v = scaled(u16::from_le_bytes(code), f32::from_le_bytes(length));
        fill(&cc, &what, o, arg(args, 0), v)
    });
    // 4: the positions, `T`'s own.
    class
}

/// Sets vector argument `v` to `value`; anything else is believed to fail
/// (2.2).
fn fill(
    c: &Context,
    what: &str,
    o: &Objects,
    v: &Value,
    value: Vector,
) -> Result<Vec<Value>, String> {
    if o.set::<Vector>(v, value) {
        Ok(Vec::new())
    } else {
        Err(c.crash(what, line!(), NO_STORAGE))
    }
}

/// The vector argument `v`; anything else is believed to fail (2.2).
fn vector_arg(c: &Context, what: &str, o: &Objects, v: &Value) -> Result<Vector, String> {
    match o.get::<Vector>(v) {
        Some(Operand::Other(v)) => Ok(v),
        _ => Err(c.crash(what, line!(), NO_STORAGE)),
    }
}

fn f32s(v: Vector) -> [u8; 12] {
    let mut out = [0; 12];
    for (i, x) in [v.x, v.y, v.z].into_iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&x.to_le_bytes());
    }
    out
}

fn vector_of(b: [u8; 12]) -> Vector {
    let f = |i: usize| f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
    Vector::new(f(0), f(4), f(8))
}

/// A write method of `net_packet`: the bytes `bytes` makes of the
/// arguments, stored at the write position (3.2).
fn write_method(
    class: EngineClass,
    c: &Context,
    name: &'static str,
    bytes: impl Fn(&[Value]) -> Vec<u8> + 'static,
) -> EngineClass {
    let c = c.clone();
    let what = format!("{PACKET}.{name}");
    class.method(name, move |p: &mut NetPacket, args| {
        c.stopped()?;
        p.put(&bytes(args))
            .ok_or_else(|| c.crash(&what, line!(), OUTSIDE))?;
        Ok(Vec::new())
    })
}

fn packet_class(c: &Context) -> EngineClass {
    let mut class = EngineClass::new(PACKET)
        .constructor(0, |_| Some(0), |_, _| Ok(NetPacket::new()))
        .constructible();

    // 3.2: the writes.
    class = write_method(class, c, "w_u8", |a| vec![to_u8(number(a, 0))]);
    class = write_method(class, c, "w_u16", |a| {
        to_u16(number(a, 0)).to_le_bytes().to_vec()
    });
    class = write_method(class, c, "w_s16", |a| {
        (to_s32(number(a, 0)) as u16).to_le_bytes().to_vec()
    });
    class = write_method(class, c, "w_u24", |a| {
        to_u32(number(a, 0)).to_le_bytes()[..3].to_vec()
    });
    class = write_method(class, c, "w_u32", |a| {
        to_u32(number(a, 0)).to_le_bytes().to_vec()
    });
    class = write_method(class, c, "w_s32", |a| {
        to_s32(number(a, 0)).to_le_bytes().to_vec()
    });
    class = write_method(class, c, "w_float", |a| {
        f32_arg(a, 0).to_le_bytes().to_vec()
    });
    class = write_method(class, c, "w_bool", |a| {
        vec![u8::from(!matches!(
            arg(a, 0),
            Value::Nil | Value::Boolean(false)
        ))]
    });
    // 3.5.
    class = write_method(class, c, "w_float_q16", |a| {
        let q = quantised(f32_arg(a, 0), f32_arg(a, 1), f32_arg(a, 2), 65535.0);
        (q as u16).to_le_bytes().to_vec()
    });
    class = write_method(class, c, "w_float_q8", |a| {
        vec![quantised(f32_arg(a, 0), f32_arg(a, 1), f32_arg(a, 2), 255.0) as u8]
    });
    class = write_method(class, c, "w_angle16", |a| {
        let q = quantised(one_turn(f32_arg(a, 0)), 0.0, TWO_PI, 65535.0);
        (q as u16).to_le_bytes().to_vec()
    });
    class = write_method(class, c, "w_angle8", |a| {
        vec![quantised(one_turn(f32_arg(a, 0)), 0.0, TWO_PI, 255.0) as u8]
    });
    // 2.2: no 64-bit integer class is registered; every call is believed
    // to fail.
    for name in ["w_u64", "w_s64"] {
        let c = c.clone();
        let what = format!("{PACKET}.{name}");
        class = class.method(name, move |_: &mut NetPacket, _| {
            c.stopped()?;
            Err(c.crash(&what, line!(), "no class of a 64-bit integer"))
        });
    }
    let cc = c.clone();
    class = class.method("w_stringZ", move |p: &mut NetPacket, args| {
        cc.stopped()?;
        let what = "net_packet.w_stringZ";
        let mut text = match arg(args, 0) {
            Value::String(s) => s.split(|&b| b == 0).next().unwrap_or_default().to_vec(),
            Value::Number(n) => number_text(*n).into_bytes(),
            _ => return Err(cc.crash(what, line!(), "no text of the value")),
        };
        text.push(0);
        p.put(&text)
            .ok_or_else(|| cc.crash(what, line!(), OUTSIDE))?;
        Ok(Vec::new())
    });
    // Objects (2.2).
    let cc = c.clone();
    class = class.method_with_objects("w_vec3", move |p: &mut NetPacket, o, args| {
        cc.stopped()?;
        let what = "net_packet.w_vec3";
        let v = vector_arg(&cc, what, o, arg(args, 0))?;
        p.put(&f32s(v))
            .ok_or_else(|| cc.crash(what, line!(), OUTSIDE))?;
        Ok(Vec::new())
    });
    let cc = c.clone();
    class = class.method_with_objects("w_dir", move |p: &mut NetPacket, o, args| {
        cc.stopped()?;
        let what = "net_packet.w_dir";
        let v = vector_arg(&cc, what, o, arg(args, 0))?;
        p.put(&direction_code(v).to_le_bytes())
            .ok_or_else(|| cc.crash(what, line!(), OUTSIDE))?;
        Ok(Vec::new())
    });
    let cc = c.clone();
    class = class.method_with_objects("w_sdir", move |p: &mut NetPacket, o, args| {
        cc.stopped()?;
        let what = "net_packet.w_sdir";
        let v = vector_arg(&cc, what, o, arg(args, 0))?;
        let (direction, length) = scaled_direction(v);
        let mut bytes = direction_code(direction).to_le_bytes().to_vec();
        bytes.extend(length.to_le_bytes());
        p.put(&bytes)
            .ok_or_else(|| cc.crash(what, line!(), OUTSIDE))?;
        Ok(Vec::new())
    });
    let cc = c.clone();
    class = class.method_with_objects("w_matrix", move |p: &mut NetPacket, o, args| {
        cc.stopped()?;
        let what = "net_packet.w_matrix";
        let Some(Operand::Other(m)) = o.get::<Matrix>(arg(args, 0)) else {
            return Err(cc.crash(what, line!(), NO_STORAGE));
        };
        let bytes: Vec<u8> = MATRIX_ELEMENTS
            .iter()
            .flat_map(|&i| m.0[i].to_le_bytes())
            .collect();
        p.put(&bytes)
            .ok_or_else(|| cc.crash(what, line!(), OUTSIDE))?;
        Ok(Vec::new())
    });
    let cc = c.clone();
    class = class.method_with_objects("w_clientID", move |p: &mut NetPacket, o, args| {
        cc.stopped()?;
        let what = "net_packet.w_clientID";
        let Some(Operand::Other(id)) = o.get::<ClientId>(arg(args, 0)) else {
            return Err(cc.crash(what, line!(), NO_STORAGE));
        };
        p.put(&id.0.to_le_bytes())
            .ok_or_else(|| cc.crash(what, line!(), OUTSIDE))?;
        Ok(Vec::new())
    });

    // 3.4: positions and chunks.
    let cc = c.clone();
    class = class.method("w_begin", move |p: &mut NetPacket, args| {
        cc.stopped()?;
        p.write_position = 0;
        p.put(&to_u16(number(args, 0)).to_le_bytes())
            .ok_or_else(|| cc.crash("net_packet.w_begin", line!(), OUTSIDE))?;
        Ok(Vec::new())
    });
    class = class.method("w_tell", |p: &mut NetPacket, _| {
        Ok(vec![f64::from(p.write_position).into()])
    });
    class = class.method("r_tell", |p: &mut NetPacket, _| {
        Ok(vec![f64::from(p.read_position).into()])
    });
    class = class.method("r_seek", |p: &mut NetPacket, args| {
        p.read_position = to_u32(number(args, 0));
        Ok(Vec::new())
    });
    class = class.method("r_advance", |p: &mut NetPacket, args| {
        p.read_position = p.read_position.wrapping_add(to_u32(number(args, 0)));
        Ok(Vec::new())
    });
    class = class.method("r_elapsed", |p: &mut NetPacket, _| {
        Ok(vec![
            f64::from(p.write_position.wrapping_sub(p.read_position)).into(),
        ])
    });
    class = class.method("r_eof", |p: &mut NetPacket, _| {
        Ok(vec![(p.write_position <= p.read_position).into()])
    });
    for (name, width) in [("w_chunk_close8", 1u32), ("w_chunk_close16", 2)] {
        let cc = c.clone();
        let what = format!("{PACKET}.{name}");
        class = class.method(name, move |p: &mut NetPacket, args| {
            cc.stopped()?;
            let at = to_u32(number(args, 0));
            let length = p.write_position.wrapping_sub(at).wrapping_sub(width);
            let bytes = &length.to_le_bytes()[..width as usize];
            p.store(at, bytes)
                .ok_or_else(|| cc.crash(&what, line!(), OUTSIDE))?;
            Ok(Vec::new())
        });
    }
    // 3.3, 3.4: the methods giving a result through a reference to a
    // number, which no script can pass: they read or write, then are
    // believed to fail storing it.
    for (name, width) in [("w_chunk_open8", 1usize), ("w_chunk_open16", 2)] {
        let cc = c.clone();
        let what = format!("{PACKET}.{name}");
        class = class.method(name, move |p: &mut NetPacket, _| {
            cc.stopped()?;
            p.put(&[0, 0][..width])
                .ok_or_else(|| cc.crash(&what, line!(), OUTSIDE))?;
            Err(cc.crash(&what, line!(), NO_STORAGE))
        });
    }
    for (name, width) in [
        ("r_float_q8", 1usize),
        ("r_float_q16", 2),
        ("r_angle8", 1),
        ("r_angle16", 2),
        ("r_begin", 2),
    ] {
        let cc = c.clone();
        let what = format!("{PACKET}.{name}");
        class = class.method(name, move |p: &mut NetPacket, _| {
            cc.stopped()?;
            if name == "r_begin" {
                p.read_position = 0;
            }
            let read = if width == 1 {
                p.take::<1>().map(drop)
            } else {
                p.take::<2>().map(drop)
            };
            read.ok_or_else(|| cc.crash(&what, line!(), OUTSIDE))?;
            Err(cc.crash(&what, line!(), NO_STORAGE))
        });
    }

    // 3.3: the reads.
    class = common_reads::<NetPacket>(class, c);
    class = read_method::<NetPacket, 3>(class, c, "r_u24", true, |b| {
        Value::Number(f64::from(u32::from_le_bytes([b[0], b[1], b[2], 0])))
    });
    let cc = c.clone();
    class = class.method_with_objects("r_clientID", move |p: &mut NetPacket, o, _| {
        cc.stopped()?;
        let b = p
            .take::<4>()
            .ok_or_else(|| cc.crash("net_packet.r_clientID", line!(), OUTSIDE))?;
        Ok(vec![
            o.new_object(CLIENT_ID, ClientId(u32::from_le_bytes(b))),
        ])
    });
    // A new vector; the argument is neither checked nor changed.
    let cc = c.clone();
    class = class.method_with_objects("r_vec3", move |p: &mut NetPacket, o, _| {
        cc.stopped()?;
        let b = p
            .take::<12>()
            .ok_or_else(|| cc.crash("net_packet.r_vec3", line!(), OUTSIDE))?;
        Ok(vec![o.new_object(crate::values::VECTOR, vector_of(b))])
    });
    let cc = c.clone();
    class = class.method_with_objects("r_matrix", move |p: &mut NetPacket, o, args| {
        cc.stopped()?;
        let what = "net_packet.r_matrix";
        let b = p
            .take::<48>()
            .ok_or_else(|| cc.crash(what, line!(), OUTSIDE))?;
        let mut m = Matrix([0.0; 16]);
        for (k, &i) in MATRIX_ELEMENTS.iter().enumerate() {
            m.0[i] = f32::from_le_bytes([b[4 * k], b[4 * k + 1], b[4 * k + 2], b[4 * k + 3]]);
        }
        m.0[15] = 1.0;
        if o.set::<Matrix>(arg(args, 0), m) {
            Ok(Vec::new())
        } else {
            Err(cc.crash(what, line!(), NO_STORAGE))
        }
    });
    class
}

fn reader_class(c: &Context) -> EngineClass {
    // No constructor (4).
    let mut class = common_reads::<Reader>(EngineClass::new(READER), c);
    class = class.method("r_tell", |r: &mut Reader, _| {
        Ok(vec![f64::from(r.position).into()])
    });
    class = class.method("r_seek", |r: &mut Reader, args| {
        r.position = to_s32(number(args, 0));
        Ok(Vec::new())
    });
    class = class.method("r_advance", |r: &mut Reader, args| {
        r.position = r.position.wrapping_add(to_s32(number(args, 0)));
        Ok(Vec::new())
    });
    class = class.method("r_elapsed", |r: &mut Reader, _| {
        Ok(vec![f64::from(r.size().wrapping_sub(r.position)).into()])
    });
    class = class.method("r_eof", |r: &mut Reader, _| {
        Ok(vec![(r.size().wrapping_sub(r.position) < 1).into()])
    });
    // Fills the vector passed, unlike the packet's.
    let cc = c.clone();
    class = class.method_with_objects("r_vec3", move |r: &mut Reader, o, args| {
        cc.stopped()?;
        let what = "reader.r_vec3";
        let b = r
            .take::<12>()
            .ok_or_else(|| cc.crash(what, line!(), OUTSIDE))?;
        fill(&cc, what, o, arg(args, 0), vector_of(b))
    });
    // 3.5: the value forms of the quantised and angle reads.
    let cc = c.clone();
    class = class.method("r_float_q16", move |r: &mut Reader, args| {
        cc.stopped()?;
        let b = r
            .take::<2>()
            .ok_or_else(|| cc.crash("reader.r_float_q16", line!(), OUTSIDE))?;
        let v = float_q16(u16::from_le_bytes(b), f32_arg(args, 0), f32_arg(args, 1));
        Ok(vec![f64::from(v).into()])
    });
    let cc = c.clone();
    class = class.method("r_float_q8", move |r: &mut Reader, args| {
        cc.stopped()?;
        let b = r
            .take::<1>()
            .ok_or_else(|| cc.crash("reader.r_float_q8", line!(), OUTSIDE))?;
        let v = float_q8(b[0], f32_arg(args, 0), f32_arg(args, 1));
        Ok(vec![f64::from(v).into()])
    });
    let cc = c.clone();
    class = class.method("r_angle16", move |r: &mut Reader, _| {
        cc.stopped()?;
        let b = r
            .take::<2>()
            .ok_or_else(|| cc.crash("reader.r_angle16", line!(), OUTSIDE))?;
        Ok(vec![f64::from(angle16(u16::from_le_bytes(b))).into()])
    });
    let cc = c.clone();
    class = class.method("r_angle8", move |r: &mut Reader, _| {
        cc.stopped()?;
        let b = r
            .take::<1>()
            .ok_or_else(|| cc.crash("reader.r_angle8", line!(), OUTSIDE))?;
        Ok(vec![f64::from(angle8(b[0])).into()])
    });
    class
}

fn client_id_class() -> EngineClass {
    EngineClass::new(CLIENT_ID)
        .constructor(0, |_| Some(0), |_, _| Ok(ClientId(0)))
        .constructible()
        .method("value", |id: &mut ClientId, _| {
            Ok(vec![f64::from(id.0).into()])
        })
        .method("set", |id: &mut ClientId, args| {
            id.0 = to_u32(number(args, 0));
            Ok(Vec::new())
        })
        .method_with_objects("__eq", |id: &mut ClientId, o, args| {
            let equal = match o.get::<ClientId>(arg(args, 0)) {
                Some(Operand::This) => true,
                Some(Operand::Other(other)) => other == *id,
                None => false,
            };
            Ok(vec![equal.into()])
        })
}

impl Scripts {
    /// Registers `net_packet`, `reader` and `ClientID` (1), before
    /// [`Scripts::load_globals`]. The believed access violations are fatal
    /// errors reported to the services' console (our wording).
    pub fn register_data_streams(
        &mut self,
        services: Rc<RefCell<EngineServices>>,
    ) -> Result<(), Error> {
        let ctx = Context {
            services,
            fatal: self.fatal_flag(),
        };
        self.register_class(client_id_class())?;
        self.register_class(packet_class(&ctx))?;
        self.register_class(reader_class(&ctx))
    }

    /// An engine packet holding `packet` (3.1), for the engine to hand to
    /// scripts as `Value::Object(part)`: they use it in place. The engine
    /// reads it back with [`Scripts::with_part`] and releases it with
    /// [`Scripts::release_stream`].
    pub fn new_packet(&mut self, packet: NetPacket) -> Result<EnginePart, Error> {
        self.guarded(|lua| lua.new_engine_object(PACKET, |_| packet))
    }

    /// An engine reader holding `reader` (4), as [`Scripts::new_packet`].
    pub fn new_reader(&mut self, reader: Reader) -> Result<EnginePart, Error> {
        self.guarded(|lua| lua.new_engine_object(READER, |_| reader))
    }

    /// Releases an engine stream once the call it was handed to returns
    /// (believed: scripts must not keep it, 10); members called on it later
    /// fail. `false` if no engine object holds `part`.
    pub fn release_stream(&mut self, part: EnginePart) -> Result<bool, Error> {
        self.guarded(|lua| lua.release_engine_object(part))
    }
}
