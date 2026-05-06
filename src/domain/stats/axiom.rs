use crate::domain::item::quality::ItemQuality;
use crate::domain::item::axiom_meta::{ForensicAxiom, ForensicMetadata, Confidence, Intentionality};
use crate::domain::forensic::v105::{V105NudgeAxiom, V105ShadowAxiom, V105HeaderGapAxiom};
use crate::domain::forensic::registry::{get_registry, MappingInfo};
use crate::domain::header::entity::{HeaderAxiom, HeaderGeometry};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsAxiom {
    pub version: u8,
    pub quality: ItemQuality,
    pub save_is_alpha: bool,
    pub is_personalized: bool,
    pub code: String,
}

impl StatsAxiom {
    pub fn new(version: u8, quality: ItemQuality, save_is_alpha: bool) -> Self {
        Self { version, quality, save_is_alpha, is_personalized: false, code: String::new() }
    }

    pub fn with_personalization(mut self, is_personalized: bool) -> Self {
        self.is_personalized = is_personalized;
        self
    }

    pub fn with_code(mut self, code: &str) -> Self {
        self.code = code.to_string();
        self
    }

    fn header_axiom(&self) -> HeaderAxiom {
        HeaderAxiom::new(self.version, self.save_is_alpha)
    }
}

impl ForensicAxiom for StatsAxiom {
    fn metadata(&self) -> ForensicMetadata {
        if self.save_is_alpha {
            let parts = vec![
                V105NudgeAxiom.metadata(),
                V105ShadowAxiom.metadata(),
                V105HeaderGapAxiom.metadata(),
                ForensicMetadata::new(
                    Confidence::StrongPattern,
                    Intentionality::Structural,
                    format!("Alpha v105 Version {} Stat mapping and property rhythm rules", self.version)
                )
            ];
            ForensicMetadata::aggregate(&parts)
        } else {
            ForensicMetadata::new(
                Confidence::VerifiedTruth,
                Intentionality::Structural,
                "Standard Retail item bitstream layout (1.10 - 1.14d)"
            )
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PropertyRhythm {
    pub id_bits: u32,
    pub value_bits: Option<u32>,
    pub has_terminal_bit: bool,
    pub has_extra_terminal_bit: bool,
}

impl StatsAxiom {
    pub fn is_alpha(&self) -> bool {
        self.save_is_alpha && (self.version == 5 || self.version == 1 || self.version == 2 || self.version == 0 || self.version == 7 || self.version == 4 || self.version == 6)
    }

    /// Maps an Alpha v105 raw stat ID to its effective (standard) ID.
    pub fn map_alpha_id(&self, raw_id: u32) -> u32 {
        if !self.is_alpha() {
            return raw_id;
        }
        let reg = get_registry();
        reg.mappings
            .get(&raw_id.to_string())
            .map(|m| m.effective_id)
            .unwrap_or(raw_id)
    }

    pub fn header_geometry(&self, flags: u32, is_compact: bool) -> HeaderGeometry {
        self.header_axiom().header_geometry(flags, is_compact, self.is_personalized)
    }

    pub fn is_runeword(&self, flags: u32) -> bool {
        self.header_axiom().is_runeword(flags)
    }

    pub fn is_socketed(&self, flags: u32, is_compact: bool) -> bool {
        self.header_axiom().is_socketed(flags, is_compact)
    }

    pub fn is_compact(&self, flags: u32) -> bool {
        self.header_axiom().is_compact(flags)
    }

    pub fn is_ethereal(&self, flags: u32) -> bool {
        self.header_axiom().is_ethereal(flags)
    }

    pub fn is_identified(&self, flags: u32) -> bool {
        self.header_axiom().is_identified(flags)
    }


    pub fn is_personalized(&self, flags: u32) -> bool {
        self.header_axiom().is_personalized(flags)
    }

    pub fn is_v105_shadow(&self, flags: u32) -> bool {
        self.header_axiom().is_v105_shadow(flags)
    }

    pub fn is_fragment(&self, flags: u32) -> bool {
        self.is_alpha() && ((flags & (1 << 26)) != 0 || (flags & (1 << 27)) != 0)
    }


    pub fn property_rhythm(&self, _is_runeword: bool, _is_shadow: bool, _is_compact: bool) -> PropertyRhythm {
        if self.is_alpha() && (self.version == 5 || self.version == 1 || self.version == 2 || self.version == 0) {
            // All Alpha items use 9-bit ID, 6-bit Value (fixture-verified)
            // Terminator is 9-bits (111111111), no extra terminal bits like Retail.
            PropertyRhythm {
                id_bits: 9,
                value_bits: Some(6),
                has_terminal_bit: false,
                has_extra_terminal_bit: false,
            }
        } else {
            // Retail and hybrids (e.g. version 4, 6, 7 in Alpha)
            PropertyRhythm {
                id_bits: 9,
                value_bits: None, // use STAT_COSTS
                has_terminal_bit: self.is_alpha(),
                has_extra_terminal_bit: self.is_alpha(),
            }
        }
    }



    /// Determines the final bit alignment for an item based on consumed bits and version.
    pub fn calculate_alignment(&self, consumed_bits: u64, is_compact: bool, code: &str, flags: u32) -> u64 {
        let mut final_len = consumed_bits;

        // All versions except version 5 and 7 (Retail and some Alpha variants) 
        // add a single terminal bit (usually false/0) before alignment.
        if self.version != 5 && self.version != 7 {
            final_len += 1;
        }

        if self.save_is_alpha {
            let reg = get_registry();
            let trimmed = code.trim();
            let is_potion = trimmed.starts_with('h') || trimmed.starts_with('m') || (self.version == 5 && trimmed.starts_with('7')) || (trimmed.starts_with('r') && trimmed.len() <= 3);
            let is_scroll = trimmed == "tsc" || trimmed == "isc";

            if is_compact {
                // Alpha v105 forensic: Compact items have specific fixed bit-lengths.
                let mut min_bits = reg.axioms.get("compact_item_fixed_width").cloned().unwrap_or(80);
                
                if is_scroll {
                    min_bits = reg.axioms.get("scroll_fixed_width").cloned().unwrap_or(72);
                } else if trimmed.starts_with('r') && (trimmed.len() == 3 || (trimmed.len() == 4 && trimmed[1..].chars().all(|c| c.is_ascii_digit()))) {
                    min_bits = reg.axioms.get("rune_fixed_width").cloned().unwrap_or(88);
                }

                if final_len < min_bits {
                    final_len = min_bits;
                }
            } else if trimmed == "7mgw" && self.version == 5 {
                let min_bits = reg.axioms.get("7mgw_fixed_width").cloned().unwrap_or(112);
                if final_len < min_bits {
                    final_len = min_bits;
                }
            } else if is_potion {
                let min_bits = reg.axioms.get("compact_item_fixed_width").cloned().unwrap_or(80);
                if final_len < min_bits { final_len = min_bits; }
            } else if is_scroll {
                let min_bits = reg.axioms.get("scroll_fixed_width").cloned().unwrap_or(72);
                if final_len < min_bits { final_len = min_bits; }
            } else {
                let mut min_bits = reg.axioms.get("rune_fixed_width").cloned().unwrap_or(88);
                
                // Check for item-specific fixed width overrides
                if let Some(overrides) = &reg.item_overrides {
                    if let Some(item_map) = overrides.get(trimmed) {
                        if let Some(&f_width) = item_map.get("fixed_width") {
                            min_bits = f_width as u64;
                        }
                    }
                }

                if final_len < min_bits {
                    final_len = min_bits;
                }
                
                // Alpha v105 32-bit Alignment Axiom
                if (self.version == 5 || self.version == 1 || self.version == 0 || self.version == 7 || self.version == 4 || self.version == 6) && !self.is_personalized(flags) {
                    if final_len % 32 != 0 {
                        final_len += 32 - (final_len % 32);
                    }
                    // Shadow nudge: shadow items are actually 8 bits shorter than full alignment
                    if self.is_v105_shadow(flags) {
                        final_len -= 8;
                    }
                }
            }

            if final_len % 8 != 0 {
                final_len += 8 - (final_len % 8);
            }
            
            if self.version == 2 {
                final_len += 8;
            }

            if self.version == 5 || self.version == 1 || self.version == 7 || self.version == 6 {
                let is_shadow = self.is_v105_shadow(flags);
                let is_rw = self.header_axiom().is_runeword(flags);
                let mut is_v105_summary = false;
                if (self.version == 5 || self.version == 6) && !is_shadow && !is_rw {
                    is_v105_summary = crate::domain::item::serialization::is_v105_summary_code(trimmed);
                }

                if !is_compact && !is_v105_summary && !self.is_personalized(flags) {
                    if final_len % 32 != 0 {
                        final_len += 32 - (final_len % 32);
                    }
                    if is_shadow {
                        final_len -= 8;
                    }
                }
            }

            if (self.version == 5 || self.version == 6 || self.version == 7) && (flags & (1 << 11)) != 0 && !is_compact {
                final_len += 16;
            }
        } else if final_len % 8 != 0 {
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

    pub fn lookup_alpha_map_by_raw(&self, raw_id: u32) -> Option<MappingInfo> {
        let reg = get_registry();
        reg.mappings.get(&raw_id.to_string()).cloned()
    }

    pub fn lookup_alpha_map_by_effective(&self, effective_id: u32) -> Option<MappingInfo> {
        let reg = get_registry();
        reg.mappings.values().find(|m| m.effective_id == effective_id).cloned()
    }

    /// Determines the bit-width for a stat value in Alpha v105 forensic mode.
    pub fn stat_bit_width(&self, raw_id: u32, default_width: u32) -> u32 {
        if self.is_alpha() {
            let reg = get_registry();
            let trimmed = self.code.trim();

            // 1. Check item-specific overrides (highest priority)
            if let Some(overrides) = &reg.item_overrides {
                if let Some(item_map) = overrides.get(trimmed) {
                    if let Some(&width) = item_map.get(&raw_id.to_string()) {
                        return width;
                    }
                }
            }

            // 2. Check stat-specific defaults from mappings
            if let Some(mapping) = self.lookup_alpha_map_by_raw(raw_id) {
                if let Some(bits) = mapping.save_bits {
                    return bits;
                }
            }

            // 3. Fallback to generic stat widths in registry
            if let Some(stat_info) = reg.stats.get(&raw_id.to_string()) {
                return stat_info.width;
            }
        }
        default_width
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
        assert_eq!(axiom.calculate_alignment(64, true, "tsc", 0), 72);
        assert_eq!(axiom.calculate_alignment(71, true, "tsc", 0), 72);
        assert_eq!(axiom.calculate_alignment(72, true, "tsc", 0), 72);
        
        // Potion (hp1) should align to 80 bits
        assert_eq!(axiom.calculate_alignment(64, true, "hp1", 0), 80);
        assert_eq!(axiom.calculate_alignment(79, true, "hp1", 0), 80);
        assert_eq!(axiom.calculate_alignment(80, true, "hp1", 0), 80);
        
        // Rune (r01) should align to 88 bits
        assert_eq!(axiom.calculate_alignment(64, true, "r01", 0), 88);
        assert_eq!(axiom.calculate_alignment(87, true, "r01", 0), 88);
        assert_eq!(axiom.calculate_alignment(88, true, "r01", 0), 88);
    }

    #[test]
    fn test_stats_axiom_forensic_metadata() {
        let retail = StatsAxiom::new(14, ItemQuality::Unique, false);
        let alpha = StatsAxiom::new(5, ItemQuality::Unique, true);

        assert_eq!(retail.metadata().confidence, Confidence::VerifiedTruth);
        // Aggregated confidence is the weakest link. 
        // V105HeaderGapAxiom is EmergingHypothesis, so the whole axiom becomes EmergingHypothesis.
        assert_eq!(alpha.metadata().confidence, Confidence::EmergingHypothesis);
        assert!(alpha.metadata().rationale.contains("Alpha v105 Version 5"));
    }
}
