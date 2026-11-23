// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! The game's videos: Ogg Theora (public formats; Ogg via `ogg`, Theora via
//! `theora_nrp`, a port of libtheora's decoder). Yields Y'CbCr frames cropped
//! to the picture region, ready for the renderer.

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
    pub chroma: Chroma,
    /// Frame rate as a fraction.
    pub fps_numerator: u32,
    pub fps_denominator: u32,
}

/// One plane of a frame, cropped to the picture region.
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
}

/// A decoded frame: Y', Cb, Cr.
#[derive(Clone, Copy, Debug)]
pub struct Frame<'a> {
    /// Index from the start of the video.
    pub number: u64,
    pub planes: [Plane<'a>; 3],
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
        let (sx, sy) = self.video.chroma.shifts();
        let info = &self.info;
        let buffer = self.decoder.ycbcr_out();
        let crop = |i: usize| {
            let p = &buffer[i];
            let (sx, sy) = if i == 0 { (0, 0) } else { (sx, sy) };
            let (x0, y0) = (info.pic_x >> sx, info.pic_y >> sy);
            let width = ((info.pic_x + info.pic_width + sx) >> sx) - x0;
            let height = ((info.pic_y + info.pic_height + sy) >> sy) - y0;
            let stride = p.stride as usize;
            let start = y0 as usize * stride + x0 as usize;
            let end = start + (height as usize - 1) * stride + width as usize;
            Plane {
                width: width as usize,
                height: height as usize,
                stride,
                data: &p.data[start..end],
            }
        };
        Ok(Some(Frame {
            number,
            planes: [crop(0), crop(1), crop(2)],
        }))
    }
}
