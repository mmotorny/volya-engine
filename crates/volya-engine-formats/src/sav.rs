// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Saved games (`.sav`, docs/formats/sav.md): the container (section 2), the
//! decompressed body's chunks (section 3), the simulation header (4), game
//! time (5, 12), the spawn binding (6) and the object list (7.1), read and
//! written. Object records are typed by [`crate::object`]; the registries of
//! chunk 9 (section 9) are kept as stored.

use std::io;

use crate::chunks::Chunks;
use crate::lzo1x;
use crate::packet::{Reader, Writer, count, invalid, length16};

/// The container's marker (section 2).
pub const MARKER: u32 = 0xFFFF_FFFF;
/// The container version the game writes.
pub const CONTAINER_VERSION: u32 = 3;
/// The simulation header version the game writes (section 4).
pub const SIMULATION_VERSION: u32 = 3;

/// Body chunk ids (section 3), in the order the game writes them.
pub const SIMULATION_HEADER_CHUNK: u32 = 0;
pub const GAME_TIME_CHUNK: u32 = 5;
pub const SPAWN_BINDING_CHUNK: u32 = 1;
pub const OBJECTS_CHUNK: u32 = 2;
pub const REGISTRIES_CHUNK: u32 = 9;

/// Whether `file` is accepted as a saved game ("valid", section 2): at least
/// 8 bytes, the marker, and a container version above 1.
pub fn is_valid(file: &[u8]) -> bool {
    let word = |at: usize| u32::from_le_bytes(file[at..at + 4].try_into().unwrap());
    file.len() >= 8 && word(0) == MARKER && word(4) > 1
}

/// Reads the container: checks it is valid and returns the decompressed
/// body. The body must decompress to exactly the stored length `n` (our
/// choice: the game takes `n` bytes whatever the decompressor produced).
pub fn read_container(file: &[u8]) -> io::Result<Vec<u8>> {
    if !is_valid(file) {
        return Err(invalid(
            "Saved game version mismatch or saved game is corrupted",
        ));
    }
    let n = file
        .get(8..12)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
        .ok_or_else(|| invalid("saved game: no body length"))?;
    let body = lzo1x::decompress(&file[12..], n)?;
    if body.len() != n {
        return Err(invalid(format!(
            "saved game: body decompresses to {} bytes, header says {n}",
            body.len()
        )));
    }
    Ok(body)
}

/// Writes the container of version 3 around `body`.
pub fn write_container(body: &[u8]) -> io::Result<Vec<u8>> {
    let n = u32::try_from(body.len()).map_err(|_| invalid("saved game body over 4 GiB"))?;
    let mut file = Vec::with_capacity(12 + body.len() / 2);
    file.extend(MARKER.to_le_bytes());
    file.extend(CONTAINER_VERSION.to_le_bytes());
    file.extend(n.to_le_bytes());
    file.extend(lzo1x::compress(body));
    Ok(file)
}

/// Chunk 5: game time (section 5).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GameTime {
    /// Game milliseconds (section 12).
    pub time: u64,
    /// Game milliseconds per real millisecond.
    pub time_factor: f32,
    /// The speed to return to after a temporary change.
    pub normal_time_factor: f32,
}

