use crate::data::generated::layout_policy::{
    ALPHA_LEGACY_AUTHORITY_CODES, V105_STAT_WIDTH_FALLBACKS, V105_SUMMARY_CODE_POLICY,
    V105_SUMMARY_GAP_POLICY, V105_SUMMARY_STEALTH_LOW8_PREFIXES, V105_TARGET_WIDTH_POLICY,
    V105_UNLISTED_STAT_WIDTH_BITS, V105PropertyLayout, v105_property_version_policy,
};
use crate::domain::forensic::header_policy::HeaderAxiom;
use crate::domain::item::axiom_meta::{
    Confidence, ForensicAxiom, ForensicMetadata, Intentionality,
};
use std::fmt;

#[derive(Debug, Clone)]
pub enum AxiomError {
    BufferTooSmall {
        expected: usize,
        actual: usize,
        context: String,
    },
    MarkerNotFound {
        marker: String,
    },
    ValidationFailure {
        expected: Vec<u8>,
        actual: Vec<u8>,
        offset: usize,
        context: String,
    },
}

impl fmt::Display for AxiomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BufferTooSmall { expected, actual, context } => {
                write!(f, "Axiom Error: Buffer too small for {context}. Expected at least {expected} bytes, but got {actual}.")
            }
            Self::MarkerNotFound { marker } => {
                write!(f, "Axiom Error: Required marker '{marker}' not found in bitstream.")
            }
            Self::ValidationFailure { expected, actual, offset, context } => {
                write!(f, "Axiom Error: Validation failed at offset {offset} during {context}. Expected {expected:?}, but found {actual:?}.")
            }
        }
    }
}

impl std::error::Error for AxiomError {}

/// 2-bit nudge logic (Item Flags and Stats gap) in Alpha v105.
#[derive(Debug, Clone, Default)]
pub struct V105NudgeAxiom;

impl ForensicAxiom for V105NudgeAxiom {
    fn metadata(&self) -> ForensicMetadata {
        ForensicMetadata::new(
            Confidence::VerifiedTruth,
            Intentionality::Artifactual,
            "2-bit nudge logic between Item Flags and Stats in Alpha v105",
        )
    }
}

/// 47-bit skip logic (fake Stats section for Shadow Items) in Alpha v105.
#[derive(Debug, Clone, Default)]
pub struct V105ShadowAxiom;

impl ForensicAxiom for V105ShadowAxiom {
    fn metadata(&self) -> ForensicMetadata {
        ForensicMetadata::new(
            Confidence::StrongPattern,
            Intentionality::Structural,
            "47-bit shadow stats section in Alpha v105 items",
        )
    }
}

/// Variable 0-bit/8-bit gap between JM header and item body in Alpha v105.
#[derive(Debug, Clone, Default)]
pub struct V105HeaderGapAxiom;

impl ForensicAxiom for V105HeaderGapAxiom {
    fn metadata(&self) -> ForensicMetadata {
        ForensicMetadata::new(
            Confidence::VerifiedTruth,
            Intentionality::Structural,
            "Variable gap between JM header and item body in Alpha v105",
        )
    }
}

/// Nudge logic for property block alignment in Version 5 items.
#[derive(Debug, Clone, Default)]
pub struct V105PropertyNudgeAxiom;

impl ForensicAxiom for V105PropertyNudgeAxiom {
    fn metadata(&self) -> ForensicMetadata {
        ForensicMetadata::new(
            Confidence::VerifiedTruth,
            Intentionality::Structural,
            "Explicit bit-level nudges for Version 5 property block alignment",
        )
    }
}

impl V105PropertyNudgeAxiom {
    pub fn get_nudge(&self, version: u8) -> u8 {
        crate::data::generated::layout_policy::ALPHA_V105_PROPERTY_RESIDUE_RULES
            .iter()
            .find(|rule| rule.header_version == version)
            .map_or(0, |rule| rule.residue_bits)
    }
}

impl V105HeaderGapAxiom {
    pub fn resolve_gap(&self, version: u8, code: Option<&str>, flags: u32, is_first_item: bool, is_compact: bool, has_checksum: bool, start_bit_offset: Option<u64>) -> usize {
        let mut gap = self.resolve_gap_internal(version, code, flags, is_first_item, is_compact, has_checksum);
        
        // Axiom 0344: Alpha v105 Bit 5 Rhythm Snap.
        // For Alpha v105 items, the body start bit B must satisfy B % 8 == 5
        // relative to a byte-aligned marker boundary.
        if start_bit_offset.is_some() && (version == 5 || version == 2 || version == 1 || version == 0 || version == 6 || version == 4 || version == 3) {
            let abs_start = start_bit_offset.unwrap();
            let header_len = if has_checksum { 53 } else { 45 }; 
            
            let current_body_start = abs_start + header_len as u64 + gap as u64;
            let target_rem = 5;
            let current_rem = (current_body_start % 8) as usize;
            
            let snap_gap = (target_rem + 8 - current_rem) % 8;
            
            // Apply snap.
            gap += snap_gap;
        }

        gap
    }

