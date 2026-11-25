// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The patrol paths of `spawns\all.spawn`, its top-level chunk 3
//! (docs/behavior/offline-simulation.md section 4): named paths of points,
//! each with a position, a level vertex and a game vertex, and links.

use std::borrow::Cow;
use std::collections::HashMap;
use std::io;

use crate::chunks::Chunks;
use crate::packet::Reader;

/// Top-level chunk of the spawn file holding the paths.
const PATHS_CHUNK: u32 = 3;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// A point of a path.
#[derive(Clone, Debug, PartialEq)]
pub struct PathPoint {
    pub name: String,
    pub position: [f32; 3],
    pub flags: u32,
    /// 0xFFFFFFFF in some points.
    pub level_vertex: u32,
    pub game_vertex: u16,
}

/// A link from one point to another.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathLink {
    pub from: u32,
    pub to: u32,
    pub weight: f32,
}

/// A named patrol path.
#[derive(Clone, Debug, PartialEq)]
pub struct PatrolPath {
    pub name: String,
    /// Point *j* is the point of point chunk *j*.
    pub points: Vec<PathPoint>,
    pub links: Vec<PathLink>,
}

/// Every path, looked up by name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PatrolPaths {
    pub paths: Vec<PatrolPath>,
    by_name: HashMap<String, usize>,
}

/// Chunk `id`'s content, borrowed unless it is compressed.
fn sub<'a>(chunks: &Chunks<'a>, id: u32, what: &str) -> io::Result<Cow<'a, [u8]>> {
    chunks.require(id, what)?.content()
}

fn path(content: &[u8], index: usize) -> io::Result<PatrolPath> {
    let what = format!("patrol path {index}");
    let chunks = Chunks::parse(content)?;
    let name = Reader::new(&sub(&chunks, 0, &what)?).string()?;
    let body = sub(&chunks, 1, &what)?;
    let body = Chunks::parse(&body)?;
    let n = Reader::new(&sub(&body, 0, &what)?).u32()?;
    let list = sub(&body, 1, &what)?;
    let list = Chunks::parse(&list)?;
    // Each point is a chunk of the list: the count cannot exceed them.
    let mut points = Vec::with_capacity((n as usize).min(list.len()));
    for j in 0..n {
        let point = sub(&list, j, &what)?;
        let point = Chunks::parse(&point)?;
        let stored = Reader::new(&sub(&point, 0, &what)?).u32()?;
        if stored != j {
            return Err(invalid(format!(
                "{what}: point chunk {j} holds index {stored}"
            )));
        }
        let fields = sub(&point, 1, &what)?;
        let mut r = Reader::new(&fields);
        points.push(PathPoint {
            name: r.string()?,
            position: r.float3()?,
            flags: r.u32()?,
            level_vertex: r.u32()?,
            game_vertex: r.u16()?,
        });
    }
    let mut links = Vec::new();
    if let Some(c) = body.find(2) {
        let bytes = c.content()?;
        let mut r = Reader::new(&bytes);
        while !r.is_empty() {
            let from = r.u32()?;
            let m = r.u32()?;
            for _ in 0..m {
                links.push(PathLink {
                    from,
                    to: r.u32()?,
                    weight: r.float()?,
                });
            }
        }
    }
    Ok(PatrolPath {
        name,
        points,
        links,
    })
}

impl PatrolPaths {
    /// The paths of spawn file `file` (its top-level chunk 3).
    pub fn parse(file: &[u8]) -> io::Result<Self> {
        let top = Chunks::parse(file)?;
        let content = top
            .require(PATHS_CHUNK, "spawn file: patrol paths")?
            .content()?;
        Self::parse_chunk(&content)
    }

