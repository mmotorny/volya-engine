// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The spawn file `spawns\all.spawn`, only as far as the start position needs
//! (docs/behavior/start-position.md, sections 2 and 3): the objects of
//! top-level chunk 1 and the common part of each spawn packet. Everything
//! else is skipped by chunk sizes.

use std::io;
use std::ops::ControlFlow;

use crate::chunks::Chunks;
use crate::cp1251;

/// Top-level chunk holding the objects.
const OBJECTS_CHUNK: u32 = 1;
/// Sub-chunk of [`OBJECTS_CHUNK`] holding one sub-chunk per object.
const OBJECT_LIST_CHUNK: u32 = 1;
/// Chunk of an object holding its two packets.
const PACKETS_CHUNK: u32 = 1;
/// Chunk of [`PACKETS_CHUNK`] holding the spawn packet.
const SPAWN_PACKET_CHUNK: u32 = 0;

/// The spawn message type every packet starts with.
const SPAWN_MESSAGE: u16 = 1;
/// Flags bit: a version field follows.
const FLAG_VERSION: u16 = 0x20;
/// Lowest version whose layout the spec describes fully (section 3.1).
const MIN_VERSION: u16 = 112;

/// Section name of the player's object.
pub const ACTOR_SECTION: &str = "actor";

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The common part of a spawn packet (section 3.1).
#[derive(Clone, Debug, PartialEq)]
pub struct SpawnObject<'a> {
    /// Object index: the id of the object's sub-chunk.
    pub index: u32,
    /// Configuration section (`actor` for the player).
    pub section: String,
    pub name: String,
    /// World position (for the actor, of its feet).
    pub position: [f32; 3],
    /// Orientation angles `a0, a1, a2`.
    pub angles: [f32; 3],
    pub flags: u16,
    pub version: u16,
    /// The object-specific data (`s − 2` bytes).
    pub data: &'a [u8],
    /// The whole packet data, for fields this parser skips.
    pub packet: &'a [u8],
}

/// Reads packet fields in order, failing on overrun.
struct Fields<'a> {
    data: &'a [u8],
}

impl<'a> Fields<'a> {
    fn take(&mut self, n: usize) -> io::Result<&'a [u8]> {
        if n > self.data.len() {
            return Err(invalid("spawn packet: truncated"));
        }
        let (head, rest) = self.data.split_at(n);
        self.data = rest;
        Ok(head)
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

    fn float3(&mut self) -> io::Result<[f32; 3]> {
        let b = self.take(12)?;
        Ok([0, 4, 8].map(|i| f32::from_le_bytes(b[i..i + 4].try_into().unwrap())))
    }

    fn string(&mut self) -> io::Result<String> {
        let end = self
            .data
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| invalid("spawn packet: unterminated string"))?;
        let s = cp1251::decode(&self.data[..end]);
        self.data = &self.data[end + 1..];
        Ok(s)
    }
}

/// Parses the common part of spawn packet data (after its `u16` length).
pub fn parse_spawn_packet(index: u32, packet: &[u8]) -> io::Result<SpawnObject<'_>> {
    let mut f = Fields { data: packet };
    let message = f.u16()?;
    if message != SPAWN_MESSAGE {
        return Err(invalid(format!(
            "spawn object {index}: message type {message}"
        )));
    }
    let section = f.string()?;
    let name = f.string()?;
    f.take(2)?; // Two bytes of unestablished meaning.
    let position = f.float3()?;
    let angles = f.float3()?;
    f.take(8)?; // Four u16 of unestablished meaning.
    let flags = f.u16()?;
    if flags & FLAG_VERSION == 0 {
        return Err(invalid(format!("spawn object {index}: no version")));
    }
    let version = f.u16()?;
    // Versions below 112 have legacy fields the spec does not describe.
    if version < MIN_VERSION {
        return Err(invalid(format!(
            "spawn object {index}: unsupported version {version}"
        )));
    }
    if version > 69 {
        f.u16()?;
    }
    if version >= 71 {
        let len = if version < 94 {
            usize::from(f.u8()?)
        } else {
            usize::from(f.u16()?)
        };
        f.take(len)?; // Client data.
    }
    if version > 79 {
        f.u16()?;
    }
    let size = usize::from(f.u16()?);
    let data = f.take(
        size.checked_sub(2)
            .ok_or_else(|| invalid(format!("spawn object {index}: data size {size} below 2")))?,
    )?;
    Ok(SpawnObject {
        index,
        section,
        name,
        position,
        angles,
        flags,
        version,
        data,
        packet,
    })
}

