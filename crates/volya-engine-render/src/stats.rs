// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! What a frame asks of the GPU: its draw calls and state changes, by the
//! part of the frame that made them (#387). Counted on the thread that
//! encodes (the engine is single-threaded) and taken with
//! [`crate::Renderer::take_frame_stats`].

use std::cell::Cell;

/// The parts of a frame commands are counted under.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    /// The static world and objects of priorities 0–1 (the first bundle).
    WorldEarly,
    /// Priorities 2–3 (the second bundle).
    WorldLate,
    /// The strict-sorted static draws.
    Strict,
    /// The dynamic light passes.
    Lights,
    /// The detail objects.
    Details,
    /// The billboard groups' billboards.
    Billboards,
    /// The sky box, sun and clouds.
    Sky,
    /// Particle effects and glows, the distortion pass included.
    Particles,
    /// Everything else: menus and HUD, presentation, gamma, video.
    Screen,
}

impl Part {
    pub const ALL: [Self; 9] = [
        Self::WorldEarly,
        Self::WorldLate,
        Self::Strict,
        Self::Lights,
        Self::Details,
        Self::Billboards,
        Self::Sky,
        Self::Particles,
        Self::Screen,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::WorldEarly => "world_early",
            Self::WorldLate => "world_late",
            Self::Strict => "strict",
            Self::Lights => "lights",
            Self::Details => "details",
            Self::Billboards => "billboards",
            Self::Sky => "sky",
            Self::Particles => "particles",
            Self::Screen => "screen",
        }
    }
}

/// Commands of one part.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub draws: u32,
    pub pipelines: u32,
    pub bind_groups: u32,
    /// Vertex and index buffers set.
    pub buffers: u32,
}

impl Counts {
    fn add(&mut self, o: Self) {
        self.draws += o.draws;
        self.pipelines += o.pipelines;
        self.bind_groups += o.bind_groups;
        self.buffers += o.buffers;
    }
}

/// A frame's commands by part, plus render passes and submissions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FrameStats {
    pub parts: [Counts; 9],
    pub passes: u32,
    pub submits: u32,
}

impl FrameStats {
    pub fn part(&self, part: Part) -> Counts {
        self.parts[part as usize]
    }

    /// Every part together.
    pub fn total(&self) -> Counts {
        let mut t = Counts::default();
        for c in self.parts {
            t.add(c);
        }
        t
    }
}

thread_local! {
    static PART: Cell<Part> = const { Cell::new(Part::Screen) };
    static FRAME: Cell<FrameStats> = Cell::new(FrameStats::default());
}

fn with(f: impl FnOnce(&mut FrameStats, usize)) {
    let part = PART.get() as usize;
    FRAME.with(|s| {
        let mut v = s.get();
        f(&mut v, part);
        s.set(v);
    });
}

/// Counts what follows under `part`.
pub(crate) fn set_part(part: Part) {
    PART.set(part);
}

pub(crate) fn draw() {
    with(|s, p| s.parts[p].draws += 1);
}

pub(crate) fn pipeline() {
    with(|s, p| s.parts[p].pipelines += 1);
}

pub(crate) fn bind_group() {
    with(|s, p| s.parts[p].bind_groups += 1);
}

pub(crate) fn buffer() {
    with(|s, p| s.parts[p].buffers += 1);
}

pub(crate) fn pass() {
    with(|s, _| s.passes += 1);
}

pub(crate) fn submit() {
    with(|s, _| s.submits += 1);
}

/// The counts since the last call.
pub(crate) fn take() -> FrameStats {
    PART.set(Part::Screen);
    FRAME.take()
}

/// How a static draw call was made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StaticCall {
    /// Covering one or more draws whose vertices are their own.
    Merged,
    /// One level draw whose vertices others share (as the instance).
    Shared,
    /// One object draw.
    Object,
}

/// The static world's calls of a frame by program and kind, and the draws
/// the merged calls covered.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StaticCalls {
    /// (program, kind, calls, draws covered).
    pub rows: Vec<(&'static str, StaticCall, u32, u32)>,
}

thread_local! {
    static STATIC: std::cell::RefCell<StaticCalls> = Default::default();
}

pub(crate) fn static_call(program: &'static str, kind: StaticCall, draws: u32) {
    STATIC.with_borrow_mut(
        |s| match s.rows.iter_mut().find(|r| r.0 == program && r.1 == kind) {
            Some(r) => {
                r.2 += 1;
                r.3 += draws;
            }
            None => s.rows.push((program, kind, 1, draws)),
        },
    );
}

/// The static calls since the last call.
pub(crate) fn take_static() -> StaticCalls {
    STATIC.take()
}