/// Game time of a calendar date (section 12, the game's formula). Fails on
/// a date or time out of range, or a result beyond `u64`.
pub fn game_time(
    year: u32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    millisecond: u32,
) -> io::Result<u64> {
    const MONTH_DAYS: [u64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let y = u64::from(year);
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let month_days = |m: usize| MONTH_DAYS[m - 1] + u64::from(m == 2 && leap);
    let bad = || {
        invalid(format!(
            "game time of {day:02}.{month:02}.{year} \
             {hour:02}:{minute:02}:{second:02}.{millisecond:03} is out of range"
        ))
    };
    if year == 0
        || !(1..=12).contains(&month)
        || day == 0
        || u64::from(day) > month_days(month as usize)
        || hour >= 24
        || minute >= 60
        || second >= 60
        || millisecond >= 1000
    {
        return Err(bad());
    }
    let months: u64 = (1..month as usize).map(month_days).sum();
    let days = (y - 1) * 365 + y / 4 - y / 100 + y / 400 + months + u64::from(day) - 1;
    let clock = ((u64::from(hour) * 60 + u64::from(minute)) * 60 + u64::from(second)) * 1000
        + u64::from(millisecond);
    days.checked_mul(24 * 60 * 60 * 1000)
        .and_then(|ms| ms.checked_add(clock))
        .ok_or_else(bad)
}

/// Chunk 1: the spawn binding (section 6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnBinding {
    /// Spawn-file name (`all`).
    pub spawn_name: String,
    /// The spawn file's identifier.
    pub identifier: [u8; 16],
    /// Ids of sub-chunk 1's empty chunks: the spawn points' object indices.
    pub spawn_points: Vec<u32>,
}

/// Sub-chunks of the spawn binding.
const BINDING_NAME_CHUNK: u32 = 0;
const BINDING_POINTS_CHUNK: u32 = 1;

impl SpawnBinding {
    /// The binding of a spawn file with `objects` object records: one spawn
    /// point per record (section 6, believed).
    pub fn new(spawn_name: &str, identifier: [u8; 16], objects: u32) -> Self {
        Self {
            spawn_name: spawn_name.to_owned(),
            identifier,
            spawn_points: (0..objects).collect(),
        }
    }

    pub fn read(payload: &[u8]) -> io::Result<Self> {
        let chunks = Chunks::parse(payload)?;
        let name = chunks.require(BINDING_NAME_CHUNK, "spawn binding")?;
        let mut r = Reader::new(name.uncompressed()?);
        let spawn_name = r.string()?;
        let identifier = r.take(16)?.try_into().unwrap();
        // Bytes after the identifier are rejected (our choice).
        r.finish("spawn binding name")?;
        // Sub-chunk 1 is not read by the game; we keep its ids. A missing one
        // reads as no spawn points, a non-empty chunk in it is rejected (our
        // choices: only empty chunks round-trip as ids); ids must increase
        // (section 6).
        let spawn_points = match chunks.find(BINDING_POINTS_CHUNK) {
            None => Vec::new(),
            Some(points) => Chunks::parse(points.uncompressed()?)?
                .iter()
                .map(|c| {
                    if c.payload.is_empty() {
                        Ok(c.id_word)
                    } else {
                        Err(invalid(format!(
                            "spawn point {:#x} is not empty",
                            c.id_word
                        )))
                    }
                })
                .collect::<io::Result<_>>()?,
        };
        let binding = Self {
            spawn_name,
            identifier,
            spawn_points,
        };
        binding.check_order()?;
        Ok(binding)
    }

    /// Spawn-point ids in increasing order (section 6).
    fn check_order(&self) -> io::Result<()> {
        match self.spawn_points.windows(2).find(|w| w[0] >= w[1]) {
            Some(w) => Err(invalid(format!(
                "spawn points out of order: {} before {}",
                w[0], w[1]
            ))),
            None => Ok(()),
        }
    }

    pub fn write(&self) -> io::Result<Vec<u8>> {
        self.check_order()?;
        let mut name = Writer::new();
        name.string(&self.spawn_name)?;
        name.bytes(&self.identifier);
        let mut points = Vec::with_capacity(self.spawn_points.len() * 8);
        for &id in &self.spawn_points {
            points.extend(chunk_bytes(id, &[])?);
        }
        let mut out = chunk_bytes(BINDING_NAME_CHUNK, &name.into_inner())?;
        out.extend(chunk_bytes(BINDING_POINTS_CHUNK, &points)?);
        Ok(out)
    }
}

