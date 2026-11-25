// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The player's footsteps (docs/behavior/player-footsteps.md, S11, cited by
//! section): the step table (section 1), the leg cycle chosen from the
//! movement state (2.1), and the timing of the steps on the step clock (3).
//! Which sound a step plays and how is the caller's (section 4): this says
//! which leg steps, at what volume.

use std::collections::HashMap;
use std::io;

use crate::config::{Config, value};
use crate::cycles::{Cycle, Cycles};
use crate::movement::Flags;

/// Room is kept for at most this many legs (section 1).
pub const MAX_LEGS: usize = 4;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// A step table line (section 1): the pass count *C* and each leg's step
/// time *t*ₖ and step volume *p*ₖ.
#[derive(Clone, Debug, PartialEq)]
pub struct StepLine {
    pub passes: u32,
    pub legs: Vec<(f32, f32)>,
}

/// The player's configuration of section 1: its own material's name, the
/// number of legs and the step table by cycle name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StepTable {
    pub legs: usize,
    lines: HashMap<String, StepLine>,
}

/// The actor's `material` (section 1): a missing line is fatal.
pub fn own_material(system: &Config, actor: &str) -> io::Result<String> {
    system
        .get(actor, "material")
        .flatten()
        .map(str::to_owned)
        .ok_or_else(|| invalid(format!("[{actor}] has no material")))
}

impl StepTable {
    /// `LegsCount` and the section `step_params` names, from the actor's
    /// section `actor` of the system configuration, each line's key looked
    /// up among `cycles` (lines naming no cycle are skipped). Without the
    /// section there are no steps. More than [`MAX_LEGS`] legs is not
    /// supported, and a pass count of 0 is fatal (section 1).
    pub fn read(system: &Config, actor: &str, cycles: &Cycles) -> io::Result<Self> {
        let get = |key: &str| system.get(actor, key).flatten();
        // TODO(#325): `[actor]` without `step_params` or `LegsCount` is not
        // stated (S11's review gap): without `step_params` no step sounds,
        // as without its section; without `LegsCount` there are no legs.
        let legs = get("LegsCount").map_or(0, |v| value::int(v) as u8 as usize);
        if legs > MAX_LEGS {
            return Err(invalid(format!(
                "[{actor}] LegsCount = {legs}: at most {MAX_LEGS} legs"
            )));
        }
        let mut table = Self {
            legs,
            lines: HashMap::new(),
        };
        let Some(section) = get("step_params").and_then(|name| system.section(name)) else {
            return Ok(table);
        };
        for entry in section.entries() {
            if cycles.get(entry.key()).is_none() {
                continue;
            }
            let items: Vec<&str> = entry
                .value()
                .map(|v| value::list(v).map(str::trim).collect())
                .unwrap_or_default();
            // TODO(#325): a line with fewer than 2n + 1 items is not stated
            // (S11's review gap): a missing item reads as 0, as an empty
            // number does.
            let item = |i: usize| items.get(i).copied().unwrap_or("");
            let passes = value::int(item(0)) as u32;
            if passes == 0 {
                return Err(invalid(format!(
                    "[{}] {}: pass count 0",
                    section.name(),
                    entry.key()
                )));
            }
            let legs = (0..legs)
                .map(|k| (value::float(item(2 * k + 1)), value::float(item(2 * k + 2))))
                .collect();
            table
                .lines
                .insert(entry.key().to_owned(), StepLine { passes, legs });
        }
        Ok(table)
    }

