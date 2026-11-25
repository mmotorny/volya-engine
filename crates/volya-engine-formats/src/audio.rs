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

/// How far from the end of a stream its last page is searched for: an Ogg
/// page is at most 65,307 bytes (RFC 3533).
const LAST_PAGE_SEARCH: u64 = 65_307 * 2;

/// The end granule of the Ogg stream read from `reader` (see
/// [`ogg_end_granule`]), searched in its last bytes; leaves the position at
/// the start.
fn end_granule<R: Read + Seek>(reader: &mut R) -> io::Result<Option<u64>> {
    let len = reader.seek(SeekFrom::End(0))?;
    let start = len.saturating_sub(LAST_PAGE_SEARCH);
    reader.seek(SeekFrom::Start(start))?;
    let mut tail = Vec::with_capacity((len - start) as usize);
    reader.by_ref().take(len - start).read_to_end(&mut tail)?;
    reader.seek(SeekFrom::Start(0))?;
    Ok(ogg_end_granule(&tail))
}

/// A Vorbis stream read from an Ogg file, decoded block by block.
///
/// The output ends at the last page's granule position, which the Vorbis I
/// specification (section A.2) defines as the stream's last sample: `lewton`
/// 0.10.2 decodes the whole last block instead when the stream is a single
/// page (its issue 106; measured: `ambient\indoors\small_fan.ogg` gives 666
/// frames too many).
pub struct VorbisStream<R: Read + Seek> {
    reader: Option<OggStreamReader<R>>,
    /// The end granule; `None` when the stream has none.
    end: Option<u64>,
    /// Frames returned so far.
    frames: u64,
    /// Decoded samples [`VorbisStream::skip`] did not skip, returned first.
    pending: Vec<f32>,
}

impl<R: Read + Seek> VorbisStream<R> {
    pub fn open(mut reader: R) -> io::Result<Self> {
        let end = end_granule(&mut reader)?;
        let reader = OggStreamReader::new(reader).map_err(|e| invalid(format!("Vorbis: {e}")))?;
        Ok(Self {
            reader: Some(reader),
            end,
            frames: 0,
            pending: Vec::new(),
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
        if !self.pending.is_empty() {
            out.append(&mut self.pending);
            return Ok(true);
        }
        let channels = u64::from(self.channels().max(1));
        loop {
            match self.reader().read_dec_packet_itl() {
                Ok(Some(samples)) if samples.is_empty() => continue,
                Ok(Some(samples)) => {
                    let mut frames = samples.len() as u64 / channels;
                    if let Some(end) = self.end {
                        frames = frames.min(end.saturating_sub(self.frames));
                        if frames == 0 {
                            return Ok(false);
                        }
                    }
                    self.frames += frames;
                    let kept = &samples[..(frames * channels) as usize];
                    out.extend(kept.iter().map(|&s| f32::from(s) / 32768.0));
                    return Ok(true);
                }
                Ok(None) => return Ok(false),
                Err(e) => return Err(invalid(format!("Vorbis: {e}"))),
            }
        }
    }

    /// Skips `frames` frames by decoding them; returns how many were
    /// skipped (fewer at the end of the stream).
    pub fn skip(&mut self, frames: u64) -> io::Result<u64> {
        let channels = usize::from(self.channels().max(1));
        let mut skipped = 0;
        let mut block = Vec::new();
        while skipped < frames {
            block.clear();
            if !self.read_block(&mut block)? {
                break;
            }
            let n = (block.len() / channels) as u64;
            if skipped + n > frames {
                let keep = ((frames - skipped) as usize) * channels;
                self.pending = block.split_off(keep);
                skipped = frames;
            } else {
                skipped += n;
            }
        }
        Ok(skipped)
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
