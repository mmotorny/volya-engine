// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The player character's conditions (docs/behavior/player-state.md,
//! cited by section): their configuration (1), values at a New Game (2.3),
//! the update over game time (4.1, 4.2), the thresholds that limit movement
//! (4.3), the jump's cost (4.4) and using eatable items (5); with the
//! wounds, their bleeding and healing of docs/behavior/player-hits.md
//! (cited as `hits`) section 5.

use std::io;

use super::hits::Wounds;
use crate::config::{Config, Section, value};

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// Psy health this close to 0 counts as 0 (4.2 item 4).
const PSY_ZERO: f32 = 1e-7;
/// The lowest health (4.2 item 13).
pub const MIN_HEALTH: f32 = -0.01;
/// Max power's range (2.3, 4.2 item 2, 5 item 6).
pub const MAX_POWER_RANGE: [f32; 2] = [0.1, 1.0];
/// Above the carry limit, each unit of weight leaks max power at this
/// share (4.2 item 2).
const OVERWEIGHT_LEAK: f32 = 0.1;

/// A threshold pair with hysteresis (4.3): on below `begin`, off above
/// `end`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hysteresis {
    pub begin: f32,
    pub end: f32,
}

/// `[actor_condition]` (section 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConditionConfig {
    pub stand_power: f32,
    pub walk_power: f32,
    pub walk_weight_power: f32,
    pub overweight_walk_k: f32,
    pub accel_k: f32,
    pub sprint_k: f32,
    pub jump_power: f32,
    pub jump_weight_power: f32,
    pub overweight_jump_k: f32,
    pub max_power_leak_speed: f32,
    pub satiety_v: f32,
    pub satiety_power_v: f32,
    pub satiety_health_v: f32,
    pub radiation_v: f32,
    pub radiation_health_v: f32,
    pub psy_health_v: f32,
    pub morale_v: f32,
    pub alcohol_v: f32,
    pub bleeding_v: f32,
    pub health_restore_v: f32,
    /// `min_wound_size` and `wound_incarnation_v` (hits 1, 5.3).
    pub min_wound_size: f32,
    pub wound_incarnation_v: f32,
    pub max_walk_weight: f32,
    pub limping_health: Hysteresis,
    pub limping_power: Hysteresis,
    pub cant_walk_power: Hysteresis,
    pub cant_sprint_power: Hysteresis,
}

fn required<'a>(section: Section<'a>, key: &str) -> io::Result<&'a str> {
    section
        .get(key)
        .flatten()
        .ok_or_else(|| invalid(format!("[{}] has no {key}", section.name())))
}

impl ConditionConfig {
    /// `[<name>]` of `system`, the section `[actor] condition_sect` names.
    /// `health_restore_v` is 0 when absent (section 1); any other missing
    /// key is an error (our choice: the spec gives no default). Each
    /// `*_begin` above its `*_end` is an error, as the game's load check
    /// stops (section 1).
    pub fn read(system: &Config, name: &str) -> io::Result<Self> {
        let s = system
            .section(name)
            .ok_or_else(|| invalid(format!("no section [{name}]")))?;
        let f = |key: &str| required(s, key).map(value::float);
        let pair = |key: &str| -> io::Result<Hysteresis> {
            let h = Hysteresis {
                begin: f(&format!("{key}_begin"))?,
                end: f(&format!("{key}_end"))?,
            };
            if h.begin > h.end {
                return Err(invalid(format!(
                    "[{name}] {key}_begin {} is above {key}_end {}",
                    h.begin, h.end
                )));
            }
            Ok(h)
        };
        Ok(Self {
            stand_power: f("stand_power")?,
            walk_power: f("walk_power")?,
            walk_weight_power: f("walk_weight_power")?,
            overweight_walk_k: f("overweight_walk_k")?,
            accel_k: f("accel_k")?,
            sprint_k: f("sprint_k")?,
            jump_power: f("jump_power")?,
            jump_weight_power: f("jump_weight_power")?,
            overweight_jump_k: f("overweight_jump_k")?,
            max_power_leak_speed: f("max_power_leak_speed")?,
            satiety_v: f("satiety_v")?,
            satiety_power_v: f("satiety_power_v")?,
            satiety_health_v: f("satiety_health_v")?,
            radiation_v: f("radiation_v")?,
            radiation_health_v: f("radiation_health_v")?,
            psy_health_v: f("psy_health_v")?,
            morale_v: f("morale_v")?,
            alcohol_v: f("alcohol_v")?,
            bleeding_v: f("bleeding_v")?,
            health_restore_v: s
                .get("health_restore_v")
                .flatten()
                .map_or(0.0, value::float),
            min_wound_size: f("min_wound_size")?,
            wound_incarnation_v: f("wound_incarnation_v")?,
            max_walk_weight: f("max_walk_weight")?,
            limping_health: pair("limping_health")?,
            limping_power: pair("limping_power")?,
            cant_walk_power: pair("cant_walk_power")?,
            cant_sprint_power: pair("cant_sprint_power")?,
        })
    }
}

