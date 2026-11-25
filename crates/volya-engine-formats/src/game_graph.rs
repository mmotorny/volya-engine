// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The game graph `game.graph` (docs/behavior/offline-simulation.md
//! section 1): the header, the level table, the vertex, edge and level
//! point tables and the trailing bytes. Not chunked. The level points are
//! read-only data the simulation does not use (1.5), so they are read on
//! demand from the file rather than kept.

use std::io;

use crate::packet::Reader;

/// The only graph version read (1.1).
pub const VERSION: u8 = 8;
/// Bytes of one vertex record (1.3).
const VERTEX_SIZE: usize = 42;
/// Bytes of one edge (1.4).
const EDGE_SIZE: usize = 6;
/// Bytes of one level point (1.5).
const POINT_SIZE: usize = 20;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// One record of the level table (1.2).
#[derive(Clone, Debug, PartialEq)]
pub struct GraphLevel {
    /// The level directory name, e.g. `l01_escape`.
    pub name: String,
    /// Added to a level point gives the global point.
    pub offset: [f32; 3],
    pub id: u8,
    /// Configuration section in `game_levels.ltx`, e.g. `level01`.
    pub section: String,
    /// Equals the identifier in the level's `level.ai`.
    pub identifier: [u8; 16],
}

/// One vertex record (1.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphVertex {
    /// The vertex's position in its level.
    pub level_point: [f32; 3],
    /// The level point plus the level's offset.
    pub global_point: [f32; 3],
    pub level_id: u8,
    /// The vertex's own level vertex, of its level's `level.ai`.
    pub level_vertex: u32,
    /// Vertex types t0 … t3 (1.6).
    pub types: [u8; 4],
    /// Index of the vertex's first edge in [`GameGraph::edges`].
    pub first_edge: u32,
    pub edge_count: u8,
    /// Index of the vertex's first level point in the level point table.
    pub first_point: u32,
    pub point_count: u8,
}

/// A directed edge (1.4).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphEdge {
    pub target: u16,
    /// Metres.
    pub distance: f32,
}

/// One level point (1.5).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LevelPoint {
    /// The centre of the level vertex below.
    pub position: [f32; 3],
    pub level_vertex: u32,
    /// The cross-table distance of that level vertex.
    pub distance: f32,
}

/// `game.graph` without its level points.
#[derive(Clone, Debug, PartialEq)]
pub struct GameGraph {
    pub version: u8,
    pub identifier: [u8; 16],
    pub levels: Vec<GraphLevel>,
    pub vertices: Vec<GraphVertex>,
    /// Every vertex's edges, each vertex's run consecutive.
    pub edges: Vec<GraphEdge>,
    /// The level point count *P* (1.1).
    pub point_count: u32,
    /// File offset of the level point table.
    pub points_start: usize,
    /// The bytes after the level points (1.7; meaning unestablished).
    pub trailing: Vec<u8>,
}