    fn resolve_gap_internal(&self, version: u8, code: Option<&str>, flags: u32, is_first_item: bool, is_compact: bool, has_checksum: bool) -> usize {
        let reg = crate::domain::forensic::registry::get_registry();
        let mut base_gap = 0;

        if let Some(c) = code {
            let trimmed = c.trim_matches(|c: char| c.is_whitespace() || c == '\0');

            if let Some(overrides) = &reg.item_overrides {
                if let Some(item_map) = overrides.get(trimmed) {
                    if let Some(&gap) = item_map.get("header_gap") {
                        base_gap = gap as usize;
                    }
                }
            }
        }

        if base_gap == 0 {
            // Runeword/Shadow Items (Bit 26/27)
            if (flags & (1 << 26)) != 0 || (flags & (1 << 27)) != 0 {
                // Slice 9: Alpha shadow items require an 8-bit gap in the parser.
                base_gap = 8;
            } else if is_compact {
                // Axiom 0718: Generic compact items (not summary) default to 0 gap.
                base_gap = 0;
            } else {
                // Standard equipment
                // Alpha v105 Forensic: Items in sequences (not first) usually have 0 gap (Slice 27).
                base_gap = if has_checksum { 0 } else if is_first_item { 8 } else { 0 };
            }
        }

        // Alpha v105 Forensic: Add version-specific residue nudge to the gap (Axiom 0340)
        // Axiom 0718: Compact items (potions, scrolls, etc.) do NOT use residue nudges.
        // Slice 27: Sequence items (not first) do NOT use residue nudges if base_gap is 0.
        let residue = if is_compact || (base_gap == 0 && !is_first_item) { 0 } else {
            match version {
                5 => 5,
                0 | 1 | 2 => 3,
                3 | 4 => 1, // Observed 1-bit drift in version 3/4
                _ => 0,
            }
        };

        base_gap + residue
    }
}

#[derive(Debug, Clone, Default)]
pub struct V105StealthCodeAxiom;

impl ForensicAxiom for V105StealthCodeAxiom {
    fn metadata(&self) -> ForensicMetadata {
        ForensicMetadata::new(
            Confidence::VerifiedTruth,
            Intentionality::Structural,
            "Non-ASCII stealth bit patterns for summary items in Alpha v105",
        )
    }
}

impl V105StealthCodeAxiom {
    pub fn resolve_stealth_code(&self, bits: &[bool]) -> Option<&'static str> {
        if bits.len() < 16 { return None; }
        
        // Pattern: `ÏO` (0xCF 0x4F) for 'hp1 '
        // 0xCF = 1100 1111, 0x4F = 0100 1111
        // MSB-first bit dump observed: 11110011 11110010 (reversed)
        if bits.len() >= 24 {
            let mut bytes = [0u8; 3];
            for i in 0..3 {
                for j in 0..8 {
                    if bits[i * 8 + j] { bytes[i] |= 1 << j; }
                }
            }
            if bytes[0] == 0xCF && bytes[1] == 0x4F { return Some("hp1 "); }
            if bytes[0] == 0xCF && bytes[1] == 0x50 { return Some("mp1 "); }
        }

        // pattern for 'isc ': 0x6A 0xF9 0x0F (LE)
        // bit sequence: 01010110 10011111 11110000 (LSB first)
        if bits.len() < 24 { return None; }
        let pattern = [
            false, true, false, true, false, true, true, false, // 0x6A
            true, false, false, true, true, true, true, true,  // 0xF9
            true, true, true, true, false, false, false, false, // 0x0F
        ];
        if bits[0..24] == pattern {
            return Some("isc ");
        }
        None
    }
}

/// Active alignment nudge (Active Nudging) for rhythm-aware boundary correction.
#[derive(Debug, Clone, Default)]
pub struct V105RhythmicNudgeAxiom;

impl ForensicAxiom for V105RhythmicNudgeAxiom {
    fn metadata(&self) -> ForensicMetadata {
        ForensicMetadata::new(
            Confidence::VerifiedTruth,
            Intentionality::Structural,
            "Active rhythmic nudge applied to align parser with predicted scanner target (Axiom 0344)",
        )
    }
}

/// Marker proximity axiom for boundary recovery in Alpha v105.
#[derive(Debug, Clone, Default)]
pub struct V105MarkerProximityAxiom;

impl ForensicAxiom for V105MarkerProximityAxiom {
    fn metadata(&self) -> ForensicMetadata {
        ForensicMetadata::new(
            Confidence::VerifiedTruth,
            Intentionality::Structural,
            "Marker proximity check for bit-level boundary recovery after child exhaustion (Axiom 0345)",
        )
    }
}