/// What the movement did, as the update reads it (4.2 item 1): the
/// movement state's flags as the latest movement update left them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Activity {
    /// Any of forward, back, left, right.
    pub moving: bool,
    /// The state runs (`movement` section 4.1, without the ability part).
    pub runs: bool,
    /// The state's sprint flag.
    pub sprints: bool,
}

/// The carried weight and what depends on the worn outfit (3.4, 4.2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Load {
    /// The carried weight `M`.
    pub weight: f32,
    /// `[inventory] max_weight`.
    pub max_weight: f32,
    /// The carry limit `W`: `max_weight` plus the worn outfit's
    /// `additional_inventory_weight2`.
    pub carry_limit: f32,
    /// The walking limit: `max_walk_weight` plus the worn outfit's
    /// `additional_inventory_weight`.
    pub walk_limit: f32,
    /// The worn outfit's power loss factor `L`.
    pub power_loss: f32,
}

/// An eatable item's effects (section 5).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Eatable {
    pub eat_health: f32,
    pub eat_power: f32,
    pub eat_satiety: f32,
    pub eat_radiation: f32,
    /// Clamped to 0 … 1 at load.
    pub wounds_heal_perc: f32,
    pub eat_portions_num: i32,
    /// 0 when absent.
    pub eat_max_power: f32,
    /// A drink's (`II_BOTTL`) `eat_alcohol`, 0 when absent; `None` for other
    /// eatables, which do not read it.
    pub eat_alcohol: Option<f32>,
}

/// Class ids of drinks (section 5 item 7).
pub const DRINK_CLASS: &str = "II_BOTTL";

impl Eatable {
    /// The eatable keys of `section` of class id `class_id`, or `None` when
    /// the section has no `eat_health` (not eatable: our test, since every
    /// eatable has all the keys of section 5 and they have no defaults).
    /// A missing key of an eatable is an error.
    pub fn read(section: Section<'_>, class_id: &str) -> io::Result<Option<Self>> {
        if !section.contains("eat_health") {
            return Ok(None);
        }
        let f = |key: &str| required(section, key).map(value::float);
        let optional = |key: &str| section.get(key).flatten().map_or(0.0, value::float);
        Ok(Some(Self {
            eat_health: f("eat_health")?,
            eat_power: f("eat_power")?,
            eat_satiety: f("eat_satiety")?,
            eat_radiation: f("eat_radiation")?,
            wounds_heal_perc: f("wounds_heal_perc")?.clamp(0.0, 1.0),
            eat_portions_num: value::int(required(section, "eat_portions_num")?) as i32,
            eat_max_power: optional("eat_max_power"),
            eat_alcohol: (class_id == DRINK_CLASS).then(|| optional("eat_alcohol")),
        }))
    }
}

/// The conditions of the online character (2.3).
#[derive(Clone, Debug, PartialEq)]
pub struct Conditions {
    pub health: f32,
    pub max_health: f32,
    pub power: f32,
    pub max_power: f32,
    pub satiety: f32,
    pub radiation: f32,
    pub psy_health: f32,
    pub morale: f32,
    pub alcohol: f32,
    /// The movement thresholds (4.3).
    pub limping: bool,
    pub cant_walk: bool,
    pub cant_sprint: bool,
    /// Whether the character can be harmed (4.2; set at creation, and in a
    /// single-player game nothing changes it, hits 6).
    pub can_be_harmed: bool,
    /// The wounds (hits 5.1).
    pub wounds: Wounds,
    /// The character was bleeding at the previous update (4.2 item 9).
    bleeding: bool,
    /// Changes accumulated until the next update applies them (4.2 item 13,
    /// section 5).
    health_change: f32,
    power_change: f32,
    radiation_change: f32,
    psy_health_change: f32,
    morale_change: f32,
}