impl GameGraph {
    /// Parses every table but the level points, which are checked to fit.
    /// A version other than 8 is the game's fatal error (1.8). An edge or
    /// level point offset that is not a whole record from its table's start
    /// is an error (our choice: the spec states they always are).
    pub fn parse(file: &[u8]) -> io::Result<Self> {
        let mut r = Reader::new(file);
        let version = r.u8()?;
        if version != VERSION {
            return Err(invalid("Graph version mismatch!"));
        }
        let vertex_count = usize::from(r.u16()?);
        let edge_count = r.u32()? as usize;
        let point_count = r.u32()?;
        let identifier: [u8; 16] = r.take(16)?.try_into().unwrap();
        let level_count = r.u8()?;
        let levels = (0..level_count)
            .map(|_| {
                Ok(GraphLevel {
                    name: r.string()?,
                    offset: r.float3()?,
                    id: r.u8()?,
                    section: r.string()?,
                    identifier: r.take(16)?.try_into().unwrap(),
                })
            })
            .collect::<io::Result<Vec<_>>>()?;
        let table_start = file.len() - r.rest().len();
        let edges_start = VERTEX_SIZE * vertex_count;
        let points_rel = edges_start + EDGE_SIZE * edge_count;
        let index = |offset: u32, start: usize, size: usize, count: usize, what: &str| {
            let rel = (offset as usize)
                .checked_sub(start)
                .filter(|o| o % size == 0 && o / size <= count)
                .ok_or_else(|| invalid(format!("game.graph: {what} offset {offset}")))?;
            Ok::<u32, io::Error>((rel / size) as u32)
        };
        let mut vertices = Vec::with_capacity(vertex_count);
        for i in 0..vertex_count {
            let mut v = Reader::new(r.take(VERTEX_SIZE)?);
            let level_point = v.float3()?;
            let global_point = v.float3()?;
            let level_id = v.u8()?;
            let b = v.take(3)?;
            let level_vertex = u32::from_le_bytes([b[0], b[1], b[2], 0]);
            let types: [u8; 4] = v.take(4)?.try_into().unwrap();
            let edges_offset = v.u32()?;
            let points_offset = v.u32()?;
            let edge_count_v = v.u8()?;
            let point_count_v = v.u8()?;
            let first_edge = index(edges_offset, edges_start, EDGE_SIZE, edge_count, "edges")?;
            let first_point = index(
                points_offset,
                points_rel,
                POINT_SIZE,
                point_count as usize,
                "level points",
            )?;
            if first_edge as usize + usize::from(edge_count_v) > edge_count
                || first_point as usize + usize::from(point_count_v) > point_count as usize
            {
                return Err(invalid(format!("game.graph: vertex {i} overruns a table")));
            }
            vertices.push(GraphVertex {
                level_point,
                global_point,
                level_id,
                level_vertex,
                types,
                first_edge,
                edge_count: edge_count_v,
                first_point,
                point_count: point_count_v,
            });
        }
        let mut edges = Vec::with_capacity(edge_count);
        for _ in 0..edge_count {
            let target = r.u16()?;
            let distance = r.float()?;
            if usize::from(target) >= vertex_count {
                return Err(invalid(format!("game.graph: edge to vertex {target}")));
            }
            edges.push(GraphEdge { target, distance });
        }
        let points_start = table_start + points_rel;
        r.take(POINT_SIZE * point_count as usize)?;
        Ok(Self {
            version,
            identifier,
            levels,
            vertices,
            edges,
            point_count,
            points_start,
            trailing: r.rest().to_vec(),
        })
    }

    /// The level record with level id `id`.
    pub fn level(&self, id: u8) -> Option<&GraphLevel> {
        self.levels.iter().find(|l| l.id == id)
    }

    /// The vertex record of `vertex`.
    pub fn vertex(&self, vertex: u16) -> Option<&GraphVertex> {
        self.vertices.get(usize::from(vertex))
    }

    /// The level holding vertex `vertex` (level-loading.md 2.2, steps 3–4;
    /// 6.1: a missing record is the game's fatal error).
    pub fn vertex_level(&self, vertex: u16) -> io::Result<&GraphLevel> {
        let v = self
            .vertex(vertex)
            .ok_or_else(|| invalid(format!("game.graph: no vertex {vertex}")))?;
        self.level(v.level_id)
            .ok_or_else(|| invalid("Graph point level ID not found!"))
    }

    /// The edges of `vertex` in stored order.
    pub fn edges_of(&self, vertex: u16) -> &[GraphEdge] {
        match self.vertex(vertex) {
            Some(v) => {
                let first = v.first_edge as usize;
                &self.edges[first..first + usize::from(v.edge_count)]
            }
            None => &[],
        }
    }

    /// The level points of `vertex`, read from `file` (the bytes this graph
    /// was parsed from).
    pub fn level_points(&self, file: &[u8], vertex: u16) -> io::Result<Vec<LevelPoint>> {
        let Some(v) = self.vertex(vertex) else {
            return Ok(Vec::new());
        };
        let start = self.points_start + POINT_SIZE * v.first_point as usize;
        let bytes = file
            .get(start..start + POINT_SIZE * usize::from(v.point_count))
            .ok_or_else(|| invalid("game.graph: level points beyond the file"))?;
        bytes
            .as_chunks::<POINT_SIZE>()
            .0
            .iter()
            .map(|b| {
                let mut p = Reader::new(b);
                Ok(LevelPoint {
                    position: p.float3()?,
                    level_vertex: p.u32()?,
                    distance: p.float()?,
                })
            })
            .collect()
    }
}

/// A terrain mask (1.6): four type bytes, 255 matching any.
pub type TerrainMask = [u8; 4];

