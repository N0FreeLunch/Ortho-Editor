use crate::domain::forensic::registry::{get_registry, MappingInfo};
use crate::domain::forensic::v105::{V105HeaderGapAxiom, V105NudgeAxiom, V105ShadowAxiom};
use crate::domain::header::entity::{HeaderAxiom, HeaderGeometry};
use crate::domain::item::axiom_meta::{
    Confidence, ForensicAxiom, ForensicMetadata, Intentionality,
};
use crate::domain::item::quality::ItemQuality;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsAxiom {
    pub version: u8,
    pub quality: ItemQuality,
    pub save_is_alpha: bool,
    pub is_personalized: bool,
    pub is_compact: bool,
    pub is_socketed: bool,
    pub code: String,
    pub idx: usize,
}

impl StatsAxiom {
    pub fn new(version: u8, quality: ItemQuality, save_is_alpha: bool) -> Self {
        Self {
            version,
            quality,
            save_is_alpha,
            is_personalized: false,
            is_compact: false,
            is_socketed: false,
            code: String::new(),
            idx: 0,
        }
    }

    pub fn with_index(mut self, idx: usize) -> Self {
        self.idx = idx;
        self
    }

    pub fn with_socketed(mut self, is_socketed: bool) -> Self {
        self.is_socketed = is_socketed;
        self
    }

    pub fn with_personalization(mut self, is_personalized: bool) -> Self {
        self.is_personalized = is_personalized;
        self
    }