impl V105MarkerProximityAxiom {
    /// Checks if we are within a reasonable 'snap' distance of the next marker.
    /// Alpha v105 typically uses a 64-bit brute-force window for marker alignment.
    pub fn calculate_nudge(&self, current_pos: u64, next_marker_pos: u64) -> Option<u64> {
        if next_marker_pos > current_pos {
            let drift = next_marker_pos - current_pos;
            if drift <= 64 {
                return Some(drift);
            }
        }
        None
    }
}

pub fn is_v105_summary_code(code: &str) -> bool {
    V105PropertyWidthAxiom::default()
        .is_summary_item(V105_SUMMARY_CODE_POLICY.wrapper_header_version, code)
}
#[derive(Debug, Clone, Default)]
pub struct V105AlignmentAxiom;

impl ForensicAxiom for V105AlignmentAxiom {
    fn metadata(&self) -> ForensicMetadata {
        ForensicMetadata::new(
            Confidence::VerifiedTruth,
            Intentionality::Structural,
            "Alpha v105 items follow a 72-bit or 80-bit slotted rhythmic boundary".to_string(),
        )
    }
}

pub fn get_v105_target_width(version: u8, code: &str, flags: u32, idx: Option<usize>) -> u32 {
    let trimmed = code.trim_matches(|c: char| c.is_whitespace() || c == '\0');
    let reg = crate::domain::forensic::registry::get_registry();

    if ALPHA_LEGACY_AUTHORITY_CODES.contains(&trimmed) {
        return 0;
    }

    // Registry overrides take absolute precedence (Axiom 0365)
    if let Some(overrides) = &reg.item_overrides {
        if let Some(map) = overrides.get(trimmed) {
            if let Some(&width) = map.get("fixed_width") {
                return width;
            }
        }
    }

    let w_axiom = V105PropertyWidthAxiom::default();
    let is_summary = w_axiom.is_summary_item(version, code);

    // Axiom 0718: Enforce registry-aware compactness check.
    // This ensures items like Leather Gloves `lgl` are correctly identified as compact when overridden in registry.
    let h_axiom = HeaderAxiom::new(version, true);
    let is_compact = h_axiom.is_compact(flags, Some(code));

    if is_summary || is_compact {
        if is_summary {
            if w_axiom.is_summary_rhythm_forced(version, code) {
                return w_axiom.summary_item_fixed_width(); // Enforce strict 80-bit slotted boundary
            }

            // Alpha v105 forensic: Potions and scrolls usually follow a 72-bit slotted rhythm (Slice 27).
            // Some summary items still use 80-bit rhythm via force_summary_rhythm_codes override.
            return V105_TARGET_WIDTH_POLICY.summary_default_bits;
        }

        // Alpha v105 Slice 20: 72-bit base slot for compact items.
        if trimmed.starts_with(V105_TARGET_WIDTH_POLICY.rune_prefix)
            && trimmed.len() <= V105_TARGET_WIDTH_POLICY.rune_max_code_bytes
        {
            if let Some(&rune_w) = reg.axioms.get("rune_fixed_width") {
                return rune_w as u32;
            }
        }
        let base_width = reg
            .axioms
            .get("compact_item_fixed_width")
            .cloned()
            .unwrap_or(V105_TARGET_WIDTH_POLICY.compact_default_bits)
            as u32;
        if V105_TARGET_WIDTH_POLICY
            .compact_nudge_versions
            .contains(&version)
        {
            return base_width + V105_TARGET_WIDTH_POLICY.compact_nudge_bits; // Alpha v105 Version 4 compact items require 1-bit nudge (Slice 5)
        }
        return base_width;
    }

    // Fixed overrides and fixed containers have already returned.
    0
}
/// JM Marker scanning axiom for Alpha v105.
#[derive(Debug, Clone, Default)]
pub struct V105JmMarkerAxiom;

impl ForensicAxiom for V105JmMarkerAxiom {
    fn metadata(&self) -> ForensicMetadata {
        ForensicMetadata::new(
            Confidence::VerifiedTruth,
            Intentionality::Structural,
            "JM Marker (0x4D4A) scanning and validation in Alpha v105",
        )
    }
}

impl V105JmMarkerAxiom {
    pub const MARKER: u16 = 0x4D4A;

    pub fn jm_marker(&self) -> u16 {
        Self::MARKER
    }

    pub fn header_len(&self) -> usize {
        4 // JM (2) + Count (2)
    }

    pub fn header_bits(&self, _version: u8) -> u32 {
        32
    }

    pub fn scan(&self, bytes: &[u8]) -> Vec<usize> {
        let mut positions = Vec::new();
        for i in 0..bytes.len().saturating_sub(1) {
            if bytes[i] == b'J' && bytes[i + 1] == b'M' {
                positions.push(i);
            }
        }
        positions
    }
}

#[derive(Debug, Clone)]
pub enum V105Mutation {
    Write { offset: usize, data: Vec<u8>, context: String },
    WriteByte { offset: usize, value: u8, context: String },
}

