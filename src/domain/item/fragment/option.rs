use super::{BitFragment, FragmentContext, FragmentError, FragmentType};
use crate::data::stat_costs::STAT_COSTS;
use bitstream_io::{BitRead, BitWrite};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionFragment {
    pub stat_id: u16,
    pub bit_width: u8,
    pub value: i32,
    pub param: Option<u32>,
    pub param_bits: Option<u8>,
    pub add_bias: i32,
}

impl OptionFragment {
    pub fn new(stat_id: u16, bit_width: u8, value: i32) -> Self {
        let (expected_bits, add_bias) = Self::lookup_stat_width(stat_id).unwrap_or((bit_width, 0));
        let width = if bit_width == 0 { expected_bits } else { bit_width };
        Self {
            stat_id,
            bit_width: width,
            value,
            param: None,
            param_bits: None,
            add_bias,
        }
    }

    pub fn with_param(mut self, param: u32, param_bits: u8) -> Self {
        self.param = Some(param);
        self.param_bits = Some(param_bits);
        self
    }

    pub fn with_bias(mut self, add_bias: i32) -> Self {
        self.add_bias = add_bias;
        self
    }

    pub fn lookup_stat_width(stat_id: u16) -> Option<(u8, i32)> {
        STAT_COSTS.iter().find(|s| s.id == stat_id as u32).map(|s| {
            (s.save_bits as u8, s.save_add as i32)
        })
    }
}

impl BitFragment for OptionFragment {
    fn fragment_type(&self) -> FragmentType {
        FragmentType::Option
    }

    fn bit_len(&self) -> usize {
        let mut len = 9 + self.bit_width as usize;
        if let Some(p_bits) = self.param_bits {
            len += p_bits as usize;
        }
        len
    }

    fn encode_to_bits<W: BitWrite>(&self, writer: &mut W) -> Result<(), FragmentError> {
        // Write 9-bit stat ID
        writer
            .write::<9, u16>(self.stat_id)
            .map_err(|e| FragmentError::Io(e.to_string()))?;

        // Write param bits if present
        if let (Some(param), Some(p_bits)) = (self.param, self.param_bits) {
            for i in 0..p_bits {
                let bit = ((param >> i) & 1) != 0;
                writer
                    .write_bit(bit)
                    .map_err(|e| FragmentError::Io(e.to_string()))?;
            }
        }

        // Write value with add_bias adjustment
        let raw_val = (self.value + self.add_bias) as u32;
        for i in 0..self.bit_width {
            let bit = ((raw_val >> i) & 1) != 0;
            writer
                .write_bit(bit)
                .map_err(|e| FragmentError::Io(e.to_string()))?;
        }

        Ok(())
    }

    fn decode_from_bits<R: BitRead>(
        reader: &mut R,
        _ctx: &FragmentContext,
    ) -> Result<Self, FragmentError> {
        let stat_id: u16 = reader
            .read::<9, u16>()
            .map_err(|e| FragmentError::Io(e.to_string()))?;

        if stat_id == 0x1FF {
            return Err(FragmentError::InvalidOption(
                "Encountered end-of-stat-list sentinel (0x1FF)".to_string(),
            ));
        }

        let (save_bits, add_bias) = Self::lookup_stat_width(stat_id).unwrap_or((8, 0));

        let mut raw_val = 0u32;
        for i in 0..save_bits {
            let bit = reader
                .read_bit()
                .map_err(|e| FragmentError::Io(e.to_string()))?;
            if bit {
                raw_val |= 1 << i;
            }
        }
        let value = (raw_val as i32) - add_bias;

        Ok(OptionFragment {
            stat_id,
            bit_width: save_bits,
            value,
            param: None,
            param_bits: None,
            add_bias,
        })
    }
}
