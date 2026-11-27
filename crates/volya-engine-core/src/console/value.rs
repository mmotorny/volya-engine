// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! How each kind of command parses its argument, shows its value (the
//! status) and describes its valid arguments (the help text)
//! (docs/behavior/console-commands.md sections 3 and 4).

use volya_engine_formats::ltx::value;

use super::commands::Kind;

/// A command's value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    OnOff(bool),
    Int(i64),
    Real(f32),
    /// The stored number of a list value.
    List(i64),
    Vector([f32; 3]),
    Text(String),
    Mode(i32, i32),
    /// `ph_frequency`'s physics step, 1 / frequency.
    Frequency(f32),
    /// Commands without a value of their own (actions, `name`).
    None,
}

/// The tolerance of real ranges (3.4).
const RANGE_EPSILON: f64 = 0.000_01;

impl Value {
    /// The argument `text` (lower-cased already where the command does not
    /// keep case) as a value of `kind`; `None` when the kind rejects it.
    /// Actions and `name` have no value to parse.
    pub fn parse(kind: Kind, text: &str) -> Option<Self> {
        match kind {
            Kind::OnOff => match text {
                "on" | "1" => Some(Self::OnOff(true)),
                "off" | "0" => Some(Self::OnOff(false)),
                _ => None,
            },
            Kind::Int { min, max } => {
                let v = i64::from(atoi(text));
                (min..=max).contains(&v).then_some(Self::Int(v))
            }
            Kind::Real { min, max } => {
                // The lower bound in double precision, the upper one once
                // converted to single precision (3.4).
                let v = value::double(text);
                let single = v as f32;
                (v >= f64::from(min) - RANGE_EPSILON && single <= max + RANGE_EPSILON as f32)
                    .then_some(Self::Real(single))
            }
            Kind::List(names) => names
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(text))
                .map(|&(_, number)| Self::List(number)),
            Kind::Vector { min, max } => {
                let v = scan_vector(text)?;
                (0..3)
                    .all(|i| (min[i]..=max[i]).contains(&v[i]))
                    .then_some(Self::Vector(v))
            }
            Kind::Text { size } => Some(Self::Text(
                text.chars().take(size.saturating_sub(1)).collect(),
            )),
            Kind::Mode => parse_mode(text).map(|(w, h)| Self::Mode(w, h)),
            Kind::Frequency => {
                // Clamped, never rejected (4.2).
                let frequency = value::double(text).clamp(50.0, 200.0) as f32;
                Some(Self::Frequency(1.0 / frequency))
            }
            Kind::Action(_) | Kind::Name => None,
        }
    }

    /// The value as the console shows and saves it (its status).
    pub fn status(&self, kind: Kind) -> String {
        match self {
            Self::OnOff(on) => if *on { "on" } else { "off" }.to_owned(),
            Self::Int(v) => v.to_string(),
            Self::Real(v) => real_status(*v),
            Self::List(number) => match kind {
                Kind::List(names) => names
                    .iter()
                    .find(|(_, n)| n == number)
                    .map_or("?", |(name, _)| name)
                    .to_owned(),
                _ => "?".to_owned(),
            },
            Self::Vector(v) => format!(
                "{:.6},{:.6},{:.6}",
                f64::from(v[0]),
                f64::from(v[1]),
                f64::from(v[2])
            ),
            Self::Text(t) => t.clone(),
            Self::Mode(w, h) => format!("{w}x{h}"),
            // `%3.5f`, not trimmed (4.2).
            Self::Frequency(step) => format!("{:.5}", f64::from(1.0 / step)),
            Self::None => String::new(),
        }
    }

    /// The value as a number: on/off as 1/0, a list value as its stored
    /// number, a vector as its first component, a mode as its width, the
    /// physics frequency as the frequency; 0 for the others.
    pub fn number(&self) -> f32 {
        match self {
            Self::OnOff(on) => f32::from(u8::from(*on)),
            Self::Int(v) => *v as f32,
            Self::Real(v) => *v,
            Self::List(n) => *n as f32,
            Self::Vector(v) => v[0],
            Self::Mode(w, _) => *w as f32,
            Self::Frequency(step) => 1.0 / step,
            Self::Text(_) | Self::None => 0.0,
        }
    }
}

