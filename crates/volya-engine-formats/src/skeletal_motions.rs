// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Skeletal motions (docs/formats/skeletal-motions.md), as far as the
//! lengths of cycles need them (docs/behavior/player-cycle-lengths.md, S10):
//! a model's motion-file list (chunk 19, section 1.2), the motion
//! parameters (chunk 15, section 2) and each motion's key count (the start
//! of chunk 14's sub-chunks, section 3). The tracks are not read: a motion
//! file is walked as a stream, reading only chunk 15 and the head of each
//! motion.

use std::io::{self, Read, Seek, SeekFrom};

use crate::chunks::{self, ChunkHeader};
use crate::cp1251;

/// A model's motion-file list (section 1.2).
pub const MOTION_FILES_CHUNK: u32 = 19;
/// Motion keys and motion parameters (sections 2, 3).
pub const KEYS_CHUNK: u32 = 14;
pub const PARAMS_CHUNK: u32 = 15;
/// The highest version chunk 15 may have (section 2).
pub const MAX_VERSION: u16 = 4;
/// A motion-file list has at most this many items (section 1.2).
pub const MAX_MOTION_FILES: usize = 3;
/// Flags bit 0: an fx motion, otherwise a cycle (section 2).
pub const FX: u16 = 0x1;
/// The scale of the 16-bit playback values (section 2).
pub const PARAM_SCALE: f64 = 655.35;
/// How many bytes of a motion's sub-chunk are read for its name and key
/// count (our choice; the shipped names are far shorter).
const MOTION_HEAD: usize = 1024;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The items of a motion-file list (section 1.2): the chunk's string split
/// at commas, each trimmed of bytes ≤ 0x20. More than 3 is an error.
pub fn motion_files(chunk: &[u8]) -> io::Result<Vec<String>> {
    let end = chunk.iter().position(|&b| b == 0).unwrap_or(chunk.len());
    let text = cp1251::decode(&chunk[..end]);
    if text.is_empty() {
        return Ok(Vec::new());
    }
    let items: Vec<String> = text
        .split(',')
        .map(|i| i.trim_matches(|c: char| c <= ' ').to_owned())
        .collect();
    if items.len() > MAX_MOTION_FILES {
        return Err(invalid(format!(
            "motion-file list '{text}': {} items, at most {MAX_MOTION_FILES}",
            items.len()
        )));
    }
    Ok(items)
}

/// The 16-bit value *q* the program keeps of a stored playback value *x*
/// (section 2): clamp(floor(f32(*x* · 655.35)), 0, 65535).
pub fn quantize(x: f32) -> u16 {
    ((f64::from(x) * PARAM_SCALE) as f32)
        .floor()
        .clamp(0.0, 65535.0) as u16
}

/// A motion definition of chunk 15 (section 2).
#[derive(Clone, Debug, PartialEq)]
pub struct MotionDefinition {
    /// Lower-cased when read.
    pub name: String,
    /// The low 16 bits of the stored flags.
    pub flags: u16,
    /// A cycle's partition (0xFFFF for all) or an fx motion's bone.
    pub bone_or_partition: u16,
    pub motion_index: u16,
    /// The stored playback values.
    pub speed: f32,
    pub power: f32,
    pub accrue: f32,
    pub falloff: f32,
}

impl MotionDefinition {
    /// An fx motion (flags bit 0); otherwise a cycle.
    pub fn is_fx(&self) -> bool {
        self.flags & FX != 0
    }
}

/// A partition (section 2): its name and bone entries (bone name, track
/// index's low 16 bits).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Partition {
    pub name: String,
    pub bones: Vec<(String, u16)>,
}

/// Chunk 15 (section 2).
#[derive(Clone, Debug, PartialEq)]
pub struct MotionParams {
    pub version: u16,
    pub partitions: Vec<Partition>,
    pub definitions: Vec<MotionDefinition>,
}

struct Cursor<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize) -> io::Result<&'a [u8]> {
        let b = self
            .data
            .get(self.at..self.at + n)
            .ok_or_else(|| invalid(format!("motion parameters: ends at {}", self.data.len())))?;
        self.at += n;
        Ok(b)
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

    fn string_z(&mut self) -> io::Result<&'a [u8]> {
        let rest = &self.data[self.at.min(self.data.len())..];
        let end = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| invalid("motion parameters: unterminated string"))?;
        self.at += end + 1;
        Ok(&rest[..end])
    }

    /// A mark-set name (section 2): past the first byte, on to the next CR
    /// or LF or the end, then past every CR or LF. The name's value is
    /// unestablished (section 8); the bytes before the line break are
    /// taken.
    fn line(&mut self) -> String {
        let start = self.at.min(self.data.len());
        let mut i = (start + 1).min(self.data.len());
        while i < self.data.len() && !matches!(self.data[i], b'\r' | b'\n') {
            i += 1;
        }
        let name = cp1251::decode(&self.data[start..i]);
        while i < self.data.len() && matches!(self.data[i], b'\r' | b'\n') {
            i += 1;
        }
        self.at = i;
        name
    }
}

