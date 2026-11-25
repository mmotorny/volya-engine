// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The game clock (docs/formats/sav.md sections 5 and 12): game time in
//! game milliseconds, as a save stores it, advancing at the time factor (game
//! milliseconds per real millisecond) while the game is not paused.

use std::io;

use crate::config::{Config, value};
use volya_engine_formats::sav;

pub use volya_engine_formats::sav::GameTime;

/// Milliseconds in a day: the time of day is the game time modulo a day
/// (section 12's formula counts whole days, then the clock time).
pub const DAY_MS: u64 = 86_400_000;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// Integers `a<sep>b<sep>c` (each part trimmed); a missing second or third
/// part reads as 0 (our choice; the shipped values have all three).
fn three(text: &str, sep: char) -> Option<(u32, u32, u32)> {
    let mut parts = text.trim().split(sep).map(|p| p.trim().parse::<u32>().ok());
    let a = parts.next()??;
    let b = parts.next().unwrap_or(Some(0))?;
    let c = parts.next().unwrap_or(Some(0))?;
    parts.next().is_none().then_some((a, b, c))
}

/// The game clock.
#[derive(Clone, Debug, PartialEq)]
pub struct GameClock {
    time: GameTime,
    /// Game milliseconds not yet whole.
    fraction: f64,
    paused: bool,
}

impl GameClock {
    /// A clock at `time`, not paused.
    pub fn new(time: GameTime) -> Self {
        Self {
            time,
            fraction: 0.0,
            paused: false,
        }
    }

    /// The clock of a New Game (section 5): `[alife]` of `alife`
    /// (`config\alife.ltx`): the time from `start_date` (day.month.year) and
    /// `start_time` (h:m:s), both factors from `time_factor` and
    /// `normal_time_factor`. All four are read: a missing one is fatal
    /// (ltx-config.md 11); a malformed one fails.
    pub fn new_game(alife: &Config) -> io::Result<Self> {
        let get = |key: &str| alife.read("alife", key);
        let parse = |key: &str, sep: char| -> io::Result<(u32, u32, u32)> {
            let text = get(key)?;
            three(text, sep).ok_or_else(|| invalid(format!("[alife] {key} = {text}: malformed")))
        };
        let (day, month, year) = parse("start_date", '.')?;
        let (hour, minute, second) = parse("start_time", ':')?;
        Ok(Self::new(GameTime {
            time: sav::game_time(year, month, day, hour, minute, second, 0)?,
            time_factor: value::float(get("time_factor")?),
            normal_time_factor: value::float(get("normal_time_factor")?),
        }))
    }

    /// The game time, game milliseconds (sav.md section 12).
    pub fn time(&self) -> u64 {
        self.time.time
    }

    /// The time and both factors, as chunk 5 of a save stores them.
    pub fn game_time(&self) -> GameTime {
        self.time
    }

    /// Game milliseconds per real millisecond.
    pub fn time_factor(&self) -> f32 {
        self.time.time_factor
    }

    /// The time of day, seconds (0 … 86,400).
    pub fn time_of_day(&self) -> f32 {
        ((self.time.time % DAY_MS) as f64 / 1000.0) as f32
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    /// Pauses or resumes the clock (a movie item's `pause_state`,
    /// level-loading.md section 2.3; the menu, title-menu.md section 2).
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    /// Advances by `real_ms` real milliseconds: `time_factor` × as many game
    /// milliseconds, nothing while paused. Parts of a game millisecond carry
    /// over to the next call (our choice; the time is whole milliseconds).
    pub fn advance(&mut self, real_ms: u64) {
        if self.paused {
            return;
        }
        let exact = real_ms as f64 * f64::from(self.time.time_factor) + self.fraction;
        let whole = exact.max(0.0).floor();
        self.fraction = exact - whole;
        self.time.time = self.time.time.saturating_add(whole as u64);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alife(text: &str) -> Config {
        volya_engine_formats::ltx::parse(format!("[alife]\n{text}").as_bytes()).unwrap()
    }

    /// The shipped `[alife]` time keys (sav.md section 15).
    const SHIPPED: &str = "time_factor = 10\nnormal_time_factor = 10\nstart_date = 01.05.2012\n\
                           start_time = 05:30:00 ; comment\n";

    #[test]
    fn starts_at_the_new_game_time() {
        // sav.md section 15.
        let clock = GameClock::new_game(&alife(
            "time_factor = 10\nnormal_time_factor = 10\nstart_date = 01.05.2012\n\
             start_time = 05:30:00 ; comment\n",
        ))
        .unwrap();
        assert_eq!(clock.time(), 63_471_533_400_000);
        assert_eq!(
            clock.time().to_le_bytes(),
            [0xc0, 0x9f, 0x33, 0x1e, 0xba, 0x39, 0, 0]
        );
        assert_eq!(clock.time_factor(), 10.0);
        assert_eq!(clock.game_time().normal_time_factor, 10.0);
        assert_eq!(clock.time_of_day(), 19_800.0);
        assert!(!clock.paused());
        // ltx-config.md 11: a missing key is fatal.
        for key in [
            "time_factor",
            "normal_time_factor",
            "start_date",
            "start_time",
        ] {
            let without: String = SHIPPED
                .lines()
                .filter(|l| !l.starts_with(key))
                .map(|l| format!("{l}\n"))
                .collect();
            let err = GameClock::new_game(&alife(&without)).unwrap_err();
            assert!(err.to_string().contains(key), "{err}");
        }
        let bad = SHIPPED.replace("01.05.2012", "1.13.2012");
        assert!(GameClock::new_game(&alife(&bad)).is_err());
        let bad = SHIPPED.replace("05:30:00", "5h");
        assert!(GameClock::new_game(&alife(&bad)).is_err());
    }

    #[test]
    fn advances_at_the_time_factor() {
        let mut clock = GameClock::new_game(&alife(SHIPPED)).unwrap();
        let start = clock.time();
        clock.advance(1000);
        assert_eq!(clock.time() - start, 10_000);
        // 180 real seconds at ×10 reach 06:00.
        clock.advance(179_000);
        assert_eq!(clock.time_of_day(), 6.0 * 3600.0);
        // Fractions carry over.
        let mut slow = GameClock::new(GameTime {
            time: 0,
            time_factor: 0.25,
            normal_time_factor: 0.25,
        });
        for _ in 0..4 {
            slow.advance(1);
        }
        assert_eq!(slow.time(), 1);
        // The time of day wraps at midnight.
        let mut late = GameClock::new(GameTime {
            time: sav::game_time(2012, 5, 1, 23, 59, 59, 0).unwrap(),
            time_factor: 10.0,
            normal_time_factor: 10.0,
        });
        late.advance(200);
        assert_eq!(late.time_of_day(), 1.0);
    }

    #[test]
    fn stands_still_while_paused() {
        let mut clock = GameClock::new_game(&alife(SHIPPED)).unwrap();
        let start = clock.time();
        clock.set_paused(true);
        clock.advance(60_000);
        assert_eq!(clock.time(), start);
        clock.set_paused(false);
        clock.advance(100);
        assert_eq!(clock.time(), start + 1000);
    }
}
