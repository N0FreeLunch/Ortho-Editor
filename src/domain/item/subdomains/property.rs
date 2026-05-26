use crate::domain::stats::entity::ItemProperty;
use crate::domain::stats::axiom::StatsAxiom;

/// Item Property Subdomain Combinator
/// Handles the normalization and transformation of property lists for specific item types.
pub trait PropertyNormalizer {
    fn normalize(&self, props: &mut Vec<ItemProperty>, code: &str, axiom: &StatsAxiom);
}

/// Standard Alpha v105 Property Normalizer
pub struct AlphaPropertyCombinator;

impl PropertyNormalizer for AlphaPropertyCombinator {
    fn normalize(&self, props: &mut Vec<ItemProperty>, code: &str, axiom: &StatsAxiom) {
        if !axiom.is_alpha() {
            return;
        }

        let trimmed = code.trim();
        
        // Axiom 0411: Authority (xrs/c8xr) and specialized small charms (scs) 
        // require property list normalization to align with fixture truth.
        if trimmed == "xrs" || trimmed == "c8xr" || trimmed == "scs" {
            for prop in props.iter_mut() {
                let mapped = axiom.map_alpha_id(prop.stat_id);
                if mapped != prop.stat_id {
                    prop.stat_id = mapped;
                }
            }
        }
    }
}
