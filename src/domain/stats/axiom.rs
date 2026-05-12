use crate::domain::item::quality::ItemQuality;
use crate::domain::item::axiom_meta::{ForensicAxiom, ForensicMetadata, Confidence, Intentionality};
use crate::domain::forensic::v105::{V105NudgeAxiom, V105ShadowAxiom, V105HeaderGapAxiom};
use crate::domain::forensic::registry::{get_registry, MappingInfo};
use crate::domain::header::entity::{HeaderAxiom, HeaderGeometry};
use crate::item_trace;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsAxiom {
    pub version: u8,
    pub quality: ItemQuality,
    pub save_is_alpha: bool,
    pub is_personalized: bool,
    pub is_compact: bool,
    pub code: String,
}

impl StatsAxiom {
    pub fn new(version: u8, quality: ItemQuality, save_is_alpha: bool) -> Self {
        Self { version, quality, save_is_alpha, is_personalized: false, is_compact: false, code: String::new() }
    }

    pub fn with_personalization(mut self, is_personalized: bool) -> Self {
        self.is_personalized = is_personalized;
        self
    }

    pub fn with_code(mut self, code: &str) -> Self {
        self.code = code.to_string();
        // Axiom 0344: Blank items and known compact types in Alpha v105 
        // often lack the compact flag despite being structurally compact.
        let trimmed = self.code.trim();
        if self.save_is_alpha && (
            trimmed.is_empty() || 
            trimmed == "tsc" || trimmed == "isc" || 
            (trimmed.starts_with('r') && (trimmed.len() == 3 || (trimmed.len() == 4 && trimmed[1..].chars().all(|c| c.is_ascii_digit())))) ||
            (trimmed.starts_with('h') && trimmed.len() == 3) || // hp1, hp2, etc
            (trimmed.starts_with('m') && trimmed.len() == 3)    // mp1, mp2, etc
        ) {
            self.is_compact = true;
        }
        self
    }

    pub fn with_compact(mut self, is_compact: bool) -> Self {
        self.is_compact = is_compact;
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
        (flags & (1 << 26)) != 0 || self.code == "w8wc" || self.code == "acww" || self.code == "umsw" || self.code == "7pw" || self.code == "oesw" || self.code == "hps7" || self.code == "ics"
    }

    pub fn is_socketed(&self, flags: u32, is_compact: bool) -> bool {
        self.header_axiom().is_socketed(flags, is_compact)
    }

