// SPDX-FileCopyrightText: 2026 Maksym Motorny
// SPDX-License-Identifier: BSD-3-Clause

//! Packets: byte strings produced and consumed field by field with no padding
//! or alignment (docs/formats/sav.md, conventions). [`Reader`] fails on
//! overrun; [`Writer`] appends the same encodings.

use std::io;

use crate::cp1251;

pub(crate) fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// Reads fields in order from a packet.
#[derive(Clone, Debug)]
pub struct Reader<'a> {
    data: &'a [u8],
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    /// Bytes not read yet.
    pub fn rest(&self) -> &'a [u8] {
        self.data
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Fails unless every byte was read; `what` names the packet.
    pub fn finish(&self, what: &str) -> io::Result<()> {
        if self.data.is_empty() {
            Ok(())
        } else {
            Err(invalid(format!(
                "{what}: {} bytes left after the last field",
                self.data.len()
            )))
        }
    }

    pub fn take(&mut self, n: usize) -> io::Result<&'a [u8]> {
        if n > self.data.len() {
            return Err(invalid(format!(
                "packet truncated: {n} bytes wanted, {} left",
                self.data.len()
            )));
        }
        let (head, rest) = self.data.split_at(n);
        self.data = rest;
        Ok(head)
    }

    fn array<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        Ok(self.take(N)?.try_into().unwrap())
    }

    pub fn u8(&mut self) -> io::Result<u8> {
        Ok(self.array::<1>()?[0])
    }

    pub fn u16(&mut self) -> io::Result<u16> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    pub fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    pub fn s32(&mut self) -> io::Result<i32> {
        Ok(i32::from_le_bytes(self.array()?))
    }

    pub fn u64(&mut self) -> io::Result<u64> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    pub fn float(&mut self) -> io::Result<f32> {
        Ok(f32::from_le_bytes(self.array()?))
    }

    pub fn float3(&mut self) -> io::Result<[f32; 3]> {
        Ok([self.float()?, self.float()?, self.float()?])
    }

    /// A `bool8`: 0 or 1; other values are rejected (our choice: the spec
    /// defines only those two, and keeping them as `bool` must round-trip).
    pub fn bool8(&mut self) -> io::Result<bool> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            b => Err(invalid(format!("bool8 holds {b}"))),
        }
    }

    /// A `stringZ`, decoded from Windows-1251 (a bijection, so it encodes
    /// back to the same bytes).
    pub fn string(&mut self) -> io::Result<String> {
        let end = self
            .data
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| invalid("unterminated string"))?;
        let s = cp1251::decode(&self.data[..end]);
        self.data = &self.data[end + 1..];
        Ok(s)
    }

    /// `u32` count, then that many items.
    pub fn list<T>(
        &mut self,
        mut item: impl FnMut(&mut Self) -> io::Result<T>,
    ) -> io::Result<Vec<T>> {
        let count = self.u32()? as usize;
        // Each item takes at least one byte: bounds the allocation.
        let mut items = Vec::with_capacity(count.min(self.data.len()));
        for _ in 0..count {
            items.push(item(self)?);
        }
        Ok(items)
    }
}

/// Appends fields to a packet.
#[derive(Clone, Debug, Default)]
pub struct Writer {
    pub data: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.data
    }

    pub fn bytes(&mut self, bytes: &[u8]) {
        self.data.extend_from_slice(bytes);
    }

    pub fn u8(&mut self, v: u8) {
        self.data.push(v);
    }

    pub fn u16(&mut self, v: u16) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn u32(&mut self, v: u32) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn s32(&mut self, v: i32) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn u64(&mut self, v: u64) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn float(&mut self, v: f32) {
        self.bytes(&v.to_le_bytes());
    }

    pub fn float3(&mut self, v: [f32; 3]) {
        v.into_iter().for_each(|f| self.float(f));
    }

    pub fn bool8(&mut self, v: bool) {
        self.u8(u8::from(v));
    }

    /// A `stringZ`: fails if the text has a character Windows-1251 lacks or a
    /// NUL.
    pub fn string(&mut self, s: &str) -> io::Result<()> {
        let bytes = cp1251::encode(s)
            .filter(|b| !b.contains(&0))
            .ok_or_else(|| invalid(format!("{s:?} is not a Windows-1251 stringZ")))?;
        self.bytes(&bytes);
        self.u8(0);
        Ok(())
    }

    /// `u32` count, then the items.
    pub fn list<T>(&mut self, items: &[T], mut item: impl FnMut(&mut Self, &T)) -> io::Result<()> {
        self.u32(count(items.len(), "list")?);
        items.iter().for_each(|i| item(self, i));
        Ok(())
    }
}

/// `n` as a `u32` count.
pub fn count(n: usize, what: &str) -> io::Result<u32> {
    u32::try_from(n).map_err(|_| invalid(format!("{what}: {n} items do not fit a u32 count")))
}

/// `n` as a `u16` length.
pub fn length16(n: usize, what: &str) -> io::Result<u16> {
    u16::try_from(n).map_err(|_| invalid(format!("{what}: {n} bytes do not fit a u16 length")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_round_trip() {
        let mut w = Writer::new();
        w.u8(1);
        w.u16(0x0203);
        w.u32(7);
        w.s32(-1);
        w.u64(63_471_533_400_000);
        w.float(0.7);
        w.float3([1.0, -2.0, 3.5]);
        w.bool8(true);
        w.string("Привет").unwrap();
        w.list(&[4u16, 5], |w, &v| w.u16(v)).unwrap();
        assert!(w.string("a\0b").is_err());
        let bytes = w.into_inner();
        let mut r = Reader::new(&bytes);
        assert_eq!(r.u8().unwrap(), 1);
        assert_eq!(r.u16().unwrap(), 0x0203);
        assert_eq!(r.u32().unwrap(), 7);
        assert_eq!(r.s32().unwrap(), -1);
        assert_eq!(r.u64().unwrap(), 63_471_533_400_000);
        assert_eq!(r.float().unwrap(), 0.7);
        assert_eq!(r.float3().unwrap(), [1.0, -2.0, 3.5]);
        assert!(r.bool8().unwrap());
        assert_eq!(r.string().unwrap(), "Привет");
        assert_eq!(r.list(|r| r.u16()).unwrap(), [4, 5]);
        r.finish("test").unwrap();
        assert!(r.u8().is_err());
        assert!(Reader::new(&[2]).bool8().is_err());
        assert!(Reader::new(b"abc").string().is_err());
        assert!(
            Reader::new(&[0xFF, 0xFF, 0xFF, 0xFF])
                .list(|r| r.u8())
                .is_err()
        );
    }
}
