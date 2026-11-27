// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Known sounds found in a recording (#838, the `audio-fidelity` suite,
//! docs/testing.md): where each starts, which it is, and how loud. Our own
//! test tooling, applied alike to the original's captured output and to
//! ours; no engine behaviour.
//!
//! A recording's spectrogram (log-spaced bands every [`HOP_MS`]) less its
//! background (each band's low percentile around each point) is matched
//! against a template of each candidate sound (its own spectrogram's first
//! [`TEMPLATE_MS`] relative to its peak): the Pearson correlation of the
//! two at every point. A sound is found where the best template's
//! correlation peaks over [`MIN_SCORE`] with enough energy over the
//! background; the template names it, so a footstep's material is told by
//! its sound's spectrum. With the recording's frames ([`Frames`], the
//! samples at which each frame ran) every find has its frame.

use std::f32::consts::PI;

/// Milliseconds between spectrogram points.
const HOP_MS: f32 = 5.0;
/// The spectrum's window, ms (rounded up to a power of two of samples).
const WINDOW_MS: f32 = 20.0;
/// The bands' range and count.
const LOW_HZ: f32 = 80.0;
const HIGH_HZ: f32 = 16_000.0;
pub const BANDS: usize = 24;
/// A template's length from its sound's start.
const TEMPLATE_MS: f32 = 100.0;
/// A template's bands: those its sound reaches within this of its peak
/// in. Only they are compared, so a background loud in other bands (the
/// bunker's, below 1 kHz) does not hide it.
const ACTIVE_DB: f32 = 20.0;
/// dB below its peak a template's cells are floored at, and the most a
/// recording's cell counts over its background.
const RANGE_DB: f32 = 40.0;
/// The background: each band's [`BACKGROUND_PERCENTILE`] over
/// [`BACKGROUND_MS`] either side, recomputed every [`BACKGROUND_STEP`]
/// points.
const BACKGROUND_MS: f32 = 600.0;
const BACKGROUND_PERCENTILE: f32 = 0.3;
const BACKGROUND_STEP: usize = 10;
/// A find: correlation at least this, its patch's cells over the
/// background by [`MIN_EXCESS_DB`] on average, the best within
/// [`REFRACTORY_MS`] either side.
pub const MIN_SCORE: f32 = 0.5;
const MIN_EXCESS_DB: f32 = 3.0;
const REFRACTORY_MS: f32 = 120.0;
/// A find's level: the loudest 10 ms RMS this long after its start.
const LEVEL_MS: f32 = 150.0;

/// A recording: mono samples (the channels' mean) and their rate.
#[derive(Clone, Debug)]
pub struct Recording {
    pub rate: u32,
    pub samples: Vec<f32>,
}

impl Recording {
    /// Interleaved `channels`-channel samples mixed to mono.
    pub fn from_interleaved(rate: u32, channels: usize, interleaved: &[f32]) -> Self {
        let channels = channels.max(1);
        let samples = interleaved
            .chunks(channels)
            .map(|c| c.iter().sum::<f32>() / channels as f32)
            .collect();
        Self { rate, samples }
    }

