// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Script API E, value types and math
//! (docs/behavior/script-api-value-types-and-math.md; section numbers are
//! its): the engine classes `vector`, `matrix`, `Frect`, `Fbox`, `RPoint`,
//! `fcolor`, `color`, `noise`, `duality`, `flags32` and `flags16`, and the
//! global functions `bit_and`, `bit_or`, `bit_xor` and `bit_not`,
//! registered on the class system ([`crate::classes`]) by [`register`].
//!
//! Arithmetic follows 2.5: SSE single-precision operations ([`Sse`], with
//! flush-to-zero and denormals-are-zero, believed in effect) and the x87
//! unit at 24-bit precision ([`X`]), each operation in the order the spec
//! writes it. See docs/architecture/scripts.md, Value types.

use std::any::Any;
use std::ops::{Add, Div, Mul, Sub};

use crate::classes::{EngineClass, Objects, Operand};
use crate::libc::string_number;
use crate::{Error, Lua, Value};

// --- Arithmetic (2.5) -------------------------------------------------------

/// A subnormal as the zero of its sign (flush-to-zero, denormals-are-zero).
fn flush(x: f32) -> f32 {
    if x.is_subnormal() {
        0.0f32.copysign(x)
    } else {
        x
    }
}

/// An f32 in the SSE unit: each `+ − × /` is the IEEE single-precision
/// operation, rounded to nearest, with subnormal operands and results as
/// zero (2.5; that those modes are in effect is believed).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sse(pub f32);

macro_rules! sse_op {
    ($trait:ident, $f:ident, $op:tt) => {
        impl $trait for Sse {
            type Output = Sse;
            fn $f(self, b: Sse) -> Sse {
                Sse(flush(flush(self.0) $op flush(b.0)))
            }
        }
    };
}
sse_op!(Add, add, +);
sse_op!(Sub, sub, -);
sse_op!(Mul, mul, *);
sse_op!(Div, div, /);

impl Sse {
    /// `−0 − a` (2, conventions).
    fn neg(self) -> Sse {
        Sse(-0.0) - self
    }
}

/// An SSE comparison `a < b` (denormals as zero; false with a NaN).
fn lt(a: f32, b: f32) -> bool {
    flush(a) < flush(b)
}

/// `x` rounded to a 24-bit significand, ties to even, the exponent range
/// unbounded (f64's is wide enough for every value here).
fn round24(x: f64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    const DROP: u32 = 52 - 23;
    let bits = x.to_bits();
    let low = bits & ((1 << DROP) - 1);
    let half = 1 << (DROP - 1);
    let mut kept = bits - low;
    if low > half || (low == half && (kept >> DROP) & 1 == 1) {
        // A carry into the exponent is the next binade's value.
        kept += 1 << DROP;
    }
    f64::from_bits(kept)
}

/// A value in the x87 unit at 24-bit precision, round to nearest (the rule
/// for X[…], 2.5; believed in effect): `+ − × /` and square roots round to
/// a 24-bit significand, the exponent range stays wider; sine, cosine and
/// arctangent are unrounded (f64 stands in for the processor's extended
/// results, established only up to them, 12). Computing in f64 and rounding
/// once to 24 bits equals the 24-bit operation: 53 ≥ 2 × 24 + 2.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct X(pub f64);

macro_rules! x87_op {
    ($trait:ident, $f:ident, $op:tt) => {
        impl $trait for X {
            type Output = X;
            fn $f(self, b: X) -> X {
                X(round24(self.0 $op b.0))
            }
        }
    };
}
x87_op!(Add, add, +);
x87_op!(Sub, sub, -);
x87_op!(Mul, mul, *);
x87_op!(Div, div, /);

impl X {
    /// An f32 loaded into the x87 unit (exact).
    pub fn of(x: f32) -> X {
        X(f64::from(x))
    }

    fn sqrt(self) -> X {
        X(round24(self.0.sqrt()))
    }

    fn neg(self) -> X {
        X(-self.0)
    }

    fn sin(self) -> X {
        X(self.0.sin())
    }

    fn cos(self) -> X {
        X(self.0.cos())
    }

    fn atan(self) -> X {
        X(self.0.atan())
    }

    /// Stored as an f32: rounded to nearest (r(…)).
    pub fn store(self) -> f32 {
        self.0 as f32
    }
}

/// e of `getH` and `getP`: `0x33D6BF95`.
const ANGLE_EPS: u32 = 0x33D6_BF95;
/// π/2 as f32, `0x3FC90FDB`.
const HALF_PI: u32 = 0x3FC9_0FDB;
/// π as f32, `0x40490FDB`.
const PI: u32 = 0x4049_0FDB;
/// The smallest normal f32, 2⁻¹²⁶, `normalize`'s threshold.
const SMALLEST_NORMAL: u32 = 0x0080_0000;
/// `fcolor:set(number)`'s factor, `0x3B808081`.
const BYTE_SCALE: u32 = 0x3B80_8081;

// --- Conversions (2.2, 2.4, 8) ----------------------------------------------

/// A `number` argument to f32: one rounding to nearest (the x87 store;
/// overflow gives ±infinity, believed, 2.2).
pub fn to_f32(n: f64) -> f32 {
    n as f32
}

/// A `number` argument of `flags32` (and `fcolor:set(number)`): truncated
/// to a 64-bit integer, of which the low 32 bits; NaN or |n| ≥ 2⁶³ give 0.
pub fn to_flags32(n: f64) -> u32 {
    const LIMIT: f64 = 9_223_372_036_854_775_808.0;
    if n.is_nan() || n.abs() >= LIMIT {
        0
    } else {
        n.trunc() as i64 as u32
    }
}

/// A `number` argument of `flags16`: truncated to a 32-bit integer, of
/// which the low 16 bits; NaN or a value outside [−2³¹, 2³¹) give 0.
pub fn to_flags16(n: f64) -> u16 {
    to_int32(n).map_or(0, |i| i as u16)
}

/// `n` truncated toward zero, if that lies in [−2³¹, 2³¹).
fn to_int32(n: f64) -> Option<i32> {
    let t = n.trunc();
    (-2_147_483_648.0..2_147_483_648.0)
        .contains(&t)
        .then_some(t as i32)
}

/// The bit functions' conversion with SSE2 (8): truncated toward zero; NaN
/// or out of range give −2³¹ (that every processor reports SSE2 is
/// believed).
pub fn to_bits32(n: f64) -> i32 {
    to_int32(n).unwrap_or(i32::MIN)
}

