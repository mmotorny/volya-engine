// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Conventional interpretations of configuration values
//! (docs/formats/ltx-config.md, section 7). Values are normalized already, so
//! separators are bare commas.

use crate::cp1251;

/// One leading and one trailing `"` removed, independently.
///
/// The spec mentions values "up to 511 bytes" for this form without saying
/// what happens to longer ones; we do not truncate.
pub fn unquote(value: &str) -> &str {
    let value = value.strip_prefix('"').unwrap_or(value);
    value.strip_suffix('"').unwrap_or(value)
}

/// Leading whitespace skipped by the C library's number parsing (`isspace`
/// in the "C" locale).
fn skip_space(s: &[u8]) -> &[u8] {
    let start = s
        .iter()
        .position(|&b| !matches!(b, 0x09..=0x0D | 0x20))
        .unwrap_or(s.len());
    &s[start..]
}

/// The longest prefix that is an optionally signed decimal integer, as by C
/// `atoi`; 0 without digits. Truncate the result to the target width with
/// `as`. Digits are accumulated with wrapping: the spec leaves overflow beyond
/// the target width unspecified.
pub fn int(value: &str) -> i64 {
    let s = skip_space(value.as_bytes());
    let (negative, s) = match s.first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let mut n: i64 = 0;
    for &b in s.iter().take_while(|b| b.is_ascii_digit()) {
        n = n.wrapping_mul(10).wrapping_add(i64::from(b - b'0'));
    }
    if negative { n.wrapping_neg() } else { n }
}

/// Length of the longest prefix of `s` forming a decimal floating-point
/// number (sign, digits with an optional `.`, at least one digit, optional
/// exponent), or 0.
fn float_prefix(s: &[u8]) -> usize {
    let digits = |from: usize| s[from..].iter().take_while(|b| b.is_ascii_digit()).count();
    let mut i = usize::from(matches!(s.first(), Some(b'+' | b'-')));
    let whole = digits(i);
    i += whole;
    let mut fraction = 0;
    if s.get(i) == Some(&b'.') {
        fraction = digits(i + 1);
        i += 1 + fraction;
    }
    if whole + fraction == 0 {
        return 0;
    }
    if matches!(s.get(i), Some(b'e' | b'E')) {
        let mut j = i + 1;
        if matches!(s.get(j), Some(b'+' | b'-')) {
            j += 1;
        }
        let exponent = digits(j);
        if exponent > 0 {
            i = j + exponent;
        }
    }
    i
}

/// The longest decimal floating-point prefix, as by C `atof` in the "C"
/// locale, converted to `f32`; 0 without digits. The spec describes decimal
/// numbers only, so the C library's `inf`, `nan` and hexadecimal forms are
/// not accepted.
pub fn float(value: &str) -> f32 {
    parse_float(value).unwrap_or(0.0)
}

/// [`float`], or `None` when the value has no number prefix.
fn parse_float(value: &str) -> Option<f32> {
    scan_double(value).map(|(v, _)| v as f32)
}

/// [`float`] in double precision, as `atof` returns it.
pub fn double(value: &str) -> f64 {
    scan_double(value).map_or(0.0, |(v, _)| v)
}

/// One decimal number as the C library scans it (`atof`, `scanf`'s `%f`):
/// leading whitespace skipped, the longest number prefix as `f64`, and the
/// text after it; `None` without a number.
pub fn scan_double(value: &str) -> Option<(f64, &str)> {
    let skipped = value.len() - skip_space(value.as_bytes()).len();
    let s = &value[skipped..];
    let len = float_prefix(s.as_bytes());
    if len == 0 {
        return None;
    }
    // Rust accepts `3.` and `.2` as C does.
    let v = s[..len].parse::<f64>().expect("valid float prefix");
    Some((v, &s[len..]))
}

/// [`int`], or `None` when the value has no integer prefix.
fn parse_int(value: &str) -> Option<i64> {
    let s = skip_space(value.as_bytes());
    let sign = usize::from(matches!(s.first(), Some(b'+' | b'-')));
    s.get(sign)
        .is_some_and(u8::is_ascii_digit)
        .then(|| int(value))
}

/// True if the value, lowercased, is `on`, `yes`, `true` or `1`.
pub fn bool(value: &str) -> bool {
    ["on", "yes", "true", "1"]
        .iter()
        .any(|t| value.eq_ignore_ascii_case(t))
}

/// Comma-separated components, each parsed by `parse`; the first component
/// that is missing or does not parse and all after it are `zero`.
///
/// The spec does not say whether text after a component's number (before the
/// next comma) stops the parse; we split at commas and parse each component
/// like a single value, ignoring such text.
fn components<T: Copy, const N: usize>(
    value: &str,
    zero: T,
    parse: impl Fn(&str) -> Option<T>,
) -> [T; N] {
    let mut out = [zero; N];
    for (slot, part) in out.iter_mut().zip(value.split(',')) {
        match parse(part) {
            Some(v) => *slot = v,
            None => break,
        }
    }
    out
}

/// A vector of `N` floats (`N` is 2, 3 or 4 in the game).
pub fn floats<const N: usize>(value: &str) -> [f32; N] {
    components(value, 0.0, parse_float)
}