    /// A 16-bit PCM WAV file's samples, mixed to mono.
    pub fn from_wav(bytes: &[u8]) -> Result<Self, String> {
        let u16_at = |i: usize| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
        let u32_at =
            |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
        if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
            return Err("not a WAV file".into());
        }
        let (mut at, mut format) = (12, None);
        while at + 8 <= bytes.len() {
            let (id, len) = (&bytes[at..at + 4], u32_at(at + 4) as usize);
            let body = at + 8;
            let end = body.saturating_add(len).min(bytes.len());
            match id {
                b"fmt " if len >= 16 => {
                    format = Some((
                        u16_at(body),
                        u16_at(body + 2),
                        u32_at(body + 4),
                        u16_at(body + 14),
                    ));
                }
                b"data" => {
                    let Some((1, channels, rate, 16)) = format else {
                        return Err(format!("not 16-bit PCM: {format:?}"));
                    };
                    let samples: Vec<f32> = bytes[body..end]
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .map(|s| f32::from(i16::from_le_bytes(*s)) / 32768.0)
                        .collect();
                    return Ok(Self::from_interleaved(
                        rate,
                        usize::from(channels),
                        &samples,
                    ));
                }
                _ => {}
            }
            at = body + len + (len & 1);
        }
        Err("no data chunk".into())
    }

    /// Seconds at sample `s`.
    pub fn seconds(&self, s: usize) -> f32 {
        s as f32 / self.rate as f32
    }

    fn samples_of(&self, ms: f32) -> usize {
        ((ms / 1000.0) * self.rate as f32).round().max(1.0) as usize
    }

    /// The band levels (dB of mean power per bin) every [`HOP_MS`], each
    /// over [`WINDOW_MS`] from its point.
    pub fn spectrogram(&self) -> Vec<[f32; BANDS]> {
        let hop = self.samples_of(HOP_MS);
        let n = self.samples_of(WINDOW_MS).next_power_of_two();
        let window: Vec<f32> = (0..n)
            .map(|k| 0.5 - 0.5 * (2.0 * PI * k as f32 / (n - 1) as f32).cos())
            .collect();
        let hz = self.rate as f32 / n as f32;
        let ratio = (HIGH_HZ / LOW_HZ).ln();
        let band_of: Vec<Option<usize>> = (0..n / 2)
            .map(|k| {
                let f = k as f32 * hz;
                (LOW_HZ..HIGH_HZ)
                    .contains(&f)
                    .then(|| ((((f / LOW_HZ).ln() / ratio) * BANDS as f32) as usize).min(BANDS - 1))
            })
            .collect();
        let mut counts = [0usize; BANDS];
        for b in band_of.iter().flatten() {
            counts[*b] += 1;
        }
        let (mut re, mut im) = (vec![0.0f32; n], vec![0.0f32; n]);
        (0..self.samples.len() / hop)
            .map(|i| {
                for k in 0..n {
                    re[k] = self.samples.get(i * hop + k).copied().unwrap_or(0.0) * window[k];
                    im[k] = 0.0;
                }
                fft(&mut re, &mut im);
                let mut bands = [0.0f32; BANDS];
                for (k, b) in band_of.iter().enumerate() {
                    if let Some(b) = b {
                        bands[*b] += re[k] * re[k] + im[k] * im[k];
                    }
                }
                for (b, c) in bands.iter_mut().zip(counts) {
                    *b = 10.0 * (*b / c.max(1) as f32 + 1e-12).log10();
                }
                bands
            })
            .collect()
    }

    /// The loudest 10 ms RMS from sample `at` over [`LEVEL_MS`], dBFS.
    pub fn level_db(&self, at: usize) -> f32 {
        let (w, end) = (self.samples_of(10.0), self.samples_of(LEVEL_MS));
        (0..end / w)
            .map(|i| {
                let from = (at + i * w).min(self.samples.len());
                let to = (from + w).min(self.samples.len());
                db(rms(&self.samples[from..to]))
            })
            .fold(f32::NEG_INFINITY, f32::max)
    }
}

/// A candidate sound: its spectrogram's first [`TEMPLATE_MS`] from its
/// start, floored [`RANGE_DB`] below its peak, its mean removed and its
/// length 1.
#[derive(Clone, Debug)]
pub struct Template {
    pub name: String,
    /// Its bands: those it reaches within [`ACTIVE_DB`] of its peak in.
    bands: Vec<usize>,
    /// Its cells in those bands, point by point.
    cells: Vec<f32>,
}

fn template_hops() -> usize {
    (TEMPLATE_MS / HOP_MS) as usize
}

/// `cells` less their mean, at length 1 (`None` when flat).
fn normalized(mut cells: Vec<f32>) -> Option<Vec<f32>> {
    let mean = cells.iter().sum::<f32>() / cells.len() as f32;
    for c in &mut cells {
        *c -= mean;
    }
    let norm = cells.iter().map(|c| c * c).sum::<f32>().sqrt();
    (norm > 1e-6).then(|| cells.iter().map(|c| c / norm).collect())
}

