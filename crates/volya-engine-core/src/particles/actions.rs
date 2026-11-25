// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! What each action does to the live particles in one step: `source`
//! (particle-lifecycle.md section 6), the motion actions
//! (particle-motion-actions.md) and the lifetime and appearance actions
//! (particle-appearance-actions.md).

use volya_engine_formats::particles::{ActionKind, Region, Source, action_flags, region_kind};

use super::effect::Notice;
use super::region::{generate, within};
use super::{FLT_MIN, Particle, Random, Vec3, add, cross, dot, scale, sub};

/// What a step lends an action.
pub(crate) struct Step<'a> {
    pub particles: &'a mut Vec<Particle>,
    /// The effect's maximum *N*.
    pub max: usize,
    /// The effect's flag word and frame count (chunk 6), for the frame
    /// values at birth (particle-drawing.md 3.2).
    pub effect_flags: u32,
    pub frame_count: i32,
    pub random: &'a mut Random,
    /// The owner's notices (P2 3.2); `None` for an effect alone.
    pub notices: Option<&'a mut Vec<Notice>>,
    /// Δ, seconds.
    pub dt: f32,
}

impl Step<'_> {
    /// Removes particle `i` (P2 3.2): reported first, then the last one
    /// moves into place.
    fn remove(&mut self, i: usize) {
        if let Some(n) = self.notices.as_deref_mut() {
            let p = &self.particles[i];
            n.push(Notice::Death {
                index: i,
                position: p.position,
                previous: p.previous,
            });
        }
        self.particles.swap_remove(i);
    }

    /// Removes every particle meeting `test`, visited from the last (P4
    /// section 2).
    fn remove_where(&mut self, mut test: impl FnMut(&Particle, &mut Random) -> bool) {
        for i in (0..self.particles.len()).rev() {
            if test(&self.particles[i], self.random) {
                self.remove(i);
            }
        }
    }
}

/// *R*² at or above this affects every particle (P3 1.2).
const UNBOUNDED: f32 = 1.0e16;

/// The particle at squared distance `r2` is affected (P3 1.2); `vortex`
/// includes the radius itself.
fn affected(r2: f32, max_radius: f32, inclusive: bool) -> bool {
    let big = max_radius * max_radius;
    big >= UNBOUNDED || r2 < big || (inclusive && r2 == big)
}

