use std::io::{self, Cursor};
use bitstream_io::{BitRead, BitReader, BitWrite, BitWriter, LittleEndian};
use crate::data::stat_costs::{StatCost, STAT_COSTS};

fn stat_cost(stat_id: u32) -> Option<&'static StatCost> {
    STAT_COSTS.iter().find(|stat| stat.id == stat_id)
}

fn read_bits_dynamic(
    reader: &mut BitReader<Cursor<&[u8]>, LittleEndian>,
    count: u32,
) -> io::Result<u32> {
    let mut value = 0;
    for i in 0..count {
        if reader.read_bit()? {
            value |= 1 << i;
        }
    }
    Ok(value)
}

fn write_bits_dynamic<W: BitWrite>(
    writer: &mut W,
    count: u32,
    value: u32,
) -> io::Result<()> {
    for i in 0..count {
        writer.write_bit((value >> i) & 1 != 0)?;
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub struct AttributeEntry {
    pub stat_id: u32,
    pub param: u32,
    pub raw_value: u32,
    pub opaque_bits: Option<Vec<bool>>,
}

#[derive(Clone, Debug)]
pub struct AttributeSection {
    pub entries: Vec<AttributeEntry>,
    pub raw_bytes: Vec<u8>,
}

impl AttributeSection {
    pub fn parse(bytes: &[u8], gf_pos: usize, if_pos: usize) -> io::Result<Self> {
        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap_or([0; 4]));
        let is_alpha = version == 105 || version == 0x69;
        
        // Skip the 'gf' marker (2 bytes) before parsing the bitstream
        let bitstream_start = gf_pos + 2;
        let bitstream_end = if_pos;
        
        let mut reader = BitReader::endian(
            Cursor::new(&bytes[bitstream_start..bitstream_end]),
            LittleEndian,
        );
        let raw_bytes = bytes[gf_pos..if_pos].to_vec();
        let mut entries = Vec::new();
        let total_bits = ((bitstream_end - bitstream_start) * 8) as u64;
        loop {
            let pos = reader.position_in_bits()?;
            if total_bits.saturating_sub(pos) < 9 {
                break;
            }
            let stat_id = reader.read::<9, u32>()?;
            if stat_id == 0x1FF {
                break;
            }
            let cost = stat_cost(stat_id);
            if let Some(cost) = cost {
                let remaining = total_bits.saturating_sub(reader.position_in_bits()?);
                if (cost.save_param_bits as u64) > remaining {
                    break;
                }
                let param = if cost.save_param_bits > 0 {
                    read_bits_dynamic(&mut reader, cost.save_param_bits as u32)?
                } else {
                    0
                };
                let save_bits = char_stat_save_bits(stat_id, is_alpha);
                let remaining = total_bits.saturating_sub(reader.position_in_bits()?);
                if (save_bits as u64) > remaining {
                    break;
                }
                let raw_value = if save_bits > 0 {
                    read_bits_dynamic(&mut reader, save_bits as u32)?
                } else {
                    0
                };
                entries.push(AttributeEntry {
                    stat_id,
                    param,
                    raw_value,
                    opaque_bits: None,
                });
            } else {
                // Unknown stat ID: collect remaining bits as opaque block
                let mut bits = Vec::new();
                while let Ok(bit) = reader.read_bit() {
                    bits.push(bit);
                }
                entries.push(AttributeEntry {
                    stat_id,
                    param: 0,
                    raw_value: 0,
                    opaque_bits: Some(bits),
                });
                break; // After one opaque block, we stop because we don't know the next stat boundary
            }
        }
        Ok(AttributeSection { entries, raw_bytes })
    }

    pub fn to_bytes(&self, is_alpha: bool) -> io::Result<Vec<u8>> {
        self.to_bytes_from_entries(is_alpha)
    }

    pub fn to_bytes_from_entries(&self, is_alpha: bool) -> io::Result<Vec<u8>> {
        let mut buf = vec![b'g', b'f'];
        let mut rest = self.to_bytes_bits(is_alpha).map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        buf.append(&mut rest);
        Ok(buf)
    }

    pub fn to_bytes_bits(&self, alpha_mode: bool) -> io::Result<Vec<u8>> {
        let mut writer = BitWriter::endian(Vec::new(), LittleEndian);

        if alpha_mode {
            // Refactored to use entries directly to ensure bit-perfect control
            let mut written_bits: u64 = 0;
            for entry in &self.entries {
                // Write Stat ID (9 bits)
                write_bits_dynamic(&mut writer, 9, entry.stat_id)?;
                written_bits += 9;

                // Get stat-specific bit widths for Alpha v105
                let (bits, param_bits) = match entry.stat_id {
                    0..=11 => (10, 0), // Base stats
                    12 => (7, 0),     // Level
                    13 => (32, 0),    // Experience
                    14 => (25, 0),    // Gold
                    15 => (25, 0),    // GoldBank
                    85 => (9, 0),     // item_addexperience (Alpha v105 verified width)
                    _ => (12, 0),     // Default for unknown Alpha stats
                };

                // Write parameter if present
                if param_bits > 0 {
                    write_bits_dynamic(&mut writer, param_bits, entry.param)?;
                    written_bits += param_bits as u64;
                }

                // Write value
                write_bits_dynamic(&mut writer, bits, entry.raw_value)?;
                written_bits += bits as u64;
            }

            // Write terminator (9 bits: 0x1FF)
            write_bits_dynamic(&mut writer, 9, 0x1FF as u32)?;
            written_bits += 9;

            // Apply forensic alignment padding for Alpha v105
            let bits_to_align = (8 - (written_bits % 8)) % 8;

            if bits_to_align == 7 {
                // Heuristic verified in Discussion 0231: 
                // Level 1 characters (missing stat 13) use zero padding.
                // Level 2+ characters (has stat 13) use 0x01 padding.
                let has_exp = self.entries.iter().any(|e| e.stat_id == 13);
                let padding_val = if has_exp { 0x01 } else { 0x00 };
                write_bits_dynamic(&mut writer, 7, padding_val)?;
            } else if bits_to_align > 0 {
                write_bits_dynamic(&mut writer, bits_to_align as u32, 0)?;
            }
        } else {
            // Retail logic
            for entry in &self.entries {
                if entry.stat_id == 5 {
                    continue;
                }

                if let Some(ref bits) = entry.opaque_bits {
                    write_bits_dynamic(&mut writer, 9, entry.stat_id)?;
                    for &bit in bits {
                        writer.write_bit(bit)?;
                    }
                    continue;
                }

                let bits = char_stat_save_bits(entry.stat_id, false);
                let cost = stat_cost(entry.stat_id);
                let param_bits = cost.map(|c| c.save_param_bits as u32).unwrap_or(0);

                write_bits_dynamic(&mut writer, 9, entry.stat_id)?;
                if param_bits > 0 {
                    write_bits_dynamic(&mut writer, param_bits, entry.param)?;
                }
                write_bits_dynamic(&mut writer, bits, entry.raw_value)?;
            }

            // Terminator 0x1FF
            write_bits_dynamic(&mut writer, 9, 0x1FFu32)?;
            writer.byte_align()?;
        }

        Ok(writer.into_writer())
    }

    pub fn set_raw(&mut self, stat_id: u32, raw_value: u32) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.stat_id == stat_id) {
            e.raw_value = raw_value;
        } else {
            self.entries.push(AttributeEntry {
                stat_id,
                param: 0,
                raw_value,
                opaque_bits: None,
            });
        }
    }

    pub fn set_value(&mut self, stat_id: u32, logical_value: i32, save_add: i32) {
        self.set_raw(stat_id, (logical_value + save_add) as u32);
    }

    pub fn actual_value(&self, stat_id: u32, is_alpha: bool) -> Option<i32> {
        self.entries
            .iter()
            .find(|entry| entry.stat_id == stat_id)
            .and_then(|entry| {
                let save_add = char_stat_save_add(stat_id, is_alpha);
                Some(entry.raw_value as i32 - save_add)
            })
    }
}

