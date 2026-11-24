// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The game clock of a New Game from the Russian edition's
//! `config\alife.ltx` (docs/formats/sav.md sections 5, 12, 15) driving the
//! default weather's hourly blending (docs/behavior/static-shading.md 9.2,
//! docs/behavior/sky.md 1, 3.3). Skips when `VOLYA_GOG_INSTALLER` is unset.

#[path = "../../volya-engine-formats/tests/support/gog_archives.rs"]
mod gog_archives;

use volya_engine_core::clock::GameClock;
use volya_engine_core::config::{self, PathAliases};
use volya_engine_core::vfs::StdVfs;
use volya_engine_core::vfs::db::DbVfs;
use volya_engine_core::weather::{Environment, Weather};

const FSGAME: &str = r"$game_data$ =     false| true | $fs_root$      | gamedata\
$game_config$ =   true | false| $game_data$    | config\
";

fn close(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-5)
}

#[test]
fn the_clock_moves_the_weather_toward_the_next_hour() {
    let Some(archives) = gog_archives::archives() else {
        return;
    };
    let root = StdVfs::new(archives[0].file.parent().unwrap());
    let vfs = DbVfs::open_game(&root, Some("mods")).unwrap();
    let aliases = PathAliases::parse(FSGAME.as_bytes(), "").unwrap();
    let alife = config::load_aliased(&vfs, &aliases, "$game_config$", "alife.ltx").unwrap();
    let system = config::load_aliased(&vfs, &aliases, "$game_config$", "system.ltx").unwrap();
    let weather = Weather::load(&system, "default").unwrap();
    assert_eq!(weather.sections.len(), 24);

    // sav.md section 15: the New Game time and factor.
    let mut clock = GameClock::new_game(&alife).unwrap();
    assert_eq!(clock.time(), 63_471_533_400_000);
    assert_eq!(clock.time_factor(), 10.0);
    let (a, b, t) = weather.bracket(clock.time_of_day());
    assert_eq!(
        (a.name.as_str(), b.name.as_str(), t),
        ("default_weather_05", "default_weather_06", 0.5)
    );
    let (a, b) = (a.clone(), b.clone());
    let start = Environment::blend(&a, &b, t, &[], [0.0; 3], 1.0);

    // 90 real seconds at ×10 are 15 game minutes: 05:45, t = 0.75
    // (sky.md 1: t grows by 1/3,600 per game second).
    clock.advance(90_000);
    assert_eq!(clock.time(), 63_471_533_400_000 + 900_000);
    let (a2, b2, t) = weather.bracket(clock.time_of_day());
    assert_eq!(
        (a2.name.as_str(), b2.name.as_str(), t),
        (a.name.as_str(), b.name.as_str(), 0.75)
    );
    let later = Environment::blend(&a, &b, t, &[], [0.0; 3], 1.0);
    let lerp = |x: [f32; 3], y: [f32; 3]| [0, 1, 2].map(|i| x[i] + (y[i] - x[i]) * 0.75);
    assert!(close(later.ambient, lerp(a.ambient, b.ambient)));
    assert!(close(later.sun_color, lerp(a.sun_color, b.sun_color)));
    assert!(close(later.sky_color, lerp(a.sky_color, b.sky_color)));
    // Toward 06:00: the sun colour moved from the start's halfway value
    // toward [default_weather_06]'s (sky.md 4: 0.759 → 0.818).
    let to_b = |e: &Environment| (b.sun_color[0] - e.sun_color[0]).abs();
    assert!(to_b(&later) < to_b(&start));
    assert!((later.sun_color[0] - 0.78850).abs() < 1e-4);

    // Paused, it stays; at 06:00 the next hour's pair begins.
    clock.set_paused(true);
    clock.advance(1_000_000);
    assert_eq!(clock.time_of_day(), 5.75 * 3600.0);
    clock.set_paused(false);
    clock.advance(90_000);
    let (a, b, t) = weather.bracket(clock.time_of_day());
    assert_eq!(
        (a.name.as_str(), b.name.as_str(), t),
        ("default_weather_06", "default_weather_07", 0.0)
    );
}