/// Lua's number conversion of an assigned property value (2.4), without a
/// type test: a numeric string gives its number, anything else 0.
pub(crate) fn lua_number(v: &Value) -> f64 {
    match v {
        Value::Number(n) => *n,
        Value::String(s) => string_number(s).unwrap_or(0.0),
        _ => 0.0,
    }
}

// --- The value types ----------------------------------------------------------

/// A `vector` (3).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vector {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vector {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Vector { x, y, z }
    }

    fn map(self, f: impl Fn(f32) -> f32) -> Vector {
        Vector::new(f(self.x), f(self.y), f(self.z))
    }

    fn zip(self, w: Vector, f: impl Fn(f32, f32) -> f32) -> Vector {
        Vector::new(f(self.x, w.x), f(self.y, w.y), f(self.z, w.z))
    }

    fn zip3(self, b: Vector, c: Vector, f: impl Fn(f32, f32, f32) -> f32) -> Vector {
        Vector::new(
            f(self.x, b.x, c.x),
            f(self.y, b.y, c.y),
            f(self.z, b.z, c.z),
        )
    }

    /// `x × x + y × y + z × z` in the SSE unit.
    fn square(self) -> Sse {
        Sse(self.x) * Sse(self.x) + Sse(self.y) * Sse(self.y) + Sse(self.z) * Sse(self.z)
    }
}

/// A `matrix` (4): element 4r + s is row r, column s.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Matrix(pub [f32; 16]);

impl Matrix {
    fn row(&self, r: usize) -> Vector {
        Vector::new(self.0[4 * r], self.0[4 * r + 1], self.0[4 * r + 2])
    }

    fn set_row(&mut self, r: usize, v: Vector) {
        self.0[4 * r] = v.x;
        self.0[4 * r + 1] = v.y;
        self.0[4 * r + 2] = v.z;
    }
}

/// A `Frect` (5.1).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Frect {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

/// An `Fbox` (5.2).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fbox {
    pub min: Vector,
    pub max: Vector,
}

/// An `RPoint` (5.3).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RPoint {
    pub p: Vector,
    pub a: Vector,
}

/// An `fcolor` (6.1).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fcolor {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// A `color` (6.2).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
}

/// A `noise` (6.3).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Noise {
    pub intensity: f32,
    pub grain: f32,
    pub fps: f32,
}

/// A `duality` (6.4).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Duality {
    pub h: f32,
    pub v: f32,
}

/// A `flags32` (7).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Flags32(pub u32);

/// A `flags16` (7).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Flags16(pub u16);

// --- Arguments and results (2.2, 2.3) ----------------------------------------

/// An argument as the methods match it.
#[derive(Clone, Copy, Debug)]
enum Arg {
    Nil,
    Bool(bool),
    Num(f64),
    Vector(Vector),
    Matrix(Matrix),
    Fcolor(Fcolor),
    Flags32(u32),
    Flags16(u16),
    Other,
}

impl Arg {
    /// Lua's truth value (2.2).
    fn truthy(self) -> bool {
        !matches!(self, Arg::Nil | Arg::Bool(false))
    }
}

/// A value type: what its objects are as arguments.
trait ValueType: Any + Clone {
    fn arg(&self) -> Arg {
        Arg::Other
    }
}

impl ValueType for Vector {
    fn arg(&self) -> Arg {
        Arg::Vector(*self)
    }
}

impl ValueType for Matrix {
    fn arg(&self) -> Arg {
        Arg::Matrix(*self)
    }
}

impl ValueType for Fcolor {
    fn arg(&self) -> Arg {
        Arg::Fcolor(*self)
    }
}

impl ValueType for Flags32 {
    fn arg(&self) -> Arg {
        Arg::Flags32(self.0)
    }
}

impl ValueType for Flags16 {
    fn arg(&self) -> Arg {
        Arg::Flags16(self.0)
    }
}

impl ValueType for Frect {}
impl ValueType for Fbox {}
impl ValueType for RPoint {}
impl ValueType for Color {}
impl ValueType for Noise {}
impl ValueType for Duality {}

fn other_arg<T: ValueType>(o: &Objects, v: &Value) -> Option<Arg> {
    match o.get::<T>(v)? {
        Operand::Other(t) => Some(t.arg()),
        Operand::This => None,
    }
}

/// The arguments after the object, each with whether it is the object's
/// own storage (the object itself or a reference to it; 2.6).
fn read_args<T: ValueType>(o: &Objects, s: &T, args: &[Value]) -> (Vec<Arg>, Vec<bool>) {
    args.iter()
        .map(|v| match v {
            Value::Nil => (Arg::Nil, false),
            Value::Boolean(b) => (Arg::Bool(*b), false),
            Value::Number(n) => (Arg::Num(*n), false),
            Value::Object(_) => match o.get::<T>(v) {
                Some(Operand::This) => (s.arg(), true),
                Some(Operand::Other(t)) => (t.arg(), false),
                None => {
                    let arg = other_arg::<Vector>(o, v)
                        .or_else(|| other_arg::<Matrix>(o, v))
                        .or_else(|| other_arg::<Fcolor>(o, v))
                        .or_else(|| other_arg::<Flags32>(o, v))
                        .or_else(|| other_arg::<Flags16>(o, v))
                        .unwrap_or(Arg::Other);
                    (arg, false)
                }
            },
            _ => (Arg::Other, false),
        })
        .unzip()
}

/// A method's result (2.3).
enum Res {
    /// s itself.
    This,
    /// A new object referring to the storage of s.
    Reference,
    /// Nothing.
    Nothing,
    Number(f64),
    Bool(bool),
}

/// A method: its effect on s and its result, `None` when no overload
/// matches the arguments. `alias[i]` tells whether argument i is s's
/// storage.
type Rule<T> = fn(&mut T, &[Arg], &[bool]) -> Option<Res>;

/// Registers method `name` of class `class` running `rule`. A call no
/// overload matches raises (our wording: it depends on A, 2.2).
fn method<T: ValueType>(
    c: EngineClass,
    class: &'static str,
    name: &'static str,
    rule: Rule<T>,
) -> EngineClass {
    c.method_with_objects(name, move |s: &mut T, o, args| {
        let (args, alias) = read_args(o, s, args);
        let res = rule(s, &args, &alias)
            .ok_or_else(|| format!("no overload of {class}.{name} matches the arguments"))?;
        Ok(match res {
            Res::This => vec![o.this()],
            Res::Reference => vec![o.reference()],
            Res::Nothing => Vec::new(),
            Res::Number(n) => vec![n.into()],
            Res::Bool(b) => vec![b.into()],
        })
    })
}

