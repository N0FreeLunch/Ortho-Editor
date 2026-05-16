use crate::domain::item::axiom_meta::{Confidence, ForensicAxiom, ForensicMetadata, Intentionality};

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
            2 | 1 | 0 => 3, // Version 2, 1, 0 require a 3-bit residue nudge
            _ => 0,
        }
    }
}

impl V105HeaderGapAxiom {
    pub fn resolve_gap(&self, version: u8, code: Option<&str>, flags: u32, is_first_item: bool, is_compact: bool, has_checksum: bool) -> usize {
        let gap = self.resolve_gap_internal(version, code, flags, is_first_item, is_compact, has_checksum);
        // println!("[DEBUG-SLICE12] Axiom resolve_gap: version={}, code={:?}, is_first={}, is_compact={}, flags=0x{:X}, has_checksum={} -> gap={}",
        //     version, code, is_first_item, is_compact, flags, has_checksum, gap);
        gap
    }

    fn resolve_gap_internal(&self, _version: u8, code: Option<&str>, flags: u32, _is_first_item: bool, is_compact: bool, has_checksum: bool) -> usize {
        let reg = crate::domain::forensic::registry::get_registry();
        if let Some(c) = code {
            let trimmed = c.trim();
            if let Some(overrides) = &reg.item_overrides {
                if let Some(item_map) = overrides.get(trimmed) {
                    if let Some(&gap) = item_map.get("header_gap") {
                        return gap as usize;
                    }
                }
            }
            
            // Axiom 0392: Summary items in Alpha v105 are structurally compact 
            // but still preserve the JM-to-Body gap to maintain the 80-bit rhythm.
            if is_v105_summary_code(trimmed) {
                return if has_checksum { 0 } else { 8 };
            }
        }

        // Runeword/Shadow Items (Bit 26/27)
        if (flags & (1 << 26)) != 0 || (flags & (1 << 27)) != 0 {
            8
        } else if is_compact {
            0
        } else {
            // Standard equipment
            if has_checksum { 0 } else { 8 }
        }
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

/// 19-bit alignment drift resolution for Huffman stream start.
#[derive(Debug, Clone, Default)]
pub struct V105AlignmentAxiom;

impl ForensicAxiom for V105AlignmentAxiom {
    fn metadata(&self) -> ForensicMetadata {
        ForensicMetadata::new(
            Confidence::VerifiedTruth,
            Intentionality::Structural,
            "19-bit huffman alignment drift resolution for Alpha v105",
        )
    }
}

impl V105AlignmentAxiom {
    pub fn get_alignment_nudge(&self, version: u8, code: &str, flags: u32, is_compact: bool) -> usize {
        if is_compact || code.trim().is_empty() { return 0; }
        let is_socketed = (flags & 0x00000008) != 0;
        let trimmed = code.trim();
        match (version, trimmed) {
            (5, "wuw8") => 176,
            (5, "w8cs") => 96,
            (0, "wuw8") | (0, "s7ds") => 22, // 3-bit drift from standard 19-bit
            (0, _) if is_socketed => 32,
            (0, _) => 19,
            (2, _) => 19, // Version 2 follows Version 0 cadence
            _ => 0,
        }
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

pub fn is_v105_summary_code(code: &str) -> bool {
    V105PropertyWidthAxiom::default().is_summary_item(0, code)
}
pub fn get_v105_target_width(version: u8, code: &str, flags: u32) -> u32 {
    let trimmed = code.trim();
    let w_axiom = V105PropertyWidthAxiom::default();
    let is_summary = w_axiom.is_summary_rhythm_forced(version, code);
    let is_compact_flag = (flags & (1 << 23)) != 0 || (flags & (1 << 21)) != 0;
    let reg = crate::domain::forensic::registry::get_registry();

    if is_summary || is_compact_flag {
        if let Some(overrides) = &reg.item_overrides {
            if let Some(map) = overrides.get(trimmed) {
                if let Some(&width) = map.get("fixed_width") { return width; }
            }
        }
        
        if is_summary {
            return w_axiom.summary_item_fixed_width();
        }

        // Alpha v105 Slice 20: 72-bit base slot for compact items.
        // Conditional 1-bit nudge (73 bits) if bit 72 is set as a potential flag or alignment.
        let base_width = reg.axioms.get("compact_item_fixed_width").cloned().unwrap_or(72) as u32;
        return base_width;
    }

    match version {
        1 | 2 | 0 | 4 | 6 => reg.axioms.get("v0_equipment_width").cloned().unwrap_or(72) as u32,
        5 | 7 => reg.axioms.get("v5_equipment_width").cloned().unwrap_or(104) as u32,
        _ => 0,
    }
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

    /// Synchronizes Alpha v105 quest data into the header.
    pub fn sync_quests(&self, header: &mut [u8], woo_pos: Option<usize>, ws_pos: Option<usize>, data: &[u8]) {
        let start = woo_pos.unwrap_or(Self::V105_QUEST_OFFSET);
        let end = ws_pos.unwrap_or(Self::V105_WAYPOINT_OFFSET);
        let max_len = end.saturating_sub(start);
        let len = data.len().min(max_len);
        if header.len() >= start + len {
            header[start..start + len].copy_from_slice(&data[..len]);
        }
    }

    /// Synchronizes Alpha v105 waypoint data into the header.
    pub fn sync_waypoints(&self, header: &mut [u8], ws_pos: Option<usize>, w4_pos: Option<usize>, data: &[u8]) {
        let start = ws_pos.unwrap_or(Self::V105_WAYPOINT_OFFSET);
        let end = w4_pos.unwrap_or(Self::V105_NPC_OFFSET);
        let max_len = end.saturating_sub(start);
        let len = data.len().min(max_len);
        if header.len() >= start + len {
            header[start..start + len].copy_from_slice(&data[..len]);
        }
    }

    /// Synchronizes Alpha v105 NPC section (Expansion) data into the header.
    pub fn sync_npc_section(&self, header: &mut [u8], w4_pos: Option<usize>, data: &[u8]) {
        let start = w4_pos.unwrap_or(Self::V105_NPC_OFFSET);
        let end = Self::V105_HEADER_LEN;
        let max_len = end.saturating_sub(start);
        let len = data.len().min(max_len);
        if header.len() >= start + len {
            header[start..start + len].copy_from_slice(&data[..len]);
        }
    }

    /// Synchronizes character level in the header based on stat section value.
    pub fn sync_char_level(&self, header: &mut [u8], level: u8, offset: usize) {
        if header.len() > offset {
            header[offset] = level;
        }
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
    pub fn is_summary_rhythm_forced(&self, version: u8, code: &str) -> bool {
        let trimmed = code.trim();
        // Axiom 0344: Identify Scroll (isc), Town Portal Scroll (tsc), and Version 0 weapon 'wuw8'
        // are forced to an 80-bit rhythm in Alpha v105.
        trimmed == "tsc" || trimmed == "isc" || (trimmed == "wuw8" && version == 0)
    }

    /// Returns true if the item code is classified as a summary item in Alpha v105 (Axiom 0365).
    pub fn is_summary_item(&self, version: u8, code: &str) -> bool {
        if self.is_summary_rhythm_forced(version, code) {
            return true;
        }

        let trimmed = code.trim();
        if trimmed.is_empty() {
            return false;
        }

        // 1. Known Stealth-Compact patterns (Markers without bit 23 set)
        // (Axiom 0365): Alpha summary items often use raw byte codes like 'H\x04'
        if self.matches_stealth_pattern(code) {
            return true;
        }

        // Pattern: 'bwcw' (Town Portal Book) - shares summary geometry rhythm in Alpha v105
        if trimmed == "bwcw" {
            return true;
        }

        // 3. Fallback to structural patterns (Potions/Runes) - Axiom 0078
        if (trimmed.starts_with('r') && (trimmed.len() == 3 || (trimmed.len() == 4 && trimmed[1..].chars().all(|c| c.is_ascii_digit())))) ||
           ((trimmed.starts_with('h') || trimmed.starts_with("wh")) && (trimmed.len() == 3 || trimmed.len() == 4)) ||
           ((trimmed.starts_with('m') || trimmed.starts_with("wm")) && (trimmed.len() == 3 || trimmed.len() == 4)) ||
           (trimmed.starts_with('v') && (trimmed.len() == 3 || trimmed.len() == 4)) || // Rejuvenation potions / Vials
           (trimmed.starts_with('g') && trimmed.len() == 3) // Gems
        {
            return true;
        }

        let reg = crate::domain::forensic::registry::get_registry();
        // 4. Check registry for explicit forced compact
        if let Some(codes) = &reg.forced_compact_codes {
            if codes.iter().any(|c| c == trimmed) { return true; }
        }

        false
    }

    fn matches_stealth_pattern(&self, code: &str) -> bool {
        // 1. Known Stealth-Compact patterns (Markers without bit 23 set)
        // (Axiom 0365): Alpha summary items often use raw byte codes like 'H\x04'
        // Forensic: Use raw u8 conversion to avoid UTF-8 mismatch for non-ASCII codes (Slice 24)
        let bytes: Vec<u8> = code.chars().map(|c| c as u32 as u8).collect();

        // Pattern: ÏO (0xCF 0x4F)
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

        // Þ. Resolution: 0xDE 0x2E pattern for Alpha v105
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
    pub fn v5_runeword_extra_bits(&self) -> u32 { 2 }
    pub fn ear_class_bits(&self) -> u32 { 3 }
    pub fn ear_level_bits(&self) -> u32 { 7 }
    
    pub fn flags_bits(&self) -> u32 { 32 }
    pub fn checksum_bits(&self) -> u32 { 8 }
    pub fn version_bits(&self) -> u32 { 3 }
    pub fn mode_bits(&self) -> u32 { 3 }
    pub fn location_bits(&self) -> u32 { 3 }
    pub fn x_bits(&self) -> u32 { 4 }
    pub fn nudge_bits(&self) -> u32 { 2 }
    
    pub fn is_extended_stats_early_exit(&self, version: u8) -> bool {
        version == 4 || version == 6 || version == 7
    }
    
    pub fn has_v5_runeword_extra(&self, version: u8) -> bool {
        version == 5 || version == 6 || version == 7
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
}
