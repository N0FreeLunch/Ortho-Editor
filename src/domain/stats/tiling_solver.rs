use crate::data::VersionStatSchema;
use crate::domain::item::entity::ItemBitRange;
use crate::domain::stats::entity::{ItemProperty, ItemStatList};

#[derive(Debug, Clone)]
struct DPState {
    score: i32,
    prev_pos: Option<usize>,
    stats_added: Vec<ItemProperty>,
    is_terminal: bool,
}

pub struct BitSpaceTilingSolver;

impl BitSpaceTilingSolver {
    fn read_u32_from_bits(bits: &[bool], start: usize, count: usize) -> Option<u32> {
        if start + count > bits.len() {
            return None;
        }
        let mut value = 0u32;
        for i in 0..count {
            if bits[start + i] {
                value |= 1 << i;
            }
        }
        Some(value)
    }

    /// Solves the optimal bit space tiling path reaching the 0x1FF terminator using dynamic programming.
    pub fn solve(
        bits: &[bool],
        schema: &VersionStatSchema,
    ) -> Result<(ItemStatList, usize, f64), &'static str> {
        let n = bits.len();
        if n < 9 {
            return Err("Bitstream too short for stat block");
        }

        let mut dp: Vec<Option<DPState>> = vec![None; n + 1];
        dp[0] = Some(DPState {
            score: 0,
            prev_pos: None,
            stats_added: Vec::new(),
            is_terminal: false,
        });

        let mut best_terminal_pos = None;
        let mut best_terminal_score = i32::MIN;