impl Template {
    /// The template of the sound `sound` called `name`: from its first
    /// point within 30 dB of its loudest; `None` for silence.
    pub fn new(name: &str, sound: &Recording) -> Option<Self> {
        let spec = sound.spectrogram();
        let total = |p: &[f32; BANDS]| p.iter().map(|d| 10f32.powf(d / 10.0)).sum::<f32>();
        let loudest = spec.iter().map(total).fold(0.0, f32::max);
        let start = spec.iter().position(|p| total(p) >= loudest * 1e-3)?;
        let peak = spec[start..]
            .iter()
            .take(template_hops())
            .flat_map(|p| p.iter().copied())
            .fold(f32::NEG_INFINITY, f32::max);
        let points: Vec<[f32; BANDS]> = (0..template_hops())
            .map(|t| {
                spec.get(start + t)
                    .copied()
                    .unwrap_or([peak - RANGE_DB; BANDS])
            })
            .collect();
        let bands: Vec<usize> = (0..BANDS)
            .filter(|&b| points.iter().any(|p| p[b] >= peak - ACTIVE_DB))
            .collect();
        let mut cells = Vec::with_capacity(bands.len() * points.len());
        for p in &points {
            cells.extend(
                bands
                    .iter()
                    .map(|&b| (p[b] - (peak - RANGE_DB)).clamp(0.0, RANGE_DB)),
            );
        }
        Some(Self {
            name: name.to_owned(),
            bands,
            cells: normalized(cells)?,
        })
    }
}

/// A sound found in a recording.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    /// The sample it starts at.
    pub sample: usize,
    /// The template that matched best, by its index.
    pub template: usize,
    /// Their correlation.
    pub score: f32,
    /// Its loudest 10 ms, dBFS ([`Recording::level_db`]).
    pub level_db: f32,
}

/// Each spectrogram cell's excess over its band's background, at most
/// [`RANGE_DB`].
fn excess(spec: &[[f32; BANDS]]) -> Vec<[f32; BANDS]> {
    let half = ((BACKGROUND_MS / HOP_MS) as usize).max(1);
    let mut background = vec![[0.0f32; BANDS]; spec.len()];
    let mut column = Vec::new();
    for at in (0..spec.len()).step_by(BACKGROUND_STEP) {
        let (from, to) = (at.saturating_sub(half), (at + half).min(spec.len()));
        let mut level = [0.0f32; BANDS];
        for (b, l) in level.iter_mut().enumerate() {
            column.clear();
            column.extend(spec[from..to].iter().map(|p| p[b]));
            column.sort_by(f32::total_cmp);
            *l = column[((column.len() - 1) as f32 * BACKGROUND_PERCENTILE) as usize];
        }
        for p in &mut background[at..(at + BACKGROUND_STEP).min(spec.len())] {
            *p = level;
        }
    }
    spec.iter()
        .zip(&background)
        .map(|(p, g)| {
            let mut e = [0.0f32; BANDS];
            for b in 0..BANDS {
                e[b] = (p[b] - g[b]).clamp(0.0, RANGE_DB);
            }
            e
        })
        .collect()
}