/// Whether vertex types `types` match `mask` (1.6).
pub fn matches_mask(types: [u8; 4], mask: TerrainMask) -> bool {
    types.iter().zip(mask).all(|(&t, m)| m == 255 || t == m)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Level id, level point, level vertex, types, edges as (target,
    /// distance).
    pub type TestVertex = (u8, [f32; 3], u32, [u8; 4], Vec<(u16, f32)>);

    /// A graph file: `levels` (name, id, offset), `vertices` (level id,
    /// level point, level vertex, types, edges as (target, distance)), one
    /// level point per vertex.
    pub fn graph_file(levels: &[(&str, u8, [f32; 3])], vertices: &[TestVertex]) -> Vec<u8> {
        let edge_count: usize = vertices.iter().map(|v| v.4.len()).sum();
        let mut file = vec![VERSION];
        file.extend((vertices.len() as u16).to_le_bytes());
        file.extend((edge_count as u32).to_le_bytes());
        file.extend((vertices.len() as u32).to_le_bytes());
        file.extend([0xAB; 16]);
        file.push(levels.len() as u8);
        for (name, id, offset) in levels {
            file.extend(name.as_bytes());
            file.push(0);
            for f in offset {
                file.extend(f.to_le_bytes());
            }
            file.push(*id);
            file.extend(format!("level{id:02}\0").as_bytes());
            file.extend([*id; 16]);
        }
        let edges_start = VERTEX_SIZE * vertices.len();
        let points_start = edges_start + EDGE_SIZE * edge_count;
        let mut first = 0;
        for (i, (level, point, lv, types, edges)) in vertices.iter().enumerate() {
            let offset = levels
                .iter()
                .find(|l| l.1 == *level)
                .map_or([0.0; 3], |l| l.2);
            for f in point {
                file.extend(f.to_le_bytes());
            }
            for (f, o) in point.iter().zip(offset) {
                file.extend((f + o).to_le_bytes());
            }
            file.push(*level);
            file.extend(&lv.to_le_bytes()[..3]);
            file.extend(types);
            file.extend(((edges_start + EDGE_SIZE * first) as u32).to_le_bytes());
            file.extend(((points_start + POINT_SIZE * i) as u32).to_le_bytes());
            file.push(edges.len() as u8);
            file.push(1);
            first += edges.len();
        }
        for (_, _, _, _, edges) in vertices {
            for (t, d) in edges {
                file.extend(t.to_le_bytes());
                file.extend(d.to_le_bytes());
            }
        }
        for (_, point, lv, _, _) in vertices {
            for f in point {
                file.extend(f.to_le_bytes());
            }
            file.extend(lv.to_le_bytes());
            file.extend(0.7f32.to_le_bytes());
        }
        file.extend([4, 0, 0, 0]);
        file
    }

    #[test]
    fn parses_every_table() {
        let file = graph_file(
            &[
                ("l01_escape", 1, [3000.0, 1000.0, 0.0]),
                ("l02_garbage", 2, [3000.0, 1000.0, 1000.0]),
            ],
            &[
                (2, [1.0, 2.0, 3.0], 7, [1, 0, 0, 0], vec![(1, 5.5)]),
                (1, [4.0, 5.0, 6.0], 0x12_3456, [8, 1, 0, 0], vec![]),
                (1, [0.0; 3], 9, [0; 4], vec![(0, 1.0), (1, 2.0)]),
            ],
        );
        let g = GameGraph::parse(&file).unwrap();
        assert_eq!(g.levels[1].section, "level02");
        assert_eq!(g.levels[0].offset, [3000.0, 1000.0, 0.0]);
        assert_eq!(g.vertex_level(1).unwrap().name, "l01_escape");
        assert_eq!(g.vertex_level(0).unwrap().name, "l02_garbage");
        assert!(g.vertex_level(3).is_err());
        let v = g.vertex(1).unwrap();
        assert_eq!((v.level_vertex, v.types), (0x12_3456, [8, 1, 0, 0]));
        assert_eq!(v.global_point, [3004.0, 1005.0, 6.0]);
        assert_eq!(
            g.edges_of(2),
            &[
                GraphEdge {
                    target: 0,
                    distance: 1.0
                },
                GraphEdge {
                    target: 1,
                    distance: 2.0
                }
            ]
        );
        assert!(g.edges_of(1).is_empty());
        assert_eq!(g.trailing, [4, 0, 0, 0]);
        let points = g.level_points(&file, 1).unwrap();
        assert_eq!(points[0].level_vertex, 0x12_3456);
        assert!(GameGraph::parse(&file[..60]).is_err());
        let mut wrong = file.clone();
        wrong[0] = 7;
        assert!(GameGraph::parse(&wrong).is_err());
    }

    #[test]
    fn masks_match_by_byte_or_255() {
        assert!(matches_mask([1, 0, 0, 0], [255; 4]));
        assert!(matches_mask([8, 0, 2, 0], [255, 0, 255, 255]));
        assert!(!matches_mask([8, 1, 0, 0], [255, 0, 255, 255]));
        assert!(matches_mask([10, 1, 0, 0], [10, 1, 255, 255]));
    }
}
