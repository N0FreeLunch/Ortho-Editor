use serde::{Deserialize, Serialize};
use crate::domain::item::entity::ItemBitRange;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum StatMaturityTier {
    #[default]
    Verified,   // Tier 1: Fixture-verified, full Read/Write/Mutate
    Inferred,   // Tier 2: Spec-derived, Read-Only render, bit-preserved
    Speculative, // Tier 3: Diagnostic draft
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct ItemStats {
    pub properties: Vec<ItemProperty>,
    pub set_attributes: Vec<Vec<ItemProperty>>,
    pub runeword_attributes: Vec<ItemProperty>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ItemProperty {
    pub stat_id: u32,
    pub name: String,
    pub param: u32,
    pub raw_value: i32,
    pub value: i32, // After applying save_add if needed
    pub range: ItemBitRange,
    #[serde(default)]
    pub is_opaque: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opaque_bits: Option<Vec<bool>>,
    #[serde(default)]
    pub maturity: StatMaturityTier,
}

impl ItemProperty {
    pub fn new(
        stat_id: u32,
        name: String,
        param: u32,
        raw_value: i32,
        value: i32,
        range: ItemBitRange,
    ) -> Self {
        Self {
            stat_id,
            name,
            param,
            raw_value,
            value,
            range,
            is_opaque: false,
            opaque_bits: None,
            maturity: StatMaturityTier::Verified,
        }
    }

    pub fn new_with_maturity(
        stat_id: u32,
        name: String,
        param: u32,
        raw_value: i32,
        value: i32,
        range: ItemBitRange,
        maturity: StatMaturityTier,
    ) -> Self {
        Self {
            stat_id,
            name,
            param,
            raw_value,
            value,
            range,
            is_opaque: false,
            opaque_bits: None,
            maturity,
        }
    }

    /// Resolves high-level semantic meaning (class skill, skill tab, charges) if applicable.
    pub fn semantics(&self) -> Option<crate::domain::stats::semantics::PropertySemantics> {
        crate::domain::stats::semantics::decode_property_semantics(self)
    }

    pub fn new_opaque(
        stat_id_candidate: u32,
        raw_bits: Vec<bool>,
        range: ItemBitRange,
    ) -> Self {
        Self {
            stat_id: stat_id_candidate,
            name: "opaque_property".to_string(),
            param: 0,
            raw_value: 0,
            value: 0,
            range,
            is_opaque: true,
            opaque_bits: Some(raw_bits),
            maturity: StatMaturityTier::Speculative,
        }
    }

    pub fn is_verified(&self) -> bool {
        self.maturity == StatMaturityTier::Verified
    }

    pub fn is_inferred(&self) -> bool {
        self.maturity == StatMaturityTier::Inferred
    }

    pub fn is_editable(&self) -> bool {
        !self.is_opaque && self.is_verified()
    }

    /// Validates if this property corresponds to a known stat definition and falls within valid value/param ranges.
    pub fn is_valid_stat_range(&self) -> bool {
        if self.is_opaque {
            return false;
        }
        if let Some(cost) = crate::data::stat_costs::STAT_COSTS.iter().find(|s| s.id == self.stat_id) {
            let max_raw_val = if cost.save_bits > 0 && cost.save_bits <= 31 {
                (1i64 << cost.save_bits) - 1
            } else {
                i64::MAX
            };
            let raw = self.raw_value as i64;
            if raw < 0 || raw > max_raw_val {
                return false;
            }

            let max_param_val = if cost.save_param_bits > 0 && cost.save_param_bits <= 31 {
                (1i64 << cost.save_param_bits) - 1
            } else if cost.save_param_bits == 0 {
                0
            } else {
                i64::MAX
            };
            let param = self.param as i64;
            if cost.save_param_bits > 0 && (param < 0 || param > max_param_val) {
                return false;
            }

            true
        } else {
            false
        }
    }
}

use bitstream_io::{BitRead, BitWrite};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Default)]
pub struct ItemStatList {
    pub stats: Vec<ItemProperty>,
}

impl ItemStatList {
    pub fn new() -> Self {
        Self { stats: Vec::new() }
    }

    fn read_bits_raw<R: BitRead>(reader: &mut R, count: usize) -> Result<u32, std::io::Error> {
        let mut value = 0u32;
        for i in 0..count {
            if reader.read_bit()? {
                value |= 1 << i;
            }
        }
        Ok(value)
    }

    fn write_bits_raw<W: BitWrite>(writer: &mut W, count: usize, value: u32) -> Result<(), std::io::Error> {
        for i in 0..count {
            writer.write_bit((value >> i) & 1 != 0)?;
        }
        Ok(())
    }

