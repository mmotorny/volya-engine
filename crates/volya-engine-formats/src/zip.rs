// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! A ZIP archive's writer, its files stored without compression (PKWARE's
//! APPNOTE.TXT: local file headers, the central directory and its end
//! record; no ZIP64, so under 4 GiB): Volya Engine's own downloads, such as
//! the logs.

/// One file of an archive: its name (`/` between folders) and contents.
pub struct Entry<'a> {
    pub name: &'a str,
    pub bytes: &'a [u8],
}

/// A local date and time, as an archive's entries carry it (MS-DOS form:
/// years 1980–2107, seconds in steps of two).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modified {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl Modified {
    /// The MS-DOS time and date fields.
    fn dos(&self) -> (u16, u16) {
        let time =
            u16::from(self.hour) << 11 | u16::from(self.minute) << 5 | u16::from(self.second / 2);
        let year = self.year.clamp(1980, 2107) - 1980;
        let date = year << 9 | u16::from(self.month) << 5 | u16::from(self.day);
        (time, date)
    }
}

/// Bit 11 of the flags: the name is UTF-8.
const UTF8: u16 = 1 << 11;

/// CRC-32 as ZIP uses it (APPNOTE 4.4.7: polynomial 0xEDB88320,
/// reflected, starting from and ending with all bits inverted).
pub fn crc32(bytes: &[u8]) -> u32 {
    const TABLE: [u32; 256] = {
        let mut table = [0u32; 256];
        let mut i = 0;
        while i < 256 {
            let mut c = i as u32;
            let mut k = 0;
            while k < 8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
                k += 1;
            }
            table[i] = c;
            i += 1;
        }
        table
    };
    !bytes.iter().fold(!0u32, |c, &b| {
        TABLE[((c ^ u32::from(b)) & 0xff) as usize] ^ (c >> 8)
    })
}

/// The archive of `entries`, in their order, each stored as it is.
pub fn store(entries: &[Entry], modified: Modified) -> Vec<u8> {
    let (time, date) = modified.dos();
    let mut out = Vec::new();
    let mut central = Vec::new();
    for entry in entries {
        let offset = out.len() as u32;
        let crc = crc32(entry.bytes);
        let size = entry.bytes.len() as u32;
        let name = entry.name.as_bytes();
        let flags = if entry.name.is_ascii() { 0 } else { UTF8 };
        // The fields both headers share, from "version needed" on.
        let mut common = Vec::new();
        for v in [10u16, flags, 0, time, date] {
            common.extend(v.to_le_bytes());
        }
        for v in [crc, size, size] {
            common.extend(v.to_le_bytes());
        }
        common.extend((name.len() as u16).to_le_bytes());
        common.extend(0u16.to_le_bytes());

        out.extend(0x0403_4b50u32.to_le_bytes());
        out.extend(&common);
        out.extend(name);
        out.extend(entry.bytes);

        central.extend(0x0201_4b50u32.to_le_bytes());
        // Made by: MS-DOS attributes, APPNOTE 2.0.
        central.extend(20u16.to_le_bytes());
        central.extend(&common);
        // Comment length, disk number, internal and external attributes.
        for v in [0u16, 0, 0] {
            central.extend(v.to_le_bytes());
        }
        central.extend(0u32.to_le_bytes());
        central.extend(offset.to_le_bytes());
        central.extend(name);
    }
    let at = out.len() as u32;
    let count = entries.len() as u16;
    out.extend(&central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    for v in [0u16, 0, count, count] {
        out.extend(v.to_le_bytes());
    }
    out.extend((central.len() as u32).to_le_bytes());
    out.extend(at.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u16_at(b: &[u8], at: usize) -> u16 {
        u16::from_le_bytes([b[at], b[at + 1]])
    }

    fn u32_at(b: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
    }

    #[test]
    fn an_archive_reads_back_through_its_central_directory() {
        let modified = Modified {
            year: 2026,
            month: 11,
            day: 27,
            hour: 13,
            minute: 5,
            second: 9,
        };
        let entries = [
            Entry {
                name: "a.log",
                bytes: b"line\r\n",
            },
            Entry {
                name: "b.json",
                bytes: b"",
            },
        ];
        let zip = store(&entries, modified);
        let end = zip.len() - 22;
        assert_eq!(u32_at(&zip, end), 0x0605_4b50);
        assert_eq!(u16_at(&zip, end + 10), 2);
        let mut at = u32_at(&zip, end + 16) as usize;
        assert_eq!(at + u32_at(&zip, end + 12) as usize, end);
        for entry in &entries {
            assert_eq!(u32_at(&zip, at), 0x0201_4b50);
            assert_eq!(u16_at(&zip, at + 10), 0, "stored");
            assert_eq!(u16_at(&zip, at + 12), 13 << 11 | 5 << 5 | 4);
            assert_eq!(u16_at(&zip, at + 14), 46 << 9 | 11 << 5 | 27);
            assert_eq!(u32_at(&zip, at + 16), crc32(entry.bytes));
            let size = u32_at(&zip, at + 20) as usize;
            let name_len = u16_at(&zip, at + 28) as usize;
            assert_eq!(&zip[at + 46..at + 46 + name_len], entry.name.as_bytes());
            let local = u32_at(&zip, at + 42) as usize;
            assert_eq!(u32_at(&zip, local), 0x0403_4b50);
            assert_eq!(zip[local + 14..local + 26], zip[at + 16..at + 28]);
            let data = local + 30 + u16_at(&zip, local + 26) as usize;
            assert_eq!(&zip[data..data + size], entry.bytes);
            at += 46 + name_len;
        }
        // CRC-32's check value, and crc32fast's agreeing.
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        assert_eq!(crc32(b"line\r\n"), crc32fast::hash(b"line\r\n"));
    }
}
