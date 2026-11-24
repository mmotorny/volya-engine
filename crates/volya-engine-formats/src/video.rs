// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The game's videos: Ogg Theora (public formats; Ogg via `ogg`, Theora via
//! `theora_nrp`, a port of libtheora's decoder). Yields whole decoded (coded)
//! Y'CbCr frames; [`Frame::texture_planes`] is the part that becomes a video
//! texture (docs/behavior/startup.md 6.6), [`Frame::picture_planes`] the
//! picture region as the Theora specification crops it.

use std::io::{self, Read, Seek, SeekFrom};

use theora_nrp::{Comment, DecCtx, Info, Packet, PixelFmt, SetupInfo, decode_headerin};

/// Chroma subsampling of a video's frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chroma {
    /// Cb and Cr at half width and half height.
    Yuv420,
    /// Cb and Cr at half width.
    Yuv422,
    /// Cb and Cr at full resolution.
    Yuv444,
}

impl Chroma {
    /// Right shifts from luma to chroma dimensions: (x, y).
    fn shifts(self) -> (u32, u32) {
        match self {
            Self::Yuv420 => (1, 1),
            Self::Yuv422 => (1, 0),
            Self::Yuv444 => (0, 0),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoInfo {
    /// Picture size in pixels (the visible region).
    pub width: u32,
    pub height: u32,
    /// Coded frame size in pixels (multiples of 16).
    pub frame_width: u32,
    pub frame_height: u32,
    /// The header's picture offset, `pic_y` converted to count from the top
    /// of the coded frame (where the Theora specification puts the picture).
    pub pic_x: u32,
    pub pic_y: u32,
    pub chroma: Chroma,
    /// Frame rate as a fraction.
    pub fps_numerator: u32,
    pub fps_denominator: u32,
}

/// One plane of a frame, or a region of one.
#[derive(Clone, Copy, Debug)]
pub struct Plane<'a> {
    pub width: usize,
    pub height: usize,
    /// Bytes between the starts of successive rows in `data`.
    pub stride: usize,
    /// Starts at the region's first pixel; row `r` is
    /// `data[r * stride..r * stride + width]`.
    pub data: &'a [u8],
}

impl Plane<'_> {
    /// Row `r` of the region.
    pub fn row(&self, r: usize) -> &[u8] {
        &self.data[r * self.stride..r * self.stride + self.width]
    }

    /// The `width` × `height` region at column `x`, row `y`.
    fn region(&self, x: usize, y: usize, width: usize, height: usize) -> Self {
        assert!(
            x + width <= self.width && y + height <= self.height,
            "region"
        );
        let start = y * self.stride + x;
        let end = start + (height.max(1) - 1) * self.stride + width;
        Plane {
            width,
            height,
            stride: self.stride,
            data: &self.data[start..end],
        }
    }
}

/// A decoded frame: the whole coded frame's Y', Cb, Cr planes, rows from the
/// top.
#[derive(Clone, Copy, Debug)]
pub struct Frame<'a> {
    /// Index from the start of the video.
    pub number: u64,
    pub coded: [Plane<'a>; 3],
    pub chroma: Chroma,
    /// Picture size in pixels.
    pub width: u32,
    pub height: u32,
    /// The picture offset from the coded frame's top-left corner, as the
    /// Theora specification places the picture.
    pub pic_x: u32,
    pub pic_y: u32,
}

impl<'a> Frame<'a> {
    /// The planes of the picture-sized block at luma (`x`, `y`); the chroma
    /// regions hold every chroma sample a luma pixel of the block uses.
    fn block(&self, x: u32, y: u32) -> [Plane<'a>; 3] {
        let (sx, sy) = self.chroma.shifts();
        let mut i = 0;
        self.coded.map(|p| {
            let (sx, sy) = if i == 0 { (0, 0) } else { (sx, sy) };
            i += 1;
            let (x0, y0) = (x >> sx, y >> sy);
            let width = ((x + self.width + sx) >> sx) - x0;
            let height = ((y + self.height + sy) >> sy) - y0;
            p.region(x0 as usize, y0 as usize, width as usize, height as usize)
        })
    }