impl Conditions {
    /// The values at a New Game with health `health` from the player's
    /// record (2.3).
    pub fn new_game(health: f32) -> Self {
        Self {
            health,
            max_health: 1.0,
            power: 1.0,
            max_power: 1.0,
            satiety: 1.0,
            radiation: 0.0,
            psy_health: 1.0,
            morale: 1.0,
            alcohol: 0.0,
            limping: false,
            cant_walk: false,
            cant_sprint: false,
            can_be_harmed: true,
            wounds: Wounds::default(),
            bleeding: false,
            health_change: 0.0,
            power_change: 0.0,
            radiation_change: 0.0,
            psy_health_change: 0.0,
            morale_change: 0.0,
        }
    }

    /// The total bleeding of the wounds (4.2 item 9, hits 5.2).
    pub fn total_bleeding(&self) -> f32 {
        self.wounds.total_bleeding()
    }

    /// Whether the previous update found the character bleeding (4.2 item
    /// 9).
    pub fn bleeding(&self) -> bool {
        self.bleeding
    }

    /// A hit's changes (hits 4.5), applied at the next update (4.7): health
    /// only while the character can be harmed.
    pub fn add_hit_changes(&mut self, health: f32, power: f32, radiation: f32, psy_health: f32) {
        if self.can_be_harmed {
            self.health_change += health;
        }
        self.power_change += power;
        self.radiation_change += radiation;
        self.psy_health_change += psy_health;
    }

    /// The first update after coming online (4.1, `dt` 0): the health, power
    /// and radiation changes pending before it are discarded, the psy
    /// health's kept (hits 4.7, as corrected in review).
    pub fn discard_pending(&mut self) {
        self.health_change = 0.0;
        self.power_change = 0.0;
        self.radiation_change = 0.0;
    }

    /// Alive: health above 0 (4.1).
    pub fn alive(&self) -> bool {
        self.health > 0.0
    }

    /// One update of `dt` game seconds (4.2) with the movement's `activity`
    /// and the carried `load`, in a single-player game (every item runs).
    pub fn update(&mut self, c: &ConditionConfig, dt: f32, activity: Activity, load: &Load) {
        self.update_items(c, dt, activity, load);
        // Hits 5.3: wounds healed to size 0 are removed right after it.
        self.wounds.remove_healed();
    }

