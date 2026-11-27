// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The sound cache (docs/behavior/console-diagnostics.md 2.1.1): the
//! decoded audio of the sounds played, in lines of [`LINE_BYTES`] of 16-bit
//! samples, *lines* = ⌊`snd_cache_size` × 2,097,152 / [`LINE_BYTES`]⌋ + 1,
//! made once with the device. A line's data is allocated when the line is
//! first used, and kept: free lines are taken in their order, so the cache
//! chooses as one allocated whole (docs/architecture/memory.md, Deviations,
//! #792). Each sound file has a table of 2 bytes per line of its decoded
//! length, the cache line holding it or none, kept for the cache's life. A voice plays its file through the cache
//! ([`CachedSource`]): a line held is copied out, one not held is decoded
//! into the line least recently used. The output is the same as decoding
//! the file straight through.
//!
//! A line's 8,820 bytes are 50 ms of the device's 16-bit stereo output at
//! 44,100 Hz (the spec's bytes per line); the game's sounds are mono, and a
//! line holds 4,410 of their 16-bit samples, 100 ms (believed: the spec
//! gives the line's bytes, not how a mono sound fills them, #748).

use std::cell::RefCell;
use std::collections::HashMap;
use std::io;
use std::rc::Rc;

use super::remote::Opener;
use super::{File, Source};

/// Bytes per line: 50 ms of 16-bit stereo at 44,100 Hz.
pub const LINE_BYTES: usize = 8_820;
/// Mono 16-bit samples a line holds.
pub const LINE_SAMPLES: usize = LINE_BYTES / 2;

/// The cache's lines for `snd_cache_size` `size` (console-diagnostics.md
/// 2.1.1: 16 gives 3,805).
pub fn lines_for(size: u32) -> usize {
    (size as usize * 2_097_152) / LINE_BYTES + 1
}

/// No line: a file line not held, or the ends of the recency list.
const NONE: u32 = u32::MAX;
const NO_LINE: u16 = u16::MAX;

/// A cache line's record: which file line it holds and its place in the
/// recency list.
#[derive(Clone, Copy)]
struct Line {
    /// The file's key, [`NONE`] while free.
    file: u32,
    /// The line of the file it holds.
    index: u32,
    /// Less and more recently used.
    older: u32,
    newer: u32,
}

/// A sound file the cache knows: its decoded length and, per line of it,
/// the cache line holding it.
struct FileLines {
    frames: u64,
    rate: u32,
    table: Vec<u16>,
}

/// The sound cache.
pub struct SoundCache {
    size: u32,
    /// The data of the lines used so far, in their order: a free line is
    /// allocated when first taken.
    data: Vec<Box<[i16]>>,
    lines: Vec<Line>,
    /// Least and most recently used.
    oldest: u32,
    newest: u32,
    files: HashMap<u32, FileLines>,
}

impl SoundCache {
    /// The cache of `snd_cache_size` `size`, every line free.
    pub fn new(size: u32) -> Self {
        let count = lines_for(size);
        let lines = (0..count as u32)
            .map(|i| Line {
                file: NONE,
                index: 0,
                older: if i == 0 { NONE } else { i - 1 },
                newer: if i as usize + 1 == count { NONE } else { i + 1 },
            })
            .collect();
        Self {
            size,
            data: Vec::with_capacity(count),
            lines,
            oldest: 0,
            newest: count as u32 - 1,
            files: HashMap::new(),
        }
    }

    /// The `snd_cache_size` it was made for.
    pub fn size(&self) -> u32 {
        self.size
    }

    /// Its lines.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// The lines used so far, their data allocated.
    pub fn lines_used(&self) -> usize {
        self.data.len()
    }

    /// The bytes it holds: the data of the lines used and every line's
    /// record, the files' tables.
    pub fn bytes(&self) -> usize {
        self.data.len() * LINE_BYTES
            + self.data.capacity() * size_of::<Box<[i16]>>()
            + self.lines.capacity() * size_of::<Line>()
            + self
                .files
                .values()
                .map(|f| f.table.capacity() * 2)
                .sum::<usize>()
    }

    /// File `key`'s frames and sample rate, if known (played once through
    /// the cache).
    pub fn knows(&self, key: u32) -> Option<(u64, u32)> {
        self.files.get(&key).map(|f| (f.frames, f.rate))
    }

    /// Records file `key` of `frames` decoded frames: its table, no line
    /// held.
    fn add_file(&mut self, key: u32, frames: u64, rate: u32) {
        let lines = frames.div_ceil(LINE_SAMPLES as u64) as usize;
        self.files.entry(key).or_insert_with(|| FileLines {
            frames,
            rate,
            table: vec![NO_LINE; lines],
        });
    }

