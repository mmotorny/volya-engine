// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! docs/behavior/script-api-value-types-and-math.md on every target, one
//! check per section. Results are compared bit for bit with the spec's
//! arithmetic recomputed here: plain f32 operations in the order the spec
//! writes them (the inputs keep every value normal, where the SSE modes and
//! the x87 unit's 24-bit precision give exactly these), and f64 sines,
//! cosines and arctangents for the x87 unit's (12).

use crate::classes::EnginePart;
use crate::{Lua, Value};

/// A section's check: `Err` lists its failures.
pub type Check = fn() -> Result<(), String>;

/// The sections' checks, by section.
pub const SECTIONS: &[(&str, Check)] = &[
    ("1 items", items),
    ("2 common rules", common_rules),
    ("3 vector", vector),
    ("4 matrix", matrix),
    ("5 rectangles, boxes and points", rectangles),
    ("6 colours", colours),
    ("7 flags", flags),
    ("8 bit functions", bit_functions),
    ("9 worked examples", worked_examples),
];

/// Runs every check; `Err` lists the failures.
pub fn check() -> Result<(), String> {
    let failures: Vec<String> = SECTIONS
        .iter()
        .filter_map(|(what, f)| f().err().map(|e| format!("values {what}: {e}")))
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

/// A state with the class system and this spec's classes and functions.
fn state() -> Result<Lua, String> {
    let mut lua = Lua::new().map_err(|e| e.message)?;
    lua.open_classes().map_err(|e| e.message)?;
    crate::values::register(&mut lua).map_err(|e| e.message)?;
    Ok(lua)
}

/// Failures collected over a section.
#[derive(Default)]
struct Failures(Vec<String>);

impl Failures {
    fn ensure(&mut self, ok: bool, what: impl FnOnce() -> String) {
        if !ok {
            self.0.push(what());
        }
    }

    fn result(self) -> Result<(), String> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(self.0.join("; "))
        }
    }
}

/// Runs `code`; its results.
fn eval(lua: &mut Lua, code: &str) -> Result<Vec<Value>, String> {
    lua.exec("=values", code.as_bytes())
        .map_err(|e| format!("{code}: {e}"))
}

/// Whether `got` is the number `want` exactly (NaN matching NaN).
fn same_number(got: &Value, want: f64) -> bool {
    match got {
        Value::Number(n) => n.to_bits() == want.to_bits() || (n.is_nan() && want.is_nan()),
        _ => false,
    }
}

/// Runs `code` and compares its results with the f32 values `want`, bit for bit.
fn expect(f: &mut Failures, lua: &mut Lua, code: &str, want: &[f32]) {
    match eval(lua, code) {
        Ok(got) => f.ensure(
            got.len() == want.len()
                && got
                    .iter()
                    .zip(want)
                    .all(|(g, w)| same_number(g, f64::from(*w))),
            || format!("{code}: got {got:?}, want {want:?}"),
        ),
        Err(e) => f.0.push(e),
    }
}

/// Runs `code` and compares its results with `want`.
fn expect_values(f: &mut Failures, lua: &mut Lua, code: &str, want: &[Value]) {
    match eval(lua, code) {
        Ok(got) => f.ensure(got == want, || {
            format!("{code}: got {got:?}, want {want:?}")
        }),
        Err(e) => f.0.push(e),
    }
}

/// Runs `code`, which must raise.
fn expect_error(f: &mut Failures, lua: &mut Lua, code: &str) {
    let result = lua.exec("=values", code.as_bytes());
    f.ensure(result.is_err(), || format!("{code}: no error, {result:?}"));
}

/// Lua text of a number, exact.
fn lit(n: f64) -> String {
    format!("{n:?}")
}

fn vlit(v: [f64; 3]) -> String {
    format!("{}, {}, {}", lit(v[0]), lit(v[1]), lit(v[2]))
}

fn v32(v: [f64; 3]) -> [f32; 3] {
    v.map(|x| x as f32)
}

// --- 1 ------------------------------------------------------------------------

/// Each class's properties, then its methods (1.1: 101 members).
const MEMBERS: &[(&str, &[&str], &[&str])] = &[
    (
        "vector",
        &["x", "y", "z"],
        &[
            "set",
            "add",
            "sub",
            "mul",
            "div",
            "invert",
            "min",
            "max",
            "abs",
            "clamp",
            "inertion",
            "average",
            "lerp",
            "mad",
            "crossproduct",
            "reflect",
            "slide",
            "set_length",
            "normalize",
            "normalize_safe",
            "align",
            "setHP",
            "similar",
            "magnitude",
            "dotproduct",
            "distance_to",
            "distance_to_xz",
            "distance_to_sqr",
            "getH",
            "getP",
        ],
    ),
    (
        "matrix",
        &["i", "j", "k", "c", "_14_", "_24_", "_34_", "_44_"],
        &[
            "set", "identity", "mul", "div", "setHPB", "setXYZ", "setXYZi", "mk_xform", "getHPB",
        ],
    ),
    ("Frect", &["x1", "y1", "x2", "y2", "lt", "rb"], &["set"]),
    ("Fbox", &["min", "max"], &[]),
    ("RPoint", &["P", "A"], &[]),
    ("fcolor", &["r", "g", "b", "a"], &["set"]),
    ("color", &["r", "g", "b"], &["set"]),
    ("noise", &["intensity", "grain", "fps"], &["set"]),
    ("duality", &["h", "v"], &["set"]),
    (
        "flags32",
        &[],
        &[
            "get", "zero", "one", "invert", "assign", "or", "and", "set", "is", "is_any", "test",
            "equal",
        ],
    ),
    (
        "flags16",
        &[],
        &[
            "get", "zero", "one", "invert", "assign", "or", "and", "set", "is", "is_any", "test",
            "equal",
        ],
    ),
];