fn lower(bytes: &[u8]) -> String {
    cp1251::decode(&bytes.to_ascii_lowercase())
}

/// Parses chunk 15 (section 2). A version above 4 is an error.
pub fn parse_params(chunk: &[u8]) -> io::Result<MotionParams> {
    let mut c = Cursor { data: chunk, at: 0 };
    let version = c.u16()?;
    if version > MAX_VERSION {
        return Err(invalid(format!("Invalid OGF/OMF version: {version}")));
    }
    let partitions = (0..c.u16()?)
        .map(|_| {
            let name = lower(c.string_z()?);
            let bones = (0..c.u16()?)
                .map(|_| {
                    let bone = cp1251::decode(c.string_z()?);
                    Ok((bone, c.u32()? as u16))
                })
                .collect::<io::Result<_>>()?;
            Ok(Partition { name, bones })
        })
        .collect::<io::Result<_>>()?;
    let definitions = (0..c.u16()?)
        .map(|_| {
            let d = MotionDefinition {
                name: lower(c.string_z()?),
                flags: c.u32()? as u16,
                bone_or_partition: c.u16()?,
                motion_index: c.u16()?,
                speed: c.f32()?,
                power: c.f32()?,
                accrue: c.f32()?,
                falloff: c.f32()?,
            };
            if version >= 4 {
                for _ in 0..c.u32()? {
                    c.line();
                    let m = c.u32()? as usize;
                    c.take(8 * m)?;
                }
            }
            Ok(d)
        })
        .collect::<io::Result<_>>()?;
    Ok(MotionParams {
        version,
        partitions,
        definitions,
    })
}

/// A motion set's parameters and each definition's key count *N*.
#[derive(Clone, Debug, PartialEq)]
pub struct MotionSet {
    pub params: MotionParams,
    /// *N* of the motion of definition *k* (sub-chunk *k* + 1 of chunk 14).
    pub key_counts: Vec<u32>,
}

fn read_at(reader: &mut (impl Read + Seek), offset: u64, out: &mut [u8]) -> io::Result<()> {
    reader.seek(SeekFrom::Start(offset))?;
    reader.read_exact(out)
}

/// The key counts of chunk 14 (section 3) at `header`, for `count`
/// definitions: the head of each sub-chunk *k* + 1, found by id, read as
/// the name `stringZ` and the `u32` key count. A compressed chunk is
/// decoded (chunks.md section 5), the whole chunk or one motion.
fn key_counts(
    reader: &mut (impl Read + Seek),
    header: &ChunkHeader,
    count: usize,
) -> io::Result<Vec<u32>> {
    if header.is_compressed() {
        let content = header.read_content(reader)?;
        let whole = ChunkHeader {
            id_word: header.id(),
            offset: 0,
            size: content.len() as u32,
        };
        return key_counts(&mut io::Cursor::new(content), &whole, count);
    }
    let end = header.offset + u64::from(header.size);
    let mut found: Vec<Option<ChunkHeader>> = vec![None; count + 1];
    let mut offset = header.offset;
    while offset < end {
        if end - offset < 8 {
            return Err(invalid("motion keys: bytes left over after the last chunk"));
        }
        let mut h = [0u8; 8];
        read_at(reader, offset, &mut h)?;
        let id_word = u32::from_le_bytes(h[..4].try_into().unwrap());
        let size = u32::from_le_bytes(h[4..].try_into().unwrap());
        if u64::from(size) > end - offset - 8 {
            return Err(invalid(format!(
                "motion keys: chunk at {offset} claims {size} bytes"
            )));
        }
        let sub = ChunkHeader {
            id_word,
            offset: offset + 8,
            size,
        };
        if let Some(slot) = found.get_mut(sub.id() as usize)
            && slot.is_none()
        {
            *slot = Some(sub);
        }
        offset += 8 + u64::from(size);
    }
    let mut buffer = vec![0u8; MOTION_HEAD];
    (1..=count)
        .map(|id| {
            let sub = found[id].ok_or_else(|| invalid(format!("motion keys: no motion {id}")))?;
            let decoded;
            let head = if sub.is_compressed() {
                decoded = sub.read_content(reader)?;
                &decoded[..decoded.len().min(MOTION_HEAD)]
            } else {
                let n = (sub.size as usize).min(MOTION_HEAD);
                read_at(reader, sub.offset, &mut buffer[..n])?;
                &buffer[..n]
            };
            let name_end = head
                .iter()
                .position(|&b| b == 0)
                .ok_or_else(|| invalid(format!("motion {id}: name not terminated")))?;
            let at = name_end + 1;
            let keys = head
                .get(at..at + 4)
                .ok_or_else(|| invalid(format!("motion {id}: no key count")))?;
            Ok(u32::from_le_bytes(keys.try_into().unwrap()))
        })
        .collect()
}