/// An f32 property (2.4).
fn f32_property<T: ValueType>(
    c: EngineClass,
    name: &str,
    get: fn(&T) -> f32,
    set: fn(&mut T, f32),
) -> EngineClass {
    c.writable_property(
        name,
        move |s: &T| f64::from(get(s)).into(),
        move |s: &mut T, v| {
            set(s, to_f32(lua_number(v)));
            Ok(())
        },
    )
}

/// A `vector`-valued property (2.4): read as a new vector holding a copy,
/// assigned by copying a vector.
fn vector_property<T: ValueType>(
    c: EngineClass,
    name: &'static str,
    get: fn(&T) -> Vector,
    set: fn(&mut T, Vector),
) -> EngineClass {
    c.property_with_objects(
        name,
        // TODO(#938): gap 2 leaves who owns this vector unstated; the
        // simplest reading: the script side, collected like a constructed one.
        move |s: &T, o| Ok(o.new_object(VECTOR, get(s))),
        Some(Box::new(move |s: &mut T, o: &Objects, v: &Value| {
            match o.get::<Vector>(v) {
                Some(Operand::Other(w)) => {
                    set(s, w);
                    Ok(())
                }
                // Unestablished (12); our choice.
                _ => Err(format!("{name}: a vector expected")),
            }
        })),
    )
}

/// A class whose objects scripts construct, with every value 0 (2.1).
fn zeroed<T: ValueType + Default>(name: &str) -> EngineClass {
    EngineClass::new(name)
        .constructor(0, |_| Some(0), |_, _| Ok(T::default()))
        .constructible()
}

fn numbers(n: usize) -> impl Fn(&[Value]) -> Option<u32> {
    move |args: &[Value]| {
        (args.len() == n && args.iter().all(|a| matches!(a, Value::Number(_)))).then_some(0)
    }
}

fn number(args: &[Value], i: usize) -> f32 {
    to_f32(args[i].as_number().unwrap_or(0.0))
}

// --- vector (3) ---------------------------------------------------------------

const VECTOR: &str = "vector";

fn f(n: f64) -> f32 {
    to_f32(n)
}

/// `div(n)`'s factor: the rounded reciprocal.
fn reciprocal(n: f64) -> Sse {
    Sse(1.0) / Sse(f(n))
}

fn vector_set(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    *s = match *args {
        [Arg::Num(a), Arg::Num(b), Arg::Num(c)] => Vector::new(f(a), f(b), f(c)),
        [Arg::Vector(w)] => w,
        _ => return None,
    };
    Some(Res::This)
}

fn vector_add(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    let op = |a: f32, b: f32| (Sse(a) + Sse(b)).0;
    *s = match *args {
        [Arg::Num(n)] => s.map(|x| op(x, f(n))),
        [Arg::Vector(w)] => w.zip(*s, op),
        [Arg::Vector(a), Arg::Vector(b)] => a.zip(b, op),
        [Arg::Vector(a), Arg::Num(n)] => a.map(|x| op(x, f(n))),
        _ => return None,
    };
    Some(Res::This)
}

fn vector_sub(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    let op = |a: f32, b: f32| (Sse(a) - Sse(b)).0;
    *s = match *args {
        [Arg::Num(n)] => s.map(|x| op(x, f(n))),
        [Arg::Vector(w)] => s.zip(w, op),
        [Arg::Vector(a), Arg::Vector(b)] => a.zip(b, op),
        [Arg::Vector(a), Arg::Num(n)] => a.map(|x| op(x, f(n))),
        _ => return None,
    };
    Some(Res::This)
}

fn vector_mul(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    let op = |a: f32, b: f32| (Sse(a) * Sse(b)).0;
    *s = match *args {
        [Arg::Num(n)] => s.map(|x| op(x, f(n))),
        [Arg::Vector(w)] => w.zip(*s, op),
        [Arg::Vector(a), Arg::Vector(b)] => a.zip(b, op),
        [Arg::Vector(a), Arg::Num(n)] => a.map(|x| op(x, f(n))),
        _ => return None,
    };
    Some(Res::This)
}

fn vector_div(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    let op = |a: f32, b: f32| (Sse(a) / Sse(b)).0;
    *s = match *args {
        [Arg::Num(n)] => {
            let r = reciprocal(n);
            s.map(|x| (Sse(x) * r).0)
        }
        [Arg::Vector(w)] => s.zip(w, op),
        [Arg::Vector(a), Arg::Vector(b)] => a.zip(b, op),
        [Arg::Vector(a), Arg::Num(n)] => {
            let r = reciprocal(n);
            a.map(|x| (Sse(x) * r).0)
        }
        _ => return None,
    };
    Some(Res::This)
}

fn vector_invert(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    let w = match *args {
        [] => *s,
        [Arg::Vector(w)] => w,
        _ => return None,
    };
    *s = w.map(|x| Sse(x).neg().0);
    Some(Res::This)
}

fn vector_min(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    let op = |a: f32, b: f32| if lt(a, b) { a } else { b };
    *s = match *args {
        [Arg::Vector(w)] => s.zip(w, op),
        [Arg::Vector(a), Arg::Vector(b)] => a.zip(b, op),
        _ => return None,
    };
    Some(Res::This)
}

fn vector_max(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    // The test (`a ≤ b` or `a > b`) is disputed in review; both give the
    // same for ordered values. With a NaN ours gives the second operand,
    // as `min` does (our choice where it is unestablished).
    let op = |a: f32, b: f32| if lt(b, a) { a } else { b };
    *s = match *args {
        [Arg::Vector(w)] => s.zip(w, op),
        [Arg::Vector(a), Arg::Vector(b)] => a.zip(b, op),
        _ => return None,
    };
    Some(Res::This)
}

fn vector_abs(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    let [Arg::Vector(w)] = *args else {
        return None;
    };
    *s = w.map(f32::abs);
    Some(Res::This)
}