    fn unlink(&mut self, i: u32) {
        let Line { older, newer, .. } = self.lines[i as usize];
        match older {
            NONE => self.oldest = newer,
            o => self.lines[o as usize].newer = newer,
        }
        match newer {
            NONE => self.newest = older,
            n => self.lines[n as usize].older = older,
        }
    }

    /// Makes cache line `i` the most recently used.
    fn touch(&mut self, i: u32) {
        if self.newest == i {
            return;
        }
        self.unlink(i);
        let line = &mut self.lines[i as usize];
        line.older = self.newest;
        line.newer = NONE;
        self.lines[self.newest as usize].newer = i;
        self.newest = i;
        if self.oldest == NONE {
            self.oldest = i;
        }
    }

    /// The samples of line `index` of file `key`, if held (made the most
    /// recently used).
    fn get(&mut self, key: u32, index: usize) -> Option<&[i16]> {
        let file = self.files.get(&key)?;
        let line = *file.table.get(index)?;
        if line == NO_LINE {
            return None;
        }
        let len = line_len(file.frames, index);
        self.touch(u32::from(line));
        Some(&self.data[usize::from(line)][..len])
    }

    /// Keeps `samples` as line `index` of file `key`, in the line least
    /// recently used (which its file no longer holds).
    fn put(&mut self, key: u32, index: usize, samples: &[i16]) {
        let held = self
            .files
            .get(&key)
            .and_then(|f| f.table.get(index).copied())
            .filter(|&l| l != NO_LINE);
        if let Some(line) = held {
            self.touch(u32::from(line));
            return;
        }
        let i = self.oldest;
        let old = self.lines[i as usize];
        if old.file != NONE
            && let Some(f) = self.files.get_mut(&old.file)
        {
            f.table[old.index as usize] = NO_LINE;
        }
        if let Some(f) = self.files.get_mut(&key) {
            f.table[index] = i as u16;
        }
        let line = &mut self.lines[i as usize];
        line.file = key;
        line.index = index as u32;
        // Free lines are the oldest, in their order: the first free one is
        // the next to allocate.
        if i as usize == self.data.len() {
            self.data.push(vec![0; LINE_SAMPLES].into_boxed_slice());
        }
        self.data[i as usize][..samples.len()].copy_from_slice(samples);
        self.touch(i);
    }
}

/// The frames of line `index` of a file of `frames` frames.
fn line_len(frames: u64, index: usize) -> usize {
    let start = (index * LINE_SAMPLES) as u64;
    frames.saturating_sub(start).min(LINE_SAMPLES as u64) as usize
}

fn to_sample(v: f32) -> i16 {
    (v * 32_768.0).round().clamp(-32_768.0, 32_767.0) as i16
}

fn to_float(s: i16) -> f32 {
    f32::from(s) / 32_768.0
}

/// A mono file played through the cache. While it is not streaming it
/// copies the lines held and decodes the first one that is not; from then
/// on its decoder runs on through the lines, held or not, keeping each in
/// the cache, until it loops: so a decoder seeks at most once per pass
/// (seeking scans the file's pages from its start, #309).
pub struct CachedSource {
    cache: Rc<RefCell<SoundCache>>,
    key: u32,
    frames: u64,
    rate: u32,
    open: Opener,
    /// The file, until a line not held needs it decoded.
    file: Option<Box<dyn File>>,
    /// Its decoder, once opened.
    decoder: Option<Box<dyn Source>>,
    /// The decoder's frame: that of the first of `pending`, else of the
    /// next it decodes.
    decoded: u64,
    /// Decoded samples not yet played.
    pending: Vec<f32>,
    /// The decoder stands at the next frame to play.
    streaming: bool,
    /// The next frame to play.
    at: u64,
    line: Vec<i16>,
}

impl CachedSource {
    /// File `key` through `cache`, `skip` frames in. A file not known to
    /// the cache is opened now and learnt, if it is mono with a known
    /// length; otherwise it plays straight from its decoder. Fails when
    /// opening it does.
    pub fn open(
        cache: &Rc<RefCell<SoundCache>>,
        key: u32,
        file: Box<dyn File>,
        skip: u64,
        open: Opener,
    ) -> io::Result<Box<dyn Source>> {
        let known = cache.borrow().knows(key);
        let (frames, rate, file, decoder) = match known {
            Some((frames, rate)) => (frames, rate, Some(file), None),
            None => {
                let source = open(file, skip)?;
                match source.frames() {
                    Some(frames) if source.channels() == 1 => {
                        let rate = source.sample_rate();
                        cache.borrow_mut().add_file(key, frames, rate);
                        (frames, rate, None, Some(source))
                    }
                    _ => return Ok(source),
                }
            }
        };
        let at = skip.min(frames);
        Ok(Box::new(Self {
            cache: cache.clone(),
            key,
            frames,
            rate,
            open,
            file,
            streaming: decoder.is_some(),
            decoder,
            decoded: at,
            pending: Vec::new(),
            at,
            line: Vec::with_capacity(LINE_SAMPLES),
        }))
    }

