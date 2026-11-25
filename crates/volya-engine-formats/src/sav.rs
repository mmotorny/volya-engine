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
    /// Sub-chunk 1 as stored, `None` when missing. The game does not read
    /// it (section 6), so whatever it holds is kept and written back.
    pub spawn_points: Option<Vec<u8>>,
}

/// Sub-chunks of the spawn binding.
const BINDING_NAME_CHUNK: u32 = 0;
const BINDING_POINTS_CHUNK: u32 = 1;

impl SpawnBinding {
    /// The binding of a spawn file with `objects` object records: sub-chunk
    /// 1 holds one empty chunk per record, ids 0 … `objects` − 1 (section 6,
    /// believed).
    pub fn new(spawn_name: &str, identifier: [u8; 16], objects: u32) -> Self {
        let points = (0..objects)
            .flat_map(|id| [id.to_le_bytes(), [0; 4]].concat())
            .collect();
        Self {
            spawn_name: spawn_name.to_owned(),
            identifier,
            spawn_points: Some(points),
        }
    }

    /// Reads chunk 1's payload as the game does: sub-chunk 0's name and
    /// identifier (bytes after them are not read), and sub-chunk 1 kept as
    /// stored, unread (section 6).
    pub fn read(payload: &[u8]) -> io::Result<Self> {
        let chunks = Chunks::parse(payload)?;
        let name = chunks.require(BINDING_NAME_CHUNK, "spawn binding")?;
        let mut r = Reader::new(name.uncompressed()?);
        let spawn_name = r.string()?;
        let identifier = r.take(16)?.try_into().unwrap();
        Ok(Self {
            spawn_name,
            identifier,
            spawn_points: chunks
                .find(BINDING_POINTS_CHUNK)
                .map(|c| c.payload.to_vec()),
        })
    }

    pub fn write(&self) -> io::Result<Vec<u8>> {
        let mut name = Writer::new();
        name.string(&self.spawn_name)?;
        name.bytes(&self.identifier);
        let mut out = chunk_bytes(BINDING_NAME_CHUNK, &name.into_inner())?;
        if let Some(points) = &self.spawn_points {
            out.extend(chunk_bytes(BINDING_POINTS_CHUNK, points)?);
        }
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

/// One record's two packets (section 7.1).
fn read_record(r: &mut Reader) -> io::Result<RawRecord> {
    let n = usize::from(r.u16()?);
    let spawn = r.take(n)?.to_vec();
    let n = usize::from(r.u16()?);
    let update = r.take(n)?.to_vec();
    Ok(RawRecord { spawn, update })
}

/// Chunk 2: `u32` count, then the records. Bytes after the last record are
/// not read (section 7.1 counts the records; nothing checks the chunk's
/// end).
pub fn read_objects(payload: &[u8]) -> io::Result<Vec<RawRecord>> {
    Reader::new(payload).list(read_record)
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

/// Chunk `id` of a body's chunks; a missing or empty one is an error naming
/// it (section 3). A compressed one is rejected (no chunk of a save is stored
/// compressed; our choice).
fn body_chunk<'a>(chunks: &Chunks<'a>, id: u32) -> io::Result<&'a [u8]> {
    match chunks.find(id) {
        Some(c) if !c.payload.is_empty() => c.uncompressed(),
        _ => Err(invalid(missing(id))),
    }
}

/// Chunk 5's fields; bytes after them are not read.
fn read_game_time(payload: &[u8]) -> io::Result<GameTime> {
    let mut r = Reader::new(payload);
    Ok(GameTime {
        time: r.u64()?,
        time_factor: r.float()?,
        normal_time_factor: r.float()?,
    })
}

/// What the load menu reads of a save (section 11.4): chunk 5's game time
/// and the first object record of chunk 2.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuData {
    pub game_time: GameTime,
    /// The first record, which must be the player's.
    pub first: RawRecord,
}

impl MenuData {
    /// Reads a valid `.sav` file as the load menu does: of the body, only
    /// chunk 5 and chunk 2's first record; the other chunks and records are
    /// not read, so nothing in them can fail this.
    pub fn read(file: &[u8]) -> io::Result<Self> {
        let body = read_container(file)?;
        let chunks = Chunks::parse(&body)?;
        let game_time = read_game_time(body_chunk(&chunks, GAME_TIME_CHUNK)?)?;
        let mut r = Reader::new(body_chunk(&chunks, OBJECTS_CHUNK)?);
        if r.u32()? == 0 {
            return Err(invalid("saved game without objects"));
        }
        Ok(Self {
            game_time,
            first: read_record(&mut r)?,
        })
    }
}