/// One object record of chunk 2 as stored: the spawn and update packets,
/// each without its `u16` length (section 7.1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawRecord {
    pub spawn: Vec<u8>,
    pub update: Vec<u8>,
}

/// Chunk 2: `u32` count, then the records.
pub fn read_objects(payload: &[u8]) -> io::Result<Vec<RawRecord>> {
    let mut r = Reader::new(payload);
    let records = r.list(|r| {
        let n = usize::from(r.u16()?);
        let spawn = r.take(n)?.to_vec();
        let n = usize::from(r.u16()?);
        let update = r.take(n)?.to_vec();
        Ok(RawRecord { spawn, update })
    })?;
    r.finish("objects chunk")?;
    Ok(records)
}

pub fn write_objects(records: &[RawRecord]) -> io::Result<Vec<u8>> {
    let mut w = Writer::new();
    w.u32(count(records.len(), "objects")?);
    for record in records {
        w.u16(length16(record.spawn.len(), "spawn packet")?);
        w.bytes(&record.spawn);
        w.u16(length16(record.update.len(), "update packet")?);
        w.bytes(&record.update);
    }
    Ok(w.into_inner())
}

/// Chunk 9 with every count 0 and no active task (section 11.3): registries
/// 1–8 empty, the active task empty with objective index 0xFFFF, statistics
/// empty.
pub fn empty_registries() -> Vec<u8> {
    let mut w = Writer::new();
    (0..8).for_each(|_| w.u32(0));
    w.u8(0);
    w.u16(0xFFFF);
    w.u32(0);
    w.into_inner()
}

/// The decompressed body of a save (section 3).
#[derive(Clone, Debug, PartialEq)]
pub struct Save {
    /// Chunk 0's version (3 when written by the game).
    pub simulation_version: u32,
    pub game_time: GameTime,
    pub spawn_binding: SpawnBinding,
    pub objects: Vec<RawRecord>,
    /// Chunk 9 as stored.
    pub registries: Vec<u8>,
}