fn vector_clamp(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    *s = match *args {
        [Arg::Vector(w)] => s.zip(w, |x, w| {
            let l = w.abs();
            let n = Sse(l).neg().0;
            if lt(x, n) {
                n
            } else if lt(l, x) {
                l
            } else {
                x
            }
        }),
        [Arg::Vector(lo), Arg::Vector(hi)] => s.zip3(lo, hi, |x, lo, hi| {
            if lt(x, lo) {
                lo
            } else if lt(hi, x) {
                hi
            } else {
                x
            }
        }),
        _ => return None,
    };
    Some(Res::This)
}

fn vector_inertion(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    let [Arg::Vector(w), Arg::Num(t)] = *args else {
        return None;
    };
    let t = Sse(f(t));
    let k = Sse(1.0) - t;
    *s = w.zip(*s, |w, x| (Sse(w) * k + Sse(x) * t).0);
    Some(Res::This)
}

fn vector_average(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    let op = |a: f32, b: f32| ((Sse(a) + Sse(b)) * Sse(0.5)).0;
    *s = match *args {
        [Arg::Vector(w)] => w.zip(*s, op),
        [Arg::Vector(a), Arg::Vector(b)] => a.zip(b, op),
        _ => return None,
    };
    Some(Res::This)
}

fn vector_lerp(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    let [Arg::Vector(a), Arg::Vector(b), Arg::Num(t)] = *args else {
        return None;
    };
    let t = Sse(f(t));
    let k = Sse(1.0) - t;
    *s = a.zip(b, |a, b| (Sse(a) * k + Sse(b) * t).0);
    Some(Res::This)
}

fn vector_mad(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    let op = |m: f32, n: f32, a: f32| (Sse(m) * Sse(n) + Sse(a)).0;
    *s = match *args {
        [Arg::Vector(w), Arg::Num(n)] => w.zip(*s, |w, x| op(w, f(n), x)),
        [Arg::Vector(a), Arg::Vector(b), Arg::Num(n)] => b.zip(a, |b, a| op(b, f(n), a)),
        [Arg::Vector(w), Arg::Vector(c)] => w.zip3(c, *s, op),
        [Arg::Vector(a), Arg::Vector(b), Arg::Vector(c)] => b.zip3(c, a, op),
        _ => return None,
    };
    Some(Res::This)
}

fn vector_crossproduct(s: &mut Vector, args: &[Arg], alias: &[bool]) -> Option<Res> {
    let [Arg::Vector(a0), Arg::Vector(b0)] = *args else {
        return None;
    };
    // Each component reads the arguments as they are when it is computed:
    // an argument that is s sees the components already stored (2.6).
    let a = |s: &Vector| if alias[0] { *s } else { a0 };
    let b = |s: &Vector| if alias[1] { *s } else { b0 };
    let (av, bv) = (a(s), b(s));
    s.x = (Sse(bv.z) * Sse(av.y) - Sse(av.z) * Sse(bv.y)).0;
    let (av, bv) = (a(s), b(s));
    s.y = (Sse(av.z) * Sse(bv.x) - Sse(av.x) * Sse(bv.z)).0;
    let (av, bv) = (a(s), b(s));
    s.z = (Sse(av.x) * Sse(bv.y) - Sse(bv.x) * Sse(av.y)).0;
    Some(Res::This)
}

/// `d.y × m.y + m.z × d.z + m.x × d.x` (`reflect`, `slide`).
fn reflect_dot(d: Vector, m: Vector) -> Sse {
    Sse(d.y) * Sse(m.y) + Sse(m.z) * Sse(d.z) + Sse(m.x) * Sse(d.x)
}

fn vector_reflect(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    let [Arg::Vector(d), Arg::Vector(m)] = *args else {
        return None;
    };
    let k = reflect_dot(d, m) * Sse(-2.0);
    *s = m.zip(d, |m, d| (Sse(m) * k + Sse(d)).0);
    Some(Res::This)
}

fn vector_slide(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    let [Arg::Vector(d), Arg::Vector(m)] = *args else {
        return None;
    };
    let k = reflect_dot(d, m).neg();
    *s = m.zip(d, |m, d| (Sse(m) * k + Sse(d)).0);
    Some(Res::This)
}

fn vector_set_length(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    let [Arg::Num(n)] = *args else {
        return None;
    };
    // No test of q: a zero vector gives NaN components.
    let k = (X::of(f(n)) / X::of(s.square().0).sqrt()).store();
    *s = s.map(|x| (Sse(x) * Sse(k)).0);
    Some(Res::This)
}

fn vector_normalize(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    let w = match *args {
        [] => *s,
        [Arg::Vector(w)] => w,
        _ => return None,
    };
    let q = w.square().0;
    if q > f32::from_bits(SMALLEST_NORMAL) {
        let r = Sse(1.0) / Sse(q);
        let g = X::of(r.0).sqrt();
        *s = w.map(|x| (X::of(x) * g).store());
    }
    Some(Res::This)
}

fn vector_align(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    let [] = *args else {
        return None;
    };
    let unit = |a: f32, d: f32| (X::of(a) / X::of(d.abs())).store();
    s.y = 0.0;
    // |s.z| < |s.x|, or either is NaN: the else branch.
    if s.z.abs() >= s.x.abs() {
        s.x = 0.0;
        s.z = unit(s.z, if s.z == 0.0 { 1.0 } else { s.z });
    } else {
        s.x = unit(s.x, s.x);
        s.z = 0.0;
    }
    Some(Res::This)
}

fn vector_set_hp(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    let [Arg::Num(h), Arg::Num(p)] = *args else {
        return None;
    };
    *s = direction_of(f(h), f(p));
    Some(Res::This)
}

/// The direction of heading `h` and pitch `p`, as `setHP` makes it (3).
pub(crate) fn direction_of(h: f32, p: f32) -> Vector {
    let (h, p) = (X::of(h), X::of(p));
    let g = X::of(p.cos().store());
    Vector::new(
        (h.sin() * g).neg().store(),
        p.sin().store(),
        (h.cos() * g).store(),
    )
}

fn vector_similar(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    let [Arg::Vector(w), Arg::Num(e)] = *args else {
        return None;
    };
    let e = f(e);
    let close = |a: f32, b: f32| (Sse(a) - Sse(b)).0.abs() < e;
    let similar = close(s.x, w.x) && close(s.y, w.y) && close(s.z, w.z);
    Some(Res::Number(if similar { 1.0 } else { 0.0 }))
}

fn number32(x: f32) -> Option<Res> {
    Some(Res::Number(f64::from(x)))
}

