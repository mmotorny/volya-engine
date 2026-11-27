// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The collision world's spatial index (our choice, #396): a binary
//! bounding-volume tree whose leaves are runs of consecutive triangles of
//! the file's order (which keeps neighbours together: the runs need no
//! index list of their own), the runs ordered by the Morton code of their
//! boxes' centres and split at its highest differing bit. Boxes are 16-bit
//! steps of the level's extent, rounded outwards, so a node is 16 bytes.
//!
//! The tree is built in its own array without temporaries: the leaves are
//! staged in its last half, sorted there, and the nodes written in depth
//! first order ahead of the leaves still to be read.

use std::io;

use super::geometry::Vec3;

/// Most triangles of a leaf (our choice, measured, #396).
const LEAF_TRIANGLES: usize = 16;
/// A run of triangles ends before one would make its box's longest side
/// longer than this (metres) and its surface more than half again the sum
/// of the run's and the triangle's (our choice, measured).
const LEAF_SIDE: f32 = 1.0;
/// Most triangles the tree indexes: a leaf's start takes 27 bits.
pub const MAX_TRIANGLES: usize = 1 << 27;
/// Steps of the extent a box coordinate takes, leaving room for the
/// outward rounding below 65,535.
const STEPS: f32 = 65_532.0;

const LEAF: u32 = 1 << 31;
const COUNT_SHIFT: u32 = 27;
const START_MASK: u32 = (1 << COUNT_SHIFT) - 1;
/// Deeper than any tree: 48 Morton bits, then halving at most 2²⁷ leaves.
const STACK: usize = 80;

/// A node: its box in steps of the extent, and either the index of its
/// second child (the first follows it) or, with [`LEAF`], its run of
/// triangles (start, count − 1 above it).
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct Node {
    lo: [u16; 3],
    hi: [u16; 3],
    link: u32,
}

impl Node {
    fn run(&self) -> Option<(u32, u32)> {
        (self.link & LEAF != 0).then(|| {
            (
                self.link & START_MASK,
                ((self.link & !LEAF) >> COUNT_SHIFT) + 1,
            )
        })
    }

    fn centre_code(&self) -> u64 {
        let spread = |v: u16| {
            let mut x = u64::from(v);
            x = (x | (x << 32)) & 0x001F_0000_0000_FFFF;
            x = (x | (x << 16)) & 0x001F_0000_FF00_00FF;
            x = (x | (x << 8)) & 0x100F_00F0_0F00_F00F;
            x = (x | (x << 4)) & 0x10C3_0C30_C30C_30C3;
            (x | (x << 2)) & 0x1249_2492_4924_9249
        };
        let c = |k: usize| ((u32::from(self.lo[k]) + u32::from(self.hi[k])) / 2) as u16;
        spread(c(0)) | (spread(c(1)) << 1) | (spread(c(2)) << 2)
    }
}

/// The box of the corners `c`.
pub fn triangle_box(c: &[Vec3; 3]) -> (Vec3, Vec3) {
    let lo = std::array::from_fn(|k| c[0][k].min(c[1][k]).min(c[2][k]));
    let hi = std::array::from_fn(|k| c[0][k].max(c[1][k]).max(c[2][k]));
    (lo, hi)
}

fn union(a: (Vec3, Vec3), b: (Vec3, Vec3)) -> (Vec3, Vec3) {
    (
        std::array::from_fn(|k| a.0[k].min(b.0[k])),
        std::array::from_fn(|k| a.1[k].max(b.1[k])),
    )
}

fn longest(b: (Vec3, Vec3)) -> f32 {
    (0..3).map(|k| b.1[k] - b.0[k]).fold(0.0, f32::max)
}

/// Half the surface area of box `b`.
fn area(b: (Vec3, Vec3)) -> f32 {
    let e: [f32; 3] = std::array::from_fn(|k| b.1[k] - b.0[k]);
    e[0] * e[1] + e[1] * e[2] + e[2] * e[0]
}

/// The segment from `origin` along `dir` for parameters 0 … `length`.
#[derive(Clone, Copy, Debug)]
pub struct Segment {
    origin: Vec3,
    dir: Vec3,
    inv: Vec3,
    length: f32,
}

impl Segment {
    pub fn new(origin: Vec3, dir: Vec3, length: f32) -> Self {
        Self {
            origin,
            dir,
            inv: dir.map(|d| 1.0 / d),
            length,
        }
    }

    /// Whether the segment meets the box `lo`…`hi` widened by `margin` on
    /// every side (false for NaN).
    pub fn meets(&self, lo: Vec3, hi: Vec3, margin: f32) -> bool {
        self.line_meets(lo, hi, margin, 0.0, self.length)
    }