/// Section 1: the 11 classes and 4 functions in the globals, every member,
/// no operator.
pub fn items() -> Result<(), String> {
    let mut lua = state()?;
    let mut f = Failures::default();
    let members: usize = MEMBERS.iter().map(|(_, p, m)| p.len() + m.len()).sum();
    f.ensure(members == 101, || format!("{members} members listed here"));
    for (class, properties, methods) in MEMBERS {
        let o = format!("local o = {class}()");
        for p in *properties {
            // `lt` and `rb` raise when read (2.4): present, but not readable.
            let code = if ["lt", "rb"].contains(p) {
                format!("{o} return (pcall(function() return o.{p} end))")
            } else {
                format!("{o} return type(o.{p})")
            };
            let want = if ["lt", "rb"].contains(p) {
                Value::Boolean(false)
            } else if ["i", "j", "k", "c", "min", "max", "P", "A"].contains(p) {
                "userdata".into()
            } else {
                "number".into()
            };
            expect_values(&mut f, &mut lua, &code, &[want]);
        }
        for m in *methods {
            expect_values(
                &mut f,
                &mut lua,
                &format!("{o} return type(o['{m}'])"),
                &["function".into()],
            );
        }
        expect_values(
            &mut f,
            &mut lua,
            &format!("{o} return o.__add, o.__eq, o.__tostring, rawget(_G, '{class}') ~= nil"),
            &[Value::Nil, Value::Nil, Value::Nil, true.into()],
        );
    }
    for name in ["bit_and", "bit_or", "bit_xor", "bit_not"] {
        expect_values(
            &mut f,
            &mut lua,
            &format!("return type({name})"),
            &["function".into()],
        );
    }
    f.result()
}

// --- 2 ------------------------------------------------------------------------