    pub fn with_code(mut self, code: &str) -> Self {
        self.code = code.to_string();
        let trimmed = self.code.trim();
        if self.save_is_alpha {
            let reg = get_registry();
            let mut is_compact = self.is_compact
                || crate::domain::forensic::v105::axioms::is_v105_summary_code(code);

            if trimmed.is_empty()
                && (self.version == 5
                    || self.version == 0
                    || self.version == 1
                    || self.version == 2
                    || self.version == 4
                    || self.version == 6)
            {
                is_compact = true;
            } else {
                if let Some(overrides) = &reg.item_overrides {
                    if let Some(map) = overrides.get(trimmed) {
                        if let Some(&val) = map.get("is_compact") {
                            is_compact = val != 0;
                            self.is_compact = is_compact;
                            return self;
                        }
                    }
                }

                if let Some(codes) = &reg.forced_compact_codes {
                    if codes.iter().any(|c| c == trimmed) {
                        is_compact = true;
                    }
                }

                if matches!(trimmed, "cm1" | "cm2" | "cm3")
                    && (self.quality <= crate::domain::item::ItemQuality::Normal
                        || self.quality == crate::domain::item::ItemQuality::Low)
                {
                    is_compact = true;
                }
            }

            self.is_compact = is_compact;
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
                    format!(
                        "Alpha v105 Version {} Stat mapping and property rhythm rules",
                        self.version
                    ),
                ),
            ];
            ForensicMetadata::aggregate(&parts)
        } else {
            ForensicMetadata::new(
                Confidence::VerifiedTruth,
                Intentionality::Structural,
                "Standard Retail item bitstream layout (1.10 - 1.14d)",
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

impl PropertyRhythm {
    fn from_layout(layout: &crate::data::generated::layout_policy::PropertyRhythmLayout) -> Self {
        Self {
            id_bits: u32::from(layout.id_bits),
            value_bits: layout.value_bits.map(u32::from),
            has_terminal_bit: layout.has_terminal_bit,
            has_extra_terminal_bit: layout.has_extra_terminal_bit,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactLayoutPolicy {
    AlphaV105,
    Retail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeEncoding {
    Huffman,
    Ascii3x8,
}

impl StatsAxiom {
    fn admits_legacy_alpha_stats_header(&self) -> bool {
        self.save_is_alpha
            && crate::data::generated::layout_policy::ALPHA_ITEM_HEADER_ADMISSION_POLICY
                .stats_versions
                .contains(&self.version)
    }

    pub fn is_alpha(&self) -> bool {
        self.admits_legacy_alpha_stats_header()
    }

    /// Whitelist for Alpha stat mapping (includes version 7).
    pub fn uses_alpha_mapping(&self) -> bool {
        self.admits_legacy_alpha_stats_header()
    }

    /// Maps an Alpha v105 raw stat ID to its effective (standard) ID.
    pub fn map_alpha_id(&self, raw_id: u32) -> u32 {
        if !self.uses_alpha_mapping() {
            return raw_id;
        }
        let reg = get_registry();
        reg.mappings
            .get(&raw_id.to_string())
            .map(|m| m.effective_id)
            .unwrap_or(raw_id)
    }

    /// Tests the legacy parameter-omission policy using explicit caller inputs.
    pub(crate) fn omits_property_parameter(&self, code: &str, header_version: u8) -> bool {
        self.save_is_alpha
            && crate::data::generated::layout_policy::ALPHA_V105_OMITTED_PROPERTY_PARAMETER_RULES
                .iter()
                .any(|rule| {
                    rule.item_code == code.trim() && rule.header_version == header_version
                })
    }

    /// Selects the default value width before caller-specific registry overrides.
    pub(crate) fn default_property_value_width(
        &self,
        header_version: u8,
        stat_save_bits: Option<u32>,
        rhythm: PropertyRhythm,
    ) -> u32 {
        let default_width = match stat_save_bits {
            Some(save_bits) => rhythm.value_bits.unwrap_or(save_bits),
            None => u32::from(crate::data::generated::layout_policy::UNKNOWN_PROPERTY_VALUE_BITS),
        };
        if !self.save_is_alpha {
            return default_width;
        }
        crate::data::generated::layout_policy::ALPHA_V105_PROPERTY_DEFAULT_WIDTH_RULES
            .iter()
            .find(|rule| {
                rule.header_version == header_version
                    && u32::from(rule.id_bits) == rhythm.id_bits
                    && u32::from(rule.from_value_bits) == default_width
            })
            .map_or(default_width, |rule| u32::from(rule.to_value_bits))
    }

    /// Determines if a specific Alpha v105 version/item-type requires 9-bit TVS padding
    /// after the property terminator (511).
    pub fn has_tvs_padding(&self, is_runeword: bool) -> bool {
        self.tvs_padding_bits(is_runeword).is_some()
    }

    pub(crate) fn tvs_padding_bits(&self, is_runeword: bool) -> Option<u32> {
        if !self.is_alpha() {
            return None;
        }
        if is_runeword || self.is_compact {
            return None;
        }
        let policy = &crate::data::generated::layout_policy::ALPHA_TVS_PADDING_POLICY;
        policy
            .header_versions
            .contains(&self.version)
            .then_some(u32::from(policy.padding_bits))
    }

    pub fn header_geometry(&self, flags: u32) -> HeaderGeometry {
        self.header_axiom().header_geometry(flags, Some(&self.code))
    }

    pub fn is_runeword(&self, flags: u32) -> bool {
        let trimmed = self.code.trim();
        if trimmed == "c8xr" || trimmed == "rhd" || trimmed == "wa2" || trimmed == "xrs" || trimmed == "Þ." {
            return true;
        }
        if (flags & (1 << 26)) != 0 {
            return true;
        }
        if self.save_is_alpha {
            if self.version == 5 || self.version == 1 {
                let is_frag = (flags & (1 << 27)) != 0;
                if !is_frag && ((flags & (1 << 11)) != 0 || (flags & (1 << 12)) != 0 || (flags & (1 << 13)) != 0 || (flags & 0x800) != 0) {
                    return true;
                }
            }
            let reg = get_registry();
            if let Some(codes) = &reg.forced_runeword_codes {
                if codes.iter().any(|c| c == trimmed) {
                    return true;
                }
            }
            if let Some(overrides) = &reg.item_overrides {
                if let Some(map) = overrides.get(trimmed) {
                    if let Some(&val) = map.get("is_runeword") {
                        return val != 0;
                    }
                }
            }
        }
        false
    }

    pub fn is_socketed(&self, flags: u32, is_compact: bool, code: Option<&str>) -> bool {
        self.header_axiom().is_socketed(flags, is_compact, code)
    }

    pub fn is_compact(&self, _flags: u32) -> bool {
        self.is_compact
    }

    pub fn is_ethereal(&self, flags: u32) -> bool {
        self.header_axiom().is_ethereal(flags)
    }

    pub fn is_identified(&self, flags: u32) -> bool {
        self.header_axiom().is_identified(flags)
    }

    pub fn is_personalized(&self, flags: u32, is_compact: bool) -> bool {
        self.header_axiom().is_personalized(flags, is_compact)
    }

    pub fn is_v105_shadow(&self, flags: u32, code_hint: Option<&str>) -> bool {
        if self.is_compact {
            return false;
        }
        self.header_axiom().is_v105_shadow(flags, code_hint)
    }

    pub fn code_encoding(&self) -> CodeEncoding {
        if self.save_is_alpha {
            let trimmed = self.code.trim();
            if trimmed == "tsc" || trimmed == "isc" {
                return CodeEncoding::Ascii3x8;
            }

            if self.is_compact {
                // Axiom 0391: Encoding Duality for Alpha v105 summary items.
                // Items identified as summary codes use raw 3x8 bit ASCII encoding
                // ONLY if they are explicitly compact or in higher versions.
                // In Version 0, they often remain Huffman encoded.
                if self.version != 0
                    && crate::domain::forensic::v105::axioms::is_v105_summary_code(&self.code)
                {
                    // Forensic (Slice 3): Potions and some markers often use Huffman even in Version 1/2.
                    // ASCII 3x8 is typically used for 'xrs' and specific quest items.
                    if trimmed == "xrs" || trimmed == "c8xr" || trimmed == "scs" {
                        return CodeEncoding::Ascii3x8;
                    }
                }

                let reg = get_registry();
                if let Some(enc) = &reg.compact_code_encoding {
                    if enc == "ascii_3x8" {
                        return CodeEncoding::Ascii3x8;
                    }
                }
            }
        }
        CodeEncoding::Huffman
    }

    pub fn is_header_only(&self, _flags: u32, _code: &str) -> bool {
        if self.is_runeword(_flags) {
            return false;
        }

        // Alpha v105 forensic: Shadow items are truly header-only (no code, no stats).
        // They are identified by flags (bit 26/27) and have NO item code.
        let trimmed = _code.trim();
        if self.is_v105_shadow(_flags, Some(_code)) && trimmed.is_empty() {
            return true;
        }

        false
    }

    pub fn header_gap(&self, _code: &str, _flags: u32) -> u32 {
        if !self.is_alpha() {
            return 0;
        }
        let trimmed = _code.trim();
        if trimmed == "ks d" {
            return 10;
        }
        let reg = get_registry();
        if let Some(overrides) = &reg.item_overrides {
            if let Some(map) = overrides.get(trimmed) {
                if let Some(&gap) = map.get("header_gap") {
                    return gap;
                }
            }
        }
        0
    }

    pub fn compact_layout_policy(&self) -> CompactLayoutPolicy {
        if self.save_is_alpha
            && (self.version == 5
                || self.version == 0
                || self.version == 1
                || self.version == 2
                || self.version == 4
                || self.version == 6
                || self.version == 7)
        {
            CompactLayoutPolicy::AlphaV105
        } else {
            CompactLayoutPolicy::Retail
        }
    }

    /// Resolve rhythmic alignment patterns (Axiom 0340/0344)
    pub fn resolve_compact_rhythm(&self, current_len: u64) -> u64 {
        let w_axiom = crate::domain::forensic::v105::axioms::V105PropertyWidthAxiom::default();
        if self.save_is_alpha && w_axiom.is_summary_rhythm_forced(self.version, &self.code) {
            return w_axiom.summary_item_fixed_width() as u64;
        }

        if self.save_is_alpha {
            let reg = get_registry();
            let trimmed = self.code.trim();
            if let Some(overrides) = &reg.item_overrides {
                if let Some(map) = overrides.get(trimmed) {
                    if let Some(&alt) = map.get("is_alternating_rhythm") {
                        if alt != 0 && current_len == 72 {
                            if self.idx % 2 != 0 {
                                return 73;
                            }
                        }
                    }
                }
            }
        }
        current_len
    }

    pub fn is_fragment(&self, flags: u32) -> bool {
        self.is_alpha() && ((flags & (1 << 26)) != 0 || (flags & (1 << 27)) != 0)
    }

    pub fn property_rhythm(
        &self,
        _is_runeword: bool,
        _is_shadow: bool,
        is_compact: bool,
        stat_id: u32,
    ) -> PropertyRhythm {
        if !self.is_alpha() {
            return PropertyRhythm::from_layout(
                &crate::data::generated::layout_policy::RETAIL_PROPERTY_RHYTHM_LAYOUT,
            );
        }
        let policy = &crate::data::generated::layout_policy::ALPHA_PROPERTY_RHYTHM_POLICY;
        let mut rhythm = PropertyRhythm::from_layout(&policy.layout);
        if stat_id == policy.dynamic_value_stat_id
            || self.map_alpha_id(stat_id) == policy.dynamic_value_stat_id
        {
            return rhythm;
        }
        if self.version == policy.initial_runeword_header_version
            && _is_runeword
            && !policy.authority_codes.contains(&self.code.trim())
        {
            rhythm.value_bits = Some(u32::from(policy.initial_runeword_value_bits));
            return rhythm;
        }
        if _is_runeword {
            rhythm.value_bits = Some(u32::from(policy.runeword_value_bits));
            return rhythm;
        }
        if let Some((_, value_bits)) = policy
            .version_value_bits
            .iter()
            .find(|(version, _)| *version == self.version)
        {
            rhythm.value_bits = Some(u32::from(*value_bits));
            return rhythm;
        }
        if self.code.trim() == policy.opaque_code {
            rhythm.value_bits = Some(u32::from(policy.opaque_value_bits));
        }
        rhythm
    }

    pub fn resolve_flag_padding(&self, flags: u32, is_socketed: bool) -> u64 {
        let mut padding = 0;
        if is_socketed {
            padding += 8;
        }
        if (flags & 0x00000008) != 0 {
            padding += 8;
        }
        if (flags & 0x00000010) != 0 {
            padding += 16;
        }
        if (flags & 0x00000020) != 0 {
            padding += 24;
        }
        if (flags & 0x00000040) != 0 {
            padding += 32;
        }
        padding
    }
    pub fn calculate_alignment(&self, current_len: u64, code: &str, flags: u32) -> u64 {
        if self.save_is_alpha {
            let target_width = crate::domain::forensic::v105::axioms::get_v105_target_width(
                self.version,
                code,
                flags,
                Some(self.idx),
            ) as u64;
            if target_width > 0 {
                return target_width;
            }
        }
        if !self.is_compact {
            let aligned = (current_len + 7) / 8 * 8;
            return aligned;
        }
        current_len
    }

    // Alpha v105 still stores these fields for the relevant templates; the
    // template gate in ExtendedStatsData already prevents unrelated items from
    // consuming the wrong widths.
    pub fn reads_defense(&self) -> bool {
        !self.is_compact
    }
    pub fn reads_durability(&self) -> bool {
        !self.is_compact || self.code.trim() == "wa2" || self.code.trim() == "rhd"
    }
    pub fn reads_quantity(&self) -> bool {
        !self.is_compact
    }

    pub fn lookup_alpha_map_by_raw(&self, raw_id: u32) -> Option<MappingInfo> {
        let reg = get_registry();
        reg.mappings.get(&raw_id.to_string()).cloned()
    }

    pub fn lookup_alpha_map_by_effective(&self, effective_id: u32) -> Option<MappingInfo> {
        let reg = get_registry();
        reg.mappings
            .values()
            .find(|m| m.effective_id == effective_id)
            .cloned()
    }

    pub fn stat_bit_width(&self, raw_id: u32, default_width: u32) -> u32 {
        if self.is_alpha() {
            let trimmed = self.code.trim();

            let reg = get_registry();
            if let Some(overrides) = &reg.item_overrides {
                if let Some(item_map) = overrides.get(trimmed) {
                    if let Some(&width) = item_map.get(&raw_id.to_string()) {
                        return width;
                    }
                }
            }

            if default_width == 9 && trimmed != "acww" {
                return 9;
            }

            if let Some(mapping) = self.lookup_alpha_map_by_raw(raw_id) {
                if let Some(bits) = mapping.save_bits {
                    return bits;
                }
            }

            if let Some(stat_info) = reg.stats.get(&raw_id.to_string()) {
                return stat_info.width;
            }
        }
        default_width
    }

    /// Resolves the stat maturity tier for a given raw stat ID.
    /// Priority:
    /// 1. Registry explicit maturity if defined.
    /// 2. If alpha save and mapped or in stats registry -> Verified.
    /// 3. If standard retail stat (in STAT_COSTS) -> Verified.
    /// 4. Fallback -> Inferred if spec exists, else Speculative.
    pub fn stat_maturity(&self, raw_id: u32, is_known_stat: bool) -> crate::domain::stats::StatMaturityTier {
        if !is_known_stat {
            return crate::domain::stats::StatMaturityTier::Speculative;
        }

        if self.is_alpha() {
            let reg = get_registry();
            if let Some(mapping) = reg.mappings.get(&raw_id.to_string()) {
                if let Some(mat) = mapping.maturity {
                    return mat;
                }
                return crate::domain::stats::StatMaturityTier::Verified;
            }
            if let Some(info) = reg.stats.get(&raw_id.to_string()) {
                if let Some(mat) = info.maturity {
                    return mat;
                }
                return crate::domain::stats::StatMaturityTier::Verified;
            }
            // In alpha save, unmapped standard stat ID decoded via spec inference is Inferred
            return crate::domain::stats::StatMaturityTier::Inferred;
        }

        // Retail standard stats default to Verified
        crate::domain::stats::StatMaturityTier::Verified
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alpha_id_mapping() {
        let axiom = StatsAxiom::new(5, ItemQuality::Unique, true);
        assert_eq!(axiom.map_alpha_id(26), 31);
        assert_eq!(axiom.map_alpha_id(312), 72);
        assert_eq!(axiom.map_alpha_id(207), 73);
        assert_eq!(axiom.map_alpha_id(380), 194);
        assert_eq!(axiom.map_alpha_id(256), 127);
        assert_eq!(axiom.map_alpha_id(496), 99);
    }

    #[test]
    fn test_stat_maturity_tier() {
        let alpha_axiom = StatsAxiom::new(5, ItemQuality::Unique, true);
        // Verified mapping in registry: 26 -> 31
        assert_eq!(
            alpha_axiom.stat_maturity(26, true),
            crate::domain::stats::StatMaturityTier::Verified
        );
        // Unmapped in alpha registry but valid stat cost -> Inferred
        assert_eq!(
            alpha_axiom.stat_maturity(12345, true),
            crate::domain::stats::StatMaturityTier::Inferred
        );
        // Unknown stat -> Speculative
        assert_eq!(
            alpha_axiom.stat_maturity(99999, false),
            crate::domain::stats::StatMaturityTier::Speculative
        );

        let retail_axiom = StatsAxiom::new(96, ItemQuality::Unique, false);
        // Retail known stat -> Verified
        assert_eq!(
            retail_axiom.stat_maturity(31, true),
            crate::domain::stats::StatMaturityTier::Verified
        );
        // Retail unknown stat -> Speculative
        assert_eq!(
            retail_axiom.stat_maturity(99999, false),
            crate::domain::stats::StatMaturityTier::Speculative
        );
    }

    #[d2r_macros::hypothesis_test]
    fn test_property_default_width_preserves_legacy_selector_classes() {
        let widths = [None, Some(0), Some(8), Some(9), Some(10), Some(32), Some(64)];
        let mut cases = 0;
        for alpha in [false, true] {
            let axiom = StatsAxiom::new(3, ItemQuality::Normal, alpha);
            for version in u8::MIN..=u8::MAX {
                for id_bits in [0, 8, 9, 10, 32, 64] {
                    for stat_save_bits in widths {
                        for value_bits in widths {
                            let rhythm = PropertyRhythm {
                                id_bits,
                                value_bits,
                                has_terminal_bit: false,
                                has_extra_terminal_bit: false,
                            };
                            let mut expected = match stat_save_bits {
                                Some(save_bits) => value_bits.unwrap_or(save_bits),
                                None => 9,
                            };
                            if alpha && matches!(version, 0 | 1) && id_bits == 9 && expected == 9 {
                                expected = 8;
                            }
                            assert_eq!(
                                axiom.default_property_value_width(version, stat_save_bits, rhythm),
                                expected,
                                "alpha {alpha}, version {version}, id bits {id_bits}, stat {stat_save_bits:?}, rhythm {value_bits:?}"
                            );
                            cases += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(cases, 150_528);
    }
}