    pub fn read_versioned<R: BitRead>(
        reader: &mut R,
        schema: &crate::data::VersionStatSchema,
    ) -> Result<Self, std::io::Error> {
        let mut stats = Vec::new();
        loop {
            let id = match Self::read_bits_raw(reader, 9) {
                Ok(id) => id as u16,
                Err(_) => break,
            };
            if id == 0x1FF {
                break;
            }
            if let Some(rule) = schema.get_rule(id) {
                let stat = Self::read_stat_entry(reader, rule)?;
                stats.push(stat);

                // Handle chained stats (e.g. firemindam -> firemaxdam)
                for chained_id in &rule.chain_next {
                    if let Some(chained_rule) = schema.get_rule(*chained_id) {
                        let chained_stat = Self::read_stat_entry(reader, chained_rule)?;
                        stats.push(chained_stat);
                    }
                }
            } else {
                break;
            }
        }
        Ok(ItemStatList { stats })
    }

    fn read_stat_entry<R: BitRead>(
        reader: &mut R,
        rule: &crate::data::StatRule,
    ) -> Result<ItemProperty, std::io::Error> {
        let param = if rule.save_param_bits > 0 {
            Self::read_bits_raw(reader, rule.save_param_bits)?
        } else {
            0
        };

        let raw_value = if rule.save_bits > 0 {
            Self::read_bits_raw(reader, rule.save_bits)? as i32
        } else {
            0
        };

        let value = raw_value - rule.save_add;

        Ok(ItemProperty::new(
            rule.id as u32,
            rule.name.clone(),
            param,
            raw_value,
            value,
            ItemBitRange::default(),
        ))
    }

    pub fn write_versioned<W: BitWrite>(
        &self,
        writer: &mut W,
        schema: &crate::data::VersionStatSchema,
    ) -> Result<(), std::io::Error> {
        let mut i = 0;
        while i < self.stats.len() {
            let stat = &self.stats[i];
            let id = stat.stat_id as u16;

            // Write 9-bit stat ID
            Self::write_bits_raw(writer, 9, id as u32)?;

            if let Some(rule) = schema.get_rule(id) {
                Self::write_stat_entry(writer, stat, rule)?;

                // Write chained stats without 9-bit ID
                for &chained_id in &rule.chain_next {
                    i += 1;
                    if i < self.stats.len() {
                        let chained_stat = &self.stats[i];
                        if let Some(chained_rule) = schema.get_rule(chained_id) {
                            Self::write_stat_entry(writer, chained_stat, chained_rule)?;
                        }
                    }
                }
            }
            i += 1;
        }
        // Write 9-bit terminator (0x1FF = 511)
        Self::write_bits_raw(writer, 9, 0x1FF)?;
        Ok(())
    }

    fn write_stat_entry<W: BitWrite>(
        writer: &mut W,
        stat: &ItemProperty,
        rule: &crate::data::StatRule,
    ) -> Result<(), std::io::Error> {
        if rule.save_param_bits > 0 {
            Self::write_bits_raw(writer, rule.save_param_bits, stat.param)?;
        }
        if rule.save_bits > 0 {
            let raw_value = (stat.value + rule.save_add) as u32;
            Self::write_bits_raw(writer, rule.save_bits, raw_value)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitstream_io::{BitReader, BitWriter, LittleEndian};
    use std::io::Cursor;

    #[test]
    fn test_chained_damage_stat_roundtrip() {
        let schema = crate::data::StatSchemaRegistry::default_v99();

        let mut list = ItemStatList::new();
        // Strength: id 0, value 15
        list.stats.push(ItemProperty::new(
            0,
            "strength".to_string(),
            0,
            47,
            15,
            ItemBitRange::default(),
        ));
        // Fire Min Dam: id 48, value 10
        list.stats.push(ItemProperty::new(
            48,
            "firemindam".to_string(),
            0,
            10,
            10,
            ItemBitRange::default(),
        ));
        // Fire Max Dam: id 49 (chained), value 25
        list.stats.push(ItemProperty::new(
            49,
            "firemaxdam".to_string(),
            0,
            25,
            25,
            ItemBitRange::default(),
        ));

        // Write to buffer
        let mut buffer = Vec::new();
        {
            let mut writer = BitWriter::endian(&mut buffer, LittleEndian);
            list.write_versioned(&mut writer, schema).expect("write should succeed");
            writer.byte_align().expect("align should succeed");
        }

        // Read back
        let mut reader = BitReader::endian(Cursor::new(&buffer), LittleEndian);
        let parsed = ItemStatList::read_versioned(&mut reader, schema).expect("read should succeed");

        assert_eq!(parsed.stats.len(), 3);
        assert_eq!(parsed.stats[0].stat_id, 0);
        assert_eq!(parsed.stats[0].value, 15);
        assert_eq!(parsed.stats[1].stat_id, 48);
        assert_eq!(parsed.stats[1].value, 10);
        assert_eq!(parsed.stats[2].stat_id, 49);
        assert_eq!(parsed.stats[2].value, 25);
    }
}