/// Section 2: objects, arguments, results, properties, the environment's
/// rounding, aliasing.
pub fn common_rules() -> Result<(), String> {
    let mut lua = state()?;
    let mut f = Failures::default();

    // 2.1: zeros; values in property order; references, not copies.
    for class in ["vector", "matrix", "Frect", "Fbox", "RPoint", "fcolor"] {
        expect_values(
            &mut f,
            &mut lua,
            &format!("return {class}() ~= nil"),
            &[true.into()],
        );
    }
    expect(
        &mut f,
        &mut lua,
        "local v = vector() local m = matrix() local r = RPoint() \
         return v.x, v.y, v.z, m._44_, m.c.z, r.P.x, r.A.z, Frect().y2, fcolor().a",
        &[0.0; 9],
    );
    expect(
        &mut f,
        &mut lua,
        "local c, n, d = color(0.1, 2, 3), noise(4, 0.5, 6), duality(7, 0.3) \
         return c.r, c.g, c.b, n.intensity, n.grain, n.fps, d.h, d.v",
        &[0.1, 2.0, 3.0, 4.0, 0.5, 6.0, 7.0, 0.3],
    );
    expect(
        &mut f,
        &mut lua,
        "local v = vector() local w = v w:set(1, 2, 3) return v.x, v.y, v.z",
        &[1.0, 2.0, 3.0],
    );
    expect_values(
        &mut f,
        &mut lua,
        "return flags32():get(), flags16():get()",
        &[0.0.into(), 0.0.into()],
    );
    // No constructor takes other arguments (our wording, depends on A).
    expect_error(&mut f, &mut lua, "return vector(1, 2, 3)");
    expect_error(&mut f, &mut lua, "return color(1, 2)");

    // 2.2: numbers rounded to f32 once, overflow to infinity, subnormals
    // kept; a numeric string doesn't match a number.
    let small = 1.0e-40;
    expect(
        &mut f,
        &mut lua,
        &format!(
            "local v = vector():set(0.1, 3.5e38, {}) return v.x, v.y, v.z",
            lit(small)
        ),
        &[0.1, f32::INFINITY, small as f32],
    );
    expect_values(
        &mut f,
        &mut lua,
        "local v = vector() return pcall(v.set, v, '1', 2, 3)",
        &[
            false.into(),
            "no overload of vector.set matches the arguments".into(),
        ],
    );
    expect_error(&mut f, &mut lua, "vector():add({})");

    // 2.3: results. s itself; reference objects; numbers; nothing.
    expect_values(
        &mut f,
        &mut lua,
        "local v = vector() local m = matrix() local r = Frect() local c = fcolor() \
         return rawequal(v:set(1, 2, 3), v), rawequal(v:sub(v):normalize(), v), \
           rawequal(m:identity(), m), rawequal(r:set(1, 2, 3, 4), r), rawequal(c:set(1), c)",
        &vec![Value::Boolean(true); 5],
    );
    expect_values(
        &mut f,
        &mut lua,
        "local f = flags32() local r = f:assign(5) r['or'](r, 2) \
         local c = color(1, 2, 3) local cr = c:set(4, 5, 6) cr.g = 9 \
         return rawequal(r, f), f:get(), type(r), c.g, select('#', flags16():one()), \
           select('#', matrix():mul(1))",
        &[
            false.into(),
            7.0.into(),
            "userdata".into(),
            9.0.into(),
            0.0.into(),
            1.0.into(),
        ],
    );
    expect_values(
        &mut f,
        &mut lua,
        "local v = vector():set(1, 2, 3) return v:similar(v, 0.5), v:similar(vector(), 0.5)",
        &[1.0.into(), 0.0.into()],
    );

    // 2.4: f32 properties take Lua's number conversion without a type
    // test; vector-valued ones are copies both ways.
    expect(
        &mut f,
        &mut lua,
        "local v = vector() v.x = '2.5' v.y = {} v.z = 'abc' local x1 = v.x v.x = ' 0.1 ' \
         return x1, v.y, v.z, v.x",
        &[2.5, 0.0, 0.0, 0.1],
    );
    expect(
        &mut f,
        &mut lua,
        "local m = matrix() m.i.x = 1 local i = m.i i:set(4, 5, 6) local x1 = m.i.x \
         m.i = vector():set(7, 8, 9) local b = Fbox() b.min:add(1) \
         local copy = m.i copy.y = 0 \
         return x1, m.i.x, m.i.y, m.i.z, b.min.x, m.j.x, m._14_",
        &[0.0, 7.0, 8.0, 9.0, 0.0, 0.0, 0.0],
    );
    expect_values(
        &mut f,
        &mut lua,
        "local m = matrix() return rawequal(m.i, m.i)",
        &[false.into()],
    );
    expect_error(&mut f, &mut lua, "matrix().i = 1");
    expect_error(&mut f, &mut lua, "return Frect().lt");
    expect_error(&mut f, &mut lua, "Frect().rb = 1");

    // 2.5: denormals are zero in the SSE unit (believed): a subnormal
    // component plus 0 is 0; copies keep it.
    expect(
        &mut f,
        &mut lua,
        &format!(
            "local v = vector():set({0}, {0}, 0) local w = vector():set(v) v:add(0) \
             return v.x, w.x",
            lit(small)
        ),
        &[0.0, small as f32],
    );

    // Objects cross to Rust and back as themselves.
    match eval(&mut lua, "v0 = vector():set(1, 2, 3) return v0") {
        Ok(got) => {
            let object = matches!(got[..], [Value::Object(_)]);
            f.ensure(object, || format!("a vector to Rust: {got:?}"));
            if object {
                let _ = eval(&mut lua, "function same(o) return rawequal(o, v0), o.y end");
                let back = lua.call("same", &got).map_err(|e| e.message);
                f.ensure(back == Ok(vec![true.into(), 2.0.into()]), || {
                    format!("a vector back to Lua: {back:?}")
                });
            }
        }
        Err(e) => f.0.push(e),
    }
    // A part without object or engine class reaches Lua as nil.
    let _ = eval(&mut lua, "function is_nil(o) return o == nil end");
    let unknown = lua.call("is_nil", &[Value::Object(EnginePart::from_value(1.0e9))]);
    f.ensure(unknown == Ok(vec![true.into()]), || {
        format!("an unknown part: {unknown:?}")
    });

    // Depending on A: a script class deriving from a value type.
    expect(
        &mut f,
        &mut lua,
        "class 'my_vector' (vector) function my_vector:__init() super() end \
         local o = my_vector() o:set(1, 2, 3):add(o) return o.x, o.y, o.z",
        &[2.0, 4.0, 6.0],
    );
    f.result()
}

// --- 3 ------------------------------------------------------------------------

/// The vectors s, w, a, b, c and the number n of a vector check.
struct Case {
    s: [f64; 3],
    w: [f64; 3],
    a: [f64; 3],
    b: [f64; 3],
    c: [f64; 3],
    n: f64,
}

/// Numbers of mixed signs and magnitudes (10⁻³ to 2 × 10⁸), where the order
/// of operations shows in the last bits.
fn awkward(seed: u64) -> impl FnMut() -> f64 {
    let mut x = seed;
    move || {
        x = x
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let mantissa = 1.0 + (x >> 11) as f64 / (1u64 << 53) as f64;
        let exponent = ((x >> 3) % 12) as i32 - 3;
        let sign = if x & 1 == 1 { -1.0 } else { 1.0 };
        sign * mantissa * 10f64.powi(exponent)
    }
}

/// A fixed case with cancellations, then pseudo-random ones.
fn cases() -> Vec<Case> {
    let mut cases = vec![Case {
        s: [0.1, 1.0e8, -1.0e8],
        w: [-2.7, 0.3, 1.0e8 / 3.0],
        a: [1.0e8, 1.0, -0.7],
        b: [3.3, -1.0e8, 2.5e-3],
        c: [-0.37, 7.1, 1.0e-3],
        n: 0.3,
    }];
    for seed in 1..=8 {
        let mut next = awkward(seed);
        let mut v = || [next(), next(), next()];
        let (s, w, a, b, c) = (v(), v(), v(), v(), v());
        cases.push(Case {
            s,
            w,
            a,
            b,
            c,
            n: awkward(seed + 100)(),
        });
    }
    cases
}