    /// Whether the line through the segment meets the box `lo`…`hi`
    /// widened by `margin` between the parameters `t0` and `t1`.
    fn line_meets(&self, lo: Vec3, hi: Vec3, margin: f32, t0: f32, t1: f32) -> bool {
        let (mut t0, mut t1) = (t0, t1);
        for k in 0..3 {
            let (a, b) = (lo[k] - margin, hi[k] + margin);
            let o = self.origin[k];
            if self.dir[k] == 0.0 {
                if !(o >= a && o <= b) {
                    return false;
                }
            } else {
                let (ta, tb) = ((a - o) * self.inv[k], (b - o) * self.inv[k]);
                t0 = t0.max(ta.min(tb));
                t1 = t1.min(ta.max(tb));
            }
        }
        t0 <= t1
    }
}

/// The tree over a level's triangles.
#[derive(Clone, Debug, Default)]
pub struct Bvh {
    origin: Vec3,
    unit: Vec3,
    nodes: Vec<Node>,
}

impl Bvh {
    /// The tree over `count` triangles (at most [`MAX_TRIANGLES`]), triangle
    /// `i` with the corners `corners(i)`, all within `min`…`max`.
    pub fn build(
        min: Vec3,
        max: Vec3,
        count: usize,
        corners: impl Fn(usize) -> [Vec3; 3],
    ) -> io::Result<Self> {
        if count > MAX_TRIANGLES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("level.cform: more than {MAX_TRIANGLES} triangles"),
            ));
        }
        let mut tree = Self {
            origin: min,
            unit: std::array::from_fn(|k| (max[k] - min[k]).max(1e-3) / STEPS),
            nodes: Vec::new(),
        };
        let leaves = runs((0..count).map(&corners), |_, _, _| {});
        let mut nodes = Vec::with_capacity((2 * leaves).saturating_sub(1));
        if leaves == 0 {
            tree.nodes = nodes;
            return Ok(tree);
        }
        let first = leaves - 1;
        nodes.resize(2 * leaves - 1, Node::default());
        let mut at = first;
        runs((0..count).map(&corners), |start, count, b| {
            let (lo, hi) = tree.quantize(b);
            nodes[at] = Node {
                lo,
                hi,
                link: LEAF | ((count as u32 - 1) << COUNT_SHIFT) | start as u32,
            };
            at += 1;
        });
        nodes[first..].sort_unstable_by_key(Node::centre_code);
        let mut at = 0;
        place(&mut nodes, first, leaves, &mut at);
        debug_assert_eq!(at, nodes.len());
        tree.nodes = nodes;
        Ok(tree)
    }

    pub fn memory_bytes(&self) -> usize {
        self.nodes.len() * std::mem::size_of::<Node>()
    }

    fn quantize(&self, b: (Vec3, Vec3)) -> ([u16; 3], [u16; 3]) {
        let step = |v: f32, k: usize| (v - self.origin[k]) / self.unit[k];
        (
            std::array::from_fn(|k| (step(b.0[k], k).floor() - 1.0).clamp(0.0, 65_535.0) as u16),
            std::array::from_fn(|k| (step(b.1[k], k).ceil() + 1.0).clamp(0.0, 65_535.0) as u16),
        )
    }

    fn bounds(&self, n: &Node) -> (Vec3, Vec3) {
        (
            std::array::from_fn(|k| self.origin[k] + f32::from(n.lo[k]) * self.unit[k]),
            std::array::from_fn(|k| self.origin[k] + f32::from(n.hi[k]) * self.unit[k]),
        )
    }

    /// Calls `leaf` with the run (start, count) of every leaf whose box,
    /// which holds its triangles' boxes, `test` accepts, and whose
    /// ancestors' boxes it accepts.
    fn visit(&self, mut test: impl FnMut(Vec3, Vec3) -> bool, mut leaf: impl FnMut(u32, u32)) {
        if self.nodes.is_empty() {
            return;
        }
        let mut stack = [0u32; STACK];
        let mut depth = 1;
        while depth > 0 {
            depth -= 1;
            let mut i = stack[depth] as usize;
            loop {
                let n = &self.nodes[i];
                let (lo, hi) = self.bounds(n);
                if !test(lo, hi) {
                    break;
                }
                if let Some((start, count)) = n.run() {
                    leaf(start, count);
                    break;
                }
                stack[depth] = n.link;
                depth += 1;
                i += 1;
            }
        }
    }

    /// The runs of the leaves whose boxes, widened by `margin`, `segment`
    /// meets.
    pub fn along(&self, segment: &Segment, margin: f32, leaf: impl FnMut(u32, u32)) {
        self.visit(|lo, hi| segment.meets(lo, hi, margin), leaf);
    }

    /// The runs of the leaves whose boxes overlap the box `min`…`max`.
    pub fn overlapping(&self, min: Vec3, max: Vec3, leaf: impl FnMut(u32, u32)) {
        self.visit(
            |lo, hi| (0..3).all(|k| lo[k] <= max[k] && hi[k] >= min[k]),
            leaf,
        );
    }
}

