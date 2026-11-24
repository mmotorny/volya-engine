// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Environment modifiers, `levels\<level>\level.env_mod`, only as far as the
//! camera's far plane needs (docs/behavior/start-position.md, section 7.1):
//! one 76-byte chunk of 19 floats per modifier.

use std::io;

use crate::chunks::Chunks;

/// Bytes of one modifier: 19 floats.
const MODIFIER_SIZE: usize = 76;

/// One environment modifier.
#[derive(Clone, Debug, PartialEq)]
pub struct EnvModifier {
    /// Id of the modifier's chunk.
    pub id: u32,
    pub position: [f32; 3],
    pub radius: f32,
    pub power: f32,
    pub far_plane: f32,
    /// The remaining 13 floats (fog and colours; environment specification).
    pub rest: [f32; 13],
}

/// The modifiers of a `level.env_mod` file. The spec does not say how the
/// chunks are enumerated; we take every top-level chunk in stored order.
pub fn parse(file: &[u8]) -> io::Result<Vec<EnvModifier>> {
    Chunks::parse(file)?
        .iter()
        .map(|c| {
            let data = c.content()?;
            if data.len() != MODIFIER_SIZE {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("environment modifier {}: {} bytes", c.id(), data.len()),
                ));
            }
            let f: Vec<f32> = data
                .as_chunks::<4>()
                .0
                .iter()
                .map(|b| f32::from_le_bytes(*b))
                .collect();
            Ok(EnvModifier {
                id: c.id(),
                position: [f[0], f[1], f[2]],
                radius: f[3],
                power: f[4],
                far_plane: f[5],
                rest: f[6..].try_into().unwrap(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunks::chunk;

    #[test]
    fn parses_modifiers() {
        let floats: Vec<u8> = (0..19).flat_map(|i| (i as f32).to_le_bytes()).collect();
        let mut file = chunk(3, &floats);
        file.extend(chunk(1, &floats));
        let m = parse(&file).unwrap();
        assert_eq!(m.len(), 2);
        assert_eq!((m[0].id, m[1].id), (3, 1));
        assert_eq!(m[0].position, [0.0, 1.0, 2.0]);
        assert_eq!((m[0].radius, m[0].power, m[0].far_plane), (3.0, 4.0, 5.0));
        assert_eq!(m[0].rest[12], 18.0);
        assert!(parse(&chunk(0, &floats[..72])).is_err());
    }
}
