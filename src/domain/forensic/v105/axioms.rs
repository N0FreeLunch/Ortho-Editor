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
    pub fn resolve_gap(&self, code: Option<&str>, flags: u32, is_first_item: bool) -> usize {
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

        if is_first_item {
            return 0; // Fixture-verified: first item in any JM section has no header gap
        }

        // Forensic: 'cwd' (compact) items often use a 24-bit alignment gap instead of the standard 32.
        // If flag bit 26 or 27 is set, use 8 bits, otherwise check for compact flag.
        if (flags & (1 << 26)) != 0 || (flags & (1 << 27)) != 0 {
            8
        } else if (flags & 0x00000008) != 0 { // Placeholder for compact bit
            0 // Bug fixed: was returning 24, but fixture forensics confirm 0 for compact first-items and sections
        } else {
            32
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
        assert_eq!(gap.metadata().confidence, Confidence::EmergingHypothesis);
    }
}
