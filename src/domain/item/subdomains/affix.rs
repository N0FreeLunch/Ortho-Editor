use crate::domain::item::serialization::BitEmitter;
use crate::data::bit_cursor::BitCursor;
use bitstream_io::BitRead;
use std::io;

/// Item Affix Subdomain Combinator
/// Handles the parsing and emission of magic, rare, and unique affixes.
pub trait AffixCombinator {
    fn emit(&self, emitter: &mut BitEmitter) -> io::Result<()>;
    fn parse<R: BitRead>(cursor: &mut BitCursor<R>) -> io::Result<Self> where Self: Sized;
}

/// Magic item affixes (prefix and suffix).
#[derive(Debug, Clone, Default)]
pub struct MagicAffixSegment {
    pub prefix: Option<u16>,
    pub suffix: Option<u16>,
}

impl AffixCombinator for MagicAffixSegment {
    fn emit(&self, emitter: &mut BitEmitter) -> io::Result<()> {
        emitter.write_bits(self.prefix.unwrap_or(0) as u32, 11)?;
        emitter.write_bits(self.suffix.unwrap_or(0) as u32, 11)?;
        Ok(())
    }

    fn parse<R: BitRead>(cursor: &mut BitCursor<R>) -> io::Result<Self> {
        let prefix = cursor.read_bits::<u16>(11).ok();
        let suffix = cursor.read_bits::<u16>(11).ok();
        Ok(Self { prefix, suffix })
    }
}

/// Unique item affix (Unique ID).
#[derive(Debug, Clone, Default)]
pub struct UniqueAffixSegment {
    pub unique_id: Option<u16>,
}

impl AffixCombinator for UniqueAffixSegment {
    fn emit(&self, emitter: &mut BitEmitter) -> io::Result<()> {
        if let Some(id) = self.unique_id {
            emitter.write_bits(id as u32, 12)?;
        }
        Ok(())
    }

    fn parse<R: BitRead>(cursor: &mut BitCursor<R>) -> io::Result<Self> {
        let unique_id = cursor.read_bits::<u16>(12).ok();
        Ok(Self { unique_id })
    }
}

/// Rare item affixes (dynamic loop).
#[derive(Debug, Clone, Default)]
pub struct RareAffixSegment {
    pub names: [Option<u8>; 2],
    pub affixes: [Option<u16>; 6],
}

impl AffixCombinator for RareAffixSegment {
    fn emit(&self, emitter: &mut BitEmitter) -> io::Result<()> {
        emitter.write_bits(self.names[0].unwrap_or(0) as u32, 8)?;
        emitter.write_bits(self.names[1].unwrap_or(0) as u32, 8)?;

        for affix in self.affixes {
            if let Some(a) = affix {
                emitter.write_bit(true)?;
                emitter.write_bits(a as u32, 11)?;
            } else {
                emitter.write_bit(false)?;
            }
        }
        Ok(())
    }

    fn parse<R: BitRead>(cursor: &mut BitCursor<R>) -> io::Result<Self> {
        let n1 = cursor.read_bits::<u8>(8).ok();
        let n2 = cursor.read_bits::<u8>(8).ok();
        let mut affixes = [None; 6];
        for i in 0..6 {
            if cursor.read_bit().unwrap_or(false) {
                affixes[i] = cursor.read_bits::<u16>(11).ok();
            }
        }
        Ok(Self { names: [n1, n2], affixes })
    }
}