    /// Brings the decoder to `self.at`: opened there, or rewound when past
    /// it, then forward past what it decoded, skipped where it can seek,
    /// else decoded and dropped.
    fn seek(&mut self) -> io::Result<()> {
        let goal = self.at;
        if let Some(file) = self.file.take() {
            self.decoder = Some((self.open)(file, goal)?);
            self.decoded = goal;
            self.pending.clear();
        }
        let decoder = self.decoder.as_mut().ok_or_else(gone)?;
        if self.decoded > goal {
            decoder.rewind()?;
            self.decoded = 0;
            self.pending.clear();
        }
        while self.decoded < goal {
            if !self.pending.is_empty() {
                let drop = ((goal - self.decoded) as usize).min(self.pending.len());
                self.pending.drain(..drop);
                self.decoded += drop as u64;
                continue;
            }
            let skipped = decoder.skip(goal - self.decoded)?;
            self.decoded += skipped;
            if skipped == 0 && !decoder.read(&mut self.pending)? {
                break;
            }
        }
        Ok(())
    }

    /// The next `n` frames from the decoder, as 16-bit samples, into
    /// `self.line`; silence where it ends early.
    fn decode(&mut self, n: usize) -> io::Result<()> {
        let decoder = self.decoder.as_mut().ok_or_else(gone)?;
        while self.pending.len() < n && decoder.read(&mut self.pending)? {}
        let take = n.min(self.pending.len());
        self.line.clear();
        self.line.extend(self.pending.drain(..take).map(to_sample));
        self.line.resize(n, 0);
        self.decoded += n as u64;
        Ok(())
    }
}

fn gone() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "sound file gone")
}

impl Source for CachedSource {
    fn sample_rate(&self) -> u32 {
        self.rate
    }

    fn channels(&self) -> u16 {
        1
    }

    fn read(&mut self, out: &mut Vec<f32>) -> io::Result<bool> {
        if self.at >= self.frames {
            return Ok(false);
        }
        let index = (self.at / LINE_SAMPLES as u64) as usize;
        let offset = (self.at % LINE_SAMPLES as u64) as usize;
        let len = line_len(self.frames, index);
        if !self.streaming {
            if let Some(samples) = self.cache.borrow_mut().get(self.key, index) {
                out.extend(samples[offset..].iter().copied().map(to_float));
                self.at += (len - offset) as u64;
                return Ok(true);
            }
            self.seek()?;
            self.streaming = true;
        }
        // The rest of the line; whole lines are kept.
        self.decode(len - offset)?;
        out.extend(self.line.iter().copied().map(to_float));
        if offset == 0 {
            self.cache.borrow_mut().put(self.key, index, &self.line);
        }
        self.at += (len - offset) as u64;
        Ok(true)
    }

    fn rewind(&mut self) -> io::Result<()> {
        self.at = 0;
        self.streaming = false;
        Ok(())
    }

