use crate::domain::item::axiom_meta::{Confidence, ForensicAxiom, ForensicMetadata, Intentionality};
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
            Confidence::StrongPattern,
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
        match version {
            5 => 5, // Version 5 requires a 5-bit residue nudge (Slice 23)
            4 => 1, // Version 4 requires a 1-bit residue nudge (Slice 5)
            2 | 0 => 3, // Version 2 and 0 require a 3-bit residue nudge
            1 => 0, // Version 1 (xrs runeword base) does not use this nudge
            _ => 0,
        }
    }
}

impl V105HeaderGapAxiom {
    pub fn resolve_gap(&self, version: u8, code: Option<&str>, flags: u32, is_first_item: bool, is_compact: bool, has_checksum: bool, start_bit_offset: Option<u64>) -> usize {
        let mut gap = self.resolve_gap_internal(version, code, flags, is_first_item, is_compact, has_checksum);
        
        // Axiom 0344: Alpha v105 Bit 5 Rhythm Snap.
        // For Alpha v105 items, the body start bit B must satisfy B % 8 == 5
        // relative to a byte-aligned marker boundary.
        if start_bit_offset.is_some() && (version == 5 || version == 2 || version == 1 || version == 0 || version == 6 || version == 4) {
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
                // Slice 9: Alpha runewords often use Bit 5 snap with 0 base gap.
                base_gap = 0;
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
                4 => 1, // Observed 1-bit drift in version 4 (Item 13)
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
    V105PropertyWidthAxiom::default().is_summary_item(5, code)
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
    
    let is_compact_flag = (flags & (1 << 23)) != 0 || (flags & (1 << 21)) != 0;
    let is_shadow = (flags & (1 << 26)) != 0 || (flags & (1 << 27)) != 0;
    let is_personalized = (flags & (1 << 24)) != 0;

    if is_summary || is_compact_flag {
        if is_summary {
            if w_axiom.is_summary_rhythm_forced(version, code) {
                return w_axiom.summary_item_fixed_width(); // Enforce strict 80-bit slotted boundary
            }

            // Alpha v105 forensic: Potions and scrolls usually follow an 80-bit slotted rhythm.
            // Scrolls (tsc/isc) are often 72-bit (Slice 27).
            let trimmed = code.trim_matches(|c: char| c.is_whitespace() || c == '\0');
            if trimmed == "tsc" || trimmed == "isc" {
                return 72;
            }
            return 80;
        }

        // Alpha v105 Slice 20: 72-bit base slot for compact items.
        let base_width = reg.axioms.get("compact_item_fixed_width").cloned().unwrap_or(72) as u32;
        if version == 4 {
            return base_width + 1; // Alpha v105 Version 4 compact items require 1-bit nudge (Slice 5)
        }
        return base_width;
    }

    // Equipment items (non-compact, non-summary) have variable width based on property lists.
    // We should NOT return a fixed width here unless it's a known shadow/personalized fixed container
    // that does NOT contain variable properties.
    if is_shadow || is_personalized {
        // Authority runeword fixtures (`xrs` / `c8xr` / `rhd`) need variable-width handling.
        if trimmed == "xrs" || trimmed == "c8xr" || trimmed == "rhd" {
            return 0;
        }
        // Runewords and personalized items in Alpha v105 are OFTEN variable.
        // Returning 80 here causes "swallowing" if the item is larger.
        // We only return 80 if it's a known fixed shadow (e.g. from registry).
        if let Some(overrides) = &reg.item_overrides {
            if let Some(map) = overrides.get(trimmed) {
                if let Some(&width) = map.get("fixed_width") { return width; }
            }
        }
        return 0; // Default to variable
    }

    0 // Default to variable for standard equipment
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

    pub const MARKER_GF: [u8; 2] = *b"gf";
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

        if trimmed == "xrs" || trimmed == "c8xr" || trimmed == "scs" {
            return false; // Authority Runeword related items are NOT summary (Slice 7)
        }
        if trimmed.is_empty() && code.chars().any(|c| c.is_whitespace()) {
            // Axiom 0344: Whitespace-only blank codes are classified as summary items
            // in Alpha v105 to preserve the 80-bit rhythm and item count parity.
            return true;
        }
        if trimmed.starts_with('h')
            || trimmed.starts_with('m')
            || (trimmed.starts_with('r') && trimmed.len() <= 3)
            || trimmed == "vps"
            || trimmed == "yps"
            || trimmed == "wms"
            || trimmed.starts_with('o')
            || trimmed.starts_with('g')
            || trimmed == "ice"
            || trimmed == "xyz"
            || trimmed == "tsc"
            || trimmed == "isc"
            || trimmed == "jav"
            || trimmed == "ks d"
            || trimmed == "k  k"
        {
            // Alpha v105 potion family are textual summaries even when the raw
            // stealth pattern has already been normalized to a readable code.
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
                if (version == 0 || version == 2) && (trimmed == "ww" || trimmed == "gcw") {
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
        // 1. Known Stealth-Compact patterns (Markers without bit 23 set)
        // (Axiom 0365): Alpha summary items often use raw byte codes like 'H\x04'
        // Forensic: Use raw u8 conversion to avoid UTF-8 mismatch for non-ASCII codes (Slice 24)
        let bytes: Vec<u8> = code.chars().map(|c| c as u32 as u8).collect();

        // Pattern: `ÏO` (0xCF 0x4F)
        if bytes.len() >= 2 && bytes[0] == 0xCF && bytes[1] == 0x4F {
            return true;
        }
        // Pattern: H\x04 (0x48 0x04)
        if bytes.len() >= 2 && bytes[0] == 0x48 && bytes[1] == 0x04 {
            return true;
        }
        // Pattern: bH\x04 (0x62 0x48 0x04)
        if bytes.len() >= 3 && bytes[0] == b'b' && bytes[1] == 0x48 && bytes[2] == 0x04 {
            return true;
        }
        // Pattern: Q€ (0x51 0x80)
        if bytes.len() >= 2 && bytes[0] == 0x51 && bytes[1] == 0x80 {
            return true;
        }
        // Pattern: ~ (0x7E 0x02 0x80) observed in amazon_initial
        if bytes.len() >= 2 && bytes[0] == 0x7E && bytes[1] == 0x02 {
            return true;
        }

        // Pattern: "   9" (0x20 0x20 0x20 0x39)
        if bytes.len() >= 4 && bytes[0] == 0x20 && bytes[1] == 0x20 && bytes[2] == 0x20 && bytes[3] == 0x39 {
            return true;
        }

        // `Þ. Resolution: 0xDE 0x2E pattern for Alpha v105`
        if bytes.len() >= 2 && bytes[0] == 0xDE && bytes[1] == 0x2E {
            return true;
        }

        false
    }


    /// The fixed width for summary items in Alpha v105 (80 bits).
    pub fn summary_item_fixed_width(&self) -> u32 {
        80
    }

    pub fn quality_bits(&self, is_alpha: bool) -> u32 { if is_alpha { 3 } else { 4 } }
    pub fn item_id_bits(&self) -> u32 { 32 }
    pub fn item_level_bits(&self) -> u32 { 7 }
    pub fn multi_graphics_bits(&self) -> u32 { 3 }
    pub fn class_specific_bits(&self) -> u32 { 11 }
    pub fn low_high_graphic_bits(&self) -> u32 { 3 }
    pub fn magic_affix_bits(&self) -> u32 { 11 }
    pub fn rare_name_bits(&self) -> u32 { 8 }
    pub fn rare_affix_bits(&self) -> u32 { 11 }
    pub fn unique_id_bits(&self) -> u32 { 12 }
    pub fn runeword_id_bits(&self) -> u32 { 12 }
    pub fn runeword_level_bits(&self) -> u32 { 4 }
    pub fn quantity_bits(&self) -> u32 { 9 }
    pub fn socket_bits(&self) -> u32 { 4 }
    pub fn set_list_bits(&self) -> u32 { 5 }
    pub fn teleport_bits(&self) -> u32 { 5 }
    pub fn v5_runeword_extra_bits(&self) -> u32 { 1 }
    pub fn ear_class_bits(&self) -> u32 { 3 }
    pub fn ear_level_bits(&self) -> u32 { 7 }
    
    pub fn flags_bits(&self) -> u32 { 32 }
    pub fn checksum_bits(&self) -> u32 { 8 }
    pub fn version_bits(&self, _alpha_mode: bool) -> u32 { 3 }
    pub fn mode_bits(&self, _alpha_mode: bool) -> u32 { 3 }
    pub fn location_bits(&self, _alpha_mode: bool) -> u32 { 3 }
    pub fn x_bits(&self, _alpha_mode: bool) -> u32 { 4 }

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
        
        if trimmed == "tsc" || trimmed == "isc" {
            0
        } else {
            8
        }
    }

    pub fn nudge_bits(&self, version: u8) -> u32 {
        if version == 1 { 0 } else { 2 }
    }
    
    pub fn is_extended_stats_early_exit(&self, version: u8) -> bool {
        version == 6 || version == 7
    }
    
    pub fn has_v5_runeword_extra(&self, version: u8) -> bool {
        version == 5 || version == 6 || version == 7 || version == 1
    }
    
    pub fn is_player_name_alpha_style(&self, version: u8) -> bool {
        version == 5 || version == 0 || version == 1
    }
    
    pub fn needs_player_name_byte_alignment(&self, version: u8) -> bool {
        version == 5 || version == 0 || version == 1
    }
    
    pub fn is_ear_name_v5_style(&self, version: u8) -> bool {
        version == 5
    }
    
    pub fn needs_ear_name_byte_alignment(&self, version: u8) -> bool {
        version == 5
    }
    
    pub fn needs_post_body_byte_alignment(&self, version: u8, is_compact: bool) -> bool {
        version == 5 && !is_compact
    }
    
    pub fn stat_bits(&self, stat_id: u32) -> u32 {
        match stat_id {
            31 => crate::domain::stats::stat_save_bits(31).unwrap_or(11), // Defense
            73 => crate::domain::stats::stat_save_bits(73).unwrap_or(8),  // Max Durability
            72 => crate::domain::stats::stat_save_bits(72).unwrap_or(9),  // Current Durability
            _ => 0,
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
        assert_eq!(gap.metadata().confidence, Confidence::StrongPattern);

        let rhythm = V105PropertyNudgeAxiom;
        assert_eq!(rhythm.metadata().confidence, Confidence::VerifiedTruth);

        let section = V105SectionMarkerAxiom;
        assert_eq!(section.metadata().confidence, Confidence::VerifiedTruth);
    }

    #[test]
    fn test_v105_summary_aliases_keep_hp1_and_mp1_compact() {
        assert!(is_v105_summary_code("hp1"));
        assert!(is_v105_summary_code("mp1"));
        assert_eq!(get_v105_target_width(5, "hp1", 0, None), 80);
        assert_eq!(get_v105_target_width(5, "mp1", 0, None), 80);
        assert!(!is_v105_summary_code("xrs"));
    }
}