/// A vector of `N` integers (`N` is 2, 3 or 4 in the game).
pub fn ints<const N: usize>(value: &str) -> [i64; N] {
    components(value, 0, parse_int)
}

/// Packed colour `r,g,b,a` as `a·2²⁴ + r·2¹⁶ + g·2⁸ + b`, components modulo
/// 256; missing `r`, `g`, `b` are 0 and missing `a` is 255. A component that
/// does not parse counts as missing, as do all after it (the spec does not
/// cover unparsable components here).
pub fn color(value: &str) -> u32 {
    let parts: [Option<i64>; 4] = components(value, None, |p| parse_int(p).map(Some));
    let [r, g, b, a] = [
        parts[0].unwrap_or(0),
        parts[1].unwrap_or(0),
        parts[2].unwrap_or(0),
        parts[3].unwrap_or(255),
    ]
    .map(|c| (c as u32) & 0xFF);
    (a << 24) | (r << 16) | (g << 8) | b
}

/// The value matched case-insensitively against `tokens`; `None` when nothing
/// matches (the game then uses 0).
pub fn token<T: Copy>(value: &str, tokens: &[(&str, T)]) -> Option<T> {
    tokens
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(value))
        .map(|&(_, v)| v)
}

/// Class identifier: the first 8 bytes of the value, padded with spaces, as a
/// big-endian number. Characters Windows-1251 cannot represent (impossible in
/// a decoded value) count as `?`.
pub fn class_id(value: &str) -> u64 {
    let mut bytes = [b' '; 8];
    for (slot, c) in bytes.iter_mut().zip(value.chars()) {
        *slot = cp1251::encode_char(c).unwrap_or(b'?');
    }
    u64::from_be_bytes(bytes)
}

/// Comma-separated items.
pub fn list(value: &str) -> impl Iterator<Item = &str> {
    value.split(',')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unquotes_independently() {
        assert_eq!(
            unquote("\"weapons\\ammo\\ammo_9x18_fmj\""),
            "weapons\\ammo\\ammo_9x18_fmj"
        );
        assert_eq!(unquote("\"\""), "");
        assert_eq!(unquote("\"l04u"), "l04u");
        assert_eq!(unquote("x\""), "x");
        assert_eq!(unquote("\""), "");
    }

    #[test]
    fn parses_ints_like_atoi() {
        assert_eq!(int("70"), 70);
        assert_eq!(int("-12abc"), -12);
        assert_eq!(int("+3"), 3);
        assert_eq!(int("abc"), 0);
        assert_eq!(int(""), 0);
        assert_eq!(int("1.9"), 1);
        assert_eq!(int("300") as u8, 44);
        assert_eq!(int("-1") as u32, u32::MAX);
    }

    #[test]
    fn parses_floats_like_atof() {
        assert_eq!(float("0.25f"), 0.25);
        assert_eq!(float("3.f"), 3.0);
        assert_eq!(float("200."), 200.0);
        assert_eq!(float(".2"), 0.2);
        assert_eq!(float("-1.5e2x"), -150.0);
        assert_eq!(float("1e"), 1.0);
        assert_eq!(float("1e+"), 1.0);
        assert_eq!(float("."), 0.0);
        assert_eq!(float("x1"), 0.0);
        assert_eq!(float("inf"), 0.0);
    }

    #[test]
    fn parses_bools() {
        for t in ["on", "yes", "true", "1", "TRUE", "On"] {
            assert!(bool(t), "{t}");
        }
        for f in ["off", "false", "no", "0", "", "truex", "2"] {
            assert!(!bool(f), "{f}");
        }
    }

    #[test]
    fn parses_vectors() {
        assert_eq!(floats::<3>("0.0,0.9,0.0"), [0.0, 0.9, 0.0]);
        assert_eq!(floats::<3>("1,2"), [1.0, 2.0, 0.0]);
        assert_eq!(floats::<4>("1,x,3,4"), [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(floats::<2>("1.0,0.05,9"), [1.0, 0.05]);
        assert_eq!(ints::<4>("13,18,22,36"), [13, 18, 22, 36]);
        assert_eq!(ints::<3>("1,,3"), [1, 0, 0]);
    }

    #[test]
    fn packs_colors() {
        assert_eq!(color("255,128,0,64"), 0x40FF_8000);
        assert_eq!(color("1,2,3"), 0xFF01_0203);
        assert_eq!(color(""), 0xFF00_0000);
        assert_eq!(color("256,-1,0,0"), 0x0000_FF00);
    }

    #[test]
    fn matches_tokens_and_class_ids() {
        let tokens = [("alpha", 1), ("Beta", 2)];
        assert_eq!(token("BETA", &tokens), Some(2));
        assert_eq!(token("gamma", &tokens), None);
        assert_eq!(class_id("AMMO"), u64::from_be_bytes(*b"AMMO    "));
        assert_eq!(class_id("P_SKELET"), u64::from_be_bytes(*b"P_SKELET"));
        assert_eq!(class_id("LONGER_THAN_8"), u64::from_be_bytes(*b"LONGER_T"));
    }

    #[test]
    fn splits_lists() {
        assert_eq!(
            list("a,b,\"c d\"").collect::<Vec<_>>(),
            ["a", "b", "\"c d\""]
        );
    }
}