/// The help text of command `name` of `kind`: what `help` lists and a
/// rejection prints (3, 4).
pub fn help_text(name: &str, kind: Kind) -> String {
    let real = |v: f32| format!("{:.3}", f64::from(v));
    let vector = |v: [f32; 3]| {
        format!(
            "[{:.6},{:.6},{:.6}]",
            f64::from(v[0]),
            f64::from(v[1]),
            f64::from(v[2])
        )
    };
    match kind {
        Kind::OnOff => "'on/off' or '1/0'".to_owned(),
        Kind::Int { min, max } => format!("integer value in range [{min},{max}]"),
        Kind::Real { min, max } => format!("float value in range [{},{}]", real(min), real(max)),
        Kind::List(_) if name == "g_game_difficulty" => "game difficulty".to_owned(),
        Kind::List(names) => names.iter().map(|(n, _)| *n).collect::<Vec<_>>().join("/"),
        Kind::Vector { min, max } => {
            format!("vector3 in range {}-{}", vector(min), vector(max))
        }
        Kind::Text { size } => format!("string with up to {size} characters"),
        Kind::Action(own) => own.unwrap_or("no arguments").to_owned(),
        Kind::Mode => "change screen resolution WxH".to_owned(),
        Kind::Frequency => "no arguments".to_owned(),
        Kind::Name => "player name".to_owned(),
    }
}

/// C `%3.5f` of `v` with its trailing `0` characters removed, the decimal
/// point kept: `1.00000` → `1.`, `0.33330` → `0.3333` (3.4).
pub fn real_status(v: f32) -> String {
    let text = format!("{:.5}", f64::from(v));
    text.trim_end_matches('0').to_owned()
}

/// C `sscanf` with `%f,%f,%f`: three numbers, each after optional
/// whitespace, separated by commas; the text after the third is ignored
/// (3.6).
fn scan_vector(text: &str) -> Option<[f32; 3]> {
    let (x, rest) = value::scan_double(text)?;
    let (y, rest) = value::scan_double(rest.strip_prefix(',')?)?;
    let (z, _) = value::scan_double(rest.strip_prefix(',')?)?;
    Some([x as f32, y as f32, z as f32])
}

/// C `sscanf` with `%dx%d`: two decimal integers, each after optional
/// whitespace and with an optional sign, separated by `x`; the text after
/// the second is ignored (4.1, display-mode.md A.1).
pub fn parse_mode(text: &str) -> Option<(i32, i32)> {
    let (w, rest) = scan_int(text)?;
    let (h, _) = scan_int(rest.strip_prefix('x')?)?;
    Some((w, h))
}

/// One `%d`: whitespace, an optional sign and at least one digit; the
/// number (wrapping, as overflow is unestablished) and the text after it.
fn scan_int(text: &str) -> Option<(i32, &str)> {
    let s = text.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let sign = usize::from(s.starts_with(['+', '-']));
    let digits = s[sign..].bytes().take_while(u8::is_ascii_digit).count();
    (digits > 0).then(|| (value::int(&s[..sign + digits]) as i32, &s[sign + digits..]))
}

/// C `atoi` as the C runtime reads it, with `strtol` in base 10: leading
/// whitespace, an optional sign, decimal digits; a number beyond the signed
/// 32-bit range saturates (3.3).
fn atoi(text: &str) -> i32 {
    let s = text.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let negative = s.starts_with('-');
    let s = s.strip_prefix(['+', '-']).unwrap_or(s);
    let mut n: i64 = 0;
    for b in s.bytes().take_while(u8::is_ascii_digit) {
        n = (n * 10 + i64::from(b - b'0')).min(1 << 32);
    }
    let n = if negative { -n } else { n };
    n.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32
}