/// Runs `kind` with its action flag word `flags` and its run-time value
/// `time` (`turbulence`'s τ).
pub(crate) fn run(kind: &ActionKind, flags: u32, time: &mut f32, s: &mut Step) {
    let dt = s.dt;
    match kind {
        ActionKind::Source(source) => {
            if flags & action_flags::SILENT == 0 {
                birth(source, flags, s);
            }
        }
        ActionKind::Move => {
            for p in s.particles.iter_mut() {
                p.previous = p.position;
                p.position = add(p.position, scale(p.velocity, dt));
                p.age += dt;
            }
        }
        ActionKind::Gravity { acceleration } => {
            let g = scale(*acceleration, dt);
            for p in s.particles.iter_mut() {
                p.velocity = add(p.velocity, g);
            }
        }
        ActionKind::TargetVelocity { velocity, scale: k } => {
            let k = k * dt;
            for p in s.particles.iter_mut() {
                p.velocity = add(p.velocity, scale(sub(*velocity, p.velocity), k));
            }
        }
        ActionKind::Damping {
            factor,
            lower,
            upper,
        } => {
            let m = factor.map(|f| 1.0 - (1.0 - f) * dt);
            for p in s.particles.iter_mut() {
                let v2 = dot(p.velocity, p.velocity);
                if *lower <= v2 && v2 <= *upper {
                    p.velocity = [0, 1, 2].map(|i| p.velocity[i] * m[i]);
                }
            }
        }
        ActionKind::SpeedLimit { min, max } => {
            for p in s.particles.iter_mut() {
                let v2 = dot(p.velocity, p.velocity);
                if v2 < min * min && v2 != 0.0 {
                    p.velocity = scale(p.velocity, min / v2.sqrt());
                } else if v2 > max * max {
                    p.velocity = scale(p.velocity, max / v2.sqrt());
                }
            }
        }
        ActionKind::OrbitPoint {
            centre,
            magnitude,
            epsilon,
            max_radius,
        } => {
            let md = magnitude * dt;
            for p in s.particles.iter_mut() {
                let d = sub(*centre, p.position);
                let r2 = dot(d, d);
                if affected(r2, *max_radius, false) {
                    let r = r2.sqrt();
                    p.velocity = add(p.velocity, scale(d, md / (epsilon + r2 + r)));
                }
            }
        }
        ActionKind::Scatter {
            centre,
            magnitude,
            epsilon,
            max_radius,
        } => {
            let md = magnitude * dt;
            for p in s.particles.iter_mut() {
                let d = sub(p.position, *centre);
                let r2 = dot(d, d);
                if affected(r2, *max_radius, false) {
                    let r = r2.sqrt();
                    let u = d.map(|c| c / r);
                    p.velocity = add(p.velocity, scale(u, md / (epsilon + r2)));
                }
            }
        }
        ActionKind::Jet {
            centre,
            acceleration,
            magnitude,
            epsilon,
            max_radius,
        } => {
            let md = magnitude * dt;
            for p in s.particles.iter_mut() {
                let d = sub(p.position, *centre);
                let r2 = dot(d, d);
                if affected(r2, *max_radius, false) {
                    let g = generate(acceleration, s.random);
                    p.velocity = add(p.velocity, scale(g, md / (r2 + epsilon)));
                }
            }
        }
        ActionKind::RandomAccel { region } => {
            for p in s.particles.iter_mut() {
                let g = generate(region, s.random);
                p.velocity = add(p.velocity, scale(g, dt));
            }
        }
        ActionKind::RandomDisplace { region } => {
            for p in s.particles.iter_mut() {
                let g = generate(region, s.random);
                p.position = add(p.position, scale(g, dt));
            }
        }
        ActionKind::RandomVelocity { region } => {
            for p in s.particles.iter_mut() {
                p.velocity = generate(region, s.random);
            }
        }
        ActionKind::Gravitate {
            magnitude,
            epsilon,
            max_radius,
        } => {
            let md = magnitude * dt;
            let n = s.particles.len();
            for i in 0..n {
                for j in i + 1..n {
                    let d = sub(s.particles[j].position, s.particles[i].position);
                    let r2 = dot(d, d) + 1.0e-7;
                    if affected(r2, *max_radius, false) {
                        let r = r2.sqrt();
                        let k = md / ((epsilon + r2) * r);
                        let dk = scale(d, k);
                        s.particles[i].velocity = add(s.particles[i].velocity, dk);
                        s.particles[j].velocity = sub(s.particles[j].velocity, dk);
                    }
                }
            }
        }
        ActionKind::Follow {
            magnitude,
            epsilon,
            max_radius,
        } => {
            // With no particle the game's visit runs past the list (P3
            // 3.13, believed a crash): nothing happens here, as with one.
            let md = magnitude * dt;
            for i in 0..s.particles.len().saturating_sub(1) {
                let d = sub(s.particles[i + 1].position, s.particles[i].position);
                let r2 = dot(d, d);
                if affected(r2, *max_radius, false) {
                    let r = r2.sqrt();
                    let k = md / ((epsilon + r2) * r);
                    s.particles[i].velocity = add(s.particles[i].velocity, scale(d, k));
                }
            }
        }
        ActionKind::Vortex {
            centre,
            axis,
            magnitude,
            epsilon,
            max_radius,
        } => {
            let md = magnitude * dt;
            let n = *axis;
            for p in s.particles.iter_mut() {
                let d = sub(p.position, *centre);
                let r2 = dot(d, d);
                if affected(r2, *max_radius, true) {
                    let r = r2.sqrt();
                    let u = d.map(|c| c / r);
                    let h = dot(n, u);
                    let q = sub(u, scale(n, h));
                    let theta = md / (epsilon + r2);
                    let turned = add(
                        add(scale(q, theta.cos()), scale(cross(n, q), theta.sin())),
                        scale(n, h),
                    );
                    p.position = add(*centre, scale(turned, r));
                }
            }
        }
        ActionKind::Turbulence {
            frequency,
            octaves,
            magnitude,
            offset,
            drift,
        } => {
            *time += dt;
            let tau = *time;
            let noise = s.random.noise();
            let (f0, o) = (*frequency, *offset);
            for p in s.particles.iter_mut() {
                let q = add(p.position, scale(*drift, tau));
                let f = noise.fractal(q, f0, *octaves);
                let fx = noise.fractal([q[0] + o, q[1], q[2]], f0, *octaves);
                let fy = noise.fractal([q[0], q[1] + o, q[2]], f0, *octaves);
                let fz = noise.fractal([q[0], q[1], q[2] + o], f0, *octaves);
                let base = f0 * f;
                let e = [fx, fy, fz].map(|v| magnitude * (f0 * v - base));
                let speed = dot(p.velocity, p.velocity).sqrt();
                let w = add(p.velocity, e);
                let l = dot(w, w).sqrt();
                p.velocity = scale(w, speed / l);
            }
        }
        ActionKind::Bounce {
            surface,
            tangential,
            resilience,
            cutoff,
        } => {
            for p in s.particles.iter_mut() {
                bounce(surface, *tangential, *resilience, *cutoff, p, dt);
            }
        }
        ActionKind::KillOld {
            age_limit,
            kill_younger,
        } => {
            let (l, k) = (*age_limit, *kill_younger);
            if k <= 1 {
                s.remove_where(|p, _| {
                    let younger = l > p.age;
                    if k == 1 { younger } else { !younger }
                });
            }
        }
        ActionKind::Sink {
            kill_inside,
            region,
        } => {
            let k = *kill_inside;
            if k <= 1 {
                s.remove_where(|p, random| {
                    let inside = within(region, p.position, random);
                    if k == 1 { inside } else { !inside }
                });
            }
        }
        ActionKind::TargetColor {
            colour,
            alpha,
            scale: k,
        } => {
            let k = k * dt;
            // Alpha, red, green, blue, from the top byte down.
            let targets = [*alpha, colour[0], colour[1], colour[2]];
            for p in s.particles.iter_mut() {
                let mut out = 0u32;
                for (c, t) in targets.iter().enumerate() {
                    let shift = 24 - 8 * c;
                    let b = (p.colour >> shift) & 0xFF;
                    out |= colour_step(b, *t, k) << shift;
                }
                p.colour = out;
            }
        }
        ActionKind::TargetSize { size, scale: k } => {
            let k = k.map(|v| v * dt);
            for p in s.particles.iter_mut() {
                p.size = [0, 1, 2].map(|i| p.size[i] + (size[i] - p.size[i]) * k[i]);
            }
        }
        ActionKind::TargetRotate { rotation, scale: k } => {
            let k = k * dt;
            for p in s.particles.iter_mut() {
                // −0 counts as not negative; NaN as negative.
                let f = if p.rotation[0] >= 0.0 { k } else { -k };
                p.rotation = [0, 1, 2]
                    .map(|i| p.rotation[i] + (rotation[i].abs() - p.rotation[i].abs()) * f);
            }
        }
        // Not specified (P1 4.4); no effect uses them.
        ActionKind::Unspecified { .. } => {}
    }
}