fn vector_prelude(case: &Case) -> String {
    format!(
        "local s = vector():set({}) local w = vector():set({}) local a = vector():set({}) \
         local b = vector():set({}) local c = vector():set({}) local n = {} ",
        vlit(case.s),
        vlit(case.w),
        vlit(case.a),
        vlit(case.b),
        vlit(case.c),
        lit(case.n)
    )
}

fn zip(p: [f32; 3], q: [f32; 3], op: impl Fn(f32, f32) -> f32) -> [f32; 3] {
    [op(p[0], q[0]), op(p[1], q[1]), op(p[2], q[2])]
}

fn zip3(p: [f32; 3], q: [f32; 3], r: [f32; 3], op: impl Fn(f32, f32, f32) -> f32) -> [f32; 3] {
    [
        op(p[0], q[0], r[0]),
        op(p[1], q[1], r[1]),
        op(p[2], q[2], r[2]),
    ]
}

fn square(v: [f32; 3]) -> f32 {
    v[0] * v[0] + v[1] * v[1] + v[2] * v[2]
}

const HALF_PI: u32 = 0x3FC9_0FDB;
const PI: u32 = 0x4049_0FDB;

fn get_h(x: f32, z: f32) -> f32 {
    let a = f64::from(x / z).atan();
    if z < 0.0 {
        -((a - f64::from(f32::from_bits(PI))) as f32)
    } else {
        -(a as f32)
    }
}

fn get_p(v: [f32; 3]) -> f32 {
    let k = (v[2] * v[2] + v[0] * v[0]).sqrt();
    f64::from(v[1] / k).atan() as f32
}

/// The arithmetic of 3 on one case's values.
fn vector_case(f: &mut Failures, lua: &mut Lua, case: &Case) {
    let prelude = vector_prelude(case);
    let (s, w, a, b, c) = (
        v32(case.s),
        v32(case.w),
        v32(case.a),
        v32(case.b),
        v32(case.c),
    );
    let n = case.n as f32;
    let mut check = |call: &str, want: [f32; 3]| {
        expect(
            f,
            lua,
            &format!("{prelude}{call} return s.x, s.y, s.z"),
            &want,
        )
    };
    check("s:set(w)", w);
    check("s:add(n)", s.map(|x| x + n));
    check("s:add(w)", zip(w, s, |w, x| w + x));
    check("s:add(a, b)", zip(a, b, |a, b| a + b));
    check("s:add(a, n)", a.map(|x| x + n));
    check("s:sub(n)", s.map(|x| x - n));
    check("s:sub(w)", zip(s, w, |x, w| x - w));
    check("s:sub(a, b)", zip(a, b, |a, b| a - b));
    check("s:sub(a, n)", a.map(|x| x - n));
    check("s:mul(n)", s.map(|x| x * n));
    check("s:mul(w)", zip(w, s, |w, x| w * x));
    check("s:mul(a, b)", zip(a, b, |a, b| a * b));
    check("s:mul(a, n)", a.map(|x| x * n));
    let r = 1.0 / n;
    check("s:div(n)", s.map(|x| x * r));
    check("s:div(w)", zip(s, w, |x, w| x / w));
    check("s:div(a, b)", zip(a, b, |a, b| a / b));
    check("s:div(a, n)", a.map(|x| x * r));
    check("s:invert()", s.map(|x| -0.0 - x));
    check("s:invert(w)", w.map(|x| -0.0 - x));
    let min = |p: f32, q: f32| if p < q { p } else { q };
    let max = |p: f32, q: f32| if p > q { p } else { q };
    check("s:min(w)", zip(s, w, min));
    check("s:min(a, b)", zip(a, b, min));
    check("s:max(w)", zip(s, w, max));
    check("s:max(a, b)", zip(a, b, max));
    check("s:abs(w)", w.map(f32::abs));
    let clamp = |x: f32, w: f32| {
        let l = w.abs();
        if x < -0.0 - l {
            -0.0 - l
        } else if x > l {
            l
        } else {
            x
        }
    };
    check("s:clamp(w)", zip(s, w, clamp));
    check(
        "s:clamp(a, b)",
        zip3(s, a, b, |x, lo, hi| {
            if x < lo {
                lo
            } else if hi < x {
                hi
            } else {
                x
            }
        }),
    );
    let k = 1.0 - n;
    check("s:inertion(w, n)", zip(w, s, |w, x| w * k + x * n));
    check("s:average(w)", zip(w, s, |w, x| (w + x) * 0.5));
    check("s:average(a, b)", zip(a, b, |a, b| (a + b) * 0.5));
    check("s:lerp(a, b, n)", zip(a, b, |a, b| a * k + b * n));
    check("s:mad(w, n)", zip(w, s, |w, x| w * n + x));
    check("s:mad(a, b, n)", zip(a, b, |a, b| b * n + a));
    check("s:mad(w, c)", zip3(w, c, s, |w, c, x| w * c + x));
    check("s:mad(a, b, c)", zip3(a, b, c, |a, b, c| b * c + a));
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            b[2] * a[1] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - b[0] * a[1],
        ]
    };
    check("s:crossproduct(a, b)", cross(a, b));
    // 2.6: s as an argument sees the components already stored.
    let mut first = s;
    first[0] = w[2] * s[1] - s[2] * w[1];
    first[1] = first[2] * w[0] - first[0] * w[2];
    first[2] = first[0] * w[1] - w[0] * first[1];
    check("s:crossproduct(s, w)", first);
    let mut second = s;
    second[0] = s[2] * w[1] - w[2] * s[1];
    second[1] = w[2] * second[0] - w[0] * second[2];
    second[2] = w[0] * second[1] - second[0] * w[1];
    check("s:crossproduct(w, s)", second);
    let dot = |d: [f32; 3], m: [f32; 3]| d[1] * m[1] + m[2] * d[2] + m[0] * d[0];
    let fr = dot(a, c) * -2.0;
    check("s:reflect(a, c)", zip(c, a, |m, d| m * fr + d));
    let fs = -0.0 - dot(a, c);
    check("s:slide(a, c)", zip(c, a, |m, d| m * fs + d));
    let len = n / square(s).sqrt();
    check("s:set_length(n)", s.map(|x| x * len));
    let g = (1.0 / square(s)).sqrt();
    check("s:normalize()", s.map(|x| x * g));
    check("s:normalize_safe()", s.map(|x| x * g));
    let g = (1.0 / square(w)).sqrt();
    check("s:normalize(w)", w.map(|x| x * g));
    check("s:normalize_safe(w)", w.map(|x| x * g));
    let (h, p) = (a[0], b[1]);
    let gp = f64::from(p).cos() as f32;
    check(
        "s:setHP(a.x, b.y)",
        [
            -((f64::from(h).sin() * f64::from(gp)) as f32),
            f64::from(p).sin() as f32,
            (f64::from(h).cos() * f64::from(gp)) as f32,
        ],
    );

    let mut number = |call: &str, want: f32| {
        expect(f, lua, &format!("{prelude}return {call}"), &[want]);
    };
    number("s:magnitude()", square(s).sqrt());
    number("s:dotproduct(w)", s[2] * w[2] + s[1] * w[1] + s[0] * w[0]);
    number("a:dotproduct(b)", a[2] * b[2] + a[1] * b[1] + a[0] * b[0]);
    let d = zip(s, w, |x, w| x - w);
    number("s:distance_to(w)", square(d).sqrt());
    number("s:distance_to_xz(w)", (d[0] * d[0] + d[2] * d[2]).sqrt());
    number(
        "s:distance_to_sqr(w)",
        d[2] * d[2] + (d[1] * d[1] + d[0] * d[0]),
    );
    for (name, v) in [("s", s), ("w", w), ("c", c)] {
        number(&format!("{name}:getH()"), get_h(v[0], v[2]));
        number(&format!("{name}:getP()"), get_p(v));
    }
}