/// The sounds of `templates` found in `recording`, in order.
pub fn find(recording: &Recording, templates: &[Template]) -> Vec<Found> {
    let spec = recording.spectrogram();
    let x = excess(&spec);
    let len = template_hops();
    let hop = recording.samples_of(HOP_MS);
    if x.len() < len || templates.is_empty() {
        return Vec::new();
    }
    // The best template and its correlation at every point, each over its
    // own bands.
    let mut best = vec![(0usize, f32::NEG_INFINITY); x.len() - len + 1];
    let mut patch = Vec::with_capacity(BANDS * len);
    for (h, slot) in best.iter_mut().enumerate() {
        for (k, t) in templates.iter().enumerate() {
            patch.clear();
            for p in &x[h..h + len] {
                patch.extend(t.bands.iter().map(|&b| p[b]));
            }
            if patch.iter().sum::<f32>() / (patch.len() as f32) < MIN_EXCESS_DB {
                continue;
            }
            let Some(p) = normalized(std::mem::take(&mut patch)) else {
                continue;
            };
            let score: f32 = t.cells.iter().zip(&p).map(|(a, b)| a * b).sum();
            patch = p;
            if score > slot.1 {
                *slot = (k, score);
            }
        }
    }
    let reach = (REFRACTORY_MS / HOP_MS) as usize;
    let mut found = Vec::new();
    for h in 0..best.len() {
        let (k, score) = best[h];
        if score < MIN_SCORE {
            continue;
        }
        let around = &best[h.saturating_sub(reach)..(h + reach + 1).min(best.len())];
        // The first of equal peaks.
        let first_peak = around.iter().all(|(_, s)| *s <= score)
            && best[h.saturating_sub(reach)..h]
                .iter()
                .all(|(_, s)| *s < score);
        if first_peak {
            // Its start: the first point of the match whose excess in the
            // template's bands reaches half the match's most.
            let bands = &templates[k].bands;
            let mean =
                |p: &[f32; BANDS]| bands.iter().map(|&b| p[b]).sum::<f32>() / bands.len() as f32;
            let most = x[h..h + len].iter().map(mean).fold(0.0, f32::max);
            let rise = x[h..h + len]
                .iter()
                .position(|p| mean(p) >= most / 2.0)
                .unwrap_or(0);
            let h = h + rise;
            found.push(Found {
                sample: h * hop,
                template: k,
                score,
                level_db: recording.level_db(h * hop),
            });
        }
    }
    found
}

fn rms(s: &[f32]) -> f32 {
    if s.is_empty() {
        return 0.0;
    }
    (s.iter().map(|v| v * v).sum::<f32>() / s.len() as f32).sqrt()
}

fn db(v: f32) -> f32 {
    20.0 * (v + 1e-9).log10()
}

/// An in-place radix-2 FFT of `re` + i `im` (lengths a power of two).
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let a = -2.0 * PI / len as f32;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let (s, c) = (a * k as f32).sin_cos();
                let (p, q) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[q] * c - im[q] * s, re[q] * s + im[q] * c);
                re[q] = re[p] - tr;
                im[q] = im[p] - ti;
                re[p] += tr;
                im[p] += ti;
            }
        }
        len <<= 1;
    }
}

/// A recording's frames: for each frame of play the sample at which it was
/// let run and at which it was presented (the capture's `<walk>.frames.txt`,
/// `volya-audio-frames 1`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frames {
    pub rows: Vec<(i64, i64, i64)>,
}

