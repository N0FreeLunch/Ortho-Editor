use super::header::HeaderFragment;
use super::option::OptionFragment;
use super::{BitFragment, FragmentError};
use bitstream_io::{BitWrite, BitWriter, LittleEndian};
use std::io::Cursor;

#[derive(Debug, Default)]
pub struct ItemAssembler {
    pub header: Option<HeaderFragment>,
    pub options: Vec<OptionFragment>,
    pub raw_payloads: Vec<Vec<bool>>,
    pub include_terminator: bool,
    pub byte_align: bool,
}

impl ItemAssembler {
    pub fn new() -> Self {
        Self {
            header: None,
            options: Vec::new(),
            raw_payloads: Vec::new(),
            include_terminator: true,
            byte_align: true,
        }
    }

    pub fn with_header(mut self, header: HeaderFragment) -> Self {
        self.header = Some(header);
        self
    }

    pub fn add_option(mut self, option: OptionFragment) -> Self {
        self.options.push(option);
        self
    }

    pub fn with_options(mut self, options: Vec<OptionFragment>) -> Self {
        self.options = options;
        self
    }

    pub fn add_raw_payload(mut self, payload: Vec<bool>) -> Self {
        self.raw_payloads.push(payload);
        self
    }

    pub fn with_terminator(mut self, enabled: bool) -> Self {
        self.include_terminator = enabled;
        self
    }

    pub fn with_byte_align(mut self, enabled: bool) -> Self {
        self.byte_align = enabled;
        self
    }

    pub fn total_bit_len(&self) -> usize {
        let mut bits = 0;
        if let Some(ref h) = self.header {
            bits += h.bit_len();
        }
        for opt in &self.options {
            bits += opt.bit_len();
        }
        if self.include_terminator {
            bits += 9;
        }
        for payload in &self.raw_payloads {
            bits += payload.len();
        }
        bits
    }

    pub fn assemble_to_bits<W: BitWrite>(&self, writer: &mut W) -> Result<usize, FragmentError> {
        let mut total_bits = 0;

        // 1. Encode Header if present
        if let Some(ref h) = self.header {
            h.encode_to_bits(writer)?;
            total_bits += h.bit_len();
        }

        // 2. Encode Options
        for opt in &self.options {
            opt.encode_to_bits(writer)?;
            total_bits += opt.bit_len();
        }

        // 3. Write 0x1FF sentinel if enabled
        if self.include_terminator {
            writer
                .write::<9, u16>(0x1FF)
                .map_err(|e| FragmentError::Io(e.to_string()))?;
            total_bits += 9;
        }

        // 4. Encode Raw bit payloads
        for payload in &self.raw_payloads {
            for &bit in payload {
                writer
                    .write_bit(bit)
                    .map_err(|e| FragmentError::Io(e.to_string()))?;
            }
            total_bits += payload.len();
        }

        // 5. Byte align if requested
        if self.byte_align {
            writer
                .byte_align()
                .map_err(|e| FragmentError::Io(e.to_string()))?;
        }

        Ok(total_bits)
    }

    pub fn assemble_to_bytes(&self) -> Result<Vec<u8>, FragmentError> {
        let mut buffer = Vec::new();
        {
            let mut writer = BitWriter::endian(&mut buffer, LittleEndian);
            self.assemble_to_bits(&mut writer)?;
        }
        Ok(buffer)
    }
}
