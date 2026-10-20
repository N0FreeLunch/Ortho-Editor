use bitstream_io::{BitWrite, BitWriter, LittleEndian};
use std::io;

pub struct BitEmitter {
    writer: BitWriter<Vec<u8>, LittleEndian>,
    written: u64,
    bits: Vec<bool>,
}

impl BitEmitter {
    pub fn new() -> Self {
        BitEmitter {
            writer: BitWriter::endian(Vec::new(), LittleEndian),
            written: 0,
            bits: Vec::new(),
        }
    }

    pub fn write_bit(&mut self, bit: bool) -> io::Result<()> {
        self.writer.write_bit(bit)?;
        self.written += 1;
        self.bits.push(bit);
        Ok(())
    }

    pub fn into_bits(self) -> Vec<bool> {
        self.bits
    }

    pub fn bits(&self) -> &[bool] {
        &self.bits
    }

    pub fn write_bits(&mut self, value: u32, count: u32) -> io::Result<()> {
        if count == 0 {
            return Ok(());
        }
        for i in 0..count {
            let bit = if i < 32 { (value >> i) & 1 != 0 } else { false };
            self.write_bit(bit)?;
        }
        Ok(())
    }

    pub fn write_bits_u64(&mut self, value: u64, count: u32) -> io::Result<()> {
        if count == 0 {
            return Ok(());
        }
        for i in 0..count {
            let bit = if i < 64 { (value >> i) & 1 != 0 } else { false };
            self.write_bit(bit)?;
        }
        Ok(())
    }

    pub fn extend_bits<I>(&mut self, bits: I) -> io::Result<()>
    where
        I: IntoIterator<Item = bool>,
    {
        for bit in bits {
            self.write_bit(bit)?;
        }
        Ok(())
    }

    pub fn byte_align(&mut self) -> io::Result<()> {
        let padding = (8 - (self.written % 8)) % 8;
        self.writer.byte_align()?;
        self.written += padding;
        Ok(())
    }

    pub fn written_bits(&self) -> u64 {
        self.written
    }

    pub fn into_bytes(mut self) -> Vec<u8> {
        let _ = self.byte_align();
        self.writer.into_writer()
    }
}

pub(in crate::domain::item) fn d2r_v3_write_checked(
    emitter: &mut BitEmitter,
    value: u64,
    width: u32,
    field: &str,
) -> io::Result<()> {
    if width > 32 || value >= (1u64 << width) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("D2R v3 {field} value {value} does not fit {width} bits"),
        ));
    }
    emitter.write_bits(value as u32, width)
}