        for pos in 0..n {
            let current_state = match &dp[pos] {
                Some(state) => state.clone(),
                None => continue,
            };

            // Option A: Check for 0x1FF (511) terminator
            if pos + 9 <= n {
                if let Some(id) = Self::read_u32_from_bits(bits, pos, 9) {
                    if id == 0x1FF {
                        let term_score = current_state.score + 1000;
                        let next_pos = pos + 9;
                        let replace = match &dp[next_pos] {
                            Some(existing) => term_score > existing.score,
                            None => true,
                        };
                        if replace {
                            dp[next_pos] = Some(DPState {
                                score: term_score,
                                prev_pos: Some(pos),
                                stats_added: Vec::new(),
                                is_terminal: true,
                            });
                            if term_score > best_terminal_score {
                                best_terminal_score = term_score;
                                best_terminal_pos = Some(next_pos);
                            }
                        }
                    }
                }
            }

            // Option B: Match known stat block and chained stats
            if pos + 9 <= n {
                if let Some(id) = Self::read_u32_from_bits(bits, pos, 9) {
                    let stat_id = id as u16;
                    if stat_id != 0x1FF {
                        if let Some(rule) = schema.get_rule(stat_id) {
                            let mut offset = pos + 9;
                            let mut valid = true;
                            let mut parsed_props = Vec::new();

                            // Read base stat param and value
                            let param = if rule.save_param_bits > 0 {
                                if let Some(p) = Self::read_u32_from_bits(bits, offset, rule.save_param_bits) {
                                    offset += rule.save_param_bits;
                                    p
                                } else {
                                    valid = false;
                                    0
                                }
                            } else {
                                0
                            };

                            let raw_value = if valid && rule.save_bits > 0 {
                                if let Some(v) = Self::read_u32_from_bits(bits, offset, rule.save_bits) {
                                    offset += rule.save_bits;
                                    v as i32
                                } else {
                                    valid = false;
                                    0
                                }
                            } else {
                                0
                            };

                            if valid {
                                let value = raw_value - rule.save_add;
                                parsed_props.push(ItemProperty::new(
                                    rule.id as u32,
                                    rule.name.clone(),
                                    param,
                                    raw_value,
                                    value,
                                    ItemBitRange::default(),
                                ));

                                // Read chained stats without 9-bit ID
                                for &chained_id in &rule.chain_next {
                                    if let Some(chained_rule) = schema.get_rule(chained_id) {
                                        let chained_param = if chained_rule.save_param_bits > 0 {
                                            if let Some(p) = Self::read_u32_from_bits(bits, offset, chained_rule.save_param_bits) {
                                                offset += chained_rule.save_param_bits;
                                                p
                                            } else {
                                                valid = false;
                                                0
                                            }
                                        } else {
                                            0
                                        };

                                        let chained_raw = if valid && chained_rule.save_bits > 0 {
                                            if let Some(v) = Self::read_u32_from_bits(bits, offset, chained_rule.save_bits) {
                                                offset += chained_rule.save_bits;
                                                v as i32
                                            } else {
                                                valid = false;
                                                0
                                            }
                                        } else {
                                            0
                                        };

                                        if valid {
                                            let chained_val = chained_raw - chained_rule.save_add;
                                            parsed_props.push(ItemProperty::new(
                                                chained_rule.id as u32,
                                                chained_rule.name.clone(),
                                                chained_param,
                                                chained_raw,
                                                chained_val,
                                                ItemBitRange::default(),
                                            ));
                                        }
                                    }
                                }

                                if valid && offset <= n {
                                    let consumed_bits = offset - pos;
                                    let new_score = current_state.score + (consumed_bits as i32 * 10);
                                    let replace = match &dp[offset] {
                                        Some(existing) => new_score > existing.score,
                                        None => true,
                                    };
                                    if replace {
                                        dp[offset] = Some(DPState {
                                            score: new_score,
                                            prev_pos: Some(pos),
                                            stats_added: parsed_props,
                                            is_terminal: false,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Option C: 1-bit gap step (fallback exploration with penalty)
            if pos + 1 <= n {
                let gap_score = current_state.score - 5;
                let replace = match &dp[pos + 1] {
                    Some(existing) => gap_score > existing.score,
                    None => true,
                };
                if replace {
                    dp[pos + 1] = Some(DPState {
                        score: gap_score,
                        prev_pos: Some(pos),
                        stats_added: Vec::new(),
                        is_terminal: false,
                    });
                }
            }
        }

        // Backtrack to reconstruct the optimal list
        let end_pos = best_terminal_pos.unwrap_or(n);
        let mut curr = end_pos;
        let mut all_stats = Vec::new();
        let mut matched_bits_count = 0;

        while let Some(state) = &dp[curr] {
            if let Some(prev) = state.prev_pos {
                if !state.stats_added.is_empty() {
                    matched_bits_count += curr - prev;
                    for stat in state.stats_added.iter().rev() {
                        all_stats.push(stat.clone());
                    }
                }
                curr = prev;
            } else {
                break;
            }
        }

        all_stats.reverse();
        let coverage_rate = if n > 9 {
            (matched_bits_count as f64 / (n - 9) as f64).min(1.0) * 100.0
        } else {
            100.0
        };

        Ok((ItemStatList { stats: all_stats }, end_pos, coverage_rate))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitstream_io::{BitWrite, BitWriter, LittleEndian};

    #[test]
    fn test_dp_solver_recovers_complex_chained_stats() {
        let schema = crate::data::StatSchemaRegistry::default_v99();

        let mut list = ItemStatList::new();
        list.stats.push(ItemProperty::new(
            0,
            "strength".to_string(),
            0,
            47,
            15,
            ItemBitRange::default(),
        ));
        list.stats.push(ItemProperty::new(
            48,
            "firemindam".to_string(),
            0,
            10,
            10,
            ItemBitRange::default(),
        ));
        list.stats.push(ItemProperty::new(
            49,
            "firemaxdam".to_string(),
            0,
            25,
            25,
            ItemBitRange::default(),
        ));

        let mut buffer = Vec::new();
        {
            let mut writer = BitWriter::endian(&mut buffer, LittleEndian);
            list.write_versioned(&mut writer, schema).unwrap();
            writer.byte_align().unwrap();
        }

        // Convert buffer to bit array
        let mut bits = Vec::new();
        for byte in buffer {
            for i in 0..8 {
                bits.push((byte >> i) & 1 != 0);
            }
        }

        let (solved_list, consumed, coverage) = BitSpaceTilingSolver::solve(&bits, schema).unwrap();
        assert_eq!(solved_list.stats.len(), 3);
        assert_eq!(solved_list.stats[0].stat_id, 0);
        assert_eq!(solved_list.stats[0].value, 15);
        assert_eq!(solved_list.stats[1].stat_id, 48);
        assert_eq!(solved_list.stats[1].value, 10);
        assert_eq!(solved_list.stats[2].stat_id, 49);
        assert_eq!(solved_list.stats[2].value, 25);
        assert!(coverage >= 90.0);
        assert!(consumed > 0);
    }
}