/// A motion set from a chunk sequence: chunk 15 (required: "Old skinned
/// model version unsupported!") and the key counts of chunk 14.
fn motion_set(
    reader: &mut (impl Read + Seek),
    headers: &[ChunkHeader],
    what: &str,
) -> io::Result<MotionSet> {
    let params = chunks::find_header(headers, PARAMS_CHUNK)
        .ok_or_else(|| invalid(format!("Old skinned model version unsupported! ({what})")))?;
    let params = parse_params(&params.read_content(reader)?)?;
    let keys = chunks::find_header(headers, KEYS_CHUNK)
        .ok_or_else(|| invalid(format!("{what}: no motion keys")))?;
    let key_counts = key_counts(reader, keys, params.definitions.len())?;
    Ok(MotionSet { params, key_counts })
}

/// A motion file (`.omf`, section 1.3), read as a stream.
pub fn read_motion_file(reader: &mut (impl Read + Seek), what: &str) -> io::Result<MotionSet> {
    let headers = chunks::index(reader)?;
    motion_set(reader, &headers, what)
}

/// Where a model's motions come from (section 1.1).
#[derive(Clone, Debug, PartialEq)]
pub enum ModelMotions {
    /// The items of its motion-file list (chunk 19).
    Files(Vec<String>),
    /// Its own chunks 14 and 15.
    Embedded(MotionSet),
    /// Neither.
    None,
}