    /// What becomes the video texture (docs/behavior/startup.md 6.6): the
    /// top-left picture-width × picture-height block of the coded frame; the
    /// header's picture offset is ignored. For 4:2:0, chroma sample
    /// (⌊x / 2⌋, ⌊y / 2⌋) of these planes belongs to texel (x, y).
    pub fn texture_planes(&self) -> [Plane<'a>; 3] {
        self.block(0, 0)
    }

    /// The picture region at the header's offset, as the Theora
    /// specification (and libtheora, FFmpeg) crops it.
    pub fn picture_planes(&self) -> [Plane<'a>; 3] {
        self.block(self.pic_x, self.pic_y)
    }
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// A Theora video read from an Ogg file. Other streams in the file (audio)
/// are skipped.
pub struct TheoraVideo<R: Read + Seek> {
    reader: Option<ogg::PacketReader<R>>,
    serial: u32,
    info: Info,
    setup: SetupInfo,
    decoder: DecCtx,
    video: VideoInfo,
    /// Video packets seen, for the decoder's packet numbers.
    packets: i64,
    frames: u64,
}

impl<R: Read + Seek> TheoraVideo<R> {
    /// Reads the headers of the file's first Theora stream.
    pub fn open(reader: R) -> io::Result<Self> {
        let mut reader = ogg::PacketReader::new(reader);
        let (mut info, mut comment, mut setup) = (Info::new(), Comment::new(), None);
        let mut serial = None;
        let mut packets = 0;
        loop {
            let packet = reader
                .read_packet()
                .map_err(|e| invalid(format!("Ogg: {e}")))?
                .ok_or_else(|| invalid("no Theora stream"))?;
            if serial.is_some_and(|s| s != packet.stream_serial()) {
                continue;
            }
            let theora = Packet {
                packet: &packet.data,
                b_o_s: i64::from(packets == 0),
                packetno: packets,
                ..Packet::default()
            };
            match decode_headerin(&mut info, &mut comment, &mut setup, &theora) {
                // A header of this stream.
                read if read > 0 => {
                    serial = Some(packet.stream_serial());
                    packets += 1;
                }
                // Some other stream's first packet: keep looking.
                read if read < 0 && serial.is_none() => {}
                read if read < 0 => return Err(invalid("damaged Theora headers")),
                // All three headers were read: the first video packet comes
                // next; rewind so `next_frame` reads it.
                _ => break,
            }
        }
        let setup = setup.ok_or_else(|| invalid("missing Theora setup header"))?;
        let decoder =
            DecCtx::alloc(&info, &setup).ok_or_else(|| invalid("unsupported Theora stream"))?;
        let chroma = match info.pixel_fmt {
            PixelFmt::Pf420 => Chroma::Yuv420,
            PixelFmt::Pf422 => Chroma::Yuv422,
            PixelFmt::Pf444 => Chroma::Yuv444,
            _ => return Err(invalid("reserved Theora pixel format")),
        };
        let video = VideoInfo {
            width: info.pic_width,
            height: info.pic_height,
            frame_width: info.frame_width,
            frame_height: info.frame_height,
            // `theora_nrp` already counts `pic_y` from the top.
            pic_x: info.pic_x,
            pic_y: info.pic_y,
            chroma,
            fps_numerator: info.fps_numerator,
            fps_denominator: info.fps_denominator,
        };
        let mut this = Self {
            reader: Some(reader),
            serial: serial.ok_or_else(|| invalid("no Theora stream"))?,
            info,
            setup,
            decoder,
            video,
            packets: 0,
            frames: 0,
        };
        this.rewind()?;
        Ok(this)
    }

    pub fn info(&self) -> &VideoInfo {
        &self.video
    }

