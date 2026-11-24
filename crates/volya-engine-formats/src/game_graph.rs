// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The game graph `game.graph`, only as far as choosing the level to load
//! needs (docs/behavior/level-loading.md, sections 2.2 and 5.11): the
//! header, the level table and each vertex's level id. Not chunked.

use std::io;

use crate::cp1251;

/// Bytes of one vertex record.
const VERTEX_SIZE: usize = 42;
/// Offset of the level id in a vertex record.
const VERTEX_LEVEL_ID: usize = 24;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// One record of the level table.
#[derive(Clone, Debug, PartialEq)]
pub struct GraphLevel {
    /// The level directory name, e.g. `l01_escape`.
    pub name: String,
    pub offset: [f32; 3],
    pub id: u8,
    /// Configuration section, e.g. `level01`.
    pub section: String,
    pub identifier: [u8; 16],
}

/// The parts of `game.graph` read here.
#[derive(Clone, Debug, PartialEq)]
pub struct GameGraph {
    pub version: u8,
    pub edge_count: u32,
    pub point_count: u32,
    pub identifier: [u8; 16],
    pub levels: Vec<GraphLevel>,
    /// Level id of each vertex.
    pub vertex_levels: Vec<u8>,
}

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> io::Result<&'a [u8]> {
        let end = self
            .at
            .checked_add(n)
            .filter(|&e| e <= self.data.len())
            .ok_or_else(|| invalid("game.graph: unexpected end"))?;
        let out = &self.data[self.at..end];
        self.at = end;
        Ok(out)
    }

    fn u8(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> io::Result<u16> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }

    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn f32(&mut self) -> io::Result<f32> {
        Ok(f32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn string(&mut self) -> io::Result<String> {
        let rest = &self.data[self.at..];
        let end = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| invalid("game.graph: unterminated string"))?;
        self.at += end + 1;
        Ok(cp1251::decode(&rest[..end]))
    }
}

impl GameGraph {
    /// Header, level table and vertex records; the further tables are not
    /// read.
    pub fn parse(file: &[u8]) -> io::Result<Self> {
        let mut r = Reader { data: file, at: 0 };
        let version = r.u8()?;
        let vertex_count = r.u16()?;
        let edge_count = r.u32()?;
        let point_count = r.u32()?;
        let identifier: [u8; 16] = r.take(16)?.try_into().unwrap();
        let level_count = r.u8()?;
        let levels = (0..level_count)
            .map(|_| {
                Ok(GraphLevel {
                    name: r.string()?,
                    offset: [r.f32()?, r.f32()?, r.f32()?],
                    id: r.u8()?,
                    section: r.string()?,
                    identifier: r.take(16)?.try_into().unwrap(),
                })
            })
            .collect::<io::Result<_>>()?;
        let vertex_levels = (0..vertex_count)
            .map(|_| Ok(r.take(VERTEX_SIZE)?[VERTEX_LEVEL_ID]))
            .collect::<io::Result<_>>()?;
        Ok(Self {
            version,
            edge_count,
            point_count,
            identifier,
            levels,
            vertex_levels,
        })
    }

    /// The level holding game-graph vertex `vertex` (section 2.2, steps 3–4).
    pub fn vertex_level(&self, vertex: u16) -> io::Result<&GraphLevel> {
        let id = *self
            .vertex_levels
            .get(usize::from(vertex))
            .ok_or_else(|| invalid(format!("game.graph: no vertex {vertex}")))?;
        self.levels
            .iter()
            .find(|l| l.id == id)
            .ok_or_else(|| invalid(format!("game.graph: no level with id {id}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_levels_and_vertices() {
        let mut file = vec![8];
        file.extend(2u16.to_le_bytes());
        file.extend(5u32.to_le_bytes());
        file.extend(7u32.to_le_bytes());
        file.extend([0xAB; 16]);
        file.push(2);
        for (name, id) in [("l01_escape", 1u8), ("l02_garbage", 2)] {
            file.extend(name.as_bytes());
            file.push(0);
            for f in [3000.0f32, 1000.0, 0.0] {
                file.extend(f.to_le_bytes());
            }
            file.push(id);
            file.extend(format!("level0{id}\0").as_bytes());
            file.extend([id; 16]);
        }
        for id in [2u8, 1] {
            let mut v = [0u8; VERTEX_SIZE];
            v[VERTEX_LEVEL_ID] = id;
            file.extend(v);
        }
        file.extend([1, 2, 3]);
        let g = GameGraph::parse(&file).unwrap();
        assert_eq!((g.version, g.edge_count, g.point_count), (8, 5, 7));
        assert_eq!(g.levels[1].section, "level02");
        assert_eq!(g.levels[0].offset, [3000.0, 1000.0, 0.0]);
        assert_eq!(g.vertex_level(1).unwrap().name, "l01_escape");
        assert_eq!(g.vertex_level(0).unwrap().name, "l02_garbage");
        assert!(g.vertex_level(2).is_err());
        assert!(GameGraph::parse(&file[..60]).is_err());
    }
}