fn vector_magnitude(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    let [] = *args else {
        return None;
    };
    number32(X::of(s.square().0).sqrt().store())
}

fn vector_dotproduct(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    let [Arg::Vector(w)] = *args else {
        return None;
    };
    let p = |a: f32, b: f32| X::of(a) * X::of(b);
    number32((p(s.z, w.z) + p(s.y, w.y) + p(s.x, w.x)).store())
}

fn vector_distance_to(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    let [Arg::Vector(w)] = *args else {
        return None;
    };
    let d = s.zip(w, |a, b| (Sse(a) - Sse(b)).0);
    number32(X::of(d.square().0).sqrt().store())
}

fn vector_distance_to_xz(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    let [Arg::Vector(w)] = *args else {
        return None;
    };
    let d = s.zip(w, |a, b| (Sse(a) - Sse(b)).0);
    let q = Sse(d.x) * Sse(d.x) + Sse(d.z) * Sse(d.z);
    number32(X::of(q.0).sqrt().store())
}

fn vector_distance_to_sqr(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    let [Arg::Vector(w)] = *args else {
        return None;
    };
    let d = |a: f32, b: f32| X::of(a) - X::of(b);
    let (dx, dy, dz) = (d(s.x, w.x), d(s.y, w.y), d(s.z, w.z));
    number32((dz * dz + (dy * dy + dx * dx)).store())
}

fn vector_get_h(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    let [] = *args else {
        return None;
    };
    let e = f32::from_bits(ANGLE_EPS);
    let (x, z) = (s.x, s.z);
    let h = if x.abs() < e && z.abs() < e {
        0.0
    } else if z.abs() < e {
        let half = f32::from_bits(HALF_PI);
        if x > 0.0 { -half } else { half }
    } else {
        let a = X::of((Sse(x) / Sse(z)).0).atan();
        if z < 0.0 {
            (a - X::of(f32::from_bits(PI))).neg().store()
        } else {
            a.neg().store()
        }
    };
    number32(h)
}

fn vector_get_p(s: &mut Vector, args: &[Arg], _: &[bool]) -> Option<Res> {
    let [] = *args else {
        return None;
    };
    let e = f32::from_bits(ANGLE_EPS);
    let half = f32::from_bits(HALF_PI);
    // A NaN y gives −π/2: the else branch (the reading of its disputed
    // note, where the spec's own reading says nothing for NaN).
    let sign = |y: f32| if y > 0.0 { half } else { -half };
    let Vector { x, y, z } = *s;
    let p = if x.abs() < e && z.abs() < e && y.abs() < e {
        0.0
    } else if x.abs() < e && z.abs() < e {
        sign(y)
    } else {
        let k = X::of((Sse(z) * Sse(z) + Sse(x) * Sse(x)).0).sqrt().store();
        if k.abs() < e {
            sign(y)
        } else {
            X::of((Sse(y) / Sse(k)).0).atan().store()
        }
    };
    number32(p)
}

