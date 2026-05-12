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

/// Variable 0-bit/8-bit gap between JM header and body in Alpha v105.
#[derive(Debug, Clone, Default)]
pub struct V105HeaderGapAxiom;

impl ForensicAxiom for V105HeaderGapAxiom {
    fn metadata(&self) -> ForensicMetadata {
        ForensicMetadata::new(
            Confidence::EmergingHypothesis,
            Intentionality::Artifactual,
            "Variable gap between JM header and item body in Alpha v105",
        )
    }
}

impl V105HeaderGapAxiom {
    pub fn resolve_gap(&self, version: u8, code: Option<&str>, flags: u32, is_first_item: bool, is_compact: bool) -> usize {
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

        if is_first_item {
            // Forensic (Axiom 0340): The first item in a JM section is gap-free.
            // Later items may still carry the version- and flag-dependent header gap below.
            return 0;
        }

        // Forensic (Axiom 0340): Some early Alpha versions may still use gaps.
        // Falling through to standard logic.

        // Forensic: 'cwd' (compact) items often use a 24-bit alignment gap instead of the standard 32.
        // If flag bit 26 or 27 is set, use 8 bits, otherwise check for compact flag.
        if (flags & (1 << 26)) != 0 || (flags & (1 << 27)) != 0 {
            8
        } else if is_compact || (flags & (1 << 21)) != 0 || (flags & (1 << 23)) != 0 {
            8 // Compact items (potions) in Alpha v105 use an 8-bit header gap when not the first item (Axiom 0340)
        } else {
            // Forensic: Amulets (umsw) and Rings often use a 24-bit gap in certain Alpha variants.
            // If the code is known to be one of these, or if we are in a 'shifted' state, use 24.
            if let Some(c) = code {
                let t = c.trim();
                if t == "umsw" || t == "rin" || t == "isc" || t == "tsc" {
                    return 24;
                }
            }
            32
        }
    }
}

/// 9+9 bit property rhythm (ID: 9, Value: 9) in Alpha v105.
#[derive(Debug, Clone, Default)]
pub struct V105PropertyRhythmAxiom;

impl ForensicAxiom for V105PropertyRhythmAxiom {
    fn metadata(&self) -> ForensicMetadata {
        ForensicMetadata::new(
            Confidence::VerifiedTruth,
            Intentionality::Structural,
            "9+9 property rhythm (9-bit ID, 9-bit Value) in Alpha v105",
        )
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
        assert_eq!(gap.metadata().confidence, Confidence::EmergingHypothesis);

        let rhythm = V105PropertyRhythmAxiom;
        assert_eq!(rhythm.metadata().confidence, Confidence::VerifiedTruth);
    }
}
