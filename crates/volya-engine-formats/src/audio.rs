// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The game's audio: Ogg Vorbis (public formats), decoded with `lewton`.

use std::io::{self, Read, Seek, SeekFrom};

use lewton::inside_ogg::OggStreamReader;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// The granule position of the last Ogg page in `bytes` (the public Ogg
/// format: pages start with `OggS`, version 0, and carry the granule at byte
/// 6). For a Vorbis stream that is the number of PCM frames up to the end of
/// the stream, so the length in seconds is this over the sample rate. `None`
/// when no page is found.
pub fn ogg_end_granule(bytes: &[u8]) -> Option<u64> {
    let mut end = bytes.len();
    while end >= 27 {
        let start = bytes[..end].windows(4).rposition(|w| w == b"OggS")?;
        let page = &bytes[start..];
        if page.len() >= 27 && page[4] == 0 {
            let granule = u64::from_le_bytes(page[6..14].try_into().unwrap());
            // -1: no packet ends on this page.
            if granule != u64::MAX {
                return Some(granule);
            }
        }
        end = start;
    }
    None
}

/// A Vorbis stream read from an Ogg file, decoded block by block.
pub struct VorbisStream<R: Read + Seek> {
    reader: Option<OggStreamReader<R>>,
}

impl<R: Read + Seek> VorbisStream<R> {
    pub fn open(reader: R) -> io::Result<Self> {
        let reader = OggStreamReader::new(reader).map_err(|e| invalid(format!("Vorbis: {e}")))?;
        Ok(Self {
            reader: Some(reader),
        })
    }

    fn reader(&mut self) -> &mut OggStreamReader<R> {
        self.reader.as_mut().expect("reader present")
    }

    pub fn sample_rate(&self) -> u32 {
        self.reader
            .as_ref()
            .expect("reader present")
            .ident_hdr
            .audio_sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.reader
            .as_ref()
            .expect("reader present")
            .ident_hdr
            .audio_channels
            .into()
    }

    /// Appends the next decoded block to `out` as interleaved samples in
    /// [-1, 1]; returns `false` at the end of the stream.
    pub fn read_block(&mut self, out: &mut Vec<f32>) -> io::Result<bool> {
        loop {
            match self.reader().read_dec_packet_itl() {
                Ok(Some(samples)) if samples.is_empty() => continue,
                Ok(Some(samples)) => {
                    out.extend(samples.iter().map(|&s| f32::from(s) / 32768.0));
                    return Ok(true);
                }
                Ok(None) => return Ok(false),
                Err(e) => return Err(invalid(format!("Vorbis: {e}"))),
            }
        }
    }

    /// Restarts from the first sample, by reopening the stream (seeking to
    /// granule 0 lands on the header packets).
    pub fn rewind(&mut self) -> io::Result<()> {
        let mut inner = self
            .reader
            .take()
            .expect("reader present")
            .into_inner()
            .into_inner();
        inner.seek(SeekFrom::Start(0))?;
        *self = Self::open(inner)?;
        Ok(())
    }
}