impl Save {
    /// Reads a decompressed body as loading does (section 11.2). Each chunk
    /// is found by id; a missing or empty one is an error (section 3). Bytes
    /// after a chunk's fields are not read.
    pub fn read_body(body: &[u8]) -> io::Result<Self> {
        let chunks = Chunks::parse(body)?;
        let get = |id| body_chunk(&chunks, id);
        let simulation_version = Reader::new(get(SIMULATION_HEADER_CHUNK)?).u32()?;
        if simulation_version < 2 {
            return Err(invalid(
                "ALife version mismatch! (Delete saved game and try again)",
            ));
        }
        let game_time = read_game_time(get(GAME_TIME_CHUNK)?)?;
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
        // A huge declared body length: an error, without reserving it.
        let mut huge = file.clone();
        huge[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read_container(&huge).is_err());
        let mut save = sample();
        save.simulation_version = 1;
        assert!(Save::read_body(&save.write_body().unwrap()).is_err());
        assert!(read_objects(&[1, 0, 0, 0, 5, 0]).is_err());
        // The load menu fails on what it reads: a missing chunk 5 or 2, a
        // short chunk 5, no first record.
        let without = |id: u32| {
            let chunks = Chunks::parse(&body).unwrap();
            let mut out = Vec::new();
            for c in chunks.iter().filter(|c| c.id() != id) {
                out.extend(chunk_bytes(c.id(), c.payload).unwrap());
            }
            write_container(&out).unwrap()
        };
        for (id, text) in [
            (5, "Can't find chunk GAME_TIME_CHUNK_DATA!"),
            (2, "Can't find chunk OBJECT_CHUNK_DATA!"),
        ] {
            let err = MenuData::read(&without(id)).unwrap_err();
            assert_eq!(err.to_string(), text);
        }
        let mut save = sample();
        save.objects.clear();
        assert!(MenuData::read(&save.write().unwrap()).is_err());
    }

    /// A body with chunk `id`'s payload replaced by `payload`.
    fn with_chunk(save: &Save, id: u32, payload: &[u8]) -> Vec<u8> {
        let body = save.write_body().unwrap();
        let mut out = Vec::new();
        for c in Chunks::parse(&body).unwrap().iter() {
            let p = if c.id() == id { payload } else { c.payload };
            out.extend(chunk_bytes(c.id(), p).unwrap());
        }
        out
    }

    /// What the game does not check is accepted: bytes after the fields of
    /// chunks 0, 5 and 2 and after the spawn binding's identifier, and
    /// anything in the unread sub-chunk 1, or none (sections 3–7.1).
    #[test]
    fn accepts_what_the_game_does_not_check() {
        let save = sample();
        let extend = |id: u32, extra: &[u8]| {
            let body = save.write_body().unwrap();
            let chunks = Chunks::parse(&body).unwrap();
            let mut p = chunks.find(id).unwrap().payload.to_vec();
            p.extend(extra);
            with_chunk(&save, id, &p)
        };
        for id in [SIMULATION_HEADER_CHUNK, GAME_TIME_CHUNK, OBJECTS_CHUNK] {
            let read = Save::read_body(&extend(id, b"more")).unwrap();
            assert_eq!(read, save, "chunk {id}");
        }
        let file = write_container(&extend(GAME_TIME_CHUNK, b"more")).unwrap();
        assert_eq!(MenuData::read(&file).unwrap().game_time, save.game_time);

        // Bytes after the identifier.
        let mut name = b"all\0".to_vec();
        name.extend([7; 16]);
        name.extend(b"after");
        let mut binding = chunk_bytes(BINDING_NAME_CHUNK, &name).unwrap();
        // Sub-chunk 1 that is no sequence of empty, ordered chunks.
        let mut points = chunk_bytes(2, b"x").unwrap();
        points.extend(chunk_bytes(1, &[]).unwrap());
        points.extend([1, 2, 3]);
        binding.extend(chunk_bytes(BINDING_POINTS_CHUNK, &points).unwrap());
        let read = SpawnBinding::read(&binding).unwrap();
        assert_eq!(
            (read.spawn_name.as_str(), read.identifier),
            ("all", [7; 16])
        );
        assert_eq!(read.spawn_points.as_deref(), Some(&points[..]));
        // Written back without the unread bytes after the identifier.
        assert_eq!(SpawnBinding::read(&read.write().unwrap()).unwrap(), read);
        // No sub-chunk 1.
        let only_name = chunk_bytes(BINDING_NAME_CHUNK, &name).unwrap();
        let read = SpawnBinding::read(&only_name).unwrap();
        assert_eq!(read.spawn_points, None);
        assert_eq!(read.write().unwrap().len(), 8 + 20);

        // The load menu reads neither chunk 0, 1 or 9 nor the records after
        // the first.
        let body = with_chunk(&save, SPAWN_BINDING_CHUNK, b"garbage");
        let chunks = Chunks::parse(&body).unwrap();
        let mut trimmed = Vec::new();
        for c in chunks.iter().filter(|c| ![0, 9].contains(&c.id())) {
            let p: &[u8] = if c.id() == OBJECTS_CHUNK {
                // Count 2, then the first record and a truncated second.
                &c.payload[..c.payload.len() - 100]
            } else {
                c.payload
            };
            trimmed.extend(chunk_bytes(c.id(), p).unwrap());
        }
        assert!(Save::read_body(&trimmed).is_err());
        let menu = MenuData::read(&write_container(&trimmed).unwrap()).unwrap();
        assert_eq!(menu.game_time, save.game_time);
        assert_eq!(menu.first, save.objects[0]);
    }
}
