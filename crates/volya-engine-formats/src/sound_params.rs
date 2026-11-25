// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The sound parameters a sound file carries in its Vorbis comment header
//! (docs/formats/sound-ogg-params.md sections 3 and 4).
//!
//! The comment header is the second Vorbis packet of the Ogg stream (the
//! public Vorbis I format): a vendor string and a list of user comments,
//! each a `u32` little-endian length and that many bytes. The parameters are
//! the first user comment, a binary record whose first field selects its
//! layout.

use std::io::{self, Read, Seek};

use ogg::PacketReader;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The parameters of a sound file (section 3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundParams {
    pub min_distance: f32,
    pub max_distance: f32,
    pub base_volume: f32,
    /// The AI sound type's bit flags (section 7.2).
    pub ai_sound_type: u32,
    pub max_ai_distance: f32,
}

/// The parameters of a file without a usable record (section 4).
pub const DEFAULT_PARAMS: SoundParams = SoundParams {
    min_distance: 1.0,
    max_distance: 300.0,
    base_volume: 1.0,
    ai_sound_type: 0,
    max_ai_distance: 300.0,
};

impl Default for SoundParams {
    fn default() -> Self {
        DEFAULT_PARAMS
    }
}

/// Why a file's record gives no parameters, so the defaults apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordProblem {
    /// The comment header has no user comment (section 4).
    NoComment,
    /// The record's version is not 1, 2 or 3 (section 4); `None` when the
    /// comment is too short to hold one.
    UnknownVersion(Option<u32>),
    /// The record is shorter than its version's size. The spec leaves this
    /// unestablished (section 3); we treat it like an unknown version.
    Short { version: u32, len: usize },
}

impl std::fmt::Display for RecordProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RecordProblem::NoComment => write!(f, "no sound parameters"),
            RecordProblem::UnknownVersion(Some(v)) => {
                write!(f, "unknown sound parameter version {v}")
            }
            RecordProblem::UnknownVersion(None) => write!(f, "no sound parameter version"),
            RecordProblem::Short { version, len } => {
                write!(
                    f,
                    "sound parameters version {version} too short ({len} bytes)"
                )
            }
        }
    }
}

/// Reads the parameter record `comment` (the first user comment's bytes).
pub fn parse_record(comment: &[u8]) -> Result<SoundParams, RecordProblem> {
    let u32_at = |at: usize| {
        comment
            .get(at..at + 4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    };
    let f32_at = |at: usize| u32_at(at).map(f32::from_bits);
    let version = u32_at(0).ok_or(RecordProblem::UnknownVersion(None))?;
    let size = match version {
        1 => 16,
        2 => 20,
        3 => 24,
        v => return Err(RecordProblem::UnknownVersion(Some(v))),
    };
    if comment.len() < size {
        return Err(RecordProblem::Short {
            version,
            len: comment.len(),
        });
    }
    let field = |at| f32_at(at).expect("length checked");
    let min_distance = field(4);
    let max_distance = field(8);
    Ok(match version {
        1 => SoundParams {
            min_distance,
            max_distance,
            base_volume: 1.0,
            ai_sound_type: u32_at(12).expect("length checked"),
            max_ai_distance: max_distance,
        },
        2 => SoundParams {
            min_distance,
            max_distance,
            base_volume: field(12),
            ai_sound_type: u32_at(16).expect("length checked"),
            max_ai_distance: max_distance,
        },
        _ => SoundParams {
            min_distance,
            max_distance,
            base_volume: field(12),
            ai_sound_type: u32_at(16).expect("length checked"),
            max_ai_distance: field(20),
        },
    })
}

/// The parameters of a file whose first user comment is `comment` (`None`
/// when it has none): the record, or the defaults and why (section 4).
pub fn params(comment: Option<&[u8]>) -> (SoundParams, Option<RecordProblem>) {
    match comment
        .ok_or(RecordProblem::NoComment)
        .and_then(parse_record)
    {
        Ok(params) => (params, None),
        Err(problem) => (DEFAULT_PARAMS, Some(problem)),
    }
}

/// The user comments of a Vorbis comment header packet (Vorbis I, section
/// 5): packet type 3, `vorbis`, the vendor string, then the comments.
pub fn user_comments(packet: &[u8]) -> io::Result<Vec<&[u8]>> {
    if packet.get(..7) != Some(b"\x03vorbis") {
        return Err(invalid("not a Vorbis comment header"));
    }
    let mut rest = &packet[7..];
    let vendor = take_length(&mut rest)?;
    take(&mut rest, vendor)?;
    let count = take_length(&mut rest)?;
    // Each comment takes at least its 4-byte length.
    let mut comments = Vec::with_capacity(count.min(rest.len() / 4));
    for _ in 0..count {
        let len = take_length(&mut rest)?;
        comments.push(take(&mut rest, len)?);
    }
    Ok(comments)
}

/// The first `len` bytes of `rest`, advancing it.
fn take<'a>(rest: &mut &'a [u8], len: usize) -> io::Result<&'a [u8]> {
    if rest.len() < len {
        return Err(invalid("Vorbis comment header truncated"));
    }
    let (bytes, tail) = rest.split_at(len);
    *rest = tail;
    Ok(bytes)
}