/// Section 3: every `vector` method.
pub fn vector() -> Result<(), String> {
    let mut lua = state()?;
    let mut f = Failures::default();
    let cases = cases();
    for case in &cases {
        vector_case(&mut f, &mut lua, case);
    }
    let prelude = vector_prelude(&cases[0]);
    let mut check = |call: &str, want: [f32; 3]| {
        expect(
            &mut f,
            &mut lua,
            &format!("{prelude}{call} return s.x, s.y, s.z"),
            &want,
        )
    };
    // q ≤ 2⁻¹²⁶: s unchanged, not set to w.
    check("s:set(1e-20, 0, 0):normalize()", [1.0e-20, 0.0, 0.0]);
    check(
        "s:set(1, 2, 3):normalize(vector():set(1e-20, 0, 0))",
        [1.0, 2.0, 3.0],
    );
    check("s:align()", [0.0, 0.0, -1.0]);
    check("s:set(-5, 1, 2):align()", [-1.0, 0.0, 0.0]);
    check("s:set(0, 3, 0):align()", [0.0, 0.0, 0.0]);
    check("s:set(2, 3, 2):align()", [0.0, 0.0, 1.0]);
    let half = f32::from_bits(HALF_PI);
    let mut number = |call: &str, want: f32| {
        expect(&mut f, &mut lua, &format!("return {call}"), &[want]);
    };
    number("vector():set(-3, 1, -0.5):getH()", get_h(-3.0, -0.5));
    number("vector():getH()", 0.0);
    number("vector():set(2, 5, 0):getH()", -half);
    number("vector():set(-2, 5, 0):getH()", half);
    number("vector():getP()", 0.0);
    number("vector():set(0, 2, 0):getP()", half);
    number("vector():set(0, -2, 0):getP()", -half);
    number("vector():set(0, 0/0, 0):getP()", -half);
    expect_values(
        &mut f,
        &mut lua,
        &format!(
            "{prelude}return s:similar(vector():set(s):add(0.5), 0.6), s:similar(w, 1e9), \
             vector():set(1, 2, 3):similar(vector():set(1, 2.5, 3), 0.5), \
             vector():set(0/0, 0, 0):similar(vector(), 1)"
        ),
        &[1.0.into(), 1.0.into(), 0.0.into(), 0.0.into()],
    );
    f.result()
}

// --- 4 ------------------------------------------------------------------------