pub fn char_stat_save_add(stat_id: u32, is_alpha: bool) -> i32 {
    if is_alpha {
        0
    } else {
        match stat_id {
            0 | 1 | 2 | 3 => 32, // Strength, Energy, Dexterity, Vitality usually have +32 in stat_costs
            _ => stat_cost(stat_id).map(|c| c.save_add).unwrap_or(0),
        }
    }
}

pub fn char_stat_save_bits(stat_id: u32, is_alpha: bool) -> u32 {
    if is_alpha {
        // Alpha v105 Research: Core stats 0-3 and 12-13 are present, but 4-5 are often undefined/skipped.
        // We exclude 4 and 5 to prevent DLC Editor 'Undefined' crashes.
        match stat_id {
            0 | 1 | 2 | 3 | 4 => 10,
            5 => 8,
            6 | 7 | 8 | 9 | 10 | 11 => 21,
            12 => 7,
            13 => 32,
            14 | 15 => 25,
            85 => 8, // Alkor Reward Stat (stat_id 85) confirmed as 17-bit (9-bit ID + 8-bit Val)
            _ => stat_cost(stat_id).map(|c| c.save_bits as u32).unwrap_or(0)
        }
    } else {
        match stat_id {
            0 | 1 | 2 | 3 | 4 => 10,
            5 => 8,
            6 | 7 | 8 | 9 | 10 | 11 => 21,
            12 => 7,
            13 => 32,
            14 | 15 => 25,
            _ => stat_cost(stat_id).map(|c| c.save_bits as u32).unwrap_or(0)
        }
    }
}
