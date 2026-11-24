// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Resources of Windows executables (the public PE/COFF format) and the
//! device-independent bitmaps stored in them (the public BMP/DIB format).
//! The game's splash picture is bitmap resource 116 of `bin\XR_3DA.exe`
//! (docs/behavior/startup.md, section 3).

use std::io;

/// Resource type of bitmaps (`RT_BITMAP`).
pub const RT_BITMAP: u32 = 2;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn u16_at(b: &[u8], o: usize) -> io::Result<u16> {
    b.get(o..o + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or_else(|| invalid("PE: truncated"))
}

fn u32_at(b: &[u8], o: usize) -> io::Result<u32> {
    b.get(o..o + 4)
        .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
        .ok_or_else(|| invalid("PE: truncated"))
}

/// A section: its virtual address range and where its data is in the file.
struct Section {
    virtual_address: u32,
    virtual_size: u32,
    raw_offset: u32,
    raw_size: u32,
}

/// The file offset of relative virtual address `rva`.
fn offset_of(sections: &[Section], rva: u32) -> io::Result<usize> {
    sections
        .iter()
        .find(|s| {
            rva >= s.virtual_address && rva - s.virtual_address < s.virtual_size.max(s.raw_size)
        })
        .map(|s| (s.raw_offset + (rva - s.virtual_address)) as usize)
        .ok_or_else(|| invalid(format!("PE: address {rva:#x} in no section")))
}

/// The data of resource `kind`/`id` (first language) in executable `exe`,
/// or `None` when it has no such resource.
pub fn resource(exe: &[u8], kind: u32, id: u32) -> io::Result<Option<&[u8]>> {
    if exe.get(..2) != Some(b"MZ") {
        return Err(invalid("not a Windows executable"));
    }
    let pe = u32_at(exe, 0x3c)? as usize;
    if exe.get(pe..pe + 4) != Some(b"PE\0\0") {
        return Err(invalid("PE: no signature"));
    }
    let coff = pe + 4;
    let sections_count = u16_at(exe, coff + 2)? as usize;
    let optional_size = u16_at(exe, coff + 16)? as usize;
    let optional = coff + 20;
    let directories = match u16_at(exe, optional)? {
        0x10b => optional + 96,
        0x20b => optional + 112,
        magic => return Err(invalid(format!("PE: optional header magic {magic:#x}"))),
    };
    let count = u32_at(exe, directories - 4)?;
    if count <= 2 {
        return Ok(None);
    }
    let resources_rva = u32_at(exe, directories + 2 * 8)?;
    if resources_rva == 0 {
        return Ok(None);
    }
    let table = optional + optional_size;
    let sections = (0..sections_count)
        .map(|i| {
            let s = table + 40 * i;
            Ok(Section {
                virtual_size: u32_at(exe, s + 8)?,
                virtual_address: u32_at(exe, s + 12)?,
                raw_size: u32_at(exe, s + 16)?,
                raw_offset: u32_at(exe, s + 20)?,
            })
        })
        .collect::<io::Result<Vec<_>>>()?;
    let root = offset_of(&sections, resources_rva)?;

    // A directory: 16-byte header with the counts of named and numbered
    // entries, then 8-byte entries (name or id; offset with the high bit set
    // for a subdirectory, relative to the resource root).
    let find = |dir: usize, want: Option<u32>| -> io::Result<Option<u32>> {
        let named = u16_at(exe, dir + 12)? as usize;
        let ids = u16_at(exe, dir + 14)? as usize;
        for i in 0..named + ids {
            let entry = dir + 16 + 8 * i;
            let name = u32_at(exe, entry)?;
            let target = u32_at(exe, entry + 4)?;
            let matches = match want {
                Some(id) => name & 0x8000_0000 == 0 && name == id,
                None => true,
            };
            if matches {
                return Ok(Some(target));
            }
        }
        Ok(None)
    };
    let subdir = |target: u32| -> io::Result<usize> {
        if target & 0x8000_0000 == 0 {
            return Err(invalid("PE: resource entry is not a directory"));
        }
        Ok(root + (target & 0x7fff_ffff) as usize)
    };
    let Some(types) = find(root, Some(kind))? else {
        return Ok(None);
    };
    let Some(names) = find(subdir(types)?, Some(id))? else {
        return Ok(None);
    };
    let Some(data) = find(subdir(names)?, None)? else {
        return Ok(None);
    };
    if data & 0x8000_0000 != 0 {
        return Err(invalid("PE: resource language entry is a directory"));
    }
    let entry = root + data as usize;
    let start = offset_of(&sections, u32_at(exe, entry)?)?;
    let size = u32_at(exe, entry + 4)? as usize;
    exe.get(start..start + size)
        .map(Some)
        .ok_or_else(|| invalid("PE: resource data truncated"))
}

/// An image decoded to RGBA, 8 bits per channel, rows top to bottom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Decodes a device-independent bitmap as stored in a bitmap resource (a
/// `BITMAPINFOHEADER` and the pixels, without the file header).
/// Uncompressed 24- and 32-bit images only (the splash is 24-bit); others
/// are rejected. Opaque.
pub fn decode_dib(dib: &[u8]) -> io::Result<Rgba> {
    let header = u32_at(dib, 0)? as usize;
    if header < 40 {
        return Err(invalid(format!("DIB header of {header} bytes")));
    }
    let width = u32_at(dib, 4)? as i32;
    let height = u32_at(dib, 8)? as i32;
    let bits = u16_at(dib, 14)?;
    let compression = u32_at(dib, 16)?;
    if width <= 0 || height == 0 || !(bits == 24 || bits == 32) || compression != 0 {
        return Err(invalid(format!(
            "unsupported DIB: {width}×{height}, {bits} bits, compression {compression}"
        )));
    }
    let (width, bottom_up) = (width as u32, height > 0);
    let height = height.unsigned_abs();
    let bytes = usize::from(bits / 8);
    let stride = (width as usize * bytes).div_ceil(4) * 4;
    let data = &dib[header..];
    if data.len() < stride * height as usize {
        return Err(invalid("DIB pixels truncated"));
    }
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height as usize {
        let row = if bottom_up {
            height as usize - 1 - y
        } else {
            y
        };
        for px in data[row * stride..][..width as usize * bytes].chunks(bytes) {
            pixels.extend_from_slice(&[px[2], px[1], px[0], 255]);
        }
    }
    Ok(Rgba {
        width,
        height,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal PE32 image with one section holding a resource tree with
    /// bitmap 116.
    fn executable(dib: &[u8]) -> Vec<u8> {
        let mut exe = vec![0u8; 0x400];
        exe[..2].copy_from_slice(b"MZ");
        exe[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        exe[0x80..0x84].copy_from_slice(b"PE\0\0");
        let coff = 0x84;
        exe[coff + 2..coff + 4].copy_from_slice(&1u16.to_le_bytes());
        exe[coff + 16..coff + 18].copy_from_slice(&224u16.to_le_bytes());
        let opt = coff + 20;
        exe[opt..opt + 2].copy_from_slice(&0x10bu16.to_le_bytes());
        exe[opt + 92..opt + 96].copy_from_slice(&16u32.to_le_bytes());
        // Resource directory at RVA 0x1000.
        exe[opt + 96 + 16..opt + 96 + 20].copy_from_slice(&0x1000u32.to_le_bytes());
        let sec = opt + 224;
        let put = |exe: &mut Vec<u8>, o: usize, v: u32| {
            exe[o..o + 4].copy_from_slice(&v.to_le_bytes());
        };
        put(&mut exe, sec + 8, 0x200);
        put(&mut exe, sec + 12, 0x1000);
        put(&mut exe, sec + 16, 0x200);
        put(&mut exe, sec + 20, 0x200);
        // At file 0x200 (RVA 0x1000): type dir → name dir → language dir →
        // data entry → data.
        let root = 0x200;
        let dir = |exe: &mut Vec<u8>, at: usize, id: u32, target: u32| {
            exe[at + 14..at + 16].copy_from_slice(&1u16.to_le_bytes());
            put(exe, at + 16, id);
            put(exe, at + 20, target);
        };
        dir(&mut exe, root, RT_BITMAP, 0x8000_0018);
        dir(&mut exe, root + 0x18, 116, 0x8000_0030);
        dir(&mut exe, root + 0x30, 1049, 0x48);
        put(&mut exe, root + 0x48, 0x1060);
        put(&mut exe, root + 0x4c, dib.len() as u32);
        exe[root + 0x60..root + 0x60 + dib.len()].copy_from_slice(dib);
        exe
    }

    fn dib() -> Vec<u8> {
        // 2 × 2, 24-bit, bottom-up: rows padded to 8 bytes.
        let mut dib = vec![0u8; 40];
        dib[..4].copy_from_slice(&40u32.to_le_bytes());
        dib[4..8].copy_from_slice(&2u32.to_le_bytes());
        dib[8..12].copy_from_slice(&2u32.to_le_bytes());
        dib[12..14].copy_from_slice(&1u16.to_le_bytes());
        dib[14..16].copy_from_slice(&24u16.to_le_bytes());
        // Bottom row: blue, green; top row: red, white (BGR order).
        dib.extend_from_slice(&[255, 0, 0, 0, 255, 0, 0, 0]);
        dib.extend_from_slice(&[0, 0, 255, 255, 255, 255, 0, 0]);
        dib
    }

    #[test]
    fn finds_bitmap_resource_and_decodes_it() {
        let exe = executable(&dib());
        let data = resource(&exe, RT_BITMAP, 116).unwrap().unwrap();
        assert_eq!(data, dib());
        assert_eq!(resource(&exe, RT_BITMAP, 117).unwrap(), None);
        assert_eq!(resource(&exe, 3, 116).unwrap(), None);
        let image = decode_dib(data).unwrap();
        assert_eq!((image.width, image.height), (2, 2));
        assert_eq!(
            image.pixels,
            [
                255, 0, 0, 255, 255, 255, 255, 255, // top: red, white
                0, 0, 255, 255, 0, 255, 0, 255, // bottom: blue, green
            ]
        );
        assert!(resource(b"no", RT_BITMAP, 116).is_err());
    }
}