/// Two matrices of awkward values.
fn matrix_values(seed: f64) -> [f64; 16] {
    std::array::from_fn(|e| {
        let e = e as f64;
        let scale = [1.0e7, 0.1, 3.0, 1.0e-3][(e as usize) % 4];
        ((e + 1.0) * seed).sin() * scale
    })
}

fn matrix_lit(m: &[f64; 16]) -> String {
    let rows: Vec<String> = (0..4)
        .map(|r| {
            format!(
                "vector():set({}, {}, {})",
                lit(m[4 * r]),
                lit(m[4 * r + 1]),
                lit(m[4 * r + 2])
            )
        })
        .collect();
    format!(
        "(function() local m = matrix():set({}) m._14_ = {} m._24_ = {} m._34_ = {} m._44_ = {} return m end)()",
        rows.join(", "),
        lit(m[3]),
        lit(m[7]),
        lit(m[11]),
        lit(m[15])
    )
}

/// The elements of matrix `name`, 0 to 15.
fn elements(name: &str) -> String {
    let e: Vec<String> = (0..4)
        .flat_map(|r| {
            let row = ["i", "j", "k", "c"][r];
            let col3 = ["_14_", "_24_", "_34_", "_44_"][r];
            [
                format!("{name}.{row}.x"),
                format!("{name}.{row}.y"),
                format!("{name}.{row}.z"),
                format!("{name}.{col3}"),
            ]
        })
        .collect();
    e.join(", ")
}

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

/// `mul(a, b)` of 4, with a or b the result (aliasing, 2.6) when flagged.
fn mul(s: [f32; 16], a: [f32; 16], b: [f32; 16], a_is_s: bool, b_is_s: bool) -> [f32; 16] {
    let mut out = s;
    for e in 0..16 {
        let a = if a_is_s { out } else { a };
        let b = if b_is_s { out } else { b };
        let (r, col) = (e / 4, e % 4);
        let t = |t: usize| a[4 * t + col] * b[4 * r + t];
        let [t1, t2, t3, t4] = MUL_TERMS[e];
        out[e] = t(t1) + t(t2) + t(t3) + t(t4);
    }
    out
}

/// `setHPB`'s rows (4).
fn set_hpb(h: f32, p: f32, b: f32, rounded_c: bool) -> [f32; 16] {
    let r = |x: f64| x as f32;
    let (h, p, b) = (f64::from(h), f64::from(p), f64::from(b));
    let (sh, ch, sp, cp, sb, cb) = (
        r(h.sin()),
        r(h.cos()),
        r(p.sin()),
        r(p.cos()),
        r(b.sin()),
        r(b.cos()),
    );
    let big_c = if rounded_c { f64::from(cb) } else { b.cos() };
    [
        cb * ch - sb * sh * sp,
        -(sb * cp),
        sb * ch * sp + cb * sh,
        0.0,
        cb * sh * sp + sb * ch,
        r(f64::from(cp) * big_c),
        sb * sh - cb * ch * sp,
        0.0,
        -0.0 - cp * sh,
        sp,
        cp * ch,
        0.0,
        0.0,
        0.0,
        0.0,
        1.0,
    ]
}

/// Section 4: every `matrix` method.
pub fn matrix() -> Result<(), String> {
    let mut lua = state()?;
    let mut f = Failures::default();
    let pairs = (0..6).map(|i| match i {
        0 => (matrix_values(0.7), matrix_values(1.9)),
        _ => {
            let mut next = awkward(200 + i);
            (
                std::array::from_fn(|_| next()),
                std::array::from_fn(|_| next()),
            )
        }
    });
    for (i, (ma, mb)) in pairs.enumerate() {
        let (a, b) = (ma.map(|x| x as f32), mb.map(|x| x as f32));
        let n = [0.3, -7.1, 1.0e5, 3.0e-3, -0.9, 13.0][i];
        let (h, p, bank) = (
            [0.4, 2.0, -3.1][i % 3],
            [-1.2, 0.7, 5.5][i % 3],
            [2.9, -0.2, 1.0][i % 3],
        );
        let prelude = format!(
            "local a = {} local b = {} local s = matrix() local n = {} ",
            matrix_lit(&ma),
            matrix_lit(&mb),
            lit(n)
        );
        let mut check = |call: &str, want: [f32; 16]| {
            expect(
                &mut f,
                &mut lua,
                &format!("{prelude}{call} return {}", elements("s")),
                &want,
            );
        };
        check("s:set(a)", a);
        let mut rows = [0.0; 16];
        for r in 0..4 {
            rows[4 * r..4 * r + 3].copy_from_slice(&b[4 * r..4 * r + 3]);
        }
        rows[15] = 1.0;
        check("s:set(a) s:set(b.i, b.j, b.k, b.c)", rows);
        check(
            "s:set(a):identity()",
            std::array::from_fn(|e| if e % 5 == 0 { 1.0 } else { 0.0 }),
        );
        check("s:mul(a, b)", mul([0.0; 16], a, b, false, false));
        check("s:set(a):mul(s, b)", mul(a, a, b, true, false));
        check("s:set(b):mul(a, s)", mul(b, a, b, false, true));
        check("s:set(a):mul(s, s)", mul(a, a, a, true, true));
        let n = n as f32;
        check("s:mul(a, n)", a.map(|x| x * n));
        check("s:set(b):mul(n)", b.map(|x| x * n));
        let r = 1.0 / n;
        check("s:div(a, n)", a.map(|x| x * r));
        check("s:set(b):div(n)", b.map(|x| x * r));
        let angles = format!("{}, {}, {}", lit(h), lit(p), lit(bank));
        let (h, p, bank) = (h as f32, p as f32, bank as f32);
        check(&format!("s:setHPB({angles})"), set_hpb(h, p, bank, false));
        check(&format!("s:setXYZ({angles})"), set_hpb(p, h, bank, false));
        check(
            &format!("s:setXYZi({angles})"),
            set_hpb(-p, -h, -bank, true),
        );
    }
    // mk_xform takes a quaternion no script can make; getHPB writes through
    // its arguments (1.3, 4).
    expect_error(&mut f, &mut lua, "matrix():mk_xform(matrix(), vector())");
    expect_error(&mut f, &mut lua, "matrix():getHPB(1, 2, 3)");
    expect_error(&mut f, &mut lua, "matrix():getHPB(nil, nil, nil)");
    expect(
        &mut f,
        &mut lua,
        "local m = matrix() m.c = vector():set(1, 2, 3) m._44_ = 0.1 return m.c.x, m.c.y, m.c.z, m._44_",
        &[1.0, 2.0, 3.0, 0.1],
    );
    f.result()
}

