use super::{BitFragment, FragmentContext, FragmentError, FragmentType};
use bitstream_io::{BitRead, BitWrite};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeaderFragment {
    pub version: u8,
    pub is_compact: bool,
    pub identified: bool,
    pub socketed: bool,
    pub quality: u8,
    pub code: [u8; 4],
    pub flags: u32,
    pub raw_bits: Vec<bool>,
}

impl HeaderFragment {
    pub fn new(
        version: u8,
        is_compact: bool,
        identified: bool,
        socketed: bool,
        quality: u8,
        code: [u8; 4],
        flags: u32,
    ) -> Self {
        Self {
            version,
            is_compact,
            identified,
            socketed,
            quality,
            code,
            flags,
            raw_bits: Vec::new(),
        }
    }
}

impl BitFragment for HeaderFragment {
    fn fragment_type(&self) -> FragmentType {
        FragmentType::Header
    }

    fn bit_len(&self) -> usize {
        if !self.raw_bits.is_empty() {
            self.raw_bits.len()
        } else {
            111
        }
    }

    fn encode_to_bits<W: BitWrite>(&self, writer: &mut W) -> Result<(), FragmentError> {
        if !self.raw_bits.is_empty() {
            for &bit in &self.raw_bits {
                writer
                    .write_bit(bit)
                    .map_err(|e| FragmentError::Io(e.to_string()))?;
            }
            return Ok(());
        }

        // Encode JM signature (16 bits: 'J' 'M')
        writer
            .write::<16, u16>(0x4D4A)
            .map_err(|e| FragmentError::Io(e.to_string()))?;

        // Encode flags (32 bits)
        writer
            .write::<32, u32>(self.flags)
            .map_err(|e| FragmentError::Io(e.to_string()))?;

        // Encode version (3 bits masked)
        writer
            .write::<3, u32>((self.version & 0x07) as u32)
            .map_err(|e| FragmentError::Io(e.to_string()))?;

        // Encode mode/location/coordinates (simplified default 28 bits)
        writer
            .write::<28, u32>(0)
            .map_err(|e| FragmentError::Io(e.to_string()))?;

        // Encode item code (32 bits)
        for &byte in &self.code {
            writer
                .write::<8, u8>(byte)
                .map_err(|e| FragmentError::Io(e.to_string()))?;
        }

        Ok(())
    }

    fn decode_from_bits<R: BitRead>(
        reader: &mut R,
        ctx: &FragmentContext,
    ) -> Result<Self, FragmentError> {
        let signature: u16 = reader
            .read::<16, u16>()
            .map_err(|e| FragmentError::Io(e.to_string()))?;
        if signature != 0x4D4A {
            return Err(FragmentError::InvalidHeader(format!(
                "Invalid header signature: 0x{:04X}",
                signature
            )));
        }

        let flags: u32 = reader
            .read::<32, u32>()
            .map_err(|e| FragmentError::Io(e.to_string()))?;
        let version_bits: u32 = reader
            .read::<3, u32>()
            .map_err(|e| FragmentError::Io(e.to_string()))?;
        let version = if version_bits == 0 {
            ctx.version
        } else {
            version_bits as u8
        };

        let _mode_loc: u32 = reader
            .read::<28, u32>()
            .map_err(|e| FragmentError::Io(e.to_string()))?;

        let mut code = [0u8; 4];
        for i in 0..4 {
            code[i] = reader
                .read::<8, u8>()
                .map_err(|e| FragmentError::Io(e.to_string()))?;
        }

        let identified = (flags & (1 << 4)) != 0;
        let socketed = (flags & (1 << 11)) != 0;
        let is_compact = (flags & (1 << 5)) != 0;
        let quality = ((flags >> 19) & 0x0F) as u8;

        Ok(HeaderFragment {
            version,
            is_compact,
            identified,
            socketed,
            quality,
            code,
            flags,
            raw_bits: Vec::new(),
        })
    }
}
