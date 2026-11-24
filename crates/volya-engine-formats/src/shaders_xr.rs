// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The shader library `shaders.xr` (docs/behavior/static-shading.md section
//! 3): definitions with their class tag, version and property stream. The
//! texture-coordinate transform records of chunk 1 are not used by the first
//! renderer's programmable passes and are not read.

use std::io;

use crate::chunks::Chunks;
use crate::cp1251;

/// Chunk holding one sub-chunk per definition.
const DEFINITIONS_CHUNK: u32 = 2;
/// Offset of the property stream in a definition record.
const PROPERTIES_AT: usize = 176;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// A property value (section 3.2's types).
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Marker,
    Transform(String),
    Constant(String),
    Texture(String),
    Integer {
        value: i32,
        min: i32,
        max: i32,
    },
    Boolean(bool),
    Token {
        value: u32,
        tokens: Vec<(u32, String)>,
    },
}

/// One definition (chunk 2 sub-chunk).
#[derive(Clone, Debug, PartialEq)]
pub struct Definition {
    /// The class tag as read (byte order restored), e.g. `LM      `.
    pub tag: String,
    /// The engine shader name.
    pub name: String,
    pub version: u16,
    /// Properties in stored order.
    pub properties: Vec<(String, Value)>,
}

/// A fixed string(n): a zstring padded with arbitrary bytes.
fn fixed(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    cp1251::decode(&b[..end])
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
            .ok_or_else(|| invalid("shaders.xr: property past the definition's end"))?;
        let out = &self.data[self.at..end];
        self.at = end;
        Ok(out)
    }

    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    fn i32(&mut self) -> io::Result<i32> {
        Ok(self.u32()? as i32)
    }

    fn zstring(&mut self) -> io::Result<String> {
        let rest = &self.data[self.at..];
        let end = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| invalid("shaders.xr: unterminated property name"))?;
        self.at += end + 1;
        Ok(cp1251::decode(&rest[..end]))
    }
}

impl Definition {
    pub fn parse(record: &[u8]) -> io::Result<Self> {
        if record.len() < PROPERTIES_AT {
            return Err(invalid("shaders.xr: definition shorter than its header"));
        }
        let mut tag = record[..8].to_vec();
        tag.reverse();
        let tag = cp1251::decode(&tag);
        let name = fixed(&record[8..136]);
        let version = u16::from_le_bytes([record[172], record[173]]);
        let mut r = Reader {
            data: record,
            at: PROPERTIES_AT,
        };
        let mut properties = Vec::new();
        while r.at < record.len() {
            let kind = r.u32()?;
            let name = r.zstring()?;
            let value = match kind {
                0 => Value::Marker,
                1 => Value::Transform(fixed(r.take(64)?)),
                2 => Value::Constant(fixed(r.take(64)?)),
                3 => Value::Texture(fixed(r.take(64)?)),
                4 => Value::Integer {
                    value: r.i32()?,
                    min: r.i32()?,
                    max: r.i32()?,
                },
                6 => Value::Boolean(r.u32()? != 0),
                7 => {
                    let value = r.u32()?;
                    let count = r.u32()?;
                    let tokens = (0..count)
                        .map(|_| Ok((r.u32()?, fixed(r.take(64)?))))
                        .collect::<io::Result<_>>()?;
                    Value::Token { value, tokens }
                }
                other => {
                    return Err(invalid(format!(
                        "shaders.xr: {name}: property type {other}"
                    )));
                }
            };
            properties.push((name, value));
        }
        Ok(Self {
            tag,
            name,
            version,
            properties,
        })
    }