#[derive(Debug, Clone, Default)]
pub struct V105RebuildPlan {
    pub mutations: Vec<V105Mutation>,
}

impl V105RebuildPlan {
    pub fn execute(&self, header: &mut [u8]) -> Result<(), AxiomError> {
        for m in &self.mutations {
            match m {
                V105Mutation::Write { offset, data, context } => {
                    let end = offset + data.len();
                    if header.len() < end {
                        return Err(AxiomError::BufferTooSmall {
                            expected: end,
                            actual: header.len(),
                            context: context.clone(),
                        });
                    }
                    header[*offset..end].copy_from_slice(data);
                }
                V105Mutation::WriteByte { offset, value, context } => {
                    if header.len() <= *offset {
                        return Err(AxiomError::BufferTooSmall {
                            expected: *offset + 1,
                            actual: header.len(),
                            context: context.clone(),
                        });
                    }
                    header[*offset] = *value;
                }
            }
        }
        Ok(())
    }
}

/// Section marker scanning axiom for Alpha v105.
#[derive(Debug, Clone, Default)]
pub struct V105SectionMarkerAxiom;

impl ForensicAxiom for V105SectionMarkerAxiom {
    fn metadata(&self) -> ForensicMetadata {
        ForensicMetadata::new(
            Confidence::VerifiedTruth,
            Intentionality::Structural,
            "Alpha v105 Section Marker (gf, if, Woo!, WS, w4, jf, kf, lf) scanning and validation",
        )
    }
}

impl V105SectionMarkerAxiom {
    pub const V105_HEADER_LEN: usize = 833;
    pub const V105_QUEST_OFFSET: usize = 0x193;
    pub const V105_QUEST_LEN: usize = 298;
    pub const V105_WAYPOINT_OFFSET: usize = 0x2BD;
    pub const V105_WAYPOINT_LEN: usize = 81;
    pub const V105_NPC_OFFSET: usize = 0x30E;
    pub const V105_NPC_LEN: usize = 51;

    pub const MARKER_GF: [u8; 2] =
        crate::data::generated::layout_policy::LEGACY_CHARACTER_ATTRIBUTE_WIRE_LAYOUT.marker;
    pub const MARKER_IF: [u8; 2] = *b"if";
    pub const MARKER_WOO: [u8; 4] = *b"Woo!";
    pub const MARKER_WS: [u8; 2] = *b"WS";
    pub const MARKER_W4: [u8; 2] = *b"w4";
    pub const MARKER_JF: [u8; 2] = *b"jf";
    pub const MARKER_KF: [u8; 2] = *b"kf";
    pub const MARKER_LF: [u8; 2] = *b"lf";

    pub fn find_gf(&self, bytes: &[u8]) -> Option<usize> {
        self.find_marker(bytes, &Self::MARKER_GF)
    }

    pub fn find_if(&self, bytes: &[u8]) -> Option<usize> {
        self.find_marker(bytes, &Self::MARKER_IF)
    }

    pub fn find_woo(&self, bytes: &[u8]) -> Option<usize> {
        self.find_marker(bytes, &Self::MARKER_WOO)
    }

    pub fn find_ws(&self, bytes: &[u8]) -> Option<usize> {
        self.find_marker(bytes, &Self::MARKER_WS)
    }

    pub fn find_w4(&self, bytes: &[u8]) -> Option<usize> {
        self.find_marker(bytes, &Self::MARKER_W4)
    }

    pub fn find_jf(&self, bytes: &[u8]) -> Option<usize> {
        self.find_marker(bytes, &Self::MARKER_JF)
    }

    pub fn find_kf(&self, bytes: &[u8]) -> Option<usize> {
        self.find_marker(bytes, &Self::MARKER_KF)
    }

    pub fn find_lf(&self, bytes: &[u8]) -> Option<usize> {
        self.find_marker(bytes, &Self::MARKER_LF)
    }

    pub fn gf_bytes(&self) -> &[u8] {
        &Self::MARKER_GF
    }

    pub fn if_bytes(&self) -> &[u8] {
        &Self::MARKER_IF
    }

    pub fn woo_bytes(&self) -> &[u8] {
        &Self::MARKER_WOO
    }

    pub fn ws_bytes(&self) -> &[u8] {
        &Self::MARKER_WS
    }

    pub fn w4_bytes(&self) -> &[u8] {
        &Self::MARKER_W4
    }

    pub fn jf_bytes(&self) -> &[u8] {
        &Self::MARKER_JF
    }

    pub fn kf_bytes(&self) -> &[u8] {
        &Self::MARKER_KF
    }

    pub fn lf_bytes(&self) -> &[u8] {
        &Self::MARKER_LF
    }

    pub fn gf_len(&self) -> usize {
        Self::MARKER_GF.len()
    }

    pub fn if_len(&self) -> usize {
        Self::MARKER_IF.len()
    }

    pub fn woo_len(&self) -> usize {
        Self::MARKER_WOO.len()
    }