    fn update_items(&mut self, c: &ConditionConfig, dt: f32, activity: Activity, load: &Load) {
        let l = load.power_loss;
        // 1. Power from movement.
        if !activity.moving {
            self.power_change -= c.stand_power * dt;
        } else {
            let w = load.weight / load.max_weight;
            let o = if w > 1.0 { c.overweight_walk_k } else { 1.0 };
            let k = match (activity.runs, activity.sprints) {
                (false, _) => 1.0,
                (true, false) => c.accel_k,
                (true, true) => c.sprint_k,
            };
            self.power_change -= dt * k * (c.walk_power + c.walk_weight_power * o * w) * l;
        }
        // 2. Max power.
        let m = load.weight;
        let cap = load.carry_limit;
        let leak = (m.min(cap) / cap + ((m - cap) * OVERWEIGHT_LEAK).max(0.0) + 1.0)
            * c.max_power_leak_speed
            * dt;
        self.max_power = (self.max_power - leak).clamp(MAX_POWER_RANGE[0], MAX_POWER_RANGE[1]);
        // 3. Alcohol.
        self.alcohol = (self.alcohol + c.alcohol_v * dt).clamp(0.0, 1.0);
        // 4. Psy health at zero kills.
        if self.psy_health.abs() < PSY_ZERO {
            self.health = 0.0;
        }
        // 5. Satiety.
        if self.satiety > 0.0 {
            self.satiety = (self.satiety - c.satiety_v * dt).clamp(0.0, 1.0);
        }
        // 6. Health from satiety.
        if !self.bleeding && self.can_be_harmed {
            self.health_change += if self.satiety > 0.0 {
                c.satiety_health_v * dt
            } else {
                -c.satiety_health_v * dt
            };
        }
        // 7. Power from satiety.
        self.power_change += c.satiety_power_v * dt;
        // 8. Nothing further unless health is above 0 here. When the changes
        // so far take it to 0 or below the character dies (out of scope);
        // items 9–13 still run, health being above 0 at this point.
        if self.health <= 0.0 {
            return;
        }
        // 9. Bleeding (hits 5.2), then the wounds heal with time (5.3).
        let bleeding = self.total_bleeding() * c.bleeding_v * dt;
        if self.can_be_harmed {
            self.health_change -= bleeding;
        }
        self.health_change += c.health_restore_v * dt;
        self.bleeding = bleeding.abs() >= 1e-7;
        self.wounds
            .heal(c.wound_incarnation_v * dt, c.min_wound_size);
        // 10. Radiation.
        if self.radiation > 0.0 {
            self.radiation_change -= c.radiation_v * dt;
            if self.can_be_harmed {
                self.health_change -= c.radiation_health_v * dt * self.radiation;
            }
        }
        // 11. Psy health.
        if self.psy_health > 0.0 {
            self.psy_health_change += c.psy_health_v * dt;
        }
        // 12. Morale.
        if self.morale < 1.0 {
            self.morale_change += c.morale_v * dt;
        }
        // 13. Apply and clamp.
        self.health = (self.health + std::mem::take(&mut self.health_change))
            .clamp(MIN_HEALTH, self.max_health);
        self.power =
            (self.power + std::mem::take(&mut self.power_change)).clamp(0.0, self.max_power);
        self.radiation =
            (self.radiation + std::mem::take(&mut self.radiation_change)).clamp(0.0, 1.0);
        self.morale = (self.morale + std::mem::take(&mut self.morale_change)).clamp(0.0, 1.0);
        self.psy_health =
            (self.psy_health + std::mem::take(&mut self.psy_health_change)).clamp(0.0, 1.0);
    }

    /// Re-evaluates the thresholds of 4.3, each keeping its value between
    /// its two thresholds.
    pub fn evaluate_thresholds(&mut self, c: &ConditionConfig) {
        let (p, h) = (self.power, self.health);
        if p < c.limping_power.begin || h < c.limping_health.begin {
            self.limping = true;
        } else if p > c.limping_power.end && h > c.limping_health.end {
            self.limping = false;
        }
        let toggle = |state: &mut bool, t: Hysteresis| {
            if p < t.begin {
                *state = true;
            } else if p > t.end {
                *state = false;
            }
        };
        toggle(&mut self.cant_walk, c.cant_walk_power);
        toggle(&mut self.cant_sprint, c.cant_sprint_power);
    }

    /// The power a jump takes at its start (4.4), directly; the next update
    /// clamps it.
    pub fn jump(&mut self, c: &ConditionConfig, load: &Load) {
        let w = load.weight / load.carry_limit;
        let o = if w > 1.0 { c.overweight_jump_k } else { 1.0 };
        self.power -= (c.jump_power + c.jump_weight_power * o * w) * load.power_loss;
    }