    fn frames(&self) -> Option<u64> {
        Some(self.frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    /// A mono "file" of `frames` frames whose frame `i` is the 16-bit
    /// sample `(i · 7) mod 65,536`, decoded in blocks of 1,000 frames.
    struct Ramp {
        frames: u64,
        pos: u64,
    }

    fn value(i: u64) -> f32 {
        to_float(((i * 7) % 65_536) as u16 as i16)
    }

    impl Source for Ramp {
        fn sample_rate(&self) -> u32 {
            44_100
        }
        fn channels(&self) -> u16 {
            1
        }
        fn read(&mut self, out: &mut Vec<f32>) -> io::Result<bool> {
            if self.pos >= self.frames {
                return Ok(false);
            }
            let end = (self.pos + 1_000).min(self.frames);
            out.extend((self.pos..end).map(value));
            self.pos = end;
            Ok(true)
        }
        fn rewind(&mut self) -> io::Result<()> {
            self.pos = 0;
            Ok(())
        }
        fn frames(&self) -> Option<u64> {
            Some(self.frames)
        }
    }

    fn open_ramp(mut file: Box<dyn File>, skip: u64) -> io::Result<Box<dyn Source>> {
        let mut b = [0; 8];
        file.read_exact(&mut b)?;
        let frames = u64::from_le_bytes(b);
        OPENED.with(|o| o.set(o.get() + 1));
        Ok(Box::new(Ramp {
            frames,
            pos: skip.min(frames),
        }))
    }

    thread_local! {
        static OPENED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    fn file(frames: u64) -> Box<dyn File> {
        Box::new(io::Cursor::new(frames.to_le_bytes().to_vec()))
    }

    fn play(source: &mut dyn Source) -> Vec<f32> {
        let mut out = Vec::new();
        while source.read(&mut out).unwrap() {}
        out
    }

    #[test]
    fn sizes_follow_the_spec() {
        assert_eq!(lines_for(16), 3_805);
        assert_eq!(lines_for(32), 7_609);
        let cache = SoundCache::new(16);
        assert_eq!(cache.line_count(), 3_805);
        // The data the sound library logs as 32,773 kb once every line is
        // used; none of it before, only the lines' records.
        assert_eq!(cache.line_count() * LINE_BYTES / 1024, 32_773);
        assert_eq!(cache.lines_used(), 0);
        assert!(cache.bytes() >= 3_805 * 16 && cache.bytes() < 3_805 * 40);
    }

    #[test]
    fn lines_are_allocated_when_first_used_in_their_order() {
        let mut c = SoundCache::new(1);
        let count = c.line_count();
        c.add_file(1, (count as u64 + 3) * LINE_SAMPLES as u64, 44_100);
        let empty = c.bytes();
        let line = vec![7; LINE_SAMPLES];
        for i in 0..count + 3 {
            c.put(1, i, &line);
            // Each line taken while free is the next allocated; once all
            // are, the least recently used is reused.
            assert_eq!(c.lines_used(), (i + 1).min(count));
            assert_eq!(c.files[&1].table[i] as usize, i % count);
        }
        assert_eq!(c.bytes(), empty + count * LINE_BYTES);
    }

    #[test]
    fn plays_what_the_decoder_gives_and_reuses_held_lines() {
        let cache = Rc::new(RefCell::new(SoundCache::new(4)));
        let frames = 10_000;
        let want: Vec<f32> = (0..frames).map(value).collect();
        OPENED.with(|o| o.set(0));
        let mut first = CachedSource::open(&cache, 1, file(frames), 0, open_ramp).unwrap();
        assert_eq!(play(&mut *first), want);
        // A file's table: 2 bytes per line of its length (3 lines).
        assert_eq!(cache.borrow().files[&1].table.len(), 3);
        // Played again: every line held, the file never opened.
        let mut again = CachedSource::open(&cache, 1, file(frames), 0, open_ramp).unwrap();
        assert_eq!(play(&mut *again), want);
        assert_eq!(OPENED.with(|o| o.get()), 1);
        // Skipped in, and looped back.
        let mut skipped = CachedSource::open(&cache, 1, file(frames), 5_000, open_ramp).unwrap();
        assert_eq!(play(&mut *skipped), want[5_000..]);
        skipped.rewind().unwrap();
        assert_eq!(play(&mut *skipped), want);
    }

    #[test]
    fn evicted_lines_are_decoded_again() {
        // snd_cache_size 0: one line, so two files evict each other.
        let cache = Rc::new(RefCell::new(SoundCache::new(0)));
        assert_eq!(cache.borrow().line_count(), 1);
        let (a, b) = (9_000, 12_345);
        let want = |n: u64, from: u64| (from..n).map(value).collect::<Vec<_>>();
        for _ in 0..2 {
            for (key, n, skip) in [(1, a, 0), (2, b, 100), (1, a, 4_500), (2, b, 0)] {
                let mut s = CachedSource::open(&cache, key, file(n), skip, open_ramp).unwrap();
                let mut got = Vec::new();
                // Read interleaved with the other file's lines.
                let mut other = CachedSource::open(&cache, 3, file(5_000), 0, open_ramp).unwrap();
                while s.read(&mut got).unwrap() {
                    let mut scratch = Vec::new();
                    other.read(&mut scratch).unwrap();
                }
                assert_eq!(got, want(n, skip), "file {key} from {skip}");
            }
        }
    }

    #[test]
    fn a_file_of_two_channels_plays_uncached() {
        fn open_stereo(_: Box<dyn File>, _: u64) -> io::Result<Box<dyn Source>> {
            struct Stereo;
            impl Source for Stereo {
                fn sample_rate(&self) -> u32 {
                    44_100
                }
                fn channels(&self) -> u16 {
                    2
                }
                fn read(&mut self, _: &mut Vec<f32>) -> io::Result<bool> {
                    Ok(false)
                }
                fn rewind(&mut self) -> io::Result<()> {
                    Ok(())
                }
                fn frames(&self) -> Option<u64> {
                    Some(10)
                }
            }
            Ok(Box::new(Stereo))
        }
        let cache = Rc::new(RefCell::new(SoundCache::new(1)));
        let s = CachedSource::open(&cache, 1, file(0), 0, open_stereo).unwrap();
        assert_eq!(s.channels(), 2);
        assert!(cache.borrow().knows(1).is_none());
    }
}
