// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

use super::*;

const PLAYER: u16 = 0;
const TRADER: u16 = 84;
const OTHER: u16 = 9;

/// 2.2: accepted when the partner's talk is enabled and both are alive;
/// the partner's side starts first, then the player character's.
#[test]
fn an_offer_starts_both_sides() {
    let mut t = Talks::default();
    assert!(t.is_talk_enabled(TRADER), "1: talk enabled from the start");
    assert!(!t.offer(TRADER, 0.0, PLAYER, 1.0), "a dead partner");
    assert!(
        !t.offer(TRADER, 1.0, PLAYER, -1.0),
        "a dead player character"
    );
    t.set_talk_enabled(TRADER, false);
    assert!(!t.offer(TRADER, 1.0, PLAYER, 1.0), "talk disabled");
    assert!(!t.is_talking(TRADER) && !t.is_talking(PLAYER));
    t.set_talk_enabled(TRADER, true);
    assert!(t.offer(TRADER, 1.0, PLAYER, 1.0));
    assert_eq!(
        (t.partner(TRADER), t.partner(PLAYER)),
        (Some(PLAYER), Some(TRADER))
    );
    assert_eq!(t.talking(), [PLAYER, TRADER]);
    // 1: disabling talk during a talk ends nothing.
    t.set_talk_enabled(TRADER, false);
    t.scheduled_update(TRADER, 1.0);
    assert!(t.is_talking(TRADER));
}

/// 5.4: a partner talking with another character already accepts and
/// takes the player character as its partner.
#[test]
fn an_offer_does_not_test_talking() {
    let mut t = Talks::default();
    assert!(t.offer(TRADER, 1.0, OTHER, 1.0));
    assert!(t.offer(TRADER, 1.0, PLAYER, 1.0));
    assert_eq!(t.partner(TRADER), Some(PLAYER));
    assert_eq!(
        t.partner(OTHER),
        Some(TRADER),
        "the other side is untouched"
    );
    t.scheduled_update(OTHER, 1.0);
    assert!(t.is_talking(OTHER), "its partner still talks");
}

/// 5.1, 5.3: stopping one side leaves the other until its scheduled
/// update; health 0 stops a side at its update.
#[test]
fn a_side_ends_at_its_scheduled_update() {
    let mut t = Talks::default();
    t.offer(TRADER, 1.0, PLAYER, 1.0);
    t.stop_talk(TRADER);
    assert!(!t.is_talking(TRADER) && t.is_talking(PLAYER));
    assert_eq!(t.partner(TRADER), None);
    t.scheduled_update(PLAYER, 1.0);
    assert!(!t.is_talking(PLAYER));

    t.offer(TRADER, 1.0, PLAYER, 1.0);
    t.scheduled_update(TRADER, 1.0);
    assert!(t.is_talking(TRADER), "both talking: nothing ends");
    t.scheduled_update(TRADER, 0.0);
    assert!(!t.is_talking(TRADER), "the partner's death");
    t.scheduled_update(PLAYER, 1.0);
    assert!(!t.is_talking(PLAYER), "then the player character's");
}

/// 6: coming online takes the description's start dialog as both, once;
/// scripts set and restore it.
#[test]
fn the_start_dialog() {
    let mut t = Talks::default();
    t.come_online(TRADER, "escape_trader_start_dialog");
    t.set_start_dialog(TRADER, "meet");
    t.come_online(TRADER, "other");
    assert_eq!(t.state(TRADER).start_dialog, "meet");
    t.restore_default_start_dialog(TRADER);
    let s = t.state(TRADER);
    assert_eq!(
        (s.start_dialog.as_str(), s.default_start_dialog.as_str()),
        ("escape_trader_start_dialog", "escape_trader_start_dialog")
    );
}

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-5
}

/// 4: the target's direction in the player-movement convention (forward
/// (sin yaw, 0, cos yaw), positive pitch down), with the special cases.
#[test]
fn the_target_angles() {
    let (y, p) = target_angles([1.0, 0.0, 1.0]);
    assert!(near(y, PI / 4.0) && near(p, 0.0));
    let (y, _) = target_angles([1.0, 0.0, -1.0]);
    assert!(near(y, PI / 4.0 - PI - PI / 2.0), "{y}: atan(dx/dz) − π");
    let (y, p) = target_angles([0.0, 1.0, 1.0]);
    assert!(near(y, 0.0) && near(p, -PI / 4.0), "above: looks up");
    assert_eq!(target_angles([1.0, 0.0, 0.0]).0, FRAC_PI_2);
    assert_eq!(target_angles([-1.0, 0.0, 0.0]).0, -FRAC_PI_2);
    assert_eq!(target_angles([0.0, 2.0, 0.0]), (0.0, -FRAC_PI_2));
    assert_eq!(target_angles([0.0, -2.0, 0.0]), (0.0, FRAC_PI_2));
    assert_eq!(target_angles([0.0, 0.0, 0.0]), (0.0, 0.0));
    assert_eq!(target_point([1.0, 2.0, 3.0, 0.8]), [1.0, 2.4, 3.0]);
}

/// 4: within 0.2 nothing moves; farther, the first update brings the
/// angle within π/6, then it creeps at 0.15 to 0.2 rad/s.
#[test]
fn the_view_turns_toward_the_partner() {
    assert_eq!(turn(0.1, 0.25, 0.016), 0.1);
    assert_eq!(
        turn(-PI + 0.05, PI - 0.05, 1.0),
        -PI + 0.05,
        "0.1 apart across ±π"
    );
    let a = turn(0.0, 2.0, 0.016);
    assert!(near(a, 2.0 - PI / 6.0), "{a}: within π/6 at once");
    let b = turn(a, 2.0, 1.0);
    assert!(near(b, a + 0.2), "{b}: 0.2 rad/s at π/6");
    let mut c = b;
    for _ in 0..100 {
        c = turn(c, 2.0, 1.0);
    }
    assert!(
        (2.0 - c) <= TURN_THRESHOLD + 1e-5 && (2.0 - c) > 0.0,
        "{c}: stops within 0.2"
    );
    let (y, p) = turn_view([0.0, 1.0, 0.0], [0.0, 0.6, 5.0, 0.8], 2.0, 0.0, 0.016);
    assert!(near(y, PI / 6.0) && p == 0.0, "{y} {p}: the yaw only");
}

/// 5.2: rules 1–3, the special mode never left by distance.
#[test]
fn the_window_closes() {
    let open = Speakers {
        player_talking: true,
        both_exist: true,
        distance: 3.0,
        special_mode: false,
    };
    assert!(!open.window_closes());
    assert!(
        Speakers {
            player_talking: false,
            ..open
        }
        .window_closes()
    );
    assert!(
        Speakers {
            both_exist: false,
            ..open
        }
        .window_closes()
    );
    assert!(
        Speakers {
            distance: 3.01,
            ..open
        }
        .window_closes()
    );
    assert!(
        !Speakers {
            distance: 30.0,
            special_mode: true,
            ..open
        }
        .window_closes()
    );
}