impl Frames {
    /// The table's text.
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut lines = text.lines();
        if lines.next().map(str::trim) != Some("volya-audio-frames 1") {
            return Err("not a volya-audio-frames 1 table".into());
        }
        let mut rows = Vec::new();
        for line in lines {
            let words: Vec<&str> = line.split_whitespace().collect();
            if let [f, r, p] = words[..]
                && let (Ok(f), Ok(r), Ok(p)) = (f.parse(), r.parse(), p.parse())
            {
                rows.push((f, r, p));
            }
        }
        if rows.is_empty() {
            return Err("a table of no frames".into());
        }
        Ok(Self { rows })
    }

    /// Our frames, each `samples` long, frame `first` at sample 0.
    pub fn even(first: i64, frames: usize, samples: i64) -> Self {
        let rows = (0..frames as i64)
            .map(|k| (first + k, k * samples, (k + 1) * samples))
            .collect();
        Self { rows }
    }

    /// The table as text, its header naming `walk` and the `rate`.
    pub fn text(&self, walk: &str, rate: u32) -> String {
        let mut s = format!("volya-audio-frames 1\nwalk {walk}\nrate {rate}\n");
        for (f, r, p) in &self.rows {
            s += &format!("{f} {r} {p}\n");
        }
        s
    }

    /// The frame whose update a sound starting at `sample` came from: the
    /// last let run at or before it; `None` before the first.
    pub fn frame_of(&self, sample: usize) -> Option<i64> {
        let s = sample as i64;
        self.rows
            .iter()
            .take_while(|(_, r, _)| *r <= s)
            .last()
            .map(|(f, _, _)| *f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    /// A hit of two decaying partials at `freq`.
    fn hit(rate: u32, freq: f32) -> Vec<f32> {
        (0..rate as usize / 4)
            .map(|k| {
                let t = k as f32 / rate as f32;
                (-t * 25.0).exp()
                    * ((2.0 * PI * freq * t).sin() + 0.5 * (2.0 * PI * freq * 2.7 * t).sin())
            })
            .collect()
    }

    /// A noise of a linear congruential generator, `gain` loud.
    fn noise(n: usize, gain: f32) -> Vec<f32> {
        let mut x = 1u32;
        (0..n)
            .map(|_| {
                x = x.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                gain * ((x >> 16) as f32 / 32_768.0 - 1.0)
            })
            .collect()
    }

    #[test]
    fn known_sounds_found_over_a_background() {
        // Templates at 44.1 kHz, the recording at 48 kHz, as the game's
        // files and the PC's output.
        let low = Recording {
            rate: 44_100,
            samples: hit(44_100, 300.0),
        };
        let high = Recording {
            rate: 44_100,
            samples: hit(44_100, 3_000.0),
        };
        let templates = vec![
            Template::new("low", &low).unwrap(),
            Template::new("high", &high).unwrap(),
        ];
        let mut s = noise(RATE as usize * 3, 0.02);
        let at = [12_000, 36_000, 60_000, 90_000];
        let which = [0, 0, 1, 0];
        for (a, k) in at.iter().zip(which) {
            let h = hit(RATE, [300.0, 3_000.0][k]);
            for (i, v) in h.iter().enumerate() {
                s[a + i] += 0.1 * v;
            }
        }
        let r = Recording {
            rate: RATE,
            samples: s,
        };
        let found = find(&r, &templates);
        assert_eq!(
            found.iter().map(|f| f.template).collect::<Vec<_>>(),
            which,
            "{found:?}"
        );
        for (f, a) in found.iter().zip(at) {
            let ms = (f.sample as f32 - a as f32) / RATE as f32 * 1000.0;
            assert!(ms.abs() <= 20.0, "{} against {a}", f.sample);
            assert!(f.score > 0.7, "{}", f.score);
            assert!((f.level_db - -24.0).abs() < 4.0, "{}", f.level_db);
        }
    }

    #[test]
    fn a_wav_file_and_a_frames_table() {
        let mut wav = b"RIFF\0\0\0\0WAVEfmt \x10\0\0\0\x01\0\x02\0".to_vec();
        wav.extend_from_slice(&44_100u32.to_le_bytes());
        wav.extend_from_slice(&(44_100u32 * 4).to_le_bytes());
        wav.extend_from_slice(&[4, 0, 16, 0]);
        wav.extend_from_slice(b"data\x08\0\0\0");
        for v in [16_384i16, 0, -16_384, -16_384] {
            wav.extend_from_slice(&v.to_le_bytes());
        }
        let r = Recording::from_wav(&wav).unwrap();
        assert_eq!((r.rate, r.samples.clone()), (44_100, vec![0.25, -0.5]));

        let t = Frames::parse(
            "volya-audio-frames 1\nwalk x\nrate 48000\n# c\n10 100 150\n11 300 350\n",
        )
        .unwrap();
        assert_eq!(t.frame_of(50), None);
        assert_eq!(t.frame_of(100), Some(10));
        assert_eq!(t.frame_of(299), Some(10));
        assert_eq!(t.frame_of(5000), Some(11));
        let even = Frames::even(725, 3, 1764);
        assert_eq!(even.frame_of(1764 * 2 + 5), Some(727));
        assert_eq!(Frames::parse(&even.text("w", 44_100)).unwrap(), even);
    }
}