// --- 5 ------------------------------------------------------------------------

/// Section 5: `Frect`, `Fbox`, `RPoint`.
pub fn rectangles() -> Result<(), String> {
    let mut lua = state()?;
    let mut f = Failures::default();
    expect(
        &mut f,
        &mut lua,
        "local r = Frect():set(0.1, 2, 3, 4) r.x2 = 0.7 return r.x1, r.y1, r.x2, r.y2",
        &[0.1, 2.0, 0.7, 4.0],
    );
    expect(
        &mut f,
        &mut lua,
        "local b, p = Fbox(), RPoint() b.max = vector():set(1, 2, 3) p.A = vector():set(4, 5, 6) \
         p.P.x = 9 return b.min.x, b.max.x, b.max.z, p.P.x, p.A.y",
        &[0.0, 1.0, 3.0, 0.0, 5.0],
    );
    f.result()
}

// --- 6 ------------------------------------------------------------------------

/// Section 6: colours and effect parameters.
pub fn colours() -> Result<(), String> {
    let mut lua = state()?;
    let mut f = Failures::default();
    let k = f64::from(f32::from_bits(0x3B80_8081));
    let byte = |b: u32| (f64::from(b) * k) as f32;
    expect(
        &mut f,
        &mut lua,
        "local c = fcolor():set(0.1, 0.2, 0.3, 0.4) local d = fcolor():set(c) \
         return d.r, d.g, d.b, d.a",
        &[0.1, 0.2, 0.3, 0.4],
    );
    expect(
        &mut f,
        &mut lua,
        "local c = fcolor():set(-1) return c.a, c.r, c.g, c.b",
        &[1.0, 1.0, 1.0, 1.0],
    );
    expect(
        &mut f,
        &mut lua,
        "local c = fcolor():set(305419896.9) return c.a, c.r, c.g, c.b",
        &[byte(18), byte(52), byte(86), byte(120)],
    );
    expect(
        &mut f,
        &mut lua,
        "local c, n, d = color(), noise(), duality() c:set(1, 0.1, 3) n:set(4, 5, 0.6) d:set(0.7, 8) \
         return c.r, c.g, c.b, n.intensity, n.grain, n.fps, d.h, d.v",
        &[1.0, 0.1, 3.0, 4.0, 5.0, 0.6, 0.7, 8.0],
    );
    expect_values(
        &mut f,
        &mut lua,
        "local c = color(1, 2, 3) local r = c:set(4, 5, 6) r:set(7, 8, 9) \
         return rawequal(r, c), c.r, pcall(c.set, c, 1, 2)",
        &[
            false.into(),
            7.0.into(),
            false.into(),
            "no overload of color.set matches the arguments".into(),
        ],
    );
    f.result()
}

// --- 7 ------------------------------------------------------------------------