    pub fn ws_len(&self) -> usize {
        Self::MARKER_WS.len()
    }

    pub fn w4_len(&self) -> usize {
        Self::MARKER_W4.len()
    }

    pub fn jf_len(&self) -> usize {
        Self::MARKER_JF.len()
    }

    pub fn kf_len(&self) -> usize {
        Self::MARKER_KF.len()
    }

    pub fn lf_len(&self) -> usize {
        Self::MARKER_LF.len()
    }

    /// Creates a rebuild plan for Alpha v105 header synchronization.
    pub fn plan_rebuild(
        &self,
        woo_pos: Option<usize>,
        ws_pos: Option<usize>,
        w4_pos: Option<usize>,
        quests: Option<&[u8]>,
        waypoints: Option<&[u8]>,
        npc: Option<&[u8]>,
        attr_level: Option<u8>,
        char_level_offset: usize,
    ) -> V105RebuildPlan {
        let mut plan = V105RebuildPlan::default();

        if let Some(data) = quests {
            let start = woo_pos.unwrap_or(Self::V105_QUEST_OFFSET);
            let end = ws_pos.unwrap_or(Self::V105_WAYPOINT_OFFSET);
            let max_len = end.saturating_sub(start);
            let len = data.len().min(max_len);
            plan.mutations.push(V105Mutation::Write {
                offset: start,
                data: data[..len].to_vec(),
                context: "Quests".to_string(),
            });
        }

        if let Some(data) = waypoints {
            let start = ws_pos.unwrap_or(Self::V105_WAYPOINT_OFFSET);
            let end = w4_pos.unwrap_or(Self::V105_NPC_OFFSET);
            let max_len = end.saturating_sub(start);
            let len = data.len().min(max_len);
            plan.mutations.push(V105Mutation::Write {
                offset: start,
                data: data[..len].to_vec(),
                context: "Waypoints".to_string(),
            });
        }

        if let Some(data) = npc {
            let start = w4_pos.unwrap_or(Self::V105_NPC_OFFSET);
            let end = Self::V105_HEADER_LEN;
            let max_len = end.saturating_sub(start);
            let len = data.len().min(max_len);
            plan.mutations.push(V105Mutation::Write {
                offset: start,
                data: data[..len].to_vec(),
                context: "NPC Section".to_string(),
            });
        }

        if let Some(lv) = attr_level {
            plan.mutations.push(V105Mutation::WriteByte {
                offset: char_level_offset,
                value: lv,
                context: "Character Level".to_string(),
            });
        }

        plan
    }

    fn find_marker(&self, bytes: &[u8], marker: &[u8]) -> Option<usize> {
        if marker.is_empty() { return None; }
        (0..bytes.len().saturating_sub(marker.len() - 1))
            .find(|&i| &bytes[i..i + marker.len()] == marker)
    }
}

/// Item property and base field bit widths in Alpha v105.
#[derive(Debug, Clone, Default)]
pub struct V105PropertyWidthAxiom;

impl ForensicAxiom for V105PropertyWidthAxiom {
    fn metadata(&self) -> ForensicMetadata {
        ForensicMetadata::new(
            Confidence::VerifiedTruth,
            Intentionality::Structural,
            "Item property and base stat bit widths in Alpha v105",
        )
    }
}

impl V105PropertyWidthAxiom {
    /// Returns true if the item code follows the 80-bit summary rhythm in Alpha v105 (Axiom 0344).
    pub fn is_summary_rhythm_forced(&self, _version: u8, code: &str) -> bool {
        let trimmed = code.trim_matches(|c: char| c.is_whitespace() || c == '\0');
        let reg = crate::domain::forensic::registry::get_registry();
        if let Some(codes) = &reg.force_summary_rhythm_codes {
            if codes.iter().any(|c| c == trimmed) { return true; }
        }
        false
    }
    /// Returns true if the item code is classified as a summary item in Alpha v105 (Axiom 0365).
    pub fn is_summary_item(&self, version: u8, code: &str) -> bool {
        let trimmed = code.trim_matches(|c: char| c.is_whitespace() || c == '\0');
        let reg = crate::domain::forensic::registry::get_registry();

        if let Some(overrides) = &reg.item_overrides {
            if let Some(map) = overrides.get(trimmed) {
                if let Some(&is_shadow) = map.get("is_shadow") {
                    let is_compact_override =
                        map.get("is_compact").map(|&v| v != 0).unwrap_or(false);
                    if is_shadow != 0 && !is_compact_override {
                        return false;
                    }
                }
            }
        }

        if ALPHA_LEGACY_AUTHORITY_CODES.contains(&trimmed) {
            return false; // Authority Runeword related items are NOT summary (Slice 7)
        }
        if trimmed.is_empty() && code.chars().any(|c| c.is_whitespace()) {
            // Axiom 0344: Whitespace-only blank codes are classified as summary items
            // in Alpha v105 to preserve the 80-bit rhythm and item count parity.
            return true;
        }

        // Alpha v105 Summary Items: Potions, Scrolls, and small consumables (Slice 3040).
        // These items follow strict 72/80 bit slotted boundaries and use 16-bit IDs.
        let policy = &V105_SUMMARY_CODE_POLICY;
        let is_potion = policy
            .potion_prefixes
            .iter()
            .any(|&prefix| trimmed.starts_with(prefix))
            || policy.potion_codes.contains(&trimmed);
        let is_scroll = policy.scroll_codes.contains(&trimmed);
        let is_misc_summary = policy.miscellaneous_codes.contains(&trimmed);

        if is_potion || is_scroll || is_misc_summary {
            return true;
        }

        // 1. Known Stealth-Compact patterns (Markers without bit 23 set)
        if self.matches_stealth_pattern(code) {
            return true;
        }

        // 2. Check registry for explicit forced compact
        if let Some(codes) = &reg.forced_compact_codes {
            if codes.iter().any(|c| c == trimmed) {
                // Alpha v105 Forensic: ww/gcw in version 0/2 are often full equipment (Slice 48)
                if policy.forced_compact_excluded_versions.contains(&version)
                    && policy.version_excluded_codes.contains(&trimmed)
                {
                    return false;
                }
                // Exclude known non-summary equipment-like shadows even if in forced_compact
                if policy.forced_compact_excluded_codes.contains(&trimmed) {
                    return false;
                }
                return true;
            }
        }

        if self.is_summary_rhythm_forced(version, code) {
            return true;
        }

        false
    }