/// Splits the triangles (their corners, in order) into the leaves' runs,
/// calling `emit` with each one's start, count and box; returns how many.
fn runs(
    corners: impl Iterator<Item = [Vec3; 3]>,
    mut emit: impl FnMut(usize, usize, (Vec3, Vec3)),
) -> usize {
    let (mut start, mut count, mut leaves) = (0, 0, 0);
    let mut run = ([0.0; 3], [0.0; 3]);
    for (i, c) in corners.enumerate() {
        let b = triangle_box(&c);
        if count > 0 {
            let merged = union(run, b);
            if count < LEAF_TRIANGLES
                && (longest(merged) <= LEAF_SIDE || area(merged) <= 1.5 * (area(run) + area(b)))
            {
                run = merged;
                count += 1;
                continue;
            }
            emit(start, count, run);
            leaves += 1;
        }
        (start, count, run) = (i, 1, b);
    }
    if count > 0 {
        emit(start, count, run);
        leaves += 1;
    }
    leaves
}

/// Writes the subtree of the `count` sorted leaves staged from `first` at
/// `at` onwards, in depth-first order, returning its box. A leaf's place
/// is never past its staged one, and a node is written only once every
/// leaf staged where it goes is placed.
fn place(nodes: &mut [Node], first: usize, count: usize, at: &mut usize) -> ([u16; 3], [u16; 3]) {
    if count == 1 {
        let leaf = nodes[first];
        nodes[*at] = leaf;
        *at += 1;
        return (leaf.lo, leaf.hi);
    }
    let (a, b) = (
        nodes[first].centre_code(),
        nodes[first + count - 1].centre_code(),
    );
    let split = if a == b {
        count / 2
    } else {
        let bit = 1u64 << (63 - (a ^ b).leading_zeros());
        nodes[first..first + count].partition_point(|n| n.centre_code() & bit == 0)
    };
    let me = *at;
    *at += 1;
    let l = place(nodes, first, split, at);
    let right = *at as u32;
    let r = place(nodes, first + split, count - split, at);
    let lo = std::array::from_fn(|k| l.0[k].min(r.0[k]));
    let hi = std::array::from_fn(|k| l.1[k].max(r.1[k]));
    nodes[me] = Node {
        lo,
        hi,
        link: right,
    };
    (lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(c: [u16; 3]) -> Node {
        Node {
            lo: c,
            hi: c,
            link: 0,
        }
    }

    #[test]
    fn centre_codes_interleave_x_y_z() {
        assert_eq!(at([1, 0, 0]).centre_code(), 1);
        assert_eq!(at([0, 1, 0]).centre_code(), 2);
        assert_eq!(at([0, 0, 1]).centre_code(), 4);
        assert_eq!(at([2, 0, 0]).centre_code(), 8);
        assert_eq!(at([0x8000, 0, 0]).centre_code(), 1 << 45);
        assert_eq!(at([u16::MAX; 3]).centre_code(), (1 << 48) - 1);
    }

    #[test]
    fn every_triangle_is_in_one_leaf_inside_every_box_above_it() {
        // A grid of small triangles in a scattered file order.
        let corners = |i: usize| {
            let j = (i * 7919) % 1000;
            let o = [(j % 10) as f32, (j / 100) as f32, ((j / 10) % 10) as f32];
            [o, [o[0] + 0.5, o[1], o[2]], [o[0], o[1] + 0.5, o[2] + 0.5]]
        };
        let tree = Bvh::build([0.0; 3], [10.0; 3], 1000, corners).unwrap();
        let mut seen = vec![0; 1000];
        let contains = |n: &Node, b: (Vec3, Vec3)| {
            let (lo, hi) = tree.bounds(n);
            (0..3).all(|k| lo[k] <= b.0[k] && hi[k] >= b.1[k])
        };
        // Each node's box holds its subtree's triangles' boxes.
        fn walk(
            tree: &Bvh,
            i: usize,
            corners: &dyn Fn(usize) -> [Vec3; 3],
            contains: &dyn Fn(&Node, (Vec3, Vec3)) -> bool,
        ) -> Vec<usize> {
            let n = &tree.nodes[i];
            let below = match n.run() {
                Some((start, count)) => (start as usize..(start + count) as usize).collect(),
                None => {
                    let mut v = walk(tree, i + 1, corners, contains);
                    v.extend(walk(tree, n.link as usize, corners, contains));
                    v
                }
            };
            for &t in &below {
                assert!(
                    contains(n, triangle_box(&corners(t))),
                    "node {i}, triangle {t}"
                );
            }
            below
        }
        for t in walk(&tree, 0, &corners, &contains) {
            seen[t] += 1;
        }
        assert!(seen.iter().all(|&s| s == 1));
        // A short segment meets few leaves.
        let mut leaves = 0;
        let s = Segment::new([5.1, 5.1, 5.1], [1.0, 0.0, 0.0], 0.2);
        tree.along(&s, 0.01, |_, _| leaves += 1);
        assert!(leaves <= 4, "{leaves}");
    }
}