/// Section 7: every `flags32` and `flags16` method. TODO(#938): gap 1
/// leaves `invert`, `assign`, `or`, `and`, `zero`, `one` and `get` without
/// a check of their own; the table's reading is the simplest.
pub fn flags() -> Result<(), String> {
    let mut lua = state()?;
    let mut f = Failures::default();
    let n = |x: f64| Value::Number(x);
    for (class, ones) in [("flags32", 4_294_967_295.0), ("flags16", 65535.0)] {
        let p = format!("local f = {class}():assign(3855) local g = {class}():assign(255) ");
        let mut get = |call: &str, want: f64| {
            expect_values(
                &mut f,
                &mut lua,
                &format!("{p}{call} return f:get()"),
                &[n(want)],
            );
        };
        get("", 3855.0);
        get("f:zero()", 0.0);
        get("f:one()", ones);
        get("f:invert()", ones - 3855.0);
        get("f:invert(g)", ones - 255.0);
        get("f:invert(255)", 4080.0);
        get("f:assign(g)", 255.0);
        get("f['or'](f, 12288)", 16143.0);
        get("f['or'](f, g, 12288)", 12543.0);
        get("f['and'](f, 240)", 0.0);
        get("f['and'](f, g, 4080)", 240.0);
        get("f:set(28672, 1)", 32527.0);
        get("f:set(3, nil)", 3852.0);
        get("f:set(3840, false)", 15.0);
        get("f:set(16, {})", 3871.0);
        get("local r = f:assign(f) r['or'](r, f, 1)", 3855.0);
        expect_values(
            &mut f,
            &mut lua,
            &format!(
                "{p}return f:is(3840), f:is(3856), f:is_any(16), f:is_any(17), \
                 f:test(17), f:equal(g), f:equal(g, 15), f:equal(f)"
            ),
            &[
                true.into(),
                false.into(),
                false.into(),
                true.into(),
                true.into(),
                false.into(),
                true.into(),
                true.into(),
            ],
        );
        // A flags object of the other class doesn't match.
        let other = if class == "flags32" {
            "flags16"
        } else {
            "flags32"
        };
        expect_error(&mut f, &mut lua, &format!("{class}():assign({other}())"));
        expect_error(
            &mut f,
            &mut lua,
            &format!("{class}()['or']({class}(), '1')"),
        );
    }
    expect_values(
        &mut f,
        &mut lua,
        "local f = flags32() local r = f:one() local s = flags16() \
         return type(r), r:get(), select('#', s:one()), s:get()",
        &["userdata".into(), n(4_294_967_295.0), n(0.0), n(65535.0)],
    );
    f.result()
}

// --- 8 ------------------------------------------------------------------------

/// Section 8: the bit functions.
pub fn bit_functions() -> Result<(), String> {
    let mut lua = state()?;
    let mut f = Failures::default();
    let n = |x: f64| Value::Number(x);
    expect_values(
        &mut f,
        &mut lua,
        "return bit_and(12, 10), bit_or(12, 10), bit_xor(12, 10), bit_not(5), \
           bit_and(-1, -1), bit_or(-2.9, 0), bit_or(2.9, 0), bit_and(0/0, -1), bit_or(-2^31, 0)",
        &[
            n(8.0),
            n(14.0),
            n(6.0),
            n(-6.0),
            n(-1.0),
            n(-2.0),
            n(2.0),
            n(-2_147_483_648.0),
            n(-2_147_483_648.0),
        ],
    );
    expect_error(&mut f, &mut lua, "bit_and('1', 2)");
    expect_error(&mut f, &mut lua, "bit_not(1, 2)");
    expect_error(&mut f, &mut lua, "bit_or(1)");
    f.result()
}

// --- 9 ------------------------------------------------------------------------

/// Section 9: the spec's worked examples, as it gives them.
pub fn worked_examples() -> Result<(), String> {
    let mut lua = state()?;
    let mut f = Failures::default();
    let examples: &[(&str, &[f64])] = &[
        (
            "local v = vector() v.x = 0.1 return v.x",
            &[0.10000000149011612],
        ),
        ("return vector():set(3.5e38, 0, 0).x", &[f64::INFINITY]),
        (
            "local v = vector():set(10, 10, 10):div(3) return v.x, v.y, v.z",
            &[3.3333334922790527; 3],
        ),
        (
            "local c = fcolor():set(4286595104) return c.a, c.r, c.g, c.b",
            &[1.0, 0.501960813999176, 0.250980406999588, 0.125490203499794],
        ),
        (
            "local f = flags32() return f:assign(4294967295):get(), f:assign(-1):get(), \
             f:assign(2.9):get(), f:assign(-2.9):get(), f:assign(2^32 + 3):get(), f:assign(1e20):get()",
            &[4294967295.0, 4294967295.0, 2.0, 4294967294.0, 3.0, 0.0],
        ),
        (
            "local f = flags16() return f:assign(65537):get(), f:assign(-1):get(), \
             f:assign(2^31):get(), f:assign(70000.7):get()",
            &[1.0, 65535.0, 0.0, 4464.0],
        ),
        (
            "return bit_and(4294967295, 1), bit_or(2^31, 0), bit_not(0), bit_and(-1, 255)",
            &[0.0, -2147483648.0, -1.0, 255.0],
        ),
        (
            "local v = vector():set(1e-20, 0, 0):normalize() return v.x, v.y, v.z",
            &[1.0e-20f32 as f64, 0.0, 0.0],
        ),
        (
            "return vector():set(1, 0, 1):getH()",
            &[-0.7853981852531433],
        ),
        (
            "return vector():set(1, 0, -1):getH()",
            &[3.9269909858703613],
        ),
        ("return vector():set(1, 1, 0):getP()", &[0.7853981852531433]),
        (
            "local v = vector():set_length(5) return v.x, v.y, v.z",
            &[f64::NAN; 3],
        ),
    ];
    for (code, want) in examples {
        match eval(&mut lua, code) {
            Ok(got) => f.ensure(
                got.len() == want.len() && got.iter().zip(*want).all(|(g, w)| same_number(g, *w)),
                || format!("{code}: got {got:?}, want {want:?}"),
            ),
            Err(e) => f.0.push(e),
        }
    }
    f.result()
}