    /// Restarts from the first frame.
    pub fn rewind(&mut self) -> io::Result<()> {
        let mut inner = self.reader.take().expect("reader present").into_inner();
        inner.seek(SeekFrom::Start(0))?;
        self.reader = Some(ogg::PacketReader::new(inner));
        self.decoder = DecCtx::alloc(&self.info, &self.setup)
            .ok_or_else(|| invalid("unsupported Theora stream"))?;
        self.packets = 0;
        self.frames = 0;
        Ok(())
    }

    /// Decodes the next frame, or `None` at the end of the video.
    pub fn next_frame(&mut self) -> io::Result<Option<Frame<'_>>> {
        let reader = self.reader.as_mut().expect("reader present");
        loop {
            let Some(packet) = reader
                .read_packet()
                .map_err(|e| invalid(format!("Ogg: {e}")))?
            else {
                return Ok(None);
            };
            if packet.stream_serial() != self.serial {
                continue;
            }
            let number = self.packets;
            self.packets += 1;
            if number < 3 {
                continue; // The headers, already read.
            }
            let theora = Packet {
                packet: &packet.data,
                packetno: number,
                ..Packet::default()
            };
            if self.decoder.packetin(&theora, None) < 0 {
                return Err(invalid(format!("damaged Theora frame {}", self.frames)));
            }
            break;
        }
        let number = self.frames;
        self.frames += 1;
        let coded = self.decoder.ycbcr_out().map(|p| Plane {
            width: p.width as usize,
            height: p.height as usize,
            stride: p.stride as usize,
            data: p.data,
        });
        let v = &self.video;
        Ok(Some(Frame {
            number,
            coded,
            chroma: v.chroma,
            width: v.width,
            height: v.height,
            pic_x: v.pic_x,
            pic_y: v.pic_y,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 16 × 16 coded 4:2:0 frame whose samples encode their position:
    /// luma = row × 16 + column, chroma = 128 + row × 8 + column.
    fn planes() -> [Vec<u8>; 3] {
        let luma = (0..16).flat_map(|r| (0..16).map(move |c| (r * 16 + c) as u8));
        let chroma = (0..8).flat_map(|r| (0..8).map(move |c| (128 + r * 8 + c) as u8));
        [luma.collect(), chroma.clone().collect(), chroma.collect()]
    }

    /// A 13 × 11 picture at header offset (2, 4) of the coded frame `data`.
    fn frame(data: &[Vec<u8>; 3]) -> Frame<'_> {
        let mut i = 0;
        Frame {
            number: 0,
            coded: data.each_ref().map(|d| {
                let side = if i == 0 { 16 } else { 8 };
                i += 1;
                Plane {
                    width: side,
                    height: side,
                    stride: side,
                    data: d,
                }
            }),
            chroma: Chroma::Yuv420,
            width: 13,
            height: 11,
            pic_x: 2,
            pic_y: 4,
        }
    }

    #[test]
    fn texture_is_the_top_left_block_of_the_coded_frame() {
        let data = planes();
        let [y, cb, cr] = frame(&data).texture_planes();
        assert_eq!((y.width, y.height), (13, 11));
        // Texel (x, y) is coded column x, row y; the offset is ignored.
        assert_eq!(y.row(0)[0], 0);
        assert_eq!(y.row(10)[12], 10 * 16 + 12);
        // Chroma for (⌊x / 2⌋, ⌊y / 2⌋) with x < 13, y < 11: 7 × 6 samples.
        for p in [cb, cr] {
            assert_eq!((p.width, p.height), (7, 6));
            assert_eq!(p.row(0)[0], 128);
            assert_eq!(p.row(5)[6], 128 + 5 * 8 + 6);
        }
    }

    #[test]
    fn picture_is_at_the_header_offset() {
        let data = planes();
        let [y, cb, _] = frame(&data).picture_planes();
        assert_eq!((y.width, y.height), (13, 11));
        assert_eq!(y.row(0)[0], 4 * 16 + 2);
        assert_eq!(cb.row(0)[0], 128 + 2 * 8 + 1);
    }
}