/// The motions of a model file (section 1.1), read as a stream: its
/// motion-file list if it has chunk 19, else its embedded motions.
pub fn read_model_motions(reader: &mut (impl Read + Seek), what: &str) -> io::Result<ModelMotions> {
    let headers = chunks::index(reader)?;
    if let Some(list) = chunks::find_header(&headers, MOTION_FILES_CHUNK) {
        return Ok(ModelMotions::Files(motion_files(
            &list.read_content(reader)?,
        )?));
    }
    if chunks::find_header(&headers, PARAMS_CHUNK).is_some() {
        return Ok(ModelMotions::Embedded(motion_set(reader, &headers, what)?));
    }
    Ok(ModelMotions::None)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::chunks::chunk;

    /// A version-3 chunk 15 with one partition and `definitions` (name,
    /// flags, speed).
    pub(crate) fn params(definitions: &[(&str, u32, f32)]) -> Vec<u8> {
        let mut b = 3u16.to_le_bytes().to_vec();
        b.extend(1u16.to_le_bytes());
        b.extend(b"Default\0");
        b.extend(1u16.to_le_bytes());
        b.extend(b"bip01\0");
        b.extend(0u32.to_le_bytes());
        b.extend((definitions.len() as u16).to_le_bytes());
        for (k, (name, flags, speed)) in definitions.iter().enumerate() {
            b.extend(name.as_bytes());
            b.push(0);
            b.extend(flags.to_le_bytes());
            b.extend(0xFFFFu16.to_le_bytes());
            b.extend((k as u16).to_le_bytes());
            for v in [*speed, 1.0, 2.0, 2.0] {
                b.extend(v.to_le_bytes());
            }
        }
        b
    }

    /// A motion file: chunk 14 with sub-chunk 0 and one motion per key
    /// count (its name and count, then some track bytes), then chunk 15.
    pub(crate) fn motion_file(definitions: &[(&str, u32, f32)], keys: &[u32]) -> Vec<u8> {
        let mut c14 = chunk(0, &(keys.len() as u32).to_le_bytes());
        for (k, (n, (name, _, _))) in keys.iter().zip(definitions).enumerate() {
            let mut m = name.as_bytes().to_vec();
            m.push(0);
            m.extend(n.to_le_bytes());
            m.extend([0u8; 13]);
            c14.extend(chunk(k as u32 + 1, &m));
        }
        let mut out = chunk(KEYS_CHUNK, &c14);
        out.extend(chunk(PARAMS_CHUNK, &params(definitions)));
        out
    }

    #[test]
    fn quantizes_as_the_program_keeps_values() {
        // player-cycle-lengths.md 3, 5.2: q of 1, 1.5 and 2.
        assert_eq!(quantize(1.0), 655);
        assert_eq!(quantize(1.5), 983);
        assert_eq!(quantize(2.0), 1310);
        assert_eq!(quantize(-1.0), 0);
        assert_eq!(quantize(1000.0), 65535);
    }

    #[test]
    fn splits_motion_file_lists() {
        assert_eq!(
            motion_files(b"actors\\stalker_animation\0").unwrap(),
            ["actors\\stalker_animation"]
        );
        assert_eq!(motion_files(b"a, b ,c\0").unwrap(), ["a", "b", "c"]);
        assert!(motion_files(b"a,b,c,d\0").is_err());
    }

    #[test]
    fn reads_definitions_and_key_counts_from_a_stream() {
        let defs = [
            ("Norm_Walk_Fwd_0", 8, 1.0),
            ("fx_hit", 1, 1.0),
            ("stop", 2, 1.5),
        ];
        let file = motion_file(&defs, &[26, 5, 40]);
        let set = read_motion_file(&mut std::io::Cursor::new(&file), "test").unwrap();
        assert_eq!(set.key_counts, [26, 5, 40]);
        let d = &set.params.definitions;
        assert_eq!(d[0].name, "norm_walk_fwd_0");
        assert_eq!((d[0].flags, d[0].speed), (8, 1.0));
        assert!(d[1].is_fx() && !d[0].is_fx());
        assert_eq!(set.params.partitions[0].name, "default");
        assert_eq!(set.params.partitions[0].bones, [("bip01".to_owned(), 0)]);
        // A model with chunk 19 lists files; without either, none.
        let model = chunk(MOTION_FILES_CHUNK, b"actors\\stalker_animation\0");
        assert_eq!(
            read_model_motions(&mut std::io::Cursor::new(&model), "m").unwrap(),
            ModelMotions::Files(vec!["actors\\stalker_animation".into()])
        );
        let embedded = read_model_motions(&mut std::io::Cursor::new(&file), "m").unwrap();
        assert!(matches!(embedded, ModelMotions::Embedded(s) if s.key_counts == [26, 5, 40]));
        let bare = chunk(1, &[0; 4]);
        assert_eq!(
            read_model_motions(&mut std::io::Cursor::new(&bare), "m").unwrap(),
            ModelMotions::None
        );
    }

    #[test]
    fn decodes_compressed_chunks() {
        // chunks.md section 5: the content of a compressed chunk is decoded,
        // for the whole chunk 14, one motion of it, chunk 15 and chunk 19.
        use crate::chunks::COMPRESSED;
        use crate::db::encode_literals;
        let defs = [("walk", 8, 1.0), ("run", 8, 1.0)];
        let plain = motion_file(&defs, &[26, 40]);
        let chunks = crate::chunks::Chunks::parse(&plain).unwrap();
        let c14 = chunks.find(KEYS_CHUNK).unwrap().payload;
        let c15 = chunks.find(PARAMS_CHUNK).unwrap().payload;
        let mut file = chunk(COMPRESSED | KEYS_CHUNK, &encode_literals(c14));
        file.extend(chunk(COMPRESSED | PARAMS_CHUNK, &encode_literals(c15)));
        let set = read_motion_file(&mut std::io::Cursor::new(&file), "t").unwrap();
        assert_eq!(set.key_counts, [26, 40]);
        assert_eq!(set.params.definitions[1].name, "run");
        let mut motion = b"run\0".to_vec();
        motion.extend(40u32.to_le_bytes());
        let mut c14 = chunk(0, &2u32.to_le_bytes());
        c14.extend(chunk(1, b"walk\0\x1a\0\0\0"));
        c14.extend(chunk(COMPRESSED | 2, &encode_literals(&motion)));
        let mut file = chunk(KEYS_CHUNK, &c14);
        file.extend(chunk(PARAMS_CHUNK, c15));
        let set = read_motion_file(&mut std::io::Cursor::new(&file), "t").unwrap();
        assert_eq!(set.key_counts, [26, 40]);
        let model = chunk(COMPRESSED | MOTION_FILES_CHUNK, &encode_literals(b"a,b\0"));
        assert_eq!(
            read_model_motions(&mut std::io::Cursor::new(&model), "m").unwrap(),
            ModelMotions::Files(vec!["a".into(), "b".into()])
        );
    }

    #[test]
    fn refuses_versions_above_4_and_missing_parameters() {
        let mut p = params(&[("a", 0, 1.0)]);
        p[..2].copy_from_slice(&5u16.to_le_bytes());
        assert!(parse_params(&p).is_err());
        let only_keys = chunk(KEYS_CHUNK, &chunk(0, &0u32.to_le_bytes()));
        assert!(read_motion_file(&mut std::io::Cursor::new(&only_keys), "t").is_err());
    }
}
