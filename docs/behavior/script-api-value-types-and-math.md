# Script API E: value types and math

What the value types of the engine interface do for the shipped scripts of S.T.A.L.K.E.R.: Shadow of Chernobyl
(Russian edition): the classes `vector`, `matrix`, `Frect`, `Fbox`, `RPoint`, `fcolor`, `color`, `noise`,
`duality`, `flags32` and `flags16`, their constructors, properties and methods, and the global functions
`bit_and`, `bit_or`, `bit_xor` and `bit_not`, with the floating-point precision and rounding of every result as
far as static analysis establishes it. Spec E of `docs/behavior/script-api-scope.md` (section 6, order 5).

Builds on (not repeated here):

- `docs/behavior/script-environment.md`: the runtime (LuaJIT 1.0.3), the binding library, the game state, and
  its section 2.5: script numbers are IEEE-754 64-bit doubles.
- `docs/behavior/script-loading.md`: script names, startup (the engine registers its interface before `_G` is
  loaded), and the 250 loadable scripts (its section 3).
- `docs/behavior/script-api-scope.md`: the listing `scripts\lua_help.script`, the counting rules and the split.
- Spec A, `docs/behavior/script-api-script-classes-deriving-from-engine-classes.md`: its terms **engine class**,
  **script class** and **script object**. Where this spec depends on ground A owns, it says "depends on A".
- `docs/behavior/particle-appearance-actions.md` section 1.2: the floating-point modes the core library sets
  (used in 2.5).

Conventions:

- `xrGame.dll` is the file identified in the script environment specification (6,246,400 bytes, MD5
  `19938b4b9fa11e3b4b6462ce7e7a12ac`) [registration.py].
- Each fact is **measured** (from the scripts or file bytes), **static** (from static analysis of
  `xrGame.dll`, not observed at run time), **believed** (inferred, with the reason) or **unestablished**. No
  binary was run.
- Checks are Python scripts in the dirty workspace directory `checks/script-values/`, named in brackets.
  `run_all.py` runs them all (109 PASS lines, output in `run_all.out`).
- **f32** is IEEE-754 single precision. "Rounded" means rounded to the nearest f32, ties to even. In formulas
  of f32 values, every `+ − × /` is one operation rounded to f32, evaluated in the order written: `a + b + c`
  means `(a + b) + c`. `−0 − a` is written where the program subtracts from negative zero; it equals `−a` for
  every value, zeros included.
- **X[…]** marks a computation done by the processor's x87 unit; its rounding is given by the rule of 2.5.
- In a method `s:m(...)`, **s** is the object the method is called on; `s.x`, `w.x` are components.
- Bit patterns of f32 constants are given in hexadecimal (`0x3F800000` is 1).

## 1. What this spec owns

### 1.1 Items (measured and static, [registration.py], [members.py])