/// 1/255 as the `f32` 0x3B808081 (P4 5.2).
const BYTE_UNIT: f32 = f32::from_bits(0x3B80_8081);

/// One `targetcolor` channel (P4 5.2).
pub(crate) fn colour_step(b: u32, t: f32, k: f32) -> u32 {
    let x = b as f32 * BYTE_UNIT;
    let y = x + (t - x) * k;
    to_byte(y * 255.0)
}

/// The conversion of P4 5.3.
pub(crate) fn to_byte(z: f32) -> u32 {
    if z.abs() < 2_147_483_648.0 {
        return z.floor().clamp(0.0, 255.0) as u32;
    }
    let bits = z.to_bits();
    let e = (bits >> 23) & 0xFF;
    let m = u64::from(bits & 0x7F_FFFF);
    let h = (158 - e as i32).rem_euclid(32) as u32;
    if bits >> 31 == 0 {
        if h == 0 {
            0
        } else {
            (((1u64 << 31) + 256 * m) >> h).min(255) as u32
        }
    } else if h == 0 && m != 0 {
        255
    } else {
        0
    }
}

/// trunc(255 *c*) limited to 0 … 255 (P2 6, step 7).
fn channel(c: f32) -> u32 {
    ((255.0 * c).trunc().clamp(0.0, 255.0)) as u32
}

/// `source`'s births in one step (P2 section 6).
fn birth(source: &Source, flags: u32, s: &mut Step) {
    let z = source.rate * s.dt;
    let mut n = z.floor() as i64;
    if z - n as f32 > s.random.u() {
        n += 1;
    }
    let count = s.particles.len() as i64;
    if count + n > s.max as i64 {
        n = s.max as i64 - count;
    }
    for _ in 0..n.max(0) {
        let position = generate(&source.position, s.random);
        let mut size = generate(&source.size, s.random);
        if flags & action_flags::SINGLE_SIZE != 0 {
            size[1] = size[0];
            size[2] = size[0];
        }
        let rotation = generate(&source.rotation, s.random);
        let velocity = add(generate(&source.velocity, s.random), source.parent_velocity);
        let c = generate(&source.colour, s.random);
        let age = source.age + s.random.normal(source.age_spread);
        let colour =
            channel(source.alpha) << 24 | channel(c[0]) << 16 | channel(c[1]) << 8 | channel(c[2]);
        s.particles.push(Particle {
            position,
            previous: position,
            velocity,
            size,
            rotation,
            colour,
            age,
            frame: 0,
            frame_flags: 0,
        });
        // The effect's own birth notice: the frame values (P7 3.2).
        let random = &mut *s.random;
        let p = s.particles.last_mut().expect("just added");
        birth_frame(p, s.effect_flags, s.frame_count, random);
        if let Some(n) = s.notices.as_deref_mut() {
            n.push(Notice::Birth {
                index: s.particles.len() - 1,
                position,
            });
        }
    }
}

