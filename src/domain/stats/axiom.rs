use crate::domain::item::quality::ItemQuality;
use super::entity::{ALPHA_STAT_MAPS, AlphaStatMap};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsAxiom {
    pub version: u8,
    pub quality: ItemQuality,
    pub save_is_alpha: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct HeaderGeometry {
    pub y_bits: u32,
    pub page_bits: u32,
    pub socket_hint_bits: u32,
    pub has_header_gap: bool,
    pub skip_geometry: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct PropertyRhythm {
    pub id_bits: u32,
    pub value_bits: Option<u32>,
    pub has_terminal_bit: bool,
    pub has_extra_terminal_bit: bool,
}

impl StatsAxiom {
    pub fn new(version: u8, quality: ItemQuality, save_is_alpha: bool) -> Self {
        Self { version, quality, save_is_alpha }
    }

    pub fn is_alpha(&self) -> bool {
        self.save_is_alpha
    }

    /// Maps an Alpha v105 raw stat ID to its effective (standard) ID.
    pub fn map_alpha_id(&self, raw_id: u32) -> u32 {
        if !self.save_is_alpha {
            return raw_id;
        }
        ALPHA_STAT_MAPS
            .iter()
            .find(|m| m.raw_id == raw_id)
            .map(|m| m.effective_id)
            .unwrap_or(raw_id)
    }

    pub fn header_geometry(&self, flags: u32, is_compact: bool) -> HeaderGeometry {
        if self.save_is_alpha {
            if self.version == 5 || self.version == 0 {
                let is_v105_shadow = (flags & (1 << 26)) != 0;
                let is_rw = self.is_runeword(flags);
                
                if is_rw || is_v105_shadow {
                    HeaderGeometry {
                        y_bits: 0,
                        page_bits: 0,
                        socket_hint_bits: 0,
                        has_header_gap: true,
                        skip_geometry: false,
                    }
                } else {
                    HeaderGeometry {
                        y_bits: if is_compact { 0 } else { 4 },
                        page_bits: if is_compact { 0 } else { 3 },
                        socket_hint_bits: if is_compact { 0 } else { 4 },
                        has_header_gap: true,
                        skip_geometry: false,
                    }
                }
            } else if self.version == 1 {
                HeaderGeometry {
                    y_bits: 4,
                    page_bits: 3,
                    socket_hint_bits: 3,
                    has_header_gap: true,
                    skip_geometry: false,
                }
            } else if self.version == 4 {
                HeaderGeometry {
                    y_bits: 0,
                    page_bits: 0,
                    socket_hint_bits: 0,
                    has_header_gap: false,
                    skip_geometry: true,
                }
            } else {
                // Alpha mode but version 0 (e.g. fragments or markers)
                HeaderGeometry {
                    y_bits: 0,
                    page_bits: 0,
                    socket_hint_bits: 0,
                    has_header_gap: false,
                    skip_geometry: true,
                }
            }
        } else {
            // Retail
            HeaderGeometry {
                y_bits: if is_compact { 0 } else { 4 },
                page_bits: if is_compact { 0 } else { 3 },
                socket_hint_bits: if is_compact { 0 } else { 3 },
                has_header_gap: false,
                skip_geometry: is_compact,
            }
        }
    }

    pub fn is_runeword(&self, flags: u32) -> bool {
        if self.save_is_alpha {
            if self.version == 5 || self.version == 1 {
                let is_frag = (flags & (1 << 26)) != 0 || (flags & (1 << 27)) != 0;
                !is_frag && ((flags & (1 << 11)) != 0 || (flags & (1 << 12)) != 0 || (flags & (1 << 13)) != 0 || (flags & 0x800) != 0)
            } else {
                (flags & (1 << 11)) != 0
            }
        } else {
            (flags & (1 << 26)) != 0
        }
    }

    pub fn is_socketed(&self, flags: u32, is_compact: bool) -> bool {
        if self.save_is_alpha {
            if self.version == 5 {
                !is_compact && ((flags & (1 << 23)) != 0 || (flags & (1 << 11)) != 0)
            } else {
                (flags & (1 << 27)) != 0
            }
        } else {
            (flags & (1 << 11)) != 0
        }
    }

    pub fn property_rhythm(&self, _is_runeword: bool, is_shadow: bool, is_compact: bool) -> PropertyRhythm {
        if self.save_is_alpha {
            if self.version == 5 {
                // Alpha v105 Property Rhythm: 9-bit ID, 6-bit Value
                return PropertyRhythm {
                    id_bits: 9,
                    value_bits: Some(6),
                    has_terminal_bit: true,
                    has_extra_terminal_bit: true,
                };
            }
            // Older Alpha versions
            PropertyRhythm {
                id_bits: 9,
                value_bits: if is_compact {
                    None // Use STAT_COSTS (e.g. for quantity)
                } else {
                    // For extended items, Alpha often uses a fixed 9-bit width
                    Some(if is_shadow { 8 } else { 9 })
                },
                has_terminal_bit: true,
                has_extra_terminal_bit: false,
            }
        } else {
            // Retail
            PropertyRhythm {
                id_bits: 9,
                value_bits: None, // use STAT_COSTS
                has_terminal_bit: false,
                has_extra_terminal_bit: false,
            }
        }
    }



    /// Determines the final bit alignment for an item based on consumed bits and version.
    pub fn calculate_alignment(&self, consumed_bits: u64, is_compact: bool, code: &str) -> u64 {
        let mut final_len = consumed_bits;

        // All versions except version 5 (Retail and some Alpha variants) 
        // add a single terminal bit (usually false/0) before alignment.
        if self.version != 5 {
            final_len += 1;
        }

        if self.save_is_alpha {
            let trimmed = code.trim();
            let is_potion = trimmed.starts_with('h') || trimmed.starts_with('m') || (self.version == 5 && trimmed.starts_with('7')) || (trimmed.starts_with('r') && trimmed.len() <= 3);
            let is_scroll = trimmed == "tsc" || trimmed == "isc";

            if is_compact {
                // Alpha v105 forensic: Compact items have specific fixed bit-lengths.
                let min_bits = if is_scroll {
                    72 // 9 bytes for scrolls
                } else if trimmed.starts_with('r') && (trimmed.len() == 3 || (trimmed.len() == 4 && trimmed[1..].chars().all(|c| c.is_ascii_digit()))) {
                    88 // 11 bytes for runes
                } else {
                    80 // 10 bytes for potions and other compacts
                };

                if final_len < min_bits {
                    final_len = min_bits;
                }
            } else if trimmed == "7mgw" && self.version == 5 {
                // Alpha v105 forensic: 7mgw aligns to 112 bits (14 bytes)
                if final_len < 112 {
                    final_len = 112;
                }
            } else if is_potion {
                // Extended potions (unlikely but possible in Alpha)
                if final_len < 80 { final_len = 80; }
            } else if is_scroll {
                // Extended scrolls
                if final_len < 72 { final_len = 72; }
            } else {
                // Alpha v105 forensic: Non-compact items align to at least 88 bits (11 bytes).
                if final_len < 88 {
                    final_len = 88;
                }
            }

            if final_len % 8 != 0 {
                final_len += 8 - (final_len % 8);
            }
        } else if final_len % 8 != 0 {
            // Retail: Only byte align at the end of the item section
            final_len += 8 - (final_len % 8);
        }

        if self.save_is_alpha && crate::item::item_trace_enabled() {
            println!("[DEBUG] calculate_alignment: code='{}', consumed={}, final={}", code.trim(), consumed_bits, final_len);
        }

        final_len
    }

    pub fn reads_defense(&self) -> bool {

        !self.is_alpha()
    }

    pub fn reads_durability(&self) -> bool {
        !self.is_alpha()
    }

    pub fn reads_quantity(&self) -> bool {
        !self.is_alpha()
    }

    pub fn lookup_alpha_map_by_raw(&self, raw_id: u32) -> Option<&'static AlphaStatMap> {

        ALPHA_STAT_MAPS.iter().find(|m| m.raw_id == raw_id)
    }

    pub fn lookup_alpha_map_by_effective(&self, effective_id: u32) -> Option<&'static AlphaStatMap> {
        ALPHA_STAT_MAPS.iter().find(|m| m.effective_id == effective_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alpha_id_mapping() {
        let axiom = StatsAxiom::new(5, ItemQuality::Unique, true);
        assert_eq!(axiom.map_alpha_id(26), 31);   // item_defense_percent
        assert_eq!(axiom.map_alpha_id(312), 72);  // item_durability
        assert_eq!(axiom.map_alpha_id(207), 73);  // item_maxdurability
        assert_eq!(axiom.map_alpha_id(380), 194); // item_indestructible
        assert_eq!(axiom.map_alpha_id(256), 127); // item_allskills
        assert_eq!(axiom.map_alpha_id(496), 99);  // item_fastergethitrate
        assert_eq!(axiom.map_alpha_id(499), 16);  // item_enandefense_percent
        assert_eq!(axiom.map_alpha_id(289), 9);   // maxmana
        assert_eq!(axiom.map_alpha_id(999), 999); // identity mapping for unknown
    }

    #[test]
    fn test_alpha_rhythm() {
        let axiom = StatsAxiom::new(5, ItemQuality::Unique, true);
        let rhythm = axiom.property_rhythm(true, false, false);
        assert_eq!(rhythm.value_bits, Some(6));
        assert!(rhythm.has_terminal_bit);
        assert!(rhythm.has_extra_terminal_bit);
    }

    #[test]
    fn test_alpha_contextual_alignment() {
        let axiom = StatsAxiom::new(5, ItemQuality::Normal, true);
        
        // Scroll (tsc) should align to 72 bits
        assert_eq!(axiom.calculate_alignment(64, true, "tsc"), 72);
        assert_eq!(axiom.calculate_alignment(71, true, "tsc"), 72);
        assert_eq!(axiom.calculate_alignment(72, true, "tsc"), 72);
        
        // Potion (hp1) should align to 80 bits
        assert_eq!(axiom.calculate_alignment(64, true, "hp1"), 80);
        assert_eq!(axiom.calculate_alignment(79, true, "hp1"), 80);
        assert_eq!(axiom.calculate_alignment(80, true, "hp1"), 80);
        
        // Rune (r01) should align to 88 bits
        assert_eq!(axiom.calculate_alignment(64, true, "r01"), 88);
        assert_eq!(axiom.calculate_alignment(87, true, "r01"), 88);
        assert_eq!(axiom.calculate_alignment(88, true, "r01"), 88);
    }
}