    fn matches_stealth_pattern(&self, code: &str) -> bool {
        let bytes: Vec<u8> = code.chars().map(|c| c as u32 as u8).collect();
        V105_SUMMARY_STEALTH_LOW8_PREFIXES
            .iter()
            .any(|prefix| bytes.starts_with(prefix))
    }


    /// The fixed width for summary items in Alpha v105 (80 bits).
    pub fn summary_item_fixed_width(&self) -> u32 {
        V105PropertyLayout::SUMMARY_ITEM_FIXED_WIDTH
    }

    pub fn quality_bits(&self, is_alpha: bool) -> u32 {
        if is_alpha {
            V105PropertyLayout::QUALITY_ALPHA_BITS
        } else {
            V105PropertyLayout::QUALITY_NONALPHA_BITS
        }
    }
    pub fn item_id_bits(&self) -> u32 {
        V105PropertyLayout::ITEM_ID_BITS
    }
    pub fn item_level_bits(&self) -> u32 {
        V105PropertyLayout::ITEM_LEVEL_BITS
    }
    pub fn multi_graphics_bits(&self) -> u32 {
        V105PropertyLayout::MULTI_GRAPHICS_BITS
    }
    pub fn class_specific_bits(&self) -> u32 {
        V105PropertyLayout::CLASS_SPECIFIC_BITS
    }
    pub fn low_high_graphic_bits(&self) -> u32 {
        V105PropertyLayout::LOW_HIGH_GRAPHIC_BITS
    }
    pub fn magic_affix_bits(&self) -> u32 {
        V105PropertyLayout::MAGIC_AFFIX_BITS
    }
    pub fn rare_name_bits(&self) -> u32 {
        V105PropertyLayout::RARE_NAME_BITS
    }
    pub fn rare_affix_bits(&self) -> u32 {
        V105PropertyLayout::RARE_AFFIX_BITS
    }
    pub fn unique_id_bits(&self) -> u32 {
        V105PropertyLayout::UNIQUE_ID_BITS
    }
    pub fn runeword_id_bits(&self) -> u32 {
        V105PropertyLayout::RUNEWORD_ID_BITS
    }
    pub fn runeword_level_bits(&self) -> u32 {
        V105PropertyLayout::RUNEWORD_LEVEL_BITS
    }
    pub fn quantity_bits(&self) -> u32 {
        V105PropertyLayout::QUANTITY_BITS
    }
    pub fn socket_bits(&self) -> u32 {
        V105PropertyLayout::SOCKET_BITS
    }
    pub fn set_list_bits(&self) -> u32 {
        V105PropertyLayout::SET_LIST_BITS
    }
    pub fn teleport_bits(&self) -> u32 {
        V105PropertyLayout::TELEPORT_BITS
    }
    pub fn v5_runeword_extra_bits(&self) -> u32 {
        V105PropertyLayout::V5_RUNEWORD_EXTRA_BITS
    }
    pub fn ear_class_bits(&self) -> u32 {
        V105PropertyLayout::EAR_CLASS_BITS
    }
    pub fn ear_level_bits(&self) -> u32 {
        V105PropertyLayout::EAR_LEVEL_BITS
    }
    