    pub fn is_compact(&self, flags: u32) -> bool {
        if self.save_is_alpha {
            (flags & (1 << 23)) != 0 || (flags & (1 << 21)) != 0
        } else {
            (flags & (1 << 21)) != 0
        }
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

    pub fn is_header_only(&self, flags: u32, code: &str) -> bool {
        if self.is_v105_shadow(flags) { return true; }
        if self.save_is_alpha && self.version == 0 && !code.is_empty() && code.trim().is_empty() {
            return true;
        }
        false
    }

    pub fn header_gap(&self, _code: &str, _flags: u32) -> u32 {
        if !self.is_alpha() {
            return 0;
        }
        // Forensic: Resolved variable gaps in Alpha v105 (e.g., 8wc Idx 1 -> 96 bits)
        // These are distinct from the JM-relative header gap and occur before property parsing.
        if _code.trim() == "8wc" {
            return 96;
        }
        0
    }

    pub fn is_fragment(&self, flags: u32) -> bool {
        self.is_alpha() && ((flags & (1 << 26)) != 0 || (flags & (1 << 27)) != 0)
    }


    pub fn property_rhythm(&self, _is_runeword: bool, _is_shadow: bool, _is_compact: bool, stat_id: u32) -> PropertyRhythm {
        if self.is_alpha() {
            if stat_id == 320 || self.map_alpha_id(stat_id) == 320 {
                return PropertyRhythm {
                    id_bits: 9,
                    value_bits: None,
                    has_terminal_bit: false,
                    has_extra_terminal_bit: false,
                };
            }
            
            // Alpha v105 18-bit rhythm (9+9) is the dominant pattern for most items
            // including Act 5 hybrids and Runewords.
            if self.version == 1 || self.version == 0 || self.version == 2 || _is_runeword || self.code == "Opaque" {
                 return PropertyRhythm {
                    id_bits: 9,
                    value_bits: Some(9),
                    has_terminal_bit: false,
                    has_extra_terminal_bit: false,
                };
            }

            if self.version == 5 {
                // Only native Version 5 (non-runeword) items use 9+6 rhythm.
                return PropertyRhythm {
                    id_bits: 9,
                    value_bits: Some(6),
                    has_terminal_bit: false,
                    has_extra_terminal_bit: false,
                };
            }
            
            // Fallback for other Alpha versions
            PropertyRhythm {
                id_bits: 9,
                value_bits: None, // use STAT_COSTS
                has_terminal_bit: false,
                has_extra_terminal_bit: false,
            }
        } else {
            // Retail rhythm
            PropertyRhythm {
                id_bits: 9,
                value_bits: None,
                has_terminal_bit: true,
                has_extra_terminal_bit: false,
            }
        }
    }



    pub fn resolve_flag_padding(&self, flags: u32, is_socketed: bool) -> u64 {
        let mut padding = 0;
        if is_socketed { padding += 8; }
        if (flags & 0x00000008) != 0 { padding += 8; }
        if (flags & 0x00000010) != 0 { padding += 16; }
        if (flags & 0x00000020) != 0 { padding += 24; }
        if (flags & 0x00000040) != 0 { padding += 32; }
        padding
    }

    /// Determines the final bit alignment for an item based on consumed bits and version.
    pub fn calculate_alignment(&self, consumed_bits: u64, code: &str, flags: u32) -> u64 {
        let mut final_len = consumed_bits;

        // All versions except version 5 and 7 (Retail and some Alpha variants)
        // add a single terminal bit (usually false/0) before alignment.
        // Alpha v105 forensic: Skip this bit for all Alpha items.
        if !self.save_is_alpha && self.version != 5 && self.version != 7 {
            final_len += 1;
        }

        if self.save_is_alpha {
            let reg = get_registry();
            let trimmed = code.trim();
            let is_personalized = self.is_personalized(flags);

            // 1. Initial Minimum Width (Contextual)
            // Alpha v105 forensic: Compact items (Potion/Scroll/Rune) have fixed baselines.
            // Non-compact items (Equipment) use an 88-bit baseline unless overridden.
            let is_compact = self.is_compact;
            let mut min_bits = if is_compact {
                reg.axioms.get("compact_item_fixed_width").cloned().unwrap_or(80)
            } else {
                reg.axioms.get("equipment_fixed_width").cloned().unwrap_or(88)
            };

            // 2. Registry Overrides (Type/Code specific)
            if is_compact {
                let is_tsc = trimmed == "tsc";
                let is_isc = trimmed == "isc";
                if is_tsc {
                    min_bits = 80;
                } else if is_isc {
                    min_bits = reg.axioms.get("scroll_fixed_width").cloned().unwrap_or(72);
                } else if trimmed.starts_with('r') && (trimmed.len() == 3 || (trimmed.len() == 4 && trimmed[1..].chars().all(|c| c.is_ascii_digit()))) {
                    min_bits = reg.axioms.get("rune_fixed_width").cloned().unwrap_or(88);
                }
            }

            if let Some(overrides) = &reg.item_overrides {
                if let Some(item_map) = overrides.get(trimmed) {
                    if let Some(&f_width) = item_map.get("fixed_width") {
                        min_bits = f_width as u64;
                    }
                }
            }

            // Alpha v105 forensic: Early Alpha versions (0, 1, 4, 6) do not use fixed-width min_bits baselines.
            // They rely on bit-packed variable lengths. (Axiom 0337)
            // Alpha v105 forensic: All Alpha items (Version 0, 1, 4, 5, 6, 7) 
            // use fixed-width min_bits baselines if they are compact. (Axiom 0344)
            let apply_min_nudge = !self.is_alpha() || (self.version == 5 || self.version == 7 || self.is_compact);
            if apply_min_nudge && final_len < min_bits {
                final_len = min_bits;
            }

            // 3. Dynamic Padding (Version 5 only)
            // Alpha v105 Forensic: Version 5 equipment adds flag-based padding before alignment.
            // Axiom 0344: Blank items and Runewords bypass this padding.
            if self.version == 5 && !self.is_compact && !self.is_runeword(flags) {
                let is_socketed = self.is_socketed(flags, self.is_compact);
                final_len += self.resolve_flag_padding(flags, is_socketed);
            }

            // 4. Alpha v105 32-bit Alignment Axiom
            // Priority: Personalized items and Compact items bypass the 32-bit forced tail.
            // This is only applied to specific equipment versions (5, 7, 4, 6).
            // Forensic (Axiom 0337): Only Alpha version 5 and 7 (Act 5 prototypes) use 32-bit forced tail-padding.
            // Version 0, 1, 4, and 6 (Initial/Early Alpha) use bit-packed alignment.
            // 4. Alpha v105 32-bit Alignment Axiom
            // Priority: Personalized items, Compact items, and Runewords bypass the 32-bit forced tail.
            if !self.is_compact && !self.is_runeword(flags) && (self.version == 5 || self.version == 7) && !is_personalized {
                if final_len % 32 != 0 {
                    final_len += 32 - (final_len % 32);
                }
            }

            // 5. Final Byte Alignment Fallback
            // Alpha v105 Forensic: All Alpha items bypass forced byte alignment unless strictly required.
            // Axiom 0337/0358: Early and Act 5 Alpha items use bit-packed tail geometry.
            if !self.save_is_alpha {
                if final_len % 8 != 0 {
                    final_len += 8 - (final_len % 8);
                }
            }


            if let Ok(nudge_val) = std::env::var("D2R_ALIGNMENT_NUDGE") {
                if let Ok(nudge_bits) = nudge_val.parse::<i64>() {
                    let apply = if std::env::var("D2R_ALIGNMENT_NUDGE_ALL").is_ok() {
                        true
                    } else {
                        let trimmed = code.trim();
                        trimmed == "acww" || trimmed == "umsw" || trimmed == "7pw" || trimmed == "jav" || trimmed == "Opaque"
                    };

                    if apply {
                        if nudge_bits >= 0 {
                            final_len += nudge_bits as u64;
                        } else {
                            let sub = (-nudge_bits) as u64;
                            if final_len >= sub {
                                final_len -= sub;
                            }
                        }
                    }
                }
            }
        } else if final_len % 8 != 0 {
            // Retail Byte Alignment
            final_len += 8 - (final_len % 8);
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
            let trimmed = self.code.trim();
            // Force 9-bit width if explicitly requested by the rhythm, 
            // UNLESS it's a known variable-width stat in a runeword (like acww Poison Resist).
            if default_width == 9 && trimmed != "acww" {
                return 9;
            }

            let reg = get_registry();
            
            // 1. Check item-specific overrides (highest priority)
            if trimmed == "acww" && raw_id == 256 {
                return 12;
            }
            if trimmed == "acww" && (raw_id == 69 || raw_id == 70 || raw_id == 68 || raw_id == 112) {
                return 6;
            }
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
        let rhythm = axiom.property_rhythm(false, false, false, 0);
        assert_eq!(rhythm.id_bits, 9);
        assert_eq!(rhythm.value_bits, Some(6));
        assert!(!rhythm.has_terminal_bit); 
    }

    #[test]
    fn test_alpha_contextual_alignment() {
        let axiom = StatsAxiom::new(5, ItemQuality::Normal, true);
        
        // Scroll (tsc) should align to 72 bits
        let tsc_axiom = StatsAxiom::new(5, ItemQuality::Normal, true).with_code("tsc");
        assert_eq!(tsc_axiom.calculate_alignment(64, "tsc", 0), 72);
        assert_eq!(tsc_axiom.calculate_alignment(71, "tsc", 0), 72);
        assert_eq!(tsc_axiom.calculate_alignment(72, "tsc", 0), 72);
        
        // Potion (hp1) should align to 80 bits
        let potion_axiom = StatsAxiom::new(5, ItemQuality::Normal, true).with_compact(true).with_code("hp1");
        assert_eq!(potion_axiom.calculate_alignment(64, "hp1", 0), 80);
        assert_eq!(potion_axiom.calculate_alignment(79, "hp1", 0), 80);
        assert_eq!(potion_axiom.calculate_alignment(80, "hp1", 0), 80);
        
        // Rune (r01) should align to 88 bits
        let rune_axiom = StatsAxiom::new(5, ItemQuality::Normal, true).with_compact(true).with_code("r01");
        assert_eq!(rune_axiom.calculate_alignment(64, "r01", 0), 88);
        assert_eq!(rune_axiom.calculate_alignment(87, "r01", 0), 88);
        assert_eq!(rune_axiom.calculate_alignment(88, "r01", 0), 88);
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