/// A `u32` little-endian length taken from `rest`.
fn take_length(rest: &mut &[u8]) -> io::Result<usize> {
    let bytes = take(rest, 4)?;
    Ok(u32::from_le_bytes(bytes.try_into().unwrap()) as usize)
}

/// The first user comment of the Ogg Vorbis stream read from `reader`
/// (`None` when it has none), read from the stream's second packet. Reads
/// only the pages up to that packet.
pub fn first_user_comment<R: Read + Seek>(reader: R) -> io::Result<Option<Vec<u8>>> {
    let mut packets = PacketReader::new(reader);
    let mut next = || -> io::Result<Vec<u8>> {
        packets
            .read_packet()
            .map_err(|e| invalid(format!("Ogg: {e}")))?
            .map(|p| p.data)
            .ok_or_else(|| invalid("Ogg stream ends before the Vorbis comment header"))
    };
    let ident = next()?;
    if ident.get(..7) != Some(b"\x01vorbis") {
        return Err(invalid("not a Vorbis stream"));
    }
    let comment = next()?;
    Ok(user_comments(&comment)?.first().map(|c| c.to_vec()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(fields: &[u32]) -> Vec<u8> {
        fields.iter().flat_map(|f| f.to_le_bytes()).collect()
    }

    #[test]
    fn reads_each_version() {
        let (min, max, vol, ai) = (0.5f32, 15.0f32, 0.8f32, 0x0800_0080);
        assert_eq!(
            parse_record(&record(&[1, min.to_bits(), max.to_bits(), ai])),
            Ok(SoundParams {
                min_distance: 0.5,
                max_distance: 15.0,
                base_volume: 1.0,
                ai_sound_type: ai,
                max_ai_distance: 15.0,
            })
        );
        assert_eq!(
            parse_record(&record(&[
                2,
                min.to_bits(),
                max.to_bits(),
                vol.to_bits(),
                ai
            ])),
            Ok(SoundParams {
                min_distance: 0.5,
                max_distance: 15.0,
                base_volume: 0.8,
                ai_sound_type: ai,
                max_ai_distance: 15.0,
            })
        );
        assert_eq!(
            parse_record(&record(&[
                3,
                min.to_bits(),
                max.to_bits(),
                vol.to_bits(),
                ai,
                1.0f32.to_bits()
            ])),
            Ok(SoundParams {
                min_distance: 0.5,
                max_distance: 15.0,
                base_volume: 0.8,
                ai_sound_type: ai,
                max_ai_distance: 1.0,
            })
        );
    }

    #[test]
    fn longer_records_read_their_version_only() {
        let mut v1 = record(&[1, 1.0f32.to_bits(), 2.0f32.to_bits(), 7]);
        v1.extend([0xFF; 8]);
        assert_eq!(parse_record(&v1).unwrap().max_ai_distance, 2.0);
    }

    #[test]
    fn defaults_for_missing_or_unknown_records() {
        assert_eq!(
            params(None),
            (DEFAULT_PARAMS, Some(RecordProblem::NoComment))
        );
        assert_eq!(
            params(Some(&record(&[4, 0, 0, 0, 0, 0]))),
            (DEFAULT_PARAMS, Some(RecordProblem::UnknownVersion(Some(4))))
        );
        assert_eq!(
            params(Some(&record(&[0, 0, 0, 0, 0, 0]))),
            (DEFAULT_PARAMS, Some(RecordProblem::UnknownVersion(Some(0))))
        );
        assert_eq!(
            params(Some(b"ab")),
            (DEFAULT_PARAMS, Some(RecordProblem::UnknownVersion(None)))
        );
        assert_eq!(
            params(Some(&record(&[3, 0, 0, 0, 0]))),
            (
                DEFAULT_PARAMS,
                Some(RecordProblem::Short {
                    version: 3,
                    len: 20
                })
            )
        );
        assert_eq!(
            DEFAULT_PARAMS,
            SoundParams {
                min_distance: 1.0,
                max_distance: 300.0,
                base_volume: 1.0,
                ai_sound_type: 0,
                max_ai_distance: 300.0,
            }
        );
    }

    fn comment_header(vendor: &[u8], comments: &[&[u8]]) -> Vec<u8> {
        let mut p = b"\x03vorbis".to_vec();
        p.extend((vendor.len() as u32).to_le_bytes());
        p.extend(vendor);
        p.extend((comments.len() as u32).to_le_bytes());
        for c in comments {
            p.extend((c.len() as u32).to_le_bytes());
            p.extend(*c);
        }
        p.push(1);
        p
    }

    #[test]
    fn reads_user_comments() {
        let binary = [3, 0, 0, 0, 0xFF, 0xFE, 0, 0x80];
        let p = comment_header(b"Xiph", &[&binary, b"ARTIST=x"]);
        assert_eq!(user_comments(&p).unwrap(), [&binary[..], b"ARTIST=x"]);
        assert!(user_comments(&comment_header(b"", &[])).unwrap().is_empty());
        let truncated = &p[..p.len() - 6];
        assert!(user_comments(truncated).is_err());
        assert!(user_comments(b"\x01vorbis").is_err());
        // A count far beyond the packet fails without a huge allocation.
        let mut huge = b"\x03vorbis".to_vec();
        huge.extend(0u32.to_le_bytes());
        huge.extend(u32::MAX.to_le_bytes());
        assert!(user_comments(&huge).is_err());
    }
}