/// The packet data of a packet chunk: `u16 n`, then `n` bytes.
fn packet_data(chunk: &[u8]) -> io::Result<&[u8]> {
    let (len, rest) = chunk
        .split_first_chunk::<2>()
        .ok_or_else(|| invalid("spawn packet: no length"))?;
    rest.get(..usize::from(u16::from_le_bytes(*len)))
        .ok_or_else(|| invalid("spawn packet: longer than its chunk"))
}

/// Calls `visit` with the common part of every object's spawn packet, in
/// object-index order, until it breaks. Objects are the list of
/// sub-chunks 0, 1, 2, … (chunks.md, section 2) of chunk 1's sub-chunk 1.
pub fn for_each_object(
    file: &[u8],
    mut visit: impl FnMut(&SpawnObject<'_>) -> ControlFlow<()>,
) -> io::Result<()> {
    let top = Chunks::parse(file)?;
    let objects = top.require(OBJECTS_CHUNK, "spawn file")?.content()?;
    let objects = Chunks::parse(&objects)?;
    let list = objects
        .require(OBJECT_LIST_CHUNK, "spawn objects")?
        .content()?;
    let list = Chunks::parse(&list)?;
    for object in list.list() {
        let index = object.id();
        let content = object.content()?;
        let chunks = Chunks::parse(&content)?;
        let packets = chunks
            .require(PACKETS_CHUNK, &format!("spawn object {index}"))?
            .content()?;
        let packets = Chunks::parse(&packets)?;
        let spawn = packets
            .require(SPAWN_PACKET_CHUNK, &format!("spawn object {index}"))?
            .content()?;
        if visit(&parse_spawn_packet(index, packet_data(&spawn)?)?).is_break() {
            break;
        }
    }
    Ok(())
}

/// The actor's start: the first object whose section is `actor` (the spec
/// knows exactly one; taking the first is our choice).
#[derive(Clone, Debug, PartialEq)]
pub struct ActorStart {
    pub index: u32,
    pub name: String,
    /// Position of the actor's feet.
    pub position: [f32; 3],
    /// Stored angles `a0, a1, a2`.
    pub angles: [f32; 3],
    /// Game-graph vertex and level-graph node from the object-specific data
    /// (section 3.3), for cross-checks.
    pub game_graph_vertex: u16,
    pub level_graph_node: u32,
}

/// Finds the actor in spawn file `file`.
pub fn actor_start(file: &[u8]) -> io::Result<ActorStart> {
    let mut found = None;
    for_each_object(file, |o| {
        if o.section != ACTOR_SECTION {
            return ControlFlow::Continue(());
        }
        found = Some(actor_fields(o));
        ControlFlow::Break(())
    })?;
    found.unwrap_or_else(|| Err(invalid("spawn file: no actor")))
}

fn actor_fields(o: &SpawnObject<'_>) -> io::Result<ActorStart> {
    // Object-specific data: u16 game-graph vertex, float, u32, u32
    // level-graph node (section 3.3).
    let mut f = Fields { data: o.data };
    let game_graph_vertex = f.u16()?;
    f.take(8)?;
    let level_graph_node = f.u32()?;
    Ok(ActorStart {
        index: o.index,
        name: o.name.clone(),
        position: o.position,
        angles: o.angles,
        game_graph_vertex,
        level_graph_node,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunks::chunk;

    fn packet(section: &str, version: u16, data: &[u8]) -> Vec<u8> {
        let mut p = 1u16.to_le_bytes().to_vec();
        for s in [section, "name"] {
            p.extend(s.as_bytes());
            p.push(0);
        }
        p.extend([0x00, 0xFE]);
        for f in [1.0f32, 2.0, 3.0, 0.5, -1.5, 0.25] {
            p.extend(f.to_le_bytes());
        }
        for v in [0u16, 0xFFFF, 0xFFFF, 0xFFFF, 0x29, version, 6] {
            p.extend(v.to_le_bytes());
        }
        p.extend(3u16.to_le_bytes()); // Client data length 3.
        p.extend(b"abc");
        p.extend(7u16.to_le_bytes());
        p.extend((data.len() as u16 + 2).to_le_bytes());
        p.extend(data);
        p
    }

    fn object(index: u32, packet: &[u8]) -> Vec<u8> {
        let mut spawn = (packet.len() as u16).to_le_bytes().to_vec();
        spawn.extend(packet);
        let mut packets = chunk(0, &spawn);
        packets.extend(chunk(1, &[0, 0]));
        let mut obj = chunk(0, &(index as u16).to_le_bytes());
        obj.extend(chunk(1, &packets));
        chunk(index, &obj)
    }

    fn file(objects: &[Vec<u8>]) -> Vec<u8> {
        let list: Vec<u8> = objects.concat();
        let mut inner = chunk(0, &(objects.len() as u32).to_le_bytes());
        inner.extend(chunk(1, &list));
        inner.extend(chunk(2, &[]));
        let mut f = chunk(0, &[0; 44]);
        f.extend(chunk(1, &inner));
        f.extend(chunk(2, &[1, 2, 3]));
        f
    }

    #[test]
    fn finds_the_actor() {
        let mut actor_data = 4u16.to_le_bytes().to_vec();
        actor_data.extend(0.7f32.to_le_bytes());
        actor_data.extend(1u32.to_le_bytes());
        actor_data.extend(11_713u32.to_le_bytes());
        actor_data.extend(b"more");
        let f = file(&[
            object(0, &packet("m_trader", 118, b"xyz")),
            object(1, &packet(ACTOR_SECTION, 118, &actor_data)),
        ]);
        let mut seen = Vec::new();
        for_each_object(&f, |o| {
            seen.push((o.index, o.section.clone(), o.data.len()));
            ControlFlow::Continue(())
        })
        .unwrap();
        assert_eq!(seen, [(0, "m_trader".into(), 3), (1, "actor".into(), 18)]);
        let actor = actor_start(&f).unwrap();
        assert_eq!(actor.index, 1);
        assert_eq!(actor.position, [1.0, 2.0, 3.0]);
        assert_eq!(actor.angles, [0.5, -1.5, 0.25]);
        assert_eq!(
            (actor.game_graph_vertex, actor.level_graph_node),
            (4, 11_713)
        );

        assert!(actor_start(&file(&[object(0, &packet("x", 118, b""))])).is_err());
    }

    #[test]
    fn rejects_invalid_packets() {
        assert!(parse_spawn_packet(0, &packet("a", 111, b"")).is_err());
        assert!(parse_spawn_packet(0, &packet("a", 112, b"")).is_ok());
        let mut no_version = packet("a", 118, b"");
        // Flags follow message type, two names ("a\0name\0"), 2 + 24 + 8 bytes.
        let flags_at = 2 + 2 + 5 + 2 + 24 + 8;
        no_version[flags_at] &= !0x20;
        assert!(parse_spawn_packet(0, &no_version).is_err());
        let mut message = packet("a", 118, b"");
        message[0] = 2;
        assert!(parse_spawn_packet(0, &message).is_err());
        let long = packet("a", 118, b"data");
        assert!(parse_spawn_packet(0, &long[..long.len() - 1]).is_err());
    }
}
