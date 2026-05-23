use crate::domain::item::serialization::BitEmitter;
use crate::data::bit_cursor::BitCursor;
use bitstream_io::BitRead;
use std::io;

/// Item Gap Subdomain Combinator
/// Handles the bit-alignment and forensic gaps between item segments.
pub trait GapCombinator {
    fn resolve_bits(&self, version: u32, is_compact: bool) -> usize;
    fn emit(&self, emitter: &mut BitEmitter) -> io::Result<()>;
    fn parse<R: BitRead>(cursor: &mut BitCursor<R>, len: usize) -> io::Result<Self> where Self: Sized;
}

/// Standard 8-bit or dynamic header gap for Alpha v105 items.
#[derive(Debug, Clone, Default)]
pub struct AlphaHeaderGap {
    pub bits: Vec<bool>,
}

impl GapCombinator for AlphaHeaderGap {
    fn resolve_bits(&self, _version: u32, _is_compact: bool) -> usize {
        self.bits.len()
    }

    fn emit(&self, emitter: &mut BitEmitter) -> io::Result<()> {
        for &bit in &self.bits {
            emitter.write_bit(bit)?;
        }
        Ok(())
    }

    fn parse<R: BitRead>(cursor: &mut BitCursor<R>, len: usize) -> io::Result<Self> {
        let bits = cursor.read_bits_as_vec(len as u32).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        Ok(Self { bits })
    }
}
