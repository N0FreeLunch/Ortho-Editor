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
        if is_compact { return 0; }
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

pub fn is_v105_summary_code(code: &str) -> bool {
    let trimmed = code.trim();
    if trimmed.is_empty() {
        return true; // Blank items are compact markers
    }

    // 1. Known Stealth-Compact patterns (Markers without bit 23 set)
    // (Axiom 0365): Alpha summary items often use raw byte codes like 'H\x04' 
    let bytes = code.as_bytes();
    if bytes.len() >= 2 && bytes[0] == 0xCF && bytes[1] == 0x4F {
        return true;
    }
    if bytes.len() == 2 && bytes[0] == b'H' && bytes[1] == 0x04 {
        return true;
    }

    // 2. Standard markers that are always compact
    if trimmed == "tsc" || trimmed == "isc" {
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
pub fn get_v105_target_width(version: u8, code: &str, flags: u32) -> u32 {
    let trimmed = code.trim();
    let is_summary = is_v105_summary_code(code);
    let is_compact_flag = (flags & (1 << 23)) != 0 || (flags & (1 << 21)) != 0;
    let reg = crate::domain::forensic::registry::get_registry();

    if is_summary || is_compact_flag {
        if let Some(overrides) = &reg.item_overrides {
            if let Some(map) = overrides.get(trimmed) {
                if let Some(&width) = map.get("fixed_width") { return width; }
            }
        }
        if trimmed == "tsc" || trimmed == "isc" || (trimmed == "wuw8" && version == 0) {
            return reg.axioms.get("scroll_fixed_width").cloned().unwrap_or(80) as u32;
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
    }
}
