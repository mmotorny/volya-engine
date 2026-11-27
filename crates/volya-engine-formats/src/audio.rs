// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The game's audio: Ogg Vorbis (public formats), decoded with `lewton`.

use std::cell::RefCell;
use std::io::{self, Read, Seek, SeekFrom};
use std::rc::{Rc, Weak};

use lewton::audio::{PreviousWindowRight, get_decoded_sample_count, read_audio_packet_generic};
use lewton::header::{
    IdentHeader, SetupHeader, read_header_comment, read_header_ident, read_header_setup,
};
use lewton::samples::{InterleavedSamples, Samples};
use ogg::reading::PacketReader;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn vorbis(e: impl std::fmt::Display) -> io::Error {
    invalid(format!("Vorbis: {e}"))
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

/// A stream's identification and setup headers, parsed. Parsing the setup
/// header (its codebooks) is most of what opening a stream costs: about
/// 0.7 ms a file in Chrome's wasm, 0.5 natively (measured, #310).
struct Headers {
    /// The two headers' packets, which identify them.
    packets: (Vec<u8>, Vec<u8>),
    ident: IdentHeader,
    setup: SetupHeader,
}

thread_local! {
    /// The headers of the streams open on this thread: a stream opened
    /// with the same headers' bytes shares them rather than parsing them
    /// again. Weak, so they last only as long as a stream uses them.
    static OPEN_HEADERS: RefCell<Vec<Weak<Headers>>> = const { RefCell::new(Vec::new()) };
}

/// The headers of the packets `ident` and `setup`: an open stream's, or
/// parsed.
fn headers(ident: Vec<u8>, setup: Vec<u8>) -> io::Result<Rc<Headers>> {
    let shared = OPEN_HEADERS.with(|open| {
        let mut open = open.borrow_mut();
        open.retain(|h| h.strong_count() > 0);
        open.iter()
            .filter_map(Weak::upgrade)
            .find(|h| h.packets.0 == ident && h.packets.1 == setup)
    });
    if let Some(headers) = shared {
        return Ok(headers);
    }
    let parsed = read_header_ident(&ident).map_err(vorbis)?;
    let setup_header = read_header_setup(
        &setup,
        parsed.audio_channels,
        (parsed.blocksize_0, parsed.blocksize_1),
    )
    .map_err(vorbis)?;
    let headers = Rc::new(Headers {
        packets: (ident, setup),
        ident: parsed,
        setup: setup_header,
    });
    OPEN_HEADERS.with(|open| open.borrow_mut().push(Rc::downgrade(&headers)));
    Ok(headers)
}

/// How far ahead [`VorbisStream::skip`] seeks rather than decodes, frames.
const SEEK_AHEAD: u64 = 1 << 14;

/// A Vorbis stream read from an Ogg file, decoded block by block.
///
/// The output ends at the last page's granule position, which the Vorbis I
/// specification (section A.2) defines as the stream's last sample: `lewton`
/// 0.10.2 decodes the whole last block instead when the stream is a single
/// page (its issue 106; measured: `ambient\indoors\small_fan.ogg` gives 666
/// frames too many).
pub struct VorbisStream<R: Read + Seek> {
    packets: PacketReader<R>,
    headers: Rc<Headers>,
    serial: u32,
    /// The previous block's right half, which the next block overlaps
    /// (Vorbis I section 1.3.2): empty at the start and after a seek.
    previous: PreviousWindowRight,
    /// The granule position the output so far ends at, from the first page
    /// end on: the stream's last packet is cut to its page's granule
    /// (Vorbis I section A.2, as `lewton`'s own reader does).
    granule: Option<u64>,
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
        let mut packets = PacketReader::new(reader);
        let first = packets.read_packet_expected().map_err(vorbis)?;
        let serial = first.stream_serial();
        // The identification header, checked before the others are read.
        read_header_ident(&first.data).map_err(vorbis)?;
        read_header_comment(&next_header(&mut packets, serial)?).map_err(vorbis)?;
        let setup = next_header(&mut packets, serial)?;
        let headers = headers(first.data, setup)?;
        // Audio starts on a page of its own (Vorbis I section A.2).
        packets.delete_unread_packets();
        Ok(Self {
            packets,
            headers,
            serial,
            previous: PreviousWindowRight::new(),
            granule: None,
            end,
            frames: 0,
            pending: Vec::new(),
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.headers.ident.audio_sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.headers.ident.audio_channels.into()
    }

    /// The granule of the stream's last page ([`ogg_end_granule`] of its
    /// last bytes): its length in frames; `None` when it has none.
    pub fn end_granule(&self) -> Option<u64> {
        self.end
    }

    /// The next audio packet decoded, interleaved; `None` at the end.
    fn decode_packet(&mut self) -> io::Result<Option<Vec<i16>>> {
        let packet = loop {
            match self.packets.read_packet().map_err(vorbis)? {
                Some(p) if p.stream_serial() == self.serial => break p,
                Some(_) => {}
                None => return Ok(None),
            }
        };
        let mut decoded: InterleavedSamples<i16> = read_audio_packet_generic(
            &self.headers.ident,
            &self.headers.setup,
            &packet.data,
            &mut self.previous,
        )
        .map_err(vorbis)?;
        if let (Some(granule), true) = (self.granule, packet.last_in_stream()) {
            decoded.truncate(packet.absgp_page().saturating_sub(granule) as usize);
        }
        if packet.last_in_page() {
            self.granule = Some(packet.absgp_page());
        } else if let Some(granule) = &mut self.granule {
            *granule += decoded.num_samples() as u64;
        }
        Ok(Some(decoded.samples))
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
            match self.decode_packet()? {
                Some(samples) if samples.is_empty() => continue,
                Some(samples) => {
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
                None => return Ok(false),
            }
        }
    }

    /// Frames returned so far.
    fn position(&self) -> u64 {
        let channels = u64::from(self.channels().max(1));
        self.frames - self.pending.len() as u64 / channels
    }

    /// Skips `frames` frames; returns how many were skipped (fewer at the
    /// end of the stream). Far ahead it seeks to an Ogg page before the
    /// frame and decodes only from there (#309): decoding every frame before
    /// a far one stalled a phone's audio for hundreds of milliseconds
    /// (#447).
    pub fn skip(&mut self, frames: u64) -> io::Result<u64> {
        let at = self.position();
        let mut goal = at.saturating_add(frames);
        if let Some(end) = self.end {
            goal = goal.min(end.max(at));
        }
        if goal - at > SEEK_AHEAD
            && let Some((page, from)) = self.page_before(goal)?
            && from > at
        {
            self.packets
                .seek_bytes(SeekFrom::Start(page))
                .map_err(|e| invalid(format!("Ogg: {e}")))?;
            self.previous = PreviousWindowRight::new();
            self.granule = None;
            self.pending.clear();
            self.frames = from;
        }
        let from = self.position();
        self.decode_skip(goal - from)?;
        Ok(self.position() - at)
    }

    /// The last page whose first packet starting on it ends the output at
    /// or before frame `goal`, and that frame. Decoding from that page, the
    /// packet only primes the decoder (a block overlaps the one before), so
    /// what follows equals a decode from the start. Reads only the pages'
    /// headers and each packet's first bytes: a packet's length in frames
    /// follows from its mode and window flags (Vorbis I section 4.3.1),
    /// without decoding it. The granules are not used: some of the game's
    /// files are off by 448 frames on some pages (measured). `None` when
    /// there is none.
    fn page_before(&mut self, goal: u64) -> io::Result<Option<(u64, u64)>> {
        let reading = self.packets.get_mut().stream_position()?;
        let found = self.scan_pages(goal);
        self.packets.get_mut().seek(SeekFrom::Start(reading))?;
        found
    }

    /// [`VorbisStream::page_before`]'s search, leaving the file anywhere.
    fn scan_pages(&mut self, goal: u64) -> io::Result<Option<(u64, u64)>> {
        let (ident, setup, serial) = (&self.headers.ident, &self.headers.setup, self.serial);
        let r = self.packets.get_mut();
        let mut found = None;
        let mut offset = 0;
        // Packets of the stream ended so far, the headers first; the frame
        // the output ends at after the last packet started.
        let (mut ended, mut frame) = (0, 0);
        let mut audio = false;
        let mut header = [0; 27];
        let mut lacing = [0; 255];
        loop {
            r.seek(SeekFrom::Start(offset))?;
            // The end, or not a page (RFC 3533 section 6).
            if r.read_exact(&mut header).is_err() || &header[..4] != b"OggS" {
                break;
            }
            let segments = &mut lacing[..usize::from(header[26])];
            r.read_exact(segments)?;
            let body = offset + 27 + segments.len() as u64;
            let ours = u32::from_le_bytes(header[14..18].try_into().unwrap()) == serial;
            // Audio from the page after the one the headers end on.
            let after_headers = ended >= 3;
            let mut first = true;
            let mut start = header[5] & 1 == 0;
            let mut at = body;
            for &l in segments.iter() {
                if ours && start && after_headers {
                    let mut bytes = [0; 2];
                    let n = usize::from(l.min(2));
                    r.seek(SeekFrom::Start(at))?;
                    r.read_exact(&mut bytes[..n])?;
                    // The first audio packet gives no frames.
                    if audio {
                        let n = get_decoded_sample_count(ident, setup, &bytes[..n]).unwrap_or(0);
                        frame += n as u64;
                    }
                    audio = true;
                    if first {
                        if frame > goal {
                            return Ok(found);
                        }
                        found = Some((offset, frame));
                        first = false;
                    }
                }
                if ours && l < 255 {
                    ended += 1;
                }
                start = l < 255;
                at += u64::from(l);
            }
            offset = at;
        }
        Ok(found)
    }

    /// Skips `frames` frames by decoding them.
    fn decode_skip(&mut self, frames: u64) -> io::Result<u64> {
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

    /// Restarts from the first sample: back to the headers, read again.
    pub fn rewind(&mut self) -> io::Result<()> {
        self.packets
            .seek_bytes(SeekFrom::Start(0))
            .map_err(|e| invalid(format!("Ogg: {e}")))?;
        for _ in 0..3 {
            next_header(&mut self.packets, self.serial)?;
        }
        self.packets.delete_unread_packets();
        self.previous = PreviousWindowRight::new();
        self.granule = None;
        self.frames = 0;
        self.pending.clear();
        Ok(())
    }
}

/// The next packet of stream `serial`, which must exist: a header's data.
fn next_header<R: Read + Seek>(packets: &mut PacketReader<R>, serial: u32) -> io::Result<Vec<u8>> {
    loop {
        let packet = packets.read_packet_expected().map_err(vorbis)?;
        if packet.stream_serial() == serial {
            return Ok(packet.data);
        }
    }
}