    /// The line of cycle `name`.
    pub fn line(&self, name: &str) -> Option<&StepLine> {
        self.lines.get(name)
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// What the leg cycle's choice reads (section 2.1).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LegState {
    /// The movement state after the frame's movement update.
    pub flags: Flags,
    /// The state runs (player-movement.md 4.1).
    pub runs: bool,
    pub climbing: bool,
    /// Turning the body while standing.
    pub turning: bool,
}

/// The leg cycle's name for `s` (section 2.1).
pub fn leg_cycle(s: &LegState) -> String {
    let f = &s.flags;
    if f.sprint {
        if f.forward {
            return "norm_escape_00".into();
        }
        if f.left {
            return "norm_escape_ls_00".into();
        }
        if f.right {
            return "norm_escape_rs_00".into();
        }
    }
    let climbing = s.climbing && !f.crouch;
    let prefix = if f.crouch { "cr" } else { "norm" };
    // The jump, landing and turn names of the climbing set are `norm`'s.
    let x = if f.landing && !f.hard_landing {
        "jump_end"
    } else if f.hard_landing {
        "jump_end_1"
    } else if s.turning && !s.climbing {
        "turn"
    } else if f.falling {
        "jump_idle"
    } else if f.jump {
        "jump_begin"
    } else {
        let direction = if f.forward {
            Some("fwd_0")
        } else if f.back {
            Some("back_0")
        } else if f.left {
            Some("ls_0")
        } else if f.right {
            Some("rs_0")
        } else {
            None
        };
        return match (direction, climbing) {
            (Some(d), true) => format!("cl_run_{d}"),
            (Some(d), false) => {
                let m = if s.runs { "run" } else { "walk" };
                format!("{prefix}_{m}_{d}")
            }
            (None, true) => "cl_idle_1".into(),
            (None, false) if f.crouch => if s.runs { "cr_idle_0" } else { "cr_idle_1" }.into(),
            (None, false) => "norm_idle_0".into(),
        };
    };
    format!("{prefix}_{x}")
}

/// One leg's step (section 3.3): it steps with this volume *p*ₖ.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    pub leg: usize,
    pub volume: f32,
}

/// The steps of the current leg cycle (section 3.2).
#[derive(Clone, Debug, PartialEq)]
struct Steps {
    /// The start time *T* (ms of the step clock).
    start: u32,
    line: StepLine,
    /// The pass *c*, from 1.
    pass: u32,
    pending: [bool; MAX_LEGS],
    cycle: Cycle,
}

impl Steps {
    /// The pass duration *d* = f32(*D* / *C*).
    fn pass_duration(&self) -> f32 {
        self.cycle.duration / self.line.passes as f32
    }

    /// trunc(*L* / *s* · 1000) in double precision (3.3 item 4).
    fn cycle_ms(&self) -> u32 {
        (f64::from(self.cycle.length) / f64::from(self.cycle.speed) * 1000.0) as u32
    }

    fn all_pending(&mut self) {
        let n = self.line.legs.len();
        self.pending = std::array::from_fn(|k| k < n);
    }
}

/// The player's leg cycle and its steps.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Footsteps {
    /// The leg cycle playing.
    playing: Option<String>,
    steps: Option<Steps>,
}

impl Footsteps {
    /// The leg cycle playing.
    pub fn playing(&self) -> Option<&str> {
        self.playing.as_deref()
    }

    /// The leg cycle `name` is chosen at step clock `now` (section 2.2): it
    /// starts if it differs from the one playing, and its steps start with
    /// it (3.2) if it has a line in `table`.
    pub fn choose(&mut self, name: &str, now: u32, cycles: &Cycles, table: &StepTable) {
        if self.playing.as_deref() == Some(name) {
            return;
        }
        self.playing = Some(name.to_owned());
        // TODO(#325): a name that is no cycle (S10's review gap: none of the
        // names chosen is, in the shipped data) gives no steps.
        self.steps = cycles.get(name).and_then(|&cycle| {
            let line = table.line(name)?.clone();
            let mut s = Steps {
                start: now,
                line,
                pass: 1,
                pending: [false; MAX_LEGS],
                cycle,
            };
            s.all_pending();
            Some(s)
        });
    }

    /// The steps' update of a frame at step clock `now` (section 3.3);
    /// `paired`: the player's own material and the ground's have a pair.
    /// The legs that step are added to `out`.
    pub fn update(&mut self, now: u32, paired: bool, out: &mut Vec<Step>) {
        let Some(s) = &mut self.steps else { return };
        // Item 1.
        if !paired {
            return;
        }
        // Item 2.
        let d = f64::from(s.pass_duration());
        for (k, &(time, volume)) in s.line.legs.iter().enumerate() {
            let offset = ((f64::from(s.pass - 1) + f64::from(time)) * d * 1000.0) as u32;
            let due = s.start.wrapping_add(offset);
            if s.pending[k] && now >= due {
                s.pending[k] = false;
                out.push(Step { leg: k, volume });
            }
        }
        // Item 3.
        if s.pass < s.line.passes {
            let elapsed = f64::from(now.wrapping_sub(s.start));
            s.pass = (elapsed / (d * 1000.0)) as u32 + 1;
        }
        // Item 4.
        let cycle_ms = s.cycle_ms();
        if !s.cycle.stops_at_end && now > s.start.wrapping_add(cycle_ms) {
            s.start = s.start.wrapping_add(cycle_ms);
            s.pass = 1;
            s.all_pending();
        }
    }
}

#[cfg(test)]
mod tests;
