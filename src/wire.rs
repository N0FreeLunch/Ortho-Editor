use bitstream_io::{BitRead, BitWrite, BitWriter, LittleEndian};
use std::io::{self, Cursor};

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

/// Retains an available raw span with the existing legacy cursor semantics.
/// Incomplete reads return an empty or partial span. The original u32 seek
/// conversion is preserved; callers decide whether completeness is required.
pub(crate) fn read_legacy_source_bits(bytes: &[u8], start_bit: u64, bit_count: u64) -> Vec<bool> {
    if bit_count == 0 {
        return Vec::new();
    }

    let mut reader = bitstream_io::BitReader::endian(Cursor::new(bytes), LittleEndian);
    let mut bits = Vec::with_capacity(bit_count as usize);

    if reader.skip(start_bit as u32).is_ok() {
        for _ in 0..bit_count {
            match reader.read_bit() {
                Ok(bit) => bits.push(bit),
                Err(_) => break,
            }
        }
    }

    bits
}

#[cfg(test)]
mod tests {
    use super::read_legacy_source_bits;
    use d2r_macros::hypothesis_test;

    #[hypothesis_test]
    fn legacy_source_bits_preserve_partial_and_wrapped_seek() {
        let bytes = [0b1010_1101, 0b0000_0011];
        let seven_bits = vec![true, false, true, false, true, true, true];
        assert_eq!(read_legacy_source_bits(&bytes, 3, 7), seven_bits);
        assert_eq!(read_legacy_source_bits(&bytes, 14, 5), vec![false, false]);
        assert!(read_legacy_source_bits(&bytes, 16, 1).is_empty());
        assert!(read_legacy_source_bits(&bytes, 17, 1).is_empty());
        assert!(read_legacy_source_bits(&bytes, u64::MAX, 0).is_empty());
        assert_eq!(
            read_legacy_source_bits(&bytes, (1_u64 << 32) + 3, 7),
            seven_bits
        );
    }
}