    pub fn flags_bits(&self) -> u32 {
        V105PropertyLayout::FLAGS_BITS
    }
    pub fn checksum_bits(&self) -> u32 {
        V105PropertyLayout::CHECKSUM_BITS
    }
    pub fn version_bits(&self, _alpha_mode: bool) -> u32 {
        V105PropertyLayout::VERSION_BITS
    }
    pub fn mode_bits(&self, _alpha_mode: bool) -> u32 {
        V105PropertyLayout::MODE_BITS
    }
    pub fn location_bits(&self, alpha_mode: bool, _version: u8) -> u32 {
        if alpha_mode {
            V105PropertyLayout::LOCATION_ALPHA_BITS
        } else {
            V105PropertyLayout::LOCATION_NONALPHA_BITS
        }
    }
    pub fn x_bits(&self, alpha_mode: bool, _version: u8) -> u32 {
        if alpha_mode {
            V105PropertyLayout::X_ALPHA_BITS
        } else {
            V105PropertyLayout::X_NONALPHA_BITS
        }
    }

    pub fn summary_gap_bits(&self, code: &str) -> u32 {
        let trimmed = code.trim_matches(|c: char| c.is_whitespace() || c == '\0');
        let reg = crate::domain::forensic::registry::get_registry();
        if let Some(overrides) = &reg.item_overrides {
            if let Some(map) = overrides.get(trimmed) {
                if let Some(&gap) = map.get("header_gap") {
                    return gap;
                }
            }
        }

        let policy = &V105_SUMMARY_GAP_POLICY;
        if policy.zero_gap_codes.contains(&trimmed)
            || policy
                .zero_gap_prefixes
                .iter()
                .any(|&prefix| trimmed.starts_with(prefix))
        {
            policy.selected_gap_bits
        } else {
            policy.default_gap_bits
        }
    }

    pub fn nudge_bits(&self, version: u8) -> u32 {
        v105_property_version_policy(version).nudge_bits
    }
    
    pub fn is_extended_stats_early_exit(&self, version: u8) -> bool {
        v105_property_version_policy(version).extended_stats_early_exit
    }
    
    pub fn has_v5_runeword_extra(&self, version: u8) -> bool {
        v105_property_version_policy(version).has_runeword_extra
    }
    
    pub fn is_player_name_alpha_style(&self, version: u8) -> bool {
        v105_property_version_policy(version).player_name_alpha_style
    }
    
    pub fn needs_player_name_byte_alignment(&self, version: u8) -> bool {
        v105_property_version_policy(version).player_name_byte_alignment
    }
    
    pub fn is_ear_name_v5_style(&self, version: u8) -> bool {
        v105_property_version_policy(version).ear_name_v5_style
    }
    
    pub fn needs_ear_name_byte_alignment(&self, version: u8) -> bool {
        v105_property_version_policy(version).ear_name_byte_alignment
    }
    
    pub fn needs_post_body_byte_alignment(&self, version: u8, is_compact: bool) -> bool {
        v105_property_version_policy(version).post_body_noncompact_byte_alignment && !is_compact
    }
    