/// The frame values a new particle takes (particle-drawing.md 3.2): with
/// effect flag 0x1000 a random start frame of the `count`, then with 0x800
/// and 0x2000 backwards on an odd draw. A frame count of 0 or less (none
/// in the data) gives frame 0 (our choice: the spec divides by it).
pub(crate) fn birth_frame(p: &mut Particle, flags: u32, count: i32, random: &mut Random) {
    use volya_engine_formats::particles::effect_flags as f;
    if flags & f::RANDOM_FRAME != 0 {
        let k = random.uniform.next_k();
        p.frame = if count > 0 {
            ((k % count as u32) * 255) as u16
        } else {
            0
        };
    }
    if flags & f::ANIMATED != 0
        && flags & f::RANDOM_PLAYBACK != 0
        && random.uniform.next_k() & 1 == 1
    {
        p.frame_flags |= 1;
    }
}

/// 1/255 as the `f32` 0x3B808081 (particle-drawing.md 3.1, 3.3).
pub(crate) const FRAME_UNIT: f32 = f32::from_bits(0x3B80_8081);

/// The frame animation of a step (particle-drawing.md 3.3), `rate` frames
/// per second over `count` frames. TODO(#329): a frame number beyond 16
/// bits keeps its low 16 bits (the spec leaves it unstated; no shipped
/// effect reaches it).
pub(crate) fn animate_frames(particles: &mut [Particle], rate: f32, count: i32, dt: f32) {
    let n = count as f32;
    let advance = rate * dt;
    for p in particles {
        let sigma = if p.frame_flags & 1 != 0 { -1.0 } else { 1.0 };
        let mut g = f32::from(p.frame) * FRAME_UNIT + sigma * advance;
        if g > n {
            g -= n;
        }
        if g < 0.0 {
            g += n;
        }
        p.frame = (g * 255.0).floor() as i64 as u16;
    }
}

/// The rule by which a particle bounces off normal `n` (P3 section 5).
fn reflect(w: Vec3, n: Vec3, tangential: f32, resilience: f32, cutoff: f32) -> Vec3 {
    let wn = dot(w, n);
    let mut t = sub(w, scale(n, wn));
    if dot(t, t) > cutoff {
        t = scale(t, tangential);
    }
    sub(t, scale(n, wn * resilience))
}

/// `bounce` for one particle (P3 section 5).
fn bounce(
    region: &Region,
    tangential: f32,
    resilience: f32,
    cutoff: f32,
    p: &mut Particle,
    dt: f32,
) {
    let x = p.position;
    let w = p.velocity;
    let next = add(x, scale(w, dt));
    match region.kind {
        region_kind::PLANE | region_kind::TRIANGLE | region_kind::RECTANGLE | region_kind::DISC => {
            let n = region.p2;
            let e = if region.kind == region_kind::DISC {
                region.r[2]
            } else {
                region.r[0]
            };
            let d0 = dot(n, x) + e;
            let d1 = dot(n, next) + e;
            if d0 * d1 >= 0.0 || (d0 * d1).is_nan() {
                return;
            }
            let hits = if region.kind == region_kind::PLANE {
                true
            } else {
                let wn = dot(w, n);
                let tau = -d0 / wn;
                let h = sub(add(x, scale(w, tau)), region.p1);
                if region.kind == region_kind::DISC {
                    let h2 = dot(h, h);
                    region.r[1] * region.r[1] <= h2 && h2 <= region.r[0] * region.r[0]
                } else {
                    let (u, v) = (region.u, region.v);
                    let c = cross(u, v);
                    let c2 = dot(c, c);
                    let alpha = dot(h, cross(v, c)) / c2;
                    let gamma = dot(h, cross(c, u)) / c2;
                    if region.kind == region_kind::TRIANGLE {
                        alpha >= 0.0 && gamma >= 0.0 && alpha + gamma <= 1.0
                    } else {
                        (0.0..=1.0).contains(&alpha) && (0.0..=1.0).contains(&gamma)
                    }
                }
            };
            if hits {
                p.velocity = reflect(w, n, tangential, resilience, cutoff);
            }
        }
        region_kind::SPHERE => {
            let inside = |q: Vec3| {
                let d = sub(q, region.p1);
                let l2 = dot(d, d);
                region.r[3] <= l2 && l2 <= region.r[2]
            };
            if !inside(next) {
                return;
            }
            let mut m = sub(x, region.p1);
            let l2 = dot(m, m);
            if l2 > FLT_MIN {
                let l = l2.sqrt();
                m = m.map(|c| c / l);
            }
            if !inside(x) {
                p.velocity = reflect(w, m, tangential, resilience, cutoff);
            } else {
                let wm = dot(w, m);
                if wm < 0.0 {
                    p.velocity = sub(sub(w, scale(m, wm)), scale(m, wm));
                }
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests;