    /// Uses an eatable item (section 5 items 1–7): health, power and
    /// radiation change at the next update; satiety, max power and a
    /// drink's alcohol at once; the wounds heal by `wounds_heal_perc` at
    /// once (hits 5.4), and those healed are removed at the next update.
    pub fn eat(&mut self, c: &ConditionConfig, e: &Eatable) {
        self.wounds.heal(e.wounds_heal_perc, c.min_wound_size);
        if e.eat_health >= 0.0 || self.can_be_harmed {
            self.health_change += e.eat_health;
        }
        self.power_change += e.eat_power;
        self.satiety = (self.satiety + e.eat_satiety).clamp(0.0, 1.0);
        self.radiation_change += e.eat_radiation;
        self.max_power =
            (self.max_power + e.eat_max_power).clamp(MAX_POWER_RANGE[0], MAX_POWER_RANGE[1]);
        if let Some(alcohol) = e.eat_alcohol {
            self.alcohol += alcohol;
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The shipped `[actor_condition]` (section 1).
    pub(crate) const ACTOR_CONDITION: &str = "[actor_condition]\n\
        stand_power = -0.005\nwalk_power = 0.00001\nwalk_weight_power = 0.00005\n\
        overweight_walk_k = 5\naccel_k = 3\nsprint_k = 75\njump_power = 0.01\n\
        jump_weight_power = 0.05\noverweight_jump_k = 5\nmax_power_leak_speed = 0.0\n\
        satiety_v = 0.000015\nsatiety_power_v = 0.000055\nsatiety_health_v = 0.0001\n\
        radiation_v = 0.0001\nradiation_health_v = 0.004\npsy_health_v = 0.001\n\
        morale_v = 0.0001\nalcohol_v = -0.0003\nbleeding_v = 0.0005\n\
        wound_incarnation_v = 0.003\nmin_wound_size = 0.0256\nmax_walk_weight = 60\n\
        health_hit_part = 1.0\npower_hit_part = 0.1\n\
        limping_health_begin = 0.1\nlimping_health_end = 0.2\n\
        limping_power_begin = 0.1\nlimping_power_end = 0.2\n\
        cant_walk_power_begin = 0.01\ncant_walk_power_end = 0.10\n\
        cant_sprint_power_begin = 0.20\ncant_sprint_power_end = 0.30\n";

    fn config() -> ConditionConfig {
        let system = volya_engine_formats::ltx::parse(ACTOR_CONDITION.as_bytes()).unwrap();
        ConditionConfig::read(&system, "actor_condition").unwrap()
    }

    /// The New Game load (3.4): 4.3 carried, the novice outfit.
    fn load() -> Load {
        Load {
            weight: 4.3,
            max_weight: 50.0,
            carry_limit: 50.0,
            walk_limit: 60.0,
            power_loss: 1.0,
        }
    }

    const STAND: Activity = Activity {
        moving: false,
        runs: false,
        sprints: false,
    };
    const WALK: Activity = Activity {
        moving: true,
        runs: false,
        sprints: false,
    };
    const RUN: Activity = Activity {
        moving: true,
        runs: true,
        sprints: false,
    };
    const SPRINT: Activity = Activity {
        moving: true,
        runs: true,
        sprints: true,
    };

    #[track_caller]
    fn near(got: f32, want: f32, tolerance: f32) {
        assert!((got - want).abs() <= tolerance, "{got} != {want}");
    }

    /// Power's change over one update of `dt` from `power` doing
    /// `activity`.
    fn power_rate(activity: Activity, power: f32, dt: f32) -> f32 {
        let c = config();
        let mut s = Conditions::new_game(1.0);
        s.power = power;
        s.update(&c, dt, activity, &load());
        (s.power - power) / dt
    }

    #[test]
    fn reads_the_configuration() {
        let c = config();
        assert_eq!(c.health_restore_v, 0.0);
        assert_eq!(c.sprint_k, 75.0);
        assert_eq!(
            c.cant_sprint_power,
            Hysteresis {
                begin: 0.2,
                end: 0.3
            }
        );
        // A begin above its end stops the load.
        let bad = ACTOR_CONDITION.replace(
            "cant_walk_power_begin = 0.01",
            "cant_walk_power_begin = 0.5",
        );
        let system = volya_engine_formats::ltx::parse(bad.as_bytes()).unwrap();
        assert!(ConditionConfig::read(&system, "actor_condition").is_err());
        let missing = ACTOR_CONDITION.replace("sprint_k = 75\n", "");
        let system = volya_engine_formats::ltx::parse(missing.as_bytes()).unwrap();
        assert!(ConditionConfig::read(&system, "actor_condition").is_err());
    }

    #[test]
    fn power_rates_at_new_game_weight() {
        // Section 4.2's table, per game second (from below max power, so
        // the clamp does not hide the change).
        near(power_rate(STAND, 0.5, 1.0), 0.005055, 1e-7);
        near(power_rate(WALK, 0.5, 1.0), 0.0000407, 1e-8);
        near(power_rate(RUN, 0.5, 1.0), 0.0000121, 1e-8);
        near(power_rate(SPRINT, 0.5, 1.0), -0.0010175, 1e-7);
    }

    #[test]
    fn overweight_and_power_loss_scale_walking() {
        let c = config();
        let mut s = Conditions::new_game(1.0);
        s.power = 0.5;
        // 75 carried: w = 1.5 > 1, O = 5; L = 2.
        let heavy = Load {
            weight: 75.0,
            power_loss: 2.0,
            ..load()
        };
        s.update(&c, 1.0, WALK, &heavy);
        let want = -(0.00001 + 0.00005 * 5.0 * 1.5) * 2.0 + 0.000055;
        near(s.power - 0.5, want, 1e-7);
    }

    #[test]
    fn power_stays_within_max_power() {
        let c = config();
        let mut s = Conditions::new_game(1.0);
        s.update(&c, 100.0, STAND, &load());
        assert_eq!(s.power, 1.0);
        s.max_power = 0.05;
        s.update(&c, 0.0, STAND, &load());
        // Max power keeps to 0.1 … 1, power to 0 … max power.
        assert_eq!(s.max_power, 0.1);
        assert_eq!(s.power, 0.1);
        s.power = 0.0;
        s.update(&c, 10.0, SPRINT, &load());
        assert_eq!(s.power, 0.0);
    }

    #[test]
    fn max_power_leaks_by_the_carry_limit() {
        let mut c = config();
        c.max_power_leak_speed = 0.001;
        let mut s = Conditions::new_game(1.0);
        // 60 carried over a limit of 50: (1 + 1 + 1) × 0.001 per second.
        let heavy = Load {
            weight: 60.0,
            ..load()
        };
        s.update(&c, 10.0, STAND, &heavy);
        near(s.max_power, 1.0 - 0.03, 1e-6);
    }

    #[test]
    fn satiety_feeds_health_and_hunger_starves_it() {
        let c = config();
        let mut s = Conditions::new_game(0.5);
        s.update(&c, 100.0, STAND, &load());
        near(s.satiety, 1.0 - 0.0015, 1e-6);
        near(s.health, 0.5 + 0.01, 1e-6);
        // Health rises only to max health.
        s.health = 1.0;
        s.update(&c, 100.0, STAND, &load());
        assert_eq!(s.health, 1.0);
        // From 1 to 0 in 66,666.7 game seconds.
        let mut s = Conditions::new_game(1.0);
        s.update(&c, 66_666.0, STAND, &load());
        assert!(s.satiety > 0.0 && s.satiety < 2e-5, "{}", s.satiety);
        s.update(&c, 1.0, STAND, &load());
        assert_eq!(s.satiety, 0.0);
        // Hungry (from the update that emptied it), health falls at the
        // same rate.
        s.update(&c, 100.0, STAND, &load());
        near(s.health, 1.0 - 0.0001 - 0.01, 1e-6);
        // Without harm, hunger does not hurt.
        s.can_be_harmed = false;
        s.update(&c, 100.0, STAND, &load());
        near(s.health, 0.9899, 1e-6);
    }

    #[test]
    fn radiation_decays_and_hurts() {
        let c = config();
        let mut s = Conditions::new_game(1.0);
        s.satiety = 0.0;
        s.radiation = 0.5;
        s.update(&c, 10.0, STAND, &load());
        near(s.radiation, 0.5 - 0.001, 1e-7);
        // −0.0001 × 10 from hunger, −0.004 × 10 × 0.5 from radiation.
        near(s.health, 1.0 - 0.001 - 0.02, 1e-6);
        // Health keeps to −0.01 … max health.
        s.radiation = 1.0;
        s.update(&c, 1000.0, STAND, &load());
        assert_eq!(s.health, MIN_HEALTH);
        assert!(!s.alive());
    }

    #[test]
    fn psy_health_morale_and_alcohol_recover() {
        let c = config();
        let mut s = Conditions::new_game(1.0);
        s.psy_health = 0.5;
        s.morale = 0.5;
        s.alcohol = 0.5;
        s.update(&c, 100.0, STAND, &load());
        near(s.psy_health, 0.6, 1e-6);
        near(s.morale, 0.51, 1e-6);
        near(s.alcohol, 0.47, 1e-6);
        s.update(&c, 10_000.0, STAND, &load());
        assert_eq!((s.psy_health, s.morale, s.alcohol), (1.0, 1.0, 0.0));
    }

    #[test]
    fn psy_health_at_zero_kills() {
        let c = config();
        let mut s = Conditions::new_game(1.0);
        s.psy_health = 0.0;
        s.power = 0.5;
        s.update(&c, 1.0, STAND, &load());
        assert_eq!(s.health, 0.0);
        // Nothing further happens in that update: no change is applied.
        assert_eq!(s.power, 0.5);
        assert_eq!(s.psy_health, 0.0);
    }

    #[test]
    fn thresholds_have_hysteresis() {
        let c = config();
        let mut s = Conditions::new_game(1.0);
        let at = |s: &mut Conditions, power: f32, health: f32| {
            s.power = power;
            s.health = health;
            s.evaluate_thresholds(&c);
            (s.limping, s.cant_walk, s.cant_sprint)
        };
        assert_eq!(at(&mut s, 1.0, 1.0), (false, false, false));
        assert_eq!(at(&mut s, 0.25, 1.0), (false, false, false));
        assert_eq!(at(&mut s, 0.19, 1.0), (false, false, true));
        // Between the thresholds a state keeps its value.
        assert_eq!(at(&mut s, 0.25, 1.0), (false, false, true));
        // Exactly the end does not end it (strict).
        assert_eq!(at(&mut s, 0.3, 1.0), (false, false, true));
        assert_eq!(at(&mut s, 0.31, 1.0), (false, false, false));
        assert_eq!(at(&mut s, 0.05, 1.0), (true, false, true));
        assert_eq!(at(&mut s, 0.005, 1.0), (true, true, true));
        assert_eq!(at(&mut s, 0.1, 1.0), (true, true, true));
        assert_eq!(at(&mut s, 0.15, 1.0), (true, false, true));
        assert_eq!(at(&mut s, 0.2, 1.0), (true, false, true));
        assert_eq!(at(&mut s, 0.21, 1.0), (false, false, true));
        // Health alone starts limping; both must be above 0.2 to end it.
        assert!(at(&mut s, 1.0, 0.05).0);
        assert!(at(&mut s, 1.0, 0.2).0);
        assert!(!at(&mut s, 1.0, 0.21).0);
    }

    #[test]
    fn sprint_endurance_and_recovery() {
        // 4.3: from full power, sprinting falls below 0.2 after 786.2 game
        // seconds; standing then lifts it above 0.3 in 19.8.
        let c = config();
        let mut s = Conditions::new_game(1.0);
        let mut t = 0.0;
        while !s.cant_sprint {
            s.update(&c, 0.1, SPRINT, &load());
            s.evaluate_thresholds(&c);
            t += 0.1;
        }
        near(t, 786.2, 0.2);
        let mut t = 0.0;
        while s.cant_sprint {
            s.update(&c, 0.1, STAND, &load());
            s.evaluate_thresholds(&c);
            t += 0.1;
        }
        near(t, 19.8, 0.2);
    }

    #[test]
    fn a_jump_costs_power() {
        let c = config();
        let mut s = Conditions::new_game(1.0);
        s.jump(&c, &load());
        near(1.0 - s.power, 0.0143, 1e-6);
        // Overweight: w_j = 1.2, O = 5.
        let mut s = Conditions::new_game(1.0);
        s.jump(
            &c,
            &Load {
                weight: 60.0,
                ..load()
            },
        );
        near(1.0 - s.power, 0.01 + 0.05 * 5.0 * 1.2, 1e-6);
        // Clamped at the next update.
        s.power = -0.5;
        s.update(&c, 0.0, STAND, &load());
        assert_eq!(s.power, 0.0);
    }

    fn eatable(text: &str, class_id: &str) -> Eatable {
        let config = volya_engine_formats::ltx::parse(format!("[e]\n{text}").as_bytes()).unwrap();
        Eatable::read(config.section("e").unwrap(), class_id)
            .unwrap()
            .unwrap()
    }

    fn medkit() -> Eatable {
        eatable(
            "eat_health = 0.6\neat_power = 0.0\neat_satiety = 0\neat_radiation = -0.1\n\
             wounds_heal_perc = 0.3\neat_portions_num = 1\neat_max_power = 0.1\n",
            "II_MEDKI",
        )
    }

    #[test]
    fn reads_eatables() {
        let m = medkit();
        assert_eq!(m.eat_alcohol, None);
        assert_eq!(m.eat_portions_num, 1);
        let vodka = eatable(
            "eat_health = 0.01\neat_power = 0\neat_satiety = 0.01\neat_radiation = -0.3\n\
             wounds_heal_perc = 0\neat_portions_num = -1\neat_alcohol = 0.1\n",
            "II_BOTTL",
        );
        assert_eq!((vodka.eat_alcohol, vodka.eat_max_power), (Some(0.1), 0.0));
        let config = volya_engine_formats::ltx::parse(b"[e]\ninv_weight = 1\n").unwrap();
        assert_eq!(
            Eatable::read(config.section("e").unwrap(), "II_FOOD").unwrap(),
            None
        );
    }

    #[test]
    fn medkit_at_new_game_values() {
        // Section 7: one update with dt = 0 after it.
        let c = config();
        let mut s = Conditions::new_game(1.0);
        s.eat(&c, &medkit());
        s.update(&c, 0.0, STAND, &load());
        assert_eq!(
            (s.health, s.radiation, s.max_power, s.satiety),
            (1.0, 0.0, 1.0, 1.0)
        );
        // From lower values: the changes wait for the update.
        let mut s = Conditions::new_game(0.2);
        s.radiation = 0.5;
        s.max_power = 0.5;
        s.eat(&c, &medkit());
        assert_eq!((s.health, s.radiation), (0.2, 0.5));
        near(s.max_power, 0.6, 1e-6);
        s.update(&c, 0.0, STAND, &load());
        near(s.health, 0.8, 1e-6);
        near(s.radiation, 0.4, 1e-6);
    }

    #[test]
    fn vodka_at_new_game_values() {
        // Section 7: alcohol 0.1 at once, 0.1 − 0.0003 × dt after an update.
        let c = config();
        let mut s = Conditions::new_game(1.0);
        s.eat(
            &c,
            &eatable(
                "eat_health = 0.01\neat_power = 0\neat_satiety = 0.01\neat_radiation = -0.3\n\
             wounds_heal_perc = 0\neat_portions_num = -1\neat_alcohol = 0.1\n",
                "II_BOTTL",
            ),
        );
        near(s.alcohol, 0.1, 1e-7);
        s.update(&c, 50.0, STAND, &load());
        near(s.alcohol, 0.1 - 0.0003 * 50.0, 1e-6);
        // A drink's alcohol is clamped only at the next update.
        s.alcohol = 0.95;
        s.eat(
            &c,
            &Eatable {
                eat_alcohol: Some(0.1),
                ..Eatable::default()
            },
        );
        near(s.alcohol, 1.05, 1e-6);
        s.update(&c, 0.0, STAND, &load());
        assert_eq!(s.alcohol, 1.0);
    }

    #[test]
    fn invariants_hold_over_long_runs() {
        // Section 7: after each update every value keeps to its range;
        // standing never lowers power, walking and running at New Game
        // weight do not either.
        let c = config();
        let mut s = Conditions::new_game(1.0);
        s.radiation = 0.7;
        for (i, activity) in [STAND, WALK, RUN, SPRINT]
            .into_iter()
            .cycle()
            .take(400)
            .enumerate()
        {
            let before = s.power;
            s.update(&c, 0.5 + i as f32, activity, &load());
            assert!((MIN_HEALTH..=1.0).contains(&s.health));
            assert!((0.0..=s.max_power).contains(&s.power));
            assert!((0.1..=1.0).contains(&s.max_power));
            for v in [s.satiety, s.radiation, s.psy_health, s.alcohol, s.morale] {
                assert!((0.0..=1.0).contains(&v), "{v}");
            }
            if !activity.sprints && s.satiety > 0.0 {
                assert!(s.power >= before, "{activity:?}");
            }
        }
    }
}
