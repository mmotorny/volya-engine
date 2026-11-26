// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The occlusion mesh of a level, `levels\<level>\level.hom`
//! (docs/formats/level-hom.md section 2): chunk 1 holds 40-byte triangle
//! records. What loading makes of them is in `volya-engine-core::occlusion`.

use std::io;

use crate::chunks::Chunks;

/// The chunk of triangle records; the only one read (section 2).
pub const RECORDS_CHUNK: u32 = 1;
/// Bytes of one triangle record (section 2.1).
pub const RECORD_SIZE: usize = 40;

/// One triangle record (section 2.1).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HomRecord {
    pub vertices: [[f32; 3]; 3],
    /// Non-zero: occludes from either side; 0: only while the camera is on
    /// its front side.
    pub two_sided: u32,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The records of chunk 1 in stored order. A missing or empty chunk 1 is
/// an error (the game is believed to crash, "Gaps found in review"), and so
/// is a size not a multiple of 40 (unestablished in the original; our
/// choice).
pub fn parse(data: &[u8]) -> io::Result<Vec<HomRecord>> {
    let chunks = Chunks::parse(data)?;
    let payload = chunks.require(RECORDS_CHUNK, "level.hom")?.uncompressed()?;
    if payload.is_empty() {
        return Err(invalid("level.hom: chunk 1 is empty"));
    }
    if payload.len() % RECORD_SIZE != 0 {
        return Err(invalid(format!(
            "level.hom: chunk 1 has {} bytes, not a multiple of {RECORD_SIZE}",
            payload.len()
        )));
    }
    let f = |b: &[u8], o: usize| f32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    Ok(payload
        .as_chunks::<RECORD_SIZE>()
        .0
        .iter()
        .map(|r| HomRecord {
            vertices: std::array::from_fn(|v| std::array::from_fn(|k| f(r, 12 * v + 4 * k))),
            two_sided: u32::from_le_bytes(r[36..40].try_into().unwrap()),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(records: &[HomRecord]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend(0u32.to_le_bytes());
        out.extend(4u32.to_le_bytes());
        out.extend(0u32.to_le_bytes());
        out.extend(1u32.to_le_bytes());
        out.extend(((records.len() * RECORD_SIZE) as u32).to_le_bytes());
        for r in records {
            for v in r.vertices.iter().flatten() {
                out.extend(v.to_le_bytes());
            }
            out.extend(r.two_sided.to_le_bytes());
        }
        out
    }

    #[test]
    fn records_are_read_from_chunk_1() {
        let r = [
            HomRecord {
                vertices: [[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]],
                two_sided: 1,
            },
            HomRecord {
                vertices: [[-1.0, 0.5, 0.0], [0.0; 3], [2.0, 2.0, 2.0]],
                two_sided: 0,
            },
        ];
        let data = file(&r);
        // Section 2: 20 + 40 n bytes.
        assert_eq!(data.len(), 20 + 40 * 2);
        assert_eq!(parse(&data).unwrap(), r);
        // Without records, or with a partial one: an error.
        assert!(parse(&file(&[])).is_err());
        let mut bad = file(&r);
        bad.truncate(bad.len() - 4);
        bad[16..20].copy_from_slice(&76u32.to_le_bytes());
        assert!(parse(&bad).is_err());
        assert!(parse(&data[..12]).is_err());
    }
}