    /// The first property named `name`.
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.properties
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v)
    }

    pub fn integer(&self, name: &str) -> Option<i32> {
        match self.get(name)? {
            Value::Integer { value, .. } => Some(*value),
            _ => None,
        }
    }

    pub fn boolean(&self, name: &str) -> Option<bool> {
        match self.get(name)? {
            Value::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    pub fn token(&self, name: &str) -> Option<u32> {
        match self.get(name)? {
            Value::Token { value, .. } => Some(*value),
            _ => None,
        }
    }

    /// `Priority` of the common block.
    pub fn priority(&self) -> i32 {
        self.integer("Priority").unwrap_or(1)
    }

    /// `Strict sorting` of the common block.
    pub fn strict(&self) -> bool {
        self.boolean("Strict sorting").unwrap_or(false)
    }

    /// The texture `Name` after the marker `Detail map` (class `BmmDold `).
    pub fn detail_texture(&self) -> Option<&str> {
        let marker = self
            .properties
            .iter()
            .position(|(n, v)| n == "Detail map" && *v == Value::Marker)?;
        self.properties[marker + 1..]
            .iter()
            .find_map(|(n, v)| match v {
                Value::Texture(t) if n == "Name" => Some(t.as_str()),
                _ => None,
            })
    }
}

/// The definitions of a `shaders.xr` file.
pub fn parse(file: &[u8]) -> io::Result<Vec<Definition>> {
    let chunks = Chunks::parse(file)?;
    let definitions = chunks.require(DEFINITIONS_CHUNK, "shaders.xr")?.content()?;
    Chunks::parse(&definitions)?
        .list()
        .into_iter()
        .map(|c| Definition::parse(&c.content()?))
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::chunks::chunk;

    fn fixed_bytes(s: &str, n: usize) -> Vec<u8> {
        let mut b = s.as_bytes().to_vec();
        b.resize(n, 0xCD);
        b[s.len()] = 0;
        b
    }

    /// A definition record with `tag` (as read), `name` and `properties`
    /// (type, name, value bytes).
    pub fn record(
        tag: &str,
        name: &str,
        version: u16,
        properties: &[(u32, &str, Vec<u8>)],
    ) -> Vec<u8> {
        let mut r: Vec<u8> = tag.bytes().rev().collect();
        r.extend(fixed_bytes(name, 128));
        r.extend(fixed_bytes("pc", 32));
        r.extend(0u32.to_le_bytes());
        r.extend(version.to_le_bytes());
        r.extend([0, 0]);
        for (kind, name, value) in properties {
            r.extend(kind.to_le_bytes());
            r.extend(name.as_bytes());
            r.push(0);
            r.extend(value);
        }
        r
    }

    pub fn int(v: i32) -> Vec<u8> {
        [v, 0, 255].iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    pub fn boolean(b: bool) -> Vec<u8> {
        u32::from(b).to_le_bytes().to_vec()
    }

    pub fn texture(t: &str) -> Vec<u8> {
        fixed_bytes(t, 64)
    }

    pub fn common(priority: i32, strict: bool) -> Vec<(u32, &'static str, Vec<u8>)> {
        vec![
            (0, "General", vec![]),
            (4, "Priority", int(priority)),
            (6, "Strict sorting", boolean(strict)),
            (0, "Base Texture", vec![]),
            (3, "Name", texture("$base0")),
            (1, "Transform", texture("$null")),
        ]
    }

    #[test]
    fn parses_definitions() {
        let mut aref = common(2, true);
        aref.extend([(4, "Alpha ref", int(0)), (6, "Alpha-blend", boolean(true))]);
        let mut terrain = common(1, false);
        terrain.extend([
            (0, "Detail map", vec![]),
            (3, "Name", texture("detail\\detail_grnd_grass")),
            (1, "Transform", texture("0001")),
        ]);
        let mut token = 1u32.to_le_bytes().to_vec();
        token.extend(2u32.to_le_bytes());
        for (id, name) in [(0u32, "SET"), (1, "BLEND")] {
            token.extend(id.to_le_bytes());
            token.extend(texture(name));
        }
        let mut set = common(1, false);
        set.push((7, "Blending", token));
        let mut defs = Vec::new();
        for (i, r) in [
            record("LM_AREF ", "def_shaders\\def_trans", 1, &aref),
            record("BmmDold ", "levels\\escape_grass", 3, &terrain),
            record("S_SET   ", "effects\\glow", 4, &set),
        ]
        .iter()
        .enumerate()
        {
            defs.extend(chunk(i as u32, r));
        }
        let mut file = chunk(0, &[]);
        file.extend(chunk(2, &defs));
        let defs = parse(&file).unwrap();
        assert_eq!(defs[0].tag, "LM_AREF ");
        assert_eq!(defs[0].name, "def_shaders\\def_trans");
        assert_eq!((defs[0].priority(), defs[0].strict()), (2, true));
        assert_eq!(defs[0].integer("Alpha ref"), Some(0));
        assert_eq!(defs[0].boolean("Alpha-blend"), Some(true));
        assert_eq!(defs[0].detail_texture(), None);
        assert_eq!(defs[1].version, 3);
        assert_eq!(defs[1].detail_texture(), Some("detail\\detail_grnd_grass"));
        assert_eq!(defs[2].token("Blending"), Some(1));
        let mut broken = record("V       ", "x", 0, &[]);
        broken.extend(5u32.to_le_bytes());
        broken.extend(b"y\0");
        assert!(Definition::parse(&broken).is_err());
    }
}