    pub fn stat_bits(&self, stat_id: u32) -> u32 {
        let reg = crate::domain::forensic::registry::get_registry();

        // Axiom 0812: Use registry truth for Alpha v105 stat widths (e.g. durability=14, defense=10).
        // 1. Try explicit stats section
        if let Some(info) = reg.stats.get(&stat_id.to_string()) {
            return info.width;
        }

        // 2. Try mappings section for save_bits
        if let Some(map) = crate::domain::stats::lookup_alpha_map_by_effective(stat_id) {
            if let Some(bits) = map.save_bits {
                return bits as u32;
            }
        }

        // 3. Fallback to retail defaults
        if let Some(rule) = V105_STAT_WIDTH_FALLBACKS
            .iter()
            .find(|rule| rule.stat_id == stat_id)
        {
            crate::domain::stats::stat_save_bits(stat_id).unwrap_or(rule.default_bits)
        } else {
            V105_UNLISTED_STAT_WIDTH_BITS
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_v105_axioms_metadata() {
        let nudge = V105NudgeAxiom;
        assert_eq!(nudge.metadata().confidence, Confidence::VerifiedTruth);

        let shadow = V105ShadowAxiom;
        assert_eq!(shadow.metadata().confidence, Confidence::StrongPattern);

        let gap = V105HeaderGapAxiom;
        assert_eq!(gap.metadata().confidence, Confidence::VerifiedTruth);

        let rhythm = V105PropertyNudgeAxiom;
        assert_eq!(rhythm.metadata().confidence, Confidence::VerifiedTruth);

        let section = V105SectionMarkerAxiom;
        assert_eq!(section.metadata().confidence, Confidence::VerifiedTruth);
    }

    #[test]
    fn test_v105_summary_aliases_keep_hp1_and_mp1_compact() {
        assert!(is_v105_summary_code("hp1"));
        assert!(is_v105_summary_code("mp1"));
        assert_eq!(get_v105_target_width(5, "hp1", 0, None), 72);
        assert_eq!(get_v105_target_width(5, "mp1", 0, None), 72);
        assert!(!is_v105_summary_code("xrs"));
    }

    #[test]
    fn test_v105_lgl_is_compact_via_registry() {
        // Leather Gloves (lgl) should be compact if registry override is respected.
        // Based on Slice 2 spec, lgl is expected to be compact in Alpha v105.
        let width = get_v105_target_width(5, "lgl", 0, None);
        assert_eq!(width, 72, "lgl should have 72-bit compact width via registry override");
    }
}

// Keep shared-axiom queries lazy; header inputs are distinct from StatsAxiom state.
impl crate::domain::item::subdomains::payload::LegacyExtendedPayloadReadPolicy
    for crate::domain::stats::StatsAxiom
{
    fn class_specific_bits(&self) -> u32 {
        V105PropertyWidthAxiom.class_specific_bits()
    }
    fn has_v5_runeword_extra(&self, version: u8) -> bool {
        V105PropertyWidthAxiom.has_v5_runeword_extra(version)
    }
    fn is_extended_stats_early_exit(&self, version: u8) -> bool {
        V105PropertyWidthAxiom.is_extended_stats_early_exit(version)
    }
    fn is_player_name_alpha_style(&self, version: u8) -> bool {
        V105PropertyWidthAxiom.is_player_name_alpha_style(version)
    }
    fn is_summary_item(&self, version: u8, code: &str) -> bool {
        V105PropertyWidthAxiom.is_summary_item(version, code)
    }
    fn item_id_bits(&self) -> u32 {
        V105PropertyWidthAxiom.item_id_bits()
    }
    fn item_level_bits(&self) -> u32 {
        V105PropertyWidthAxiom.item_level_bits()
    }
    fn low_high_graphic_bits(&self) -> u32 {
        V105PropertyWidthAxiom.low_high_graphic_bits()
    }
    fn multi_graphics_bits(&self) -> u32 {
        V105PropertyWidthAxiom.multi_graphics_bits()
    }
    fn needs_player_name_byte_alignment(&self, version: u8) -> bool {
        V105PropertyWidthAxiom.needs_player_name_byte_alignment(version)
    }
    fn quality_bits(&self, is_alpha: bool) -> u32 {
        V105PropertyWidthAxiom.quality_bits(is_alpha)
    }
    fn quantity_bits(&self) -> u32 {
        V105PropertyWidthAxiom.quantity_bits()
    }
    fn runeword_id_bits(&self) -> u32 {
        V105PropertyWidthAxiom.runeword_id_bits()
    }
    fn runeword_level_bits(&self) -> u32 {
        V105PropertyWidthAxiom.runeword_level_bits()
    }
    fn set_list_bits(&self) -> u32 {
        V105PropertyWidthAxiom.set_list_bits()
    }
    fn socket_bits(&self) -> u32 {
        V105PropertyWidthAxiom.socket_bits()
    }
    fn stat_bits(&self, stat_id: u32) -> u32 {
        V105PropertyWidthAxiom.stat_bits(stat_id)
    }
    fn teleport_bits(&self) -> u32 {
        V105PropertyWidthAxiom.teleport_bits()
    }
    fn v5_runeword_extra_bits(&self) -> u32 {
        V105PropertyWidthAxiom.v5_runeword_extra_bits()
    }
    fn is_alpha(&self) -> bool {
        crate::domain::stats::StatsAxiom::is_alpha(self)
    }
    fn is_runeword(&self, flags: u32) -> bool {
        crate::domain::stats::StatsAxiom::is_runeword(self, flags)
    }
    fn reads_defense(&self) -> bool {
        crate::domain::payload::legacy_presence::LegacyPayloadPresence::new(
            self.is_compact,
            &self.code,
        )
        .allows_defense()
    }
    fn reads_durability(&self) -> bool {
        crate::domain::payload::legacy_presence::LegacyPayloadPresence::new(
            self.is_compact,
            &self.code,
        )
        .allows_durability()
    }
    fn reads_quantity(&self) -> bool {
        crate::domain::payload::legacy_presence::LegacyPayloadPresence::new(
            self.is_compact,
            &self.code,
        )
        .allows_quantity()
    }
    fn header_is_alpha(&self, version: u8, alpha_mode: bool) -> bool {
        HeaderAxiom::new(version, alpha_mode).is_alpha()
    }
    fn header_is_v105_shadow(
        &self,
        version: u8,
        alpha_mode: bool,
        flags: u32,
        code_hint: Option<&str>,
    ) -> bool {
        HeaderAxiom::new(version, alpha_mode).is_v105_shadow(flags, code_hint)
    }
    fn in_nested_recovery(&self) -> bool {
        crate::domain::header::entity::IN_NESTED_RECOVERY.with(|value| value.get())
    }
    fn player_name_alignment_required(&self) -> bool {
        crate::domain::item::entity::AlphaV105PersonalizedAlignment::align_required()
    }
}
