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
        crate::domain::forensic::stats_recovery::normalize_legacy_item_properties(
            props,
            code,
            axiom.version,
            axiom.save_is_alpha,
        )
    }
}