fn chunk_bytes(id: u32, payload: &[u8]) -> io::Result<Vec<u8>> {
    let size = u32::try_from(payload.len()).map_err(|_| invalid("chunk over 4 GiB"))?;
    let mut out = Vec::with_capacity(8 + payload.len());
    out.extend(id.to_le_bytes());
    out.extend(size.to_le_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

/// The assertion text naming each missing chunk (section 3).
fn missing(id: u32) -> &'static str {
    match id {
        SIMULATION_HEADER_CHUNK => "Can't find chunk ALIFE_CHUNK_DATA",
        SPAWN_BINDING_CHUNK => "Cannot find chunk SPAWN_CHUNK_DATA!",
        OBJECTS_CHUNK => "Can't find chunk OBJECT_CHUNK_DATA!",
        GAME_TIME_CHUNK => "Can't find chunk GAME_TIME_CHUNK_DATA!",
        _ => "Can't find chunk REGISTRY_CHUNK_DATA!",
    }
}

impl Save {
    /// Reads a decompressed body. Each chunk is found by id; a missing or
    /// empty one is an error (section 3). Compressed chunks are rejected
    /// (no chunk of a save is stored compressed; our choice).
    pub fn read_body(body: &[u8]) -> io::Result<Self> {
        let chunks = Chunks::parse(body)?;
        let get = |id| -> io::Result<&[u8]> {
            match chunks.find(id) {
                Some(c) if !c.payload.is_empty() => c.uncompressed(),
                _ => Err(invalid(missing(id))),
            }
        };
        let mut r = Reader::new(get(SIMULATION_HEADER_CHUNK)?);
        let simulation_version = r.u32()?;
        // Here and in chunk 5, bytes after the fields are rejected (our
        // choice).
        r.finish("simulation header")?;
        if simulation_version < 2 {
            return Err(invalid(
                "ALife version mismatch! (Delete saved game and try again)",
            ));
        }
        let mut r = Reader::new(get(GAME_TIME_CHUNK)?);
        let game_time = GameTime {
            time: r.u64()?,
            time_factor: r.float()?,
            normal_time_factor: r.float()?,
        };
        r.finish("game time")?;
        Ok(Self {
            simulation_version,
            game_time,
            spawn_binding: SpawnBinding::read(get(SPAWN_BINDING_CHUNK)?)?,
            objects: read_objects(get(OBJECTS_CHUNK)?)?,
            registries: get(REGISTRIES_CHUNK)?.to_vec(),
        })
    }

    /// Writes the body: the five chunks in the game's order (section 3).
    /// The records are written as given.
    // TODO(#75): the save builder: select, order and number the records
    // (sections 7.1 and 11.3).
    pub fn write_body(&self) -> io::Result<Vec<u8>> {
        if self.registries.is_empty() {
            return Err(invalid(
                "registries chunk empty: a reader takes it as missing",
            ));
        }
        let mut time = Writer::new();
        time.u64(self.game_time.time);
        time.float(self.game_time.time_factor);
        time.float(self.game_time.normal_time_factor);
        let mut body = chunk_bytes(
            SIMULATION_HEADER_CHUNK,
            &self.simulation_version.to_le_bytes(),
        )?;
        body.extend(chunk_bytes(GAME_TIME_CHUNK, &time.into_inner())?);
        body.extend(chunk_bytes(
            SPAWN_BINDING_CHUNK,
            &self.spawn_binding.write()?,
        )?);
        body.extend(chunk_bytes(OBJECTS_CHUNK, &write_objects(&self.objects)?)?);
        body.extend(chunk_bytes(REGISTRIES_CHUNK, &self.registries)?);
        Ok(body)
    }

    /// Reads a `.sav` file.
    pub fn read(file: &[u8]) -> io::Result<Self> {
        Self::read_body(&read_container(file)?)
    }

    /// Writes a `.sav` file.
    pub fn write(&self) -> io::Result<Vec<u8>> {
        write_container(&self.write_body()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Save {
        Save {
            simulation_version: SIMULATION_VERSION,
            game_time: GameTime {
                time: game_time(2012, 5, 1, 5, 30, 0, 0).unwrap(),
                time_factor: 10.0,
                normal_time_factor: 10.0,
            },
            spawn_binding: SpawnBinding::new("all", [7; 16], 3),
            objects: vec![
                RawRecord {
                    spawn: b"spawn packet".to_vec(),
                    update: vec![0, 0],
                },
                RawRecord {
                    spawn: vec![1; 300],
                    update: vec![],
                },
            ],
            registries: empty_registries(),
        }
    }

    #[test]
    fn new_game_time() {
        let t = game_time(2012, 5, 1, 5, 30, 0, 0).unwrap();
        assert_eq!(t, 63_471_533_400_000);
        assert_eq!(
            t.to_le_bytes(),
            [0xc0, 0x9f, 0x33, 0x1e, 0xba, 0x39, 0x00, 0x00]
        );
        assert_eq!(game_time(1, 1, 1, 0, 0, 0, 1).unwrap(), 1);
        assert!(game_time(2012, 2, 29, 23, 59, 59, 999).is_ok());
        for bad in [
            (0, 1, 1, 0, 0, 0, 0),
            (2011, 2, 29, 0, 0, 0, 0),
            (2012, 13, 1, 0, 0, 0, 0),
            (2012, 0, 1, 0, 0, 0, 0),
            (2012, 1, 0, 0, 0, 0, 0),
            (2012, 1, 1, 24, 0, 0, 0),
            (2012, 1, 1, 0, 60, 0, 0),
            (2012, 1, 1, 0, 0, 60, 0),
            (2012, 1, 1, 0, 0, 0, 1000),
            (u32::MAX, 12, 31, 23, 59, 59, 999),
        ] {
            let (y, mo, d, h, mi, s, ms) = bad;
            assert!(game_time(y, mo, d, h, mi, s, ms).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn container_round_trips() {
        let save = sample();
        let file = save.write().unwrap();
        assert_eq!(file[..8], [0xff, 0xff, 0xff, 0xff, 3, 0, 0, 0]);
        let body = save.write_body().unwrap();
        assert_eq!(file[8..12], (body.len() as u32).to_le_bytes());
        assert_eq!(read_container(&file).unwrap(), body);
        assert_eq!(Save::read(&file).unwrap(), save);

        // The body's chunks in the game's order, chunk 0 = 03 00 00 00 and a
        // 16-byte chunk 5.
        let chunks = Chunks::parse(&body).unwrap();
        let ids: Vec<u32> = chunks.iter().map(|c| c.id()).collect();
        assert_eq!(ids, [0, 5, 1, 2, 9]);
        assert_eq!(chunks.find(0).unwrap().payload, [3, 0, 0, 0]);
        assert_eq!(chunks.find(5).unwrap().payload.len(), 16);
        assert_eq!(chunks.find(9).unwrap().payload.len(), 39);
    }

    #[test]
    fn spawn_binding_layout() {
        let b = SpawnBinding::new("all", [0xAB; 16], 2);
        let bytes = b.write().unwrap();
        // Sub-chunk 0: 20 bytes; sub-chunk 1: two empty chunks, 16 bytes.
        assert_eq!(bytes.len(), 8 + 20 + 8 + 16);
        assert_eq!(&bytes[8..12], b"all\0");
        assert_eq!(&bytes[36..44], [0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&bytes[44..52], [1, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(SpawnBinding::read(&bytes).unwrap(), b);
    }

    #[test]
    fn rejects_invalid_saves() {
        let file = sample().write().unwrap();
        assert!(is_valid(&file));
        assert!(!is_valid(&file[..7]));
        let mut old = file.clone();
        old[4] = 1;
        assert!(!is_valid(&old));
        assert!(read_container(&old).is_err());
        let mut marker = file.clone();
        marker[0] = 0;
        assert!(read_container(&marker).is_err());
        let mut length = file.clone();
        length[8] ^= 1;
        assert!(read_container(&length).is_err());

        let body = sample().write_body().unwrap();
        // Missing chunk 9.
        let chunks = Chunks::parse(&body).unwrap();
        let last = chunks.find(9).unwrap().offset - 8;
        let err = Save::read_body(&body[..last]).unwrap_err();
        assert_eq!(err.to_string(), "Can't find chunk REGISTRY_CHUNK_DATA!");
        // An empty chunk reads as missing, so it is not written.
        let mut empty = body[..last].to_vec();
        empty.extend([9, 0, 0, 0, 0, 0, 0, 0]);
        let err = Save::read_body(&empty).unwrap_err();
        assert_eq!(err.to_string(), "Can't find chunk REGISTRY_CHUNK_DATA!");
        let mut save = sample();
        save.registries.clear();
        assert!(save.write_body().is_err());
        // Spawn points out of order.
        let mut save = sample();
        save.spawn_binding.spawn_points = vec![0, 2, 1];
        assert!(save.write_body().is_err());
        let mut binding = sample().spawn_binding.write().unwrap()[..28].to_vec();
        binding.extend(1u32.to_le_bytes());
        binding.extend(16u32.to_le_bytes());
        for id in [1u32, 0] {
            binding.extend(id.to_le_bytes());
            binding.extend([0; 4]);
        }
        assert!(SpawnBinding::read(&binding).is_err());
        // A huge declared body length: an error, without reserving it.
        let mut huge = file.clone();
        huge[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read_container(&huge).is_err());
        let mut save = sample();
        save.simulation_version = 1;
        assert!(Save::read_body(&save.write_body().unwrap()).is_err());
        assert!(read_objects(&[1, 0, 0, 0, 5, 0]).is_err());
    }
}