The listing has the 11 classes, none with a base class or constants, with 101 declared members, and the 4 global
functions: 116 listed items, all owned by this spec (scoping spec section 6). The binary registers exactly what the
listing gives: per class the same property names, the same method names with the same number of overloads (109
method overloads), the same constructors, and the 4 global functions; it registers nothing else for these classes:
every name its registration code passes is one of these or a member of another spec's class registered alongside
(so there is no property the listing lacks, the scoping spec's gap 2).
No operator name (`__add`, `__eq`, `__tostring` and the like) is registered for any of them.

All 11 classes and the 4 functions are registered in the global namespace (no namespace name), before `_G` is
loaded. `color`, `noise` and `duality` are registered by the same code as the engine classes `effector` and
`effector_params` (spec U), which use them as property types; they are owned here.

### 1.2 Candidate strings (static, [candidates.py])

The scoping spec assigns 36 further candidate strings to this spec (`E`, `ERROR`, `actor_location`, …,
`tutorial_conditions_thresholds`). None of them is referenced by the code that registers this spec's classes and
functions; each is paired with a registration call only inside other functions. None is a registration of this
spec.

### 1.3 Types without a class (static, [registration.py])

Two parameter or property types of these classes have no registered class: the pair of f32 values of `Frect`'s
`lt` and `rb` (section 5.1), and the quaternion of `matrix:mk_xform` (section 4). No script can make a value of
either type.

## 2. Rules common to all value types

### 2.1 Objects (static, [registration.py], [results.py])

- An object of a value type holds its values itself. `vector()`, `matrix()`, `Frect()`, `Fbox()`, `RPoint()`,
  `fcolor()`, `flags32()` and `flags16()` make a new object with every value 0. `color()`, `noise()` and
  `duality()` make a new object whose values are **not initialised** (whatever the allocated memory holds;
  unestablished). `color(a, b, c)`, `noise(a, b, c)` and `duality(a, b)` set the values in the order of the
  properties (sections 6.2–6.4), each argument converted as in 2.2.
- A Lua variable holds a reference to the object: assignment (`local w = v`) and argument passing never copy it, so
  a method that changes `w` changes what `v` refers to (Lua semantics).
- An object made by a constructor is owned by the script side and collected with it (believed: the constructors
  allocate through the binding library's ownership path).

### 2.2 Arguments (static, [results.py], [fpmode.py])

- A parameter listed as `number` accepts only an argument of Lua type number (a numeric string does not match). A
  parameter of a value type accepts an object of that class. Which overload a call selects, and what happens when
  none matches, depend on A (A 7.3 states that the class system has no message for a failed match).
- **To f32.** Every `number` parameter of the `vector`, `matrix`, `Frect`, `fcolor`, `color`, `noise` and
  `duality` methods and constructors (except `fcolor:set(number)`) receives the Lua number rounded to f32. The
  rounding is done when the number is stored as a 32-bit float by the x87 unit (round to nearest; one rounding
  of the double): numbers too large for f32 become ±infinity, small ones become subnormal or zero.
  **Disputed in review:** static analysis establishes only the 32-bit store; its rounding direction, and that an
  overflow gives infinity rather than a floating-point exception, depend on the x87 control word in effect when
  scripts run (round to nearest with exceptions masked is **believed**, from the start-up mode of 2.5). The same
  holds for the f32 property assignments of 2.4 and for the 0 results of the conversions to flags below.
- **To flags.** A `number` argument of a `flags32` method, and of `fcolor:set(number)`, is truncated toward zero to
  a 64-bit integer, of which the low 32 bits are used; a number that is NaN or whose magnitude is 2⁶³ or more gives
  0. A `number` argument of a `flags16` method is truncated toward zero to a 32-bit integer, of which the low 16
  bits are used; a NaN or a value outside [−2³¹, 2³¹) gives 0.
- **To boolean.** The third argument of `flags32:set` and `flags16:set` is Lua's truth value: nil and false are
  false, every other value true.
- **Bit functions**: section 8.

### 2.3 Results (static, [results.py])

Each method gives one of these results:

| Result | Methods |
|---|---|
| **s itself**: the same Lua value the method was called on | every `vector`, `matrix`, `Frect` and `fcolor` method not listed below |
| **a reference object**: a new Lua object referring to the storage of s, which it neither owns nor keeps alive | every `color`, `noise`, `duality`, `flags32` and `flags16` method not listed below |
| an f32 number (the f32 value, exactly as a Lua number) | `vector`: `magnitude`, `dotproduct`, `distance_to`, `distance_to_sqr`, `distance_to_xz`, `getH`, `getP` |
| the number 1 or 0 (not a boolean) | `vector:similar` |
| a boolean | `flags32` and `flags16`: `is`, `is_any`, `test`, `equal` |
| a number in [0, 2³²) | `flags32:get` |
| a number in [0, 65535] | `flags16:get` |
| nothing | `flags16:one`, `matrix:getHPB` |

So chained calls such as `vector():set(0, 0, 1)` or `v:sub(w):normalize()` change and return the same object. A
reference object returned by a flags or colour method changes the original's storage when it is changed; using it
after the original has been collected is unestablished (believed to read freed memory). `similar`'s 0 is true in
Lua: only its value tells the result.

### 2.4 Properties (static, [registration.py], [results.py])

- An f32 property (all properties except those named in the next item) reads as the stored f32, exactly, as a Lua
  number. Assigning to it converts the assigned value with Lua's number conversion, with no test of its type (a
  numeric string gives its number; any other non-number gives 0), and stores it rounded to f32.
- A `vector`-valued property (`matrix` `i`, `j`, `k`, `c`; `Fbox` `min`, `max`; `RPoint` `P`, `A`) reads as a
  **new** `vector` holding a copy of the three values. Changing that vector does not change the owner: `m.i.x = 1`
  and `b.min:add(d)` leave `m` and `b` unchanged; `m.i = w` copies `w` into the matrix. Assigning a value that is
  not a vector is unestablished.
- Reading or assigning `Frect` `lt` or `rb` uses a type with no registered class (1.3): the read looks up that
  class, finds none and proceeds without a test; the outcome is unestablished (believed to fail with an access
  violation). No shipped script reads either (section 10).
- Reading or assigning a key that is not a registered property or method depends on A.

### 2.5 Floating-point environment (static, [fpmode.py], [members.py])

- Most arithmetic of the value types is done with SSE scalar single-precision instructions: each operation is an
  exact IEEE f32 operation (rounded to nearest, ties to even). Where the core library's thread start-up has set
  flush-to-zero and denormals-are-zero (particle-appearance-actions.md 1.2), subnormal results of those operations
  become zero and subnormal operands count as zero; that these modes are in effect when scripts run is
  **believed** (the thread that runs scripts was not examined).
- The x87 unit is used, beyond plain 32-bit copies, only by: `vector` `similar`, `set_length`, `align`,
  `normalize`, `normalize_safe`, `dotproduct`, `distance_to_sqr`, `setHP`, `getH`, `getP`; `matrix` `setHPB`,
  `setXYZ`, `setXYZi`; `fcolor:set(number)`; and by the one-instruction helpers for sine, cosine, square root,
  absolute value and arctangent, called by `vector` `abs`, `similar`, `set_length`, `align`, `clamp`,
  `magnitude`, `normalize`, `normalize_safe`, `distance_to_xz`, `distance_to`, `setHP`, `getH`, `getP` and
  `matrix` `setHPB`, `setXYZ`, `setXYZi`. These helpers take one f32 argument (the two-argument arctangent of
  `getHPB` takes two). **Disputed in review:** `matrix:getHPB` is missing from both lists: it also calls the
  square-root helper (its k, section 4) and negates its arctangents in the x87 unit.
- **Rule for X[…]**: the core library sets the x87 mode to 24-bit precision, round to nearest, at start-up
  (particle-appearance-actions.md 1.2); that this mode is in effect when scripts run is **believed**. In that
  mode each x87 addition, subtraction, multiplication, division and square root is rounded to a 24-bit
  significand (the exponent range stays wider), which equals rounding to f32 while the values stay in the f32
  range; sine, cosine and arctangent are not affected by the precision mode and give the processor's
  extended-precision results (processor documentation). A value leaves the x87 unit rounded to f32 when it is
  stored or returned (the conversion of a result to a Lua number goes through a 32-bit store, 2.3). Where an
  unrounded sine, cosine or arctangent enters a further operation, this spec says so. If the mode were 53- or
  64-bit, the operations inside X[…] would be rounded at that precision and once more to f32 at the store.
- Square roots of an f32 argument rounded to f32 are correctly rounded in any of the three modes (believed:
  the double rounding of a square root from 53 or 64 bits to 24 bits is exact).
- Copies (`set(vector)`, `matrix:set`, `fcolor:set(fcolor)`, properties) are exact.

### 2.6 Order and aliasing (static, [semantics.py])

Components are computed and stored in the order x, y, z (matrix elements 0 to 15), each from the operands' values in
memory at that time. When s is also one of the arguments, a method whose later components read earlier components
of the arguments therefore sees the values it has already stored. Among the methods, this happens only for
`vector:crossproduct` and `matrix:mul(matrix, matrix)` (and `matrix:set` with s as an argument, which changes
nothing). All other methods read an argument component only for the component of the same index, apart from values they
compute from all components before the first store (the dot products of `reflect` and `slide`, the squared
lengths of `normalize` and `set_length`). **Disputed in review:** the store order x, y, z (elements 0 to 15)
does not hold for every method: `setHPB`, `setXYZ` and `setXYZi` store element 1 before element 0, `mk_xform`
starts with element 4, and `set(i, j, k, c)` stores element 3 before element 2. None of these reads a matrix
argument, so the difference cannot be seen by scripts; the aliasing statements above are unaffected.

### 2.7 Depending on A

How a call that matches no overload behaves, how operators (`==`, `<`, `..`, `tostring`) apply to objects of
these classes (none registers an operator, 1.1; A 7.2 gives the class system's operator lookup for script
objects), and what reading an unregistered key gives, are the class system's general behaviour and depend on A.

## 3. `vector` (static, [semantics.py], [results.py])

Three f32 properties `x`, `y`, `z` (in this order in the object). `vector()` is (0, 0, 0). Every method below
changes s and gives s (2.3), except those giving numbers. Arguments: `n`, `t`, `e`, `h`, `p` are numbers; `w`, `a`,
`b`, `c`, `d`, `m`, `lo`, `hi` vectors. Formulas are per component and written for x; y and z are alike unless shown.

| Method | Effect on s (or result) |
|---|---|
| `set(n1, n2, n3)` | x = n1, y = n2, z = n3 |
| `set(w)` | s = w (copy) |
| `add(n)` / `add(w)` / `add(a, b)` / `add(a, n)` | x = s.x + n / w.x + s.x / a.x + b.x / a.x + n |
| `sub(n)` / `sub(w)` / `sub(a, b)` / `sub(a, n)` | x = s.x − n / s.x − w.x / a.x − b.x / a.x − n |
| `mul(n)` / `mul(w)` / `mul(a, b)` / `mul(a, n)` | x = s.x × n / w.x × s.x / a.x × b.x / a.x × n |
| `div(n)` | r = 1 / n; x = s.x × r (multiplied by the rounded reciprocal, not divided) |
| `div(w)` / `div(a, b)` | x = s.x / w.x / a.x / b.x |
| `div(a, n)` | r = 1 / n; x = a.x × r |
| `invert()` / `invert(w)` | x = −0 − s.x / −0 − w.x |
| `min(w)` | x = s.x if s.x < w.x, else w.x |
| `min(a, b)` | x = a.x if a.x < b.x, else b.x |
| `max(w)` | x = w.x if s.x ≤ w.x, else s.x. **Disputed in review:** the test is s.x > w.x: x = s.x if s.x > w.x, else w.x; the same for ordered values, but with a NaN the result is w.x |
| `max(a, b)` | x = b.x if a.x ≤ b.x, else a.x. **Disputed in review:** the test is a.x > b.x: x = a.x if a.x > b.x, else b.x; with a NaN the result is b.x |
| `abs(w)` | x = \|w.x\| (exact) |
| `clamp(w)` | with l = \|w.x\|: x = −0 − l if s.x < −0 − l, else l if s.x > l, else unchanged |
| `clamp(lo, hi)` | x = lo.x if s.x < lo.x, else hi.x if hi.x < s.x, else unchanged (`hi` is passed as a copy) |
| `inertion(w, t)` | f = 1 − t; x = w.x × f + s.x × t |
| `average(w)` / `average(a, b)` | x = (w.x + s.x) × 0.5 / (a.x + b.x) × 0.5 |
| `lerp(a, b, t)` | f = 1 − t; x = a.x × f + b.x × t |
| `mad(w, n)` | x = w.x × n + s.x |
| `mad(a, b, n)` | x = b.x × n + a.x |
| `mad(w, c)` | x = w.x × c.x + s.x |
| `mad(a, b, c)` | x = b.x × c.x + a.x |
| `crossproduct(a, b)` | x = b.z × a.y − a.z × b.y; y = a.z × b.x − a.x × b.z; z = a.x × b.y − b.x × a.y (a × b; aliasing 2.6) |
| `reflect(d, m)` | f = (d.y × m.y + m.z × d.z + m.x × d.x) × −2; x = m.x × f + d.x (and y, z with the same f) |
| `slide(d, m)` | f = −0 − (d.y × m.y + m.z × d.z + m.x × d.x); x = m.x × f + d.x (and y, z with the same f) |
| `set_length(n)` | q = s.x × s.x + s.y × s.y + s.z × s.z; f = r(X[n / √q]); x = s.x × f (and y, z). No test of q: for a zero vector f is ±infinity or NaN and every component NaN |
| `normalize()`, `normalize_safe()` | q = s.x × s.x + s.y × s.y + s.z × s.z. If q > 2⁻¹²⁶ (`0x00800000`, the smallest normal f32; false for NaN): r = 1 / q, g = X[√r], x = r(X[s.x × g]) (and y, z). Otherwise s is unchanged. The two names are the same function |
| `normalize(w)`, `normalize_safe(w)` | the same with q and the components taken from w: x = r(X[w.x × g]); when q ≤ 2⁻¹²⁶, s is unchanged (not set to w) |
| `align()` | y = 0. If \|s.z\| < \|s.x\| (or either is NaN): x = s.x / \|s.x\| and z = 0. Otherwise x = 0 and z = r(X[s.z / \|d\|]) with d = s.z, or d = 1 when s.z = 0. So the larger of x and z becomes ±1 with its sign (z stays 0 when x and z are both 0) |
| `setHP(h, p)` | g = r(X[cos p]); x = r(X[−(sin h × g)]); y = r(X[sin p]); z = r(X[cos h × g]); sin h and cos h enter unrounded |
| `similar(w, e)` → number | 1 if \|s.x − w.x\| < e, \|s.y − w.y\| < e and \|s.z − w.z\| < e (tested in this order), else 0; a NaN gives 0. The differences are f32 |
| `magnitude()` → number | r(√(s.x × s.x + s.y × s.y + s.z × s.z)) |
| `dotproduct(w)` → number | r(X[(s.z × w.z + s.y × w.y) + s.x × w.x]) |
| `distance_to(w)` → number | with d = s − w (f32 per component): r(√(d.x × d.x + d.y × d.y + d.z × d.z)) |
| `distance_to_xz(w)` → number | r(√(d.x × d.x + d.z × d.z)) |
| `distance_to_sqr(w)` → number | r(X[d.z × d.z + (d.y × d.y + d.x × d.x)]) with the differences d computed in the x87 unit too |
| `getH()` → number | with e = `0x33D6BF95` (1.0000000116860974 × 10⁻⁷): 0 if \|x\| < e and \|z\| < e; else, if \|z\| < e: −π/2 (`0xBFC90FDB`) when x > 0, +π/2 (`0x3FC90FDB`) otherwise; else with q = x / z (f32) and a = X[arctan q] unrounded: r(X[−(a − π)]) when z < 0 (π the f32 `0x40490FDB`), r(X[−a]) otherwise. The result lies between −π/2 and 3π/2; it is not reduced to [−π, π] |
| `getP()` → number | 0 if \|x\|, \|z\| and \|y\| are all < e; +π/2 if \|x\| < e and \|z\| < e and y > 0, −π/2 if they are and y ≤ 0 (**Disputed in review:** −π/2 whenever y > 0 is false, so also for a NaN y). Otherwise k = r(X[√(z × z + x × x)]) (the sum in f32): if \|k\| < e, +π/2 when y > 0 and −π/2 otherwise; else r(X[arctan(y / k)]) with y / k in f32 |

Comparisons are those of the processor: a comparison with a NaN is false, so `min(w)` with a NaN gives w.x, and
`max(w)` gives s.x. **Disputed in review:** `max(w)` with a NaN gives w.x (see its row).

## 4. `matrix` (static, [semantics.py], [results.py], [registration.py])

Sixteen f32 values, numbered 0–15; element 4r + s is row r, column s. Rows 0–3 are the vector properties `i`, `j`,
`k`, `c` (columns 0–2) and the f32 properties `_14_`, `_24_`, `_34_`, `_44_` are column 3 of rows 0–3. `matrix()`
is all zeros. Every method changes s and gives s, except `getHPB`.

| Method | Effect on s |
|---|---|
| `set(m)` | s = m (copy) |
| `set(i, j, k, c)` | rows 0–3 = the four vectors, column 3 = 0, 0, 0, 1 |
| `identity()` | elements 0, 5, 10, 15 = 1, the others 0 |
| `mul(a, b)` | element 4r + s = Σ over t of a(t, s) × b(r, t), summed as `((T₁ + T₂) + T₃) + T₄` with terms Tᵢ = a(tᵢ, s) × b(r, tᵢ) in the order of t given below; elements computed and stored 0 to 15 (aliasing 2.6) |
| `mul(a, n)` | element e = a(e) × n |
| `mul(n)` | element e = s(e) × n |
| `div(a, n)` | r = 1 / n; element e = a(e) × r |
| `div(n)` | r = 1 / n; element e = s(e) × r |
| `setHPB(h, p, b)` | with sh = r(sin h), ch = r(cos h), sp = r(sin p), cp = r(cos p), sb = r(sin b), cb = r(cos b), and C = cos b unrounded: row 0 = (cb·ch − (sb·sh)·sp, r(X[−(sb·cp)]), (sb·ch)·sp + cb·sh, 0); row 1 = ((cb·sh)·sp + sb·ch, r(X[cp·C]), sb·sh − (cb·ch)·sp, 0); row 2 = (−0 − cp·sh, sp, cp·ch, 0); row 3 = (0, 0, 0, 1). The sines and cosines are computed in the order sin h, cos h, sin p, cos p, sin b, cos b |
| `setXYZ(x, y, z)` | exactly `setHPB(y, x, z)` |
| `setXYZi(x, y, z)` | the angles negated first (−x, −y, −z rounded to f32; exact), then the `setXYZ` formulas with every sine and cosine rounded, including element 5 = r(X[cos(−z) · cos(−x)]) with both cosines rounded |
| `mk_xform(q, t)` | rotation from the quaternion's four values x, y, z, w (in storage order) and translation t: element 0 = 1 − (z·z + y·y)·2, 1 = (y·x − w·z)·2, 2 = (w·y + z·x)·2, 4 = (w·z + y·x)·2, 5 = 1 − (z·z + x·x)·2, 6 = (z·y − w·x)·2, 8 = (z·x − w·y)·2, 9 = (w·x + z·y)·2, 10 = 1 − (y·y + x·x)·2, 3, 7, 11 = 0, row 3 = (t.x, t.y, t.z, 1). No script can make a quaternion (1.3), so whether any call can reach this is unestablished |
| `getHPB(h, p, b)` | listed with three number pointers. A number argument does not match them; nil does (each such parameter accepts nil), and the method then writes its three results through absent storage (believed to fail with an access violation). It gives no result either way. Its computation, for the record (the rounding of its intermediate values was not examined): with k = √(e1·e1 + e5·e5), if k > 2⁻¹⁹ (`0x36000000`): h = −arctan2(e8, e10), p = −arctan2(−0 − e9, k), b = −arctan2(e1, e5); otherwise h = −arctan2(−0 − e2, e0), p = −arctan2(−0 − e9, k), b = 0; each arctangent from the x87 unit, each result stored as f32 |

All products and sums in the table are f32 operations except where X[…] is shown; `·` is ×.

Term order t₁ t₂ t₃ t₄ of `mul(a, b)` for elements 0–15: 0 2 1 3, 1 2 0 3, 2 0 3 1, 3 1 2 0, 1 0 3 2, 1 0 3 2,
1 0 3 2, 0 3 2 1, 1 0 3 2, 1 0 3 2, 1 3 2 0, 0 3 2 1, 1 0 2 3, 0 1 2 3, 1 3 2 0, 0 3 2 1.

## 5. Rectangles, boxes and points (static, [registration.py], [semantics.py])

### 5.1 `Frect`

Four f32 values in the order `x1`, `y1`, `x2`, `y2`, each an f32 property. `Frect()` is all zeros. `set(a, b, c, d)`
sets x1 = a, y1 = b, x2 = c, y2 = d and gives s. The properties `lt` (x1, y1) and `rb` (x2, y2) are pairs of
f32 values of a type with no registered class (1.3, 2.4).

### 5.2 `Fbox`

Two vectors, the properties `min` and `max` (in this order), read and assigned as copies (2.4). `Fbox()` is all
zeros. No methods.

### 5.3 `RPoint`

Two vectors, the properties `P` and `A` (in this order), read and assigned as copies (2.4). `RPoint()` sets both to
zero (the object holds 37 bytes, of which the first 29 are cleared; the rest is not visible to scripts). No
methods.

## 6. Colours and effect parameters (static, [registration.py], [semantics.py], [results.py])

### 6.1 `fcolor`

Four f32 properties `r`, `g`, `b`, `a` (in this order). `fcolor()` is all zeros. Methods give s.

- `set(r, g, b, a)`: the four values.
- `set(c)` with c an `fcolor`: a copy.
- `set(n)` with n a number: n is converted as for `flags32` (2.2) to a 32-bit value v; then a = (v bits 24–31) ×
  K, r = (bits 16–23) × K, g = (bits 8–15) × K, b = (bits 0–7) × K, each rounded to f32, with K the f32
  `0x3B808081` (0.003921568859368563). Each product is exact before its one rounding, so the result does not
  depend on the x87 mode; 255 × K rounds to 1.

### 6.2 `color`

Three f32 properties `r`, `g`, `b` (in this order). `color()` is uninitialised, `color(r, g, b)` sets them (2.1).
`set(r, g, b)` sets them and gives a reference object (2.3).

### 6.3 `noise`

Three f32 properties `intensity`, `grain`, `fps` (in this order). `noise()` uninitialised, `noise(i, g, f)` and
`set(i, g, f)` as for `color`.

### 6.4 `duality`

Two f32 properties `h`, `v` (in this order). `duality()` uninitialised, `duality(h, v)` and `set(h, v)` as for
`color`.

What these values mean to effects is spec U's.

## 7. `flags32` and `flags16` (static, [semantics.py], [results.py])

A `flags32` holds an unsigned 32-bit value, a `flags16` an unsigned 16-bit value; `flags32()` and `flags16()` are 0.
Number arguments are converted as in 2.2 (to 32 or 16 bits); in the table, v is the value of s, m a converted
number, f another flags object of the same class. Results are as in 2.3: methods that change v give a reference
object, `flags16:one` gives nothing.

| Method | Effect or result |
|---|---|
| `get()` | v as a number (2.3) |
| `zero()` / `one()` | v = 0 / v = all ones (0xFFFFFFFF or 0xFFFF) |
| `invert()` / `invert(f)` / `invert(m)` | v = NOT v / NOT f / v XOR m |
| `assign(f)` / `assign(m)` | v = f's value / m |
| `or(m)` / `or(f, m)` | v = v OR m / f OR m |
| `and(m)` / `and(f, m)` | v = v AND m / f AND m |
| `set(m, b)` | v = v OR m when b is true, v AND NOT m otherwise |
| `is(m)` → boolean | (v AND m) = m |
| `is_any(m)`, `test(m)` → boolean | (v AND m) ≠ 0 (the two names are the same function) |
| `equal(f)` → boolean | v = f's value |
| `equal(f, m)` → boolean | (v AND m) = (f's value AND m) |

`flags32:one()` gives a reference object; `flags16:one()` gives nothing (the listing shows it taking the object as
its only parameter).

## 8. Bit functions (static, [fpmode.py], [semantics.py])

`bit_and(a, b)`, `bit_or(a, b)`, `bit_xor(a, b)` and `bit_not(a)` convert each number to a signed 32-bit integer,
compute AND, OR, XOR or NOT of the 32 bits, and give the result as a **signed** number in [−2³¹, 2³¹). So
`bit_not(0)` is −1 and `bit_and(-1, -1)` is −1.

The conversion is the run-time library's: it truncates toward zero. On a processor that reports SSE2 (processor
identification leaf 1, EDX bit 26, and a further test) a number whose truncation lies outside [−2³¹, 2³¹), or a
NaN, gives −2³¹; so `bit_and(4294967295, 1)` is 0. Without SSE2 the truncation goes through a 64-bit integer and
its low 32 bits are used (4294967295 would give −1). That every processor the game runs on reports SSE2 is believed.
The argument types are checked as for other number parameters (believed: the same binding-library matching).

## 9. Worked examples (computed with a model of the rules above, [examples.py])

| Call | Result |
|---|---|
| a number argument 0.1 | stored as 0.10000000149011612 |
| a number argument 3.5 × 10³⁸ | stored as +infinity |
| `vector():set(10, 10, 10):div(3)` | each component 3.3333334922790527 (the f32 quotient 10 / 3 would be 3.3333332538604736) |
| `fcolor():set(0xFF804020)` | a = 1, r = 0.501960813999176, g = 0.250980406999588, b = 0.125490203499794 |
| `flags32` arguments 4294967295, −1, 2.9, −2.9, 2³² + 3, 10²⁰ | 0xFFFFFFFF, 0xFFFFFFFF, 2, 0xFFFFFFFE, 3, 0 |
| `flags16` arguments 65537, −1, 2³¹, 70000.7 | 1, 0xFFFF, 0, 4464 |
| `bit_and(4294967295, 1)`, `bit_or(2^31, 0)`, `bit_not(0)`, `bit_and(-1, 255)` (SSE2) | 0, −2147483648, −1, 255 |
| `vector():set(1e-20, 0, 0):normalize()` | unchanged (q is below 2⁻¹²⁶) |
| `vector():set(1, 0, 1):getH()` | −0.7853981852531433 |
| `vector():set(1, 0, -1):getH()` | 3.9269909858703613 (5π/4) |
| `vector():set(1, 1, 0):getP()` | 0.7853981852531433 |
| `vector():set_length(5)` | every component NaN |

The angle examples use a double-precision arctangent for the x87 one; that they match the processor's in these
cases is believed.

## 10. Use by the shipped scripts (measured, [scripts.py])

Of the 250 loadable scripts (comments and strings removed, occurrences counted by name whatever the receiver):

- None calls `similar`, `getHPB`, `mk_xform`, `set_length`, `align`, `getP` or `setHP`, reads `lt` or `rb`,
  constructs a `matrix` or a `flags16`, or calls `bit_xor` or `bit_not` (no script of the 439 calls the last
  two).
- `flags32` is constructed in 1 loadable script, `bit_and` is called in 2, `bit_or` in 1, `getH` in 4,
  `crossproduct` in 1.

The scoping spec counts 46 distinct names of this spec used by 60 (64) loadable scripts (its section 6).

## 11. Verification facts

All static unless marked; `run_all.py` reproduces every fact above (109 PASS lines).

- `xrGame.dll` identity (measured); 11 classes, 101 members, 4 functions, 116 listed items (measured, listing);
  registered names and overload counts equal to the listing; 113 registered overloads each paired with its
  implementation; property offsets and kinds; constructors; global namespace; no operator names; no class for the
  pair and quaternion types (305 class registrations read) [registration.py], [members.py].
- 36 candidate strings, none in this spec's registration code [candidates.py].
- Result kinds of all 109 method overloads; argument conversions; property access [results.py].
- Instruction classes per implementation; the x87 modes (from the approved particle spec); the bit functions'
  conversion and its processor test [fpmode.py].
- Formulas and operand order: 56 straight-line implementations evaluated symbolically from their instructions
  (x87 operand order read from the instruction bytes), the branching ones from their decompilation and
  disassembly; constants from the file bytes [semantics.py].
- Worked examples (model) [examples.py]; script use (measured) [scripts.py].

## 12. Unestablished

- Whether the x87 24-bit precision mode and the SSE flush-to-zero and denormals-are-zero modes are in effect while
  scripts run (2.5, believed).
- The exact last-bit results of the processor's sine, cosine and arctangent instructions; results that depend on
  them (`setHP`, `getH`, `getP`, `setHPB`, `setXYZ`, `setXYZi`, `getHPB`) are established only up to them.
- The values of `color()`, `noise()` and `duality()` when constructed without arguments (uninitialised memory).
- Reading or assigning `Frect` `lt` and `rb`; calling `matrix:getHPB` with nil arguments and `matrix:mk_xform` at all
  (believed to fail).
- Using a reference object (2.3) after the original object has been collected.
- Assigning a non-vector to a vector-valued property.
- Overload selection, failed matches, operators and unknown keys on these objects (depend on A, 2.7).
- Whether every processor the game runs on reports SSE2 (section 8, believed).

## Provenance

- Date: 2026-11-27.
- Edition: S.T.A.L.K.E.R.: Shadow of Chernobyl, Russian edition (GOG build); `xrGame.dll` as identified above; the
  archive file tree of `gamedata.db0` … `gamedata.dbd`, extracted with the project's archive tool
  (`scripts\lua_help.script` and the scripts).
- Analysis: static analysis of `xrGame.dll` with Ghidra (decompilation and disassembly of the registration code of
  the value types and the bit functions, the definition helpers, the call functions they install, the argument
  conversions and result handling, every implementation and constructor); a throwaway tracker of pushed values
  in the registration code pairing names with implementations; a throwaway symbolic evaluator of the
  straight-line floating-point code; PE import table and file bytes read by a throwaway parser; a throwaway
  scanner of the scripts. The scoping spec's static evidence (registration events and decompilation) was reused
  for the candidate strings and the list of registered types. No binary was executed.
- No leaked source code, derived engines, third-party engine documentation or recollection was used. Processor
  behaviour (x87 precision control, SSE flush-to-zero, the SSE2 feature bit) is from the processor's public
  documentation.

## Gaps found in review

1. The effects of the `flags32` and `flags16` methods `invert` (all three forms), `assign`, `or`, `and`, `zero`,
   `one` and `get` in section 7 have no check of their own (the checks cover `set`, `is`, `is_any`, `test` and
   `equal`); the reviewer's static reading of the implementations agrees with the table.
2. Who owns the new `vector` that reading a vector-valued property gives (2.4), and whether it is collected like
   a constructed one (2.1), is not stated.