    /// The paths of the content of the spawn file's chunk 3.
    pub fn parse_chunk(content: &[u8]) -> io::Result<Self> {
        let chunks = Chunks::parse(content)?;
        let count = Reader::new(&sub(&chunks, 0, "patrol paths")?).u32()?;
        let list = sub(&chunks, 1, "patrol paths")?;
        let list = Chunks::parse(&list)?;
        // Each path is a chunk of the list: the count cannot exceed them.
        let capacity = (count as usize).min(list.len());
        let mut paths = Vec::with_capacity(capacity);
        let mut by_name = HashMap::with_capacity(capacity);
        for i in 0..count {
            let p = path(&sub(&list, i, "patrol paths")?, i as usize)?;
            by_name.insert(p.name.clone(), paths.len());
            paths.push(p);
        }
        Ok(Self { paths, by_name })
    }

    /// The paths `paths`, in that order.
    pub fn from_paths(paths: Vec<PatrolPath>) -> Self {
        let by_name = paths
            .iter()
            .enumerate()
            .map(|(i, p)| (p.name.clone(), i))
            .collect();
        Self { paths, by_name }
    }

    /// The path named `name`.
    pub fn get(&self, name: &str) -> Option<&PatrolPath> {
        self.by_name.get(name).map(|&i| &self.paths[i])
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn chunk(id: u32, payload: &[u8]) -> Vec<u8> {
        let mut c = id.to_le_bytes().to_vec();
        c.extend((payload.len() as u32).to_le_bytes());
        c.extend(payload);
        c
    }

    /// Position, level vertex, game vertex.
    pub type TestPoint = ([f32; 3], u32, u16);

    /// The content of chunk 3 for `paths`: (name, points as (position,
    /// level vertex, game vertex)), each point linked to the next.
    pub fn paths_chunk(paths: &[(&str, Vec<TestPoint>)]) -> Vec<u8> {
        let mut list = Vec::new();
        for (i, (name, points)) in paths.iter().enumerate() {
            let mut point_list = Vec::new();
            for (j, (position, lv, gv)) in points.iter().enumerate() {
                let mut fields = format!("p{j}\0").into_bytes();
                for f in position {
                    fields.extend(f.to_le_bytes());
                }
                fields.extend(0u32.to_le_bytes());
                fields.extend(lv.to_le_bytes());
                fields.extend(gv.to_le_bytes());
                let mut point = chunk(0, &(j as u32).to_le_bytes());
                point.extend(chunk(1, &fields));
                point_list.extend(chunk(j as u32, &point));
            }
            let mut links = Vec::new();
            for j in 1..points.len() as u32 {
                links.extend((j - 1).to_le_bytes());
                links.extend(1u32.to_le_bytes());
                links.extend(j.to_le_bytes());
                links.extend(1.0f32.to_le_bytes());
            }
            let mut body = chunk(0, &(points.len() as u32).to_le_bytes());
            body.extend(chunk(1, &point_list));
            body.extend(chunk(2, &links));
            let mut p = chunk(0, format!("{name}\0").as_bytes());
            p.extend(chunk(1, &body));
            list.extend(chunk(i as u32, &p));
        }
        let mut content = chunk(0, &(paths.len() as u32).to_le_bytes());
        content.extend(chunk(1, &list));
        content
    }

    #[test]
    fn parses_points_and_links() {
        let content = paths_chunk(&[
            (
                "a",
                vec![([1.0, 2.0, 3.0], 5, 7), ([0.0; 3], u32::MAX, 51_048)],
            ),
            ("b", vec![([4.0, 0.0, 0.0], 9, 2)]),
        ]);
        let p = PatrolPaths::parse_chunk(&content).unwrap();
        assert_eq!(p.paths.len(), 2);
        let a = p.get("a").unwrap();
        assert_eq!((a.points[0].level_vertex, a.points[0].game_vertex), (5, 7));
        assert_eq!(a.points[1].level_vertex, u32::MAX);
        assert_eq!(
            a.links,
            [PathLink {
                from: 0,
                to: 1,
                weight: 1.0
            }]
        );
        assert_eq!(p.get("b").unwrap().points[0].position, [4.0, 0.0, 0.0]);
        assert!(p.get("c").is_none());
        // A path count beyond the paths stored fails without allocating
        // for it.
        let mut big = content.clone();
        big[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(PatrolPaths::parse_chunk(&big).is_err());
    }
}