fn vector_class() -> EngineClass {
    let mut c = zeroed::<Vector>(VECTOR);
    c = f32_property(c, "x", |s: &Vector| s.x, |s, v| s.x = v);
    c = f32_property(c, "y", |s: &Vector| s.y, |s, v| s.y = v);
    c = f32_property(c, "z", |s: &Vector| s.z, |s, v| s.z = v);
    let rules: &[(&'static str, Rule<Vector>)] = &[
        ("set", vector_set),
        ("add", vector_add),
        ("sub", vector_sub),
        ("mul", vector_mul),
        ("div", vector_div),
        ("invert", vector_invert),
        ("min", vector_min),
        ("max", vector_max),
        ("abs", vector_abs),
        ("clamp", vector_clamp),
        ("inertion", vector_inertion),
        ("average", vector_average),
        ("lerp", vector_lerp),
        ("mad", vector_mad),
        ("crossproduct", vector_crossproduct),
        ("reflect", vector_reflect),
        ("slide", vector_slide),
        ("set_length", vector_set_length),
        // The two names are the same function.
        ("normalize", vector_normalize),
        ("normalize_safe", vector_normalize),
        ("align", vector_align),
        ("setHP", vector_set_hp),
        ("similar", vector_similar),
        ("magnitude", vector_magnitude),
        ("dotproduct", vector_dotproduct),
        ("distance_to", vector_distance_to),
        ("distance_to_xz", vector_distance_to_xz),
        ("distance_to_sqr", vector_distance_to_sqr),
        ("getH", vector_get_h),
        ("getP", vector_get_p),
    ];
    for &(name, rule) in rules {
        c = method(c, VECTOR, name, rule);
    }
    c
}

// --- matrix (4) -----------------------------------------------------------------

/// The term order t₁ t₂ t₃ t₄ of `mul(a, b)` per element.
const MUL_TERMS: [[usize; 4]; 16] = [
    [0, 2, 1, 3],
    [1, 2, 0, 3],
    [2, 0, 3, 1],
    [3, 1, 2, 0],
    [1, 0, 3, 2],
    [1, 0, 3, 2],
    [1, 0, 3, 2],
    [0, 3, 2, 1],
    [1, 0, 3, 2],
    [1, 0, 3, 2],
    [1, 3, 2, 0],
    [0, 3, 2, 1],
    [1, 0, 2, 3],
    [0, 1, 2, 3],
    [1, 3, 2, 0],
    [0, 3, 2, 1],
];

fn matrix_set(s: &mut Matrix, args: &[Arg], _: &[bool]) -> Option<Res> {
    match *args {
        [Arg::Matrix(m)] => *s = m,
        [
            Arg::Vector(i),
            Arg::Vector(j),
            Arg::Vector(k),
            Arg::Vector(c),
        ] => {
            for (r, v) in [i, j, k, c].into_iter().enumerate() {
                s.set_row(r, v);
                s.0[4 * r + 3] = if r == 3 { 1.0 } else { 0.0 };
            }
        }
        _ => return None,
    }
    Some(Res::This)
}

fn matrix_identity(s: &mut Matrix, args: &[Arg], _: &[bool]) -> Option<Res> {
    let [] = *args else {
        return None;
    };
    *s = Matrix(std::array::from_fn(|e| if e % 5 == 0 { 1.0 } else { 0.0 }));
    Some(Res::This)
}

fn matrix_mul(s: &mut Matrix, args: &[Arg], alias: &[bool]) -> Option<Res> {
    match *args {
        [Arg::Matrix(a0), Arg::Matrix(b0)] => {
            // Elements computed and stored 0 to 15, each reading the
            // arguments as they are then (2.6).
            for e in 0..16 {
                let a = if alias[0] { *s } else { a0 };
                let b = if alias[1] { *s } else { b0 };
                let (r, col) = (e / 4, e % 4);
                let term = |t: usize| Sse(a.0[4 * t + col]) * Sse(b.0[4 * r + t]);
                let [t1, t2, t3, t4] = MUL_TERMS[e];
                s.0[e] = (term(t1) + term(t2) + term(t3) + term(t4)).0;
            }
        }
        [Arg::Matrix(a), Arg::Num(n)] => *s = Matrix(a.0.map(|x| (Sse(x) * Sse(f(n))).0)),
        [Arg::Num(n)] => *s = Matrix(s.0.map(|x| (Sse(x) * Sse(f(n))).0)),
        _ => return None,
    }
    Some(Res::This)
}

fn matrix_div(s: &mut Matrix, args: &[Arg], _: &[bool]) -> Option<Res> {
    let (a, n) = match *args {
        [Arg::Matrix(a), Arg::Num(n)] => (a, n),
        [Arg::Num(n)] => (*s, n),
        _ => return None,
    };
    let r = reciprocal(n);
    *s = Matrix(a.0.map(|x| (Sse(x) * r).0));
    Some(Res::This)
}

/// `setHPB`'s rows from the angles; `rounded_c`: element 5 takes cos b
/// rounded (`setXYZi`), else unrounded.
fn set_hpb(s: &mut Matrix, h: f32, p: f32, b: f32, rounded_c: bool) {
    let (h, p, b) = (X::of(h), X::of(p), X::of(b));
    // In this order: sin h, cos h, sin p, cos p, sin b, cos b.
    let sh = Sse(h.sin().store());
    let ch = Sse(h.cos().store());
    let sp = Sse(p.sin().store());
    let cp = Sse(p.cos().store());
    let sb = Sse(b.sin().store());
    let c = b.cos();
    let cb = Sse(c.store());
    let c = if rounded_c { X::of(cb.0) } else { c };
    s.0 = [
        (cb * ch - sb * sh * sp).0,
        (X::of(sb.0) * X::of(cp.0)).neg().store(),
        (sb * ch * sp + cb * sh).0,
        0.0,
        (cb * sh * sp + sb * ch).0,
        (X::of(cp.0) * c).store(),
        (sb * sh - cb * ch * sp).0,
        0.0,
        (cp * sh).neg().0,
        sp.0,
        (cp * ch).0,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ];
}

fn angles(args: &[Arg]) -> Option<(f32, f32, f32)> {
    let [Arg::Num(a), Arg::Num(b), Arg::Num(c)] = *args else {
        return None;
    };
    Some((f(a), f(b), f(c)))
}

fn matrix_set_hpb(s: &mut Matrix, args: &[Arg], _: &[bool]) -> Option<Res> {
    let (h, p, b) = angles(args)?;
    set_hpb(s, h, p, b, false);
    Some(Res::This)
}

fn matrix_set_xyz(s: &mut Matrix, args: &[Arg], _: &[bool]) -> Option<Res> {
    let (x, y, z) = angles(args)?;
    set_hpb(s, y, x, z, false);
    Some(Res::This)
}

fn matrix_set_xyzi(s: &mut Matrix, args: &[Arg], _: &[bool]) -> Option<Res> {
    let (x, y, z) = angles(args)?;
    set_hpb(s, -y, -x, -z, true);
    Some(Res::This)
}

/// `mk_xform(q, t)`: no script can make a quaternion (1.3), so no call
/// matches.
fn matrix_mk_xform(_: &mut Matrix, _: &[Arg], _: &[bool]) -> Option<Res> {
    None
}

fn matrix_class() -> EngineClass {
    const MATRIX: &str = "matrix";
    let mut c = zeroed::<Matrix>(MATRIX);
    c = vector_property(c, "i", |s: &Matrix| s.row(0), |s, v| s.set_row(0, v));
    c = vector_property(c, "j", |s: &Matrix| s.row(1), |s, v| s.set_row(1, v));
    c = vector_property(c, "k", |s: &Matrix| s.row(2), |s, v| s.set_row(2, v));
    c = vector_property(c, "c", |s: &Matrix| s.row(3), |s, v| s.set_row(3, v));
    c = f32_property(c, "_14_", |s: &Matrix| s.0[3], |s, v| s.0[3] = v);
    c = f32_property(c, "_24_", |s: &Matrix| s.0[7], |s, v| s.0[7] = v);
    c = f32_property(c, "_34_", |s: &Matrix| s.0[11], |s, v| s.0[11] = v);
    c = f32_property(c, "_44_", |s: &Matrix| s.0[15], |s, v| s.0[15] = v);
    let rules: &[(&'static str, Rule<Matrix>)] = &[
        ("set", matrix_set),
        ("identity", matrix_identity),
        ("mul", matrix_mul),
        ("div", matrix_div),
        ("setHPB", matrix_set_hpb),
        ("setXYZ", matrix_set_xyz),
        ("setXYZi", matrix_set_xyzi),
        ("mk_xform", matrix_mk_xform),
    ];
    for &(name, rule) in rules {
        c = method(c, MATRIX, name, rule);
    }
    // Three number pointers: numbers don't match; nil does, and the method
    // writes through absent storage (believed to fail, 12; ours raises).
    c.method_with_objects("getHPB", |_: &mut Matrix, _, args| {
        if args.len() == 3 && args.iter().all(|a| *a == Value::Nil) {
            Err("matrix.getHPB: its results have no storage".into())
        } else {
            Err("no overload of matrix.getHPB matches the arguments".into())
        }
    })
}

// --- Frect, Fbox, RPoint (5) ----------------------------------------------------

fn frect_set(s: &mut Frect, args: &[Arg], _: &[bool]) -> Option<Res> {
    let [Arg::Num(a), Arg::Num(b), Arg::Num(c), Arg::Num(d)] = *args else {
        return None;
    };
    *s = Frect {
        x1: f(a),
        y1: f(b),
        x2: f(c),
        y2: f(d),
    };
    Some(Res::This)
}

fn frect_class() -> EngineClass {
    let mut c = zeroed::<Frect>("Frect");
    c = f32_property(c, "x1", |s: &Frect| s.x1, |s, v| s.x1 = v);
    c = f32_property(c, "y1", |s: &Frect| s.y1, |s, v| s.y1 = v);
    c = f32_property(c, "x2", |s: &Frect| s.x2, |s, v| s.x2 = v);
    c = f32_property(c, "y2", |s: &Frect| s.y2, |s, v| s.y2 = v);
    // A type with no registered class (1.3): believed to fail (12); ours
    // raises.
    for name in ["lt", "rb"] {
        let message = format!("Frect.{name}: its type has no class");
        let set_message = message.clone();
        c = c.property_with_objects(
            name,
            move |_: &Frect, _| Err(message.clone()),
            Some(Box::new(move |_: &mut Frect, _: &Objects, _: &Value| {
                Err(set_message.clone())
            })),
        );
    }
    method(c, "Frect", "set", frect_set)
}

fn fbox_class() -> EngineClass {
    let c = zeroed::<Fbox>("Fbox");
    let c = vector_property(c, "min", |s: &Fbox| s.min, |s, v| s.min = v);
    vector_property(c, "max", |s: &Fbox| s.max, |s, v| s.max = v)
}

fn rpoint_class() -> EngineClass {
    let c = zeroed::<RPoint>("RPoint");
    let c = vector_property(c, "P", |s: &RPoint| s.p, |s, v| s.p = v);
    vector_property(c, "A", |s: &RPoint| s.a, |s, v| s.a = v)
}

// --- Colours and effect parameters (6) ------------------------------------------

/// `fcolor:set(n)`: v's bytes from the top, times K, each rounded once (the
/// product is exact before).
pub(crate) fn fcolor_of(n: f64) -> Fcolor {
    let v = to_flags32(n);
    let k = f64::from(f32::from_bits(BYTE_SCALE));
    let byte = |shift: u32| (f64::from((v >> shift) & 0xFF) * k) as f32;
    Fcolor {
        a: byte(24),
        r: byte(16),
        g: byte(8),
        b: byte(0),
    }
}

fn fcolor_set(s: &mut Fcolor, args: &[Arg], _: &[bool]) -> Option<Res> {
    *s = match *args {
        [Arg::Num(r), Arg::Num(g), Arg::Num(b), Arg::Num(a)] => Fcolor {
            r: f(r),
            g: f(g),
            b: f(b),
            a: f(a),
        },
        [Arg::Fcolor(c)] => c,
        [Arg::Num(n)] => fcolor_of(n),
        _ => return None,
    };
    Some(Res::This)
}

fn fcolor_class() -> EngineClass {
    let mut c = zeroed::<Fcolor>("fcolor");
    c = f32_property(c, "r", |s: &Fcolor| s.r, |s, v| s.r = v);
    c = f32_property(c, "g", |s: &Fcolor| s.g, |s, v| s.g = v);
    c = f32_property(c, "b", |s: &Fcolor| s.b, |s, v| s.b = v);
    c = f32_property(c, "a", |s: &Fcolor| s.a, |s, v| s.a = v);
    method(c, "fcolor", "set", fcolor_set)
}

/// A colour class (6.2–6.4): its f32 properties in order, a constructor
/// without arguments and one with a value per property, and `set` of them
/// all, giving a reference object.
fn parameters<T: ValueType + Default>(
    name: &'static str,
    fields: &'static [Field<T>],
) -> EngineClass {
    let n = fields.len();
    let make = move |args: &[Value]| {
        let mut t = T::default();
        for (i, (_, _, set)) in fields.iter().enumerate() {
            set(&mut t, number(args, i));
        }
        t
    };
    // Without arguments the values are uninitialised in the original
    // (unestablished, 12); ours are 0.
    let mut c = EngineClass::new(name)
        .constructor(0, |_| Some(0), |_, _| Ok(T::default()))
        .constructor(n, numbers(n), move |_, args| Ok(make(args)))
        .constructible();
    for &(field, get, set) in fields {
        c = f32_property(c, field, get, set);
    }
    c.method_with_objects("set", move |s: &mut T, o, args| {
        if numbers(n)(args).is_none() {
            return Err(format!("no overload of {name}.set matches the arguments"));
        }
        *s = make(args);
        Ok(vec![o.reference()])
    })
}

type Field<T> = (&'static str, fn(&T) -> f32, fn(&mut T, f32));

const COLOR_FIELDS: &[Field<Color>] = &[
    ("r", |s| s.r, |s, v| s.r = v),
    ("g", |s| s.g, |s, v| s.g = v),
    ("b", |s| s.b, |s, v| s.b = v),
];

const NOISE_FIELDS: &[Field<Noise>] = &[
    ("intensity", |s| s.intensity, |s, v| s.intensity = v),
    ("grain", |s| s.grain, |s, v| s.grain = v),
    ("fps", |s| s.fps, |s, v| s.fps = v),
];

const DUALITY_FIELDS: &[Field<Duality>] = &[
    ("h", |s| s.h, |s, v| s.h = v),
    ("v", |s| s.v, |s, v| s.v = v),
];

// --- flags32, flags16 (7) ------------------------------------------------------

/// What `flags32` and `flags16` differ in: the value's width, the
/// conversion of numbers, and `one()`'s result.
trait Flags: ValueType + Default {
    const NAME: &'static str;
    const ONES: u32;
    fn get(&self) -> u32;
    fn put(&mut self, v: u32);
    /// A `number` argument converted (2.2).
    fn convert(n: f64) -> u32;
    /// Another flags object of the same class.
    fn same(arg: Arg) -> Option<u32>;
    /// `one()`'s result.
    const ONE: Res;
}

impl Flags for Flags32 {
    const NAME: &'static str = "flags32";
    const ONES: u32 = u32::MAX;
    const ONE: Res = Res::Reference;
    fn get(&self) -> u32 {
        self.0
    }
    fn put(&mut self, v: u32) {
        self.0 = v;
    }
    fn convert(n: f64) -> u32 {
        to_flags32(n)
    }
    fn same(arg: Arg) -> Option<u32> {
        match arg {
            Arg::Flags32(v) => Some(v),
            _ => None,
        }
    }
}

impl Flags for Flags16 {
    const NAME: &'static str = "flags16";
    const ONES: u32 = 0xFFFF;
    const ONE: Res = Res::Nothing;
    fn get(&self) -> u32 {
        u32::from(self.0)
    }
    fn put(&mut self, v: u32) {
        self.0 = v as u16;
    }
    fn convert(n: f64) -> u32 {
        u32::from(to_flags16(n))
    }
    fn same(arg: Arg) -> Option<u32> {
        match arg {
            Arg::Flags16(v) => Some(u32::from(v)),
            _ => None,
        }
    }
}

/// A converted number argument.
fn m<T: Flags>(arg: Arg) -> Option<u32> {
    match arg {
        Arg::Num(n) => Some(T::convert(n)),
        _ => None,
    }
}

fn flags_get<T: Flags>(s: &mut T, args: &[Arg], _: &[bool]) -> Option<Res> {
    let [] = *args else {
        return None;
    };
    Some(Res::Number(f64::from(s.get())))
}

fn flags_zero<T: Flags>(s: &mut T, args: &[Arg], _: &[bool]) -> Option<Res> {
    let [] = *args else {
        return None;
    };
    s.put(0);
    Some(Res::Reference)
}

fn flags_one<T: Flags>(s: &mut T, args: &[Arg], _: &[bool]) -> Option<Res> {
    let [] = *args else {
        return None;
    };
    s.put(T::ONES);
    Some(T::ONE)
}

fn flags_invert<T: Flags>(s: &mut T, args: &[Arg], _: &[bool]) -> Option<Res> {
    let v = match *args {
        [] => !s.get(),
        [a] => match (T::same(a), m::<T>(a)) {
            (Some(f), _) => !f,
            (_, Some(m)) => s.get() ^ m,
            _ => return None,
        },
        _ => return None,
    };
    s.put(v & T::ONES);
    Some(Res::Reference)
}

fn flags_assign<T: Flags>(s: &mut T, args: &[Arg], _: &[bool]) -> Option<Res> {
    let [a] = *args else {
        return None;
    };
    s.put(T::same(a).or_else(|| m::<T>(a))?);
    Some(Res::Reference)
}

/// `or` and `and`: `op(v, m)` or `op(f, m)`.
fn flags_combine<T: Flags>(s: &mut T, args: &[Arg], op: fn(u32, u32) -> u32) -> Option<Res> {
    let v = match *args {
        [a] => op(s.get(), m::<T>(a)?),
        [f, a] => op(T::same(f)?, m::<T>(a)?),
        _ => return None,
    };
    s.put(v);
    Some(Res::Reference)
}

fn flags_or<T: Flags>(s: &mut T, args: &[Arg], _: &[bool]) -> Option<Res> {
    flags_combine(s, args, |a, b| a | b)
}

fn flags_and<T: Flags>(s: &mut T, args: &[Arg], _: &[bool]) -> Option<Res> {
    flags_combine(s, args, |a, b| a & b)
}

fn flags_set<T: Flags>(s: &mut T, args: &[Arg], _: &[bool]) -> Option<Res> {
    let [a, b] = *args else {
        return None;
    };
    let m = m::<T>(a)?;
    s.put(if b.truthy() {
        s.get() | m
    } else {
        s.get() & !m
    });
    Some(Res::Reference)
}

fn flags_is<T: Flags>(s: &mut T, args: &[Arg], _: &[bool]) -> Option<Res> {
    let [a] = *args else {
        return None;
    };
    let m = m::<T>(a)?;
    Some(Res::Bool(s.get() & m == m))
}

fn flags_is_any<T: Flags>(s: &mut T, args: &[Arg], _: &[bool]) -> Option<Res> {
    let [a] = *args else {
        return None;
    };
    Some(Res::Bool(s.get() & m::<T>(a)? != 0))
}

fn flags_equal<T: Flags>(s: &mut T, args: &[Arg], _: &[bool]) -> Option<Res> {
    Some(Res::Bool(match *args {
        [f] => s.get() == T::same(f)?,
        [f, a] => {
            let m = m::<T>(a)?;
            s.get() & m == T::same(f)? & m
        }
        _ => return None,
    }))
}

fn flags_class<T: Flags>() -> EngineClass {
    let mut c = zeroed::<T>(T::NAME);
    let rules: &[(&'static str, Rule<T>)] = &[
        ("get", flags_get::<T>),
        ("zero", flags_zero::<T>),
        ("one", flags_one::<T>),
        ("invert", flags_invert::<T>),
        ("assign", flags_assign::<T>),
        ("or", flags_or::<T>),
        ("and", flags_and::<T>),
        ("set", flags_set::<T>),
        ("is", flags_is::<T>),
        // The two names are the same function.
        ("is_any", flags_is_any::<T>),
        ("test", flags_is_any::<T>),
        ("equal", flags_equal::<T>),
    ];
    for &(name, rule) in rules {
        c = method(c, T::NAME, name, rule);
    }
    c
}

// --- Bit functions (8) ------------------------------------------------------------

/// Registers bit function `name` of `N` numbers.
fn bit_function<const N: usize>(
    lua: &mut Lua,
    name: &'static str,
    op: fn([i32; N]) -> i32,
) -> Result<(), Error> {
    lua.register(name, move |args| {
        let ints: Option<Vec<i32>> = args.iter().map(|a| a.as_number().map(to_bits32)).collect();
        let ints: [i32; N] = ints
            .and_then(|v| v.try_into().ok())
            .ok_or_else(|| format!("no overload of {name} matches the arguments"))?;
        Ok(vec![f64::from(op(ints)).into()])
    })
}

// --- Registration -----------------------------------------------------------------

/// Registers this spec's 11 classes and 4 functions in the globals (1.1),
/// on a state whose class system is open.
pub fn register(lua: &mut Lua) -> Result<(), Error> {
    for class in [
        vector_class(),
        matrix_class(),
        frect_class(),
        fbox_class(),
        rpoint_class(),
        fcolor_class(),
        parameters("color", COLOR_FIELDS),
        parameters("noise", NOISE_FIELDS),
        parameters("duality", DUALITY_FIELDS),
        flags_class::<Flags32>(),
        flags_class::<Flags16>(),
    ] {
        lua.register_class(class)?;
    }
    bit_function(lua, "bit_and", |[a, b]| a & b)?;
    bit_function(lua, "bit_or", |[a, b]| a | b)?;
    bit_function(lua, "bit_xor", |[a, b]| a ^ b)?;
    bit_function(lua, "bit_not", |[a]| !a)?;
    Ok(())
}
