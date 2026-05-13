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
            5 => 3, // Version 5 requires a 3-bit nudge
            _ => 0,
        }
    }
}

impl V105HeaderGapAxiom {
    pub fn resolve_gap(&self, version: u8, code: Option<&str>, flags: u32, is_first_item: bool, is_compact: bool, has_checksum: bool) -> usize {
        let gap = self.resolve_gap_internal(version, code, flags, is_first_item, is_compact, has_checksum);
        println!("[DEBUG-SLICE12] Axiom resolve_gap: version={}, code={:?}, is_first={}, is_compact={}, flags=0x{:X}, has_checksum={} -> gap={}", 
            version, code, is_first_item, is_compact, flags, has_checksum, gap);
        gap
    }

    fn resolve_gap_internal(&self, version: u8, code: Option<&str>, flags: u32, _is_first_item: bool, is_compact: bool, has_checksum: bool) -> usize {
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
            if trimmed == "acww" || trimmed == "umsw" || trimmed == "7pw" || trimmed == "oesw" || trimmed == "hps7" || trimmed == "isc" || trimmed == "tsc" {
                return 24;
            }
        }

        // Runeword/Shadow Items (Bit 26/27)
        if (flags & (1 << 26)) != 0 || (flags & (1 << 27)) != 0 {
            8
        } else if is_compact || (flags & (1 << 21)) != 0 || (flags & (1 << 23)) != 0 {
            if has_checksum { 0 } else { 8 }
        } else {
            // Standard equipment
            if has_checksum { 0 } else { 8 }
        }
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
    pub fn get_alignment_nudge(&self, version: u8, _is_compact: bool) -> usize {
        match version {
            0 | 1 | 2 | 5 => 19, // 19-bit drift identified in standard items
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
    }
}
